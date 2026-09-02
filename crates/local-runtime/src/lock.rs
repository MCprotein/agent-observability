use fs2::FileExt;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub struct Singleton {
    file: File,
    pub boot_nonce: [u8; 32],
    metadata_path: PathBuf,
}

/// Serializes short-lived mutations that share one runtime accounting root.
#[derive(Debug)]
pub struct MutationGuard {
    file: File,
}
#[derive(Debug)]
pub enum SingletonError {
    Io(std::io::Error),
    AlreadyRunning,
    CorruptMetadata,
    InsecurePermissions,
    Symlink,
    UnsupportedPlatform,
}
impl From<std::io::Error> for SingletonError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
impl std::fmt::Display for SingletonError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "singleton I/O error: {error}"),
            Self::AlreadyRunning => formatter.write_str("local runtime is already running"),
            Self::CorruptMetadata => formatter.write_str("runtime metadata is corrupt"),
            Self::InsecurePermissions => formatter.write_str("runtime path is not private"),
            Self::Symlink => formatter.write_str("runtime path must not be a symlink"),
            Self::UnsupportedPlatform => {
                formatter.write_str("private singleton files are unsupported on this platform")
            }
        }
    }
}
impl std::error::Error for SingletonError {}
impl Singleton {
    pub fn acquire(dir: &Path) -> Result<Self, SingletonError> {
        private_runtime_dir(dir)?;
        let lock_path = dir.join("runtime.lock");
        let metadata_path = dir.join("runtime.meta");
        reject_symlink(&lock_path)?;
        let mut options = OpenOptions::new();
        options.create(true).read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600).custom_flags(no_follow_flag());
        }
        let file = options.open(&lock_path)?;
        private_open_file(&file)?;
        file.try_lock_exclusive().map_err(|e| {
            if e.kind() == std::io::ErrorKind::WouldBlock {
                SingletonError::AlreadyRunning
            } else {
                SingletonError::Io(e)
            }
        })?;
        let mut nonce = [0; 32];
        getrandom::fill(&mut nonce)
            .map_err(|e| SingletonError::Io(std::io::Error::other(e.to_string())))?;
        let temporary = dir.join(format!(".runtime.meta.tmp.{}", std::process::id()));
        let _ = fs::remove_file(&temporary);
        let mut meta_options = OpenOptions::new();
        meta_options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            meta_options.mode(0o600);
        }
        let mut meta = meta_options.open(&temporary)?;
        writeln!(meta, "runtime_metadata.v1")?;
        writeln!(meta, "pid={}", std::process::id())?;
        writeln!(meta, "boot_nonce={}", encode_nonce(&nonce))?;
        meta.sync_all()?;
        private_open_file(&meta)?;
        fs::rename(&temporary, &metadata_path)?;
        File::open(dir)?.sync_all()?;
        Ok(Self {
            file,
            boot_nonce: nonce,
            metadata_path,
        })
    }
    pub fn metadata_path(&self) -> &Path {
        &self.metadata_path
    }
    pub fn read_nonce(path: &Path) -> Result<[u8; 32], SingletonError> {
        reject_symlink(path)?;
        let mut body = String::new();
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(no_follow_flag());
        }
        let mut file = options.open(path)?;
        private_open_file(&file)?;
        file.read_to_string(&mut body)?;
        let mut lines = body.lines();
        if lines.next() != Some("runtime_metadata.v1") {
            return Err(SingletonError::CorruptMetadata);
        }
        let pid = lines.next().ok_or(SingletonError::CorruptMetadata)?;
        if !pid.starts_with("pid=") || pid[4..].parse::<u32>().is_err() {
            return Err(SingletonError::CorruptMetadata);
        }
        let nonce = lines.next().ok_or(SingletonError::CorruptMetadata)?;
        if lines.next().is_some() || !nonce.starts_with("boot_nonce=") {
            return Err(SingletonError::CorruptMetadata);
        }
        decode_nonce(&nonce[11..])
    }
}
impl Drop for Singleton {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.metadata_path);
        let _ = FileExt::unlock(&self.file);
    }
}

impl MutationGuard {
    pub fn acquire(runtime_dir: &Path) -> Result<Self, SingletonError> {
        Self::acquire_with(runtime_dir, false)
    }

    pub fn try_acquire(runtime_dir: &Path) -> Result<Self, SingletonError> {
        Self::acquire_with(runtime_dir, true)
    }

    fn acquire_with(runtime_dir: &Path, nonblocking: bool) -> Result<Self, SingletonError> {
        private_runtime_dir(runtime_dir)?;
        let lock_path = runtime_dir.join("mutation.lock");
        reject_symlink(&lock_path)?;
        let mut options = OpenOptions::new();
        options.create(true).read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600).custom_flags(no_follow_flag());
        }
        let file = options.open(&lock_path)?;
        private_open_file(&file)?;
        if nonblocking {
            file.try_lock_exclusive().map_err(|error| {
                if error.kind() == std::io::ErrorKind::WouldBlock {
                    SingletonError::AlreadyRunning
                } else {
                    SingletonError::Io(error)
                }
            })?;
        } else {
            file.lock_exclusive().map_err(SingletonError::Io)?;
        }
        Ok(Self { file })
    }
}

impl Drop for MutationGuard {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

fn encode_nonce(nonce: &[u8; 32]) -> String {
    use std::fmt::Write as _;

    nonce
        .iter()
        .fold(String::with_capacity(64), |mut encoded, byte| {
            write!(encoded, "{byte:02x}").expect("writing to a String cannot fail");
            encoded
        })
}

fn decode_nonce(value: &str) -> Result<[u8; 32], SingletonError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(SingletonError::CorruptMetadata);
    }
    let mut nonce = [0_u8; 32];
    let (chunks, remainder) = value.as_bytes().as_chunks::<2>();
    debug_assert!(remainder.is_empty());
    for (index, chunk) in chunks.iter().enumerate() {
        let text = std::str::from_utf8(chunk).map_err(|_| SingletonError::CorruptMetadata)?;
        nonce[index] = u8::from_str_radix(text, 16).map_err(|_| SingletonError::CorruptMetadata)?;
    }
    Ok(nonce)
}

#[cfg(unix)]
fn private_runtime_dir(path: &Path) -> Result<(), SingletonError> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    if !path.exists() {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true).mode(0o700);
        builder.create(path)?;
    }
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(SingletonError::Symlink);
    }
    if !metadata.is_dir() {
        return Err(SingletonError::Io(std::io::Error::other(
            "runtime path is not a directory",
        )));
    }
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(SingletonError::InsecurePermissions);
    }
    Ok(())
}

#[cfg(not(unix))]
fn private_runtime_dir(_path: &Path) -> Result<(), SingletonError> {
    Err(SingletonError::UnsupportedPlatform)
}

fn reject_symlink(path: &Path) -> Result<(), SingletonError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(SingletonError::Symlink),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(unix)]
fn private_open_file(file: &File) -> Result<(), SingletonError> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(SingletonError::Io(std::io::Error::other(
            "runtime file is not regular",
        )));
    }
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(SingletonError::InsecurePermissions);
    }
    Ok(())
}

#[cfg(not(unix))]
fn private_open_file(_file: &File) -> Result<(), SingletonError> {
    Err(SingletonError::UnsupportedPlatform)
}

#[cfg(any(target_os = "linux", target_os = "android"))]
const fn no_follow_flag() -> i32 {
    0x20_000
}

#[cfg(target_os = "macos")]
const fn no_follow_flag() -> i32 {
    0x100
}

#[cfg(test)]
mod tests {
    use super::*;

    fn private_dir(path: &Path) {
        let _ = fs::remove_dir_all(path);
        fs::create_dir_all(path).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
        }
    }

    #[test]
    fn exclusive_and_nonce_are_private() {
        let d = std::env::temp_dir().join(format!("local-runtime-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        let a = Singleton::acquire(&d).unwrap();
        assert!(Singleton::acquire(&d).is_err());
        assert_eq!(
            Singleton::read_nonce(a.metadata_path()).unwrap(),
            a.boot_nonce
        );
        drop(a);
        let b = Singleton::acquire(&d).unwrap();
        assert_ne!(b.boot_nonce, [0; 32]);
        drop(b);
        let _ = fs::remove_dir_all(d);
    }
    #[test]
    fn corrupt_metadata_is_rejected() {
        let d = std::env::temp_dir().join(format!("local-runtime-corrupt-{}", std::process::id()));
        private_dir(&d);
        let metadata = d.join("runtime.meta");
        fs::write(&metadata, b"bad").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&metadata, fs::Permissions::from_mode(0o600)).unwrap();
        }
        assert!(matches!(
            Singleton::read_nonce(&d.join("runtime.meta")),
            Err(SingletonError::CorruptMetadata)
        ));
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn stale_metadata_is_replaced_after_lock_is_available() {
        let d = std::env::temp_dir().join(format!("local-runtime-stale-{}", std::process::id()));
        private_dir(&d);
        fs::write(d.join("runtime.meta"), b"stale").unwrap();
        let owner = Singleton::acquire(&d).unwrap();
        assert_ne!(owner.boot_nonce, [7_u8; 32]);
        assert_eq!(
            Singleton::read_nonce(owner.metadata_path()).unwrap(),
            owner.boot_nonce
        );
        drop(owner);
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn concurrent_launches_have_one_owner() {
        let d = std::env::temp_dir().join(format!("local-runtime-race-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        let first = d.clone();
        let second = d.clone();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let a_barrier = barrier.clone();
        let a = std::thread::spawn(move || {
            a_barrier.wait();
            Singleton::acquire(&first)
        });
        let b_barrier = barrier;
        let b = std::thread::spawn(move || {
            b_barrier.wait();
            Singleton::acquire(&second)
        });
        let a = a.join().unwrap();
        let b = b.join().unwrap();
        assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
        drop(a);
        drop(b);
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn mutation_guard_waits_for_the_current_writer() {
        let d = std::env::temp_dir().join(format!("local-runtime-mutation-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        let first = MutationGuard::acquire(&d).unwrap();
        let second_dir = d.clone();
        let acquired = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let acquired_by_thread = acquired.clone();
        let thread = std::thread::spawn(move || {
            let _second = MutationGuard::acquire(&second_dir).unwrap();
            acquired_by_thread.store(true, std::sync::atomic::Ordering::Release);
        });
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert!(!acquired.load(std::sync::atomic::Ordering::Acquire));
        drop(first);
        thread.join().unwrap();
        assert!(acquired.load(std::sync::atomic::Ordering::Acquire));
        let _ = fs::remove_dir_all(d);
    }

    #[cfg(unix)]
    #[test]
    fn broad_runtime_directory_is_rejected() {
        use std::os::unix::fs::PermissionsExt;

        let d = std::env::temp_dir().join(format!("local-runtime-broad-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        fs::set_permissions(&d, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(matches!(
            Singleton::acquire(&d),
            Err(SingletonError::InsecurePermissions)
        ));
        let _ = fs::remove_dir_all(d);
    }

    #[cfg(unix)]
    #[test]
    fn metadata_symlink_is_rejected() {
        use std::os::unix::fs::symlink;

        let d = std::env::temp_dir().join(format!("local-runtime-link-{}", std::process::id()));
        private_dir(&d);
        let target = d.join("target");
        fs::write(
            &target,
            b"runtime_metadata.v1\npid=1\nboot_nonce=0000000000000000000000000000000000000000000000000000000000000000\n",
        )
        .unwrap();
        symlink(&target, d.join("runtime.meta")).unwrap();
        assert!(matches!(
            Singleton::read_nonce(&d.join("runtime.meta")),
            Err(SingletonError::Symlink)
        ));
        let _ = fs::remove_dir_all(d);
    }
}
