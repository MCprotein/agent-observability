use fs2::FileExt;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, Write},
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub struct Singleton {
    file: File,
    pub boot_nonce: [u8; 32],
    metadata_path: PathBuf,
    remove_metadata_on_drop: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoordinatedSingletonScope {
    Runtime,
    Collector,
    SettingsUi,
    DashboardUi,
}

#[derive(Debug)]
pub struct CoordinatedSingleton {
    singleton: Singleton,
    barrier: crate::storage_coherence::StorageBarrier,
    directory: File,
    metadata: File,
}

#[derive(Debug)]
pub enum CoordinatedSingletonError {
    Coherence(crate::storage_coherence::StorageCoherenceError),
    Singleton(SingletonError),
}

#[derive(Clone, Copy)]
enum SingletonAcquireMode<'permit, 'barrier> {
    Legacy,
    Coordinated {
        permit: &'permit crate::storage_coherence::StorageWriteGuard<'barrier>,
        #[cfg(test)]
        before_metadata_rename: Option<fn(&Path)>,
    },
}

const RUNTIME_METADATA_MAX_BYTES: usize = 256;
const RUNTIME_METADATA_MAX_BYTES_U64: u64 = 256;

/// Serializes short-lived mutations that share one runtime accounting root.
#[derive(Debug)]
pub struct MutationGuard {
    file: File,
    directory: File,
    runtime_dir: PathBuf,
}
#[derive(Debug)]
pub enum SingletonError {
    Io(std::io::Error),
    AlreadyRunning,
    CorruptMetadata,
    InsecurePermissions,
    Symlink,
    UnsupportedPlatform,
    WrongMutationRoot,
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
            Self::WrongMutationRoot => {
                formatter.write_str("mutation guard does not own this runtime root")
            }
            Self::UnsupportedPlatform => {
                formatter.write_str("private singleton files are unsupported on this platform")
            }
        }
    }
}
impl std::error::Error for SingletonError {}

impl std::fmt::Display for CoordinatedSingletonError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Coherence(error) => write!(formatter, "singleton coherence error: {error}"),
            Self::Singleton(error) => error.fmt(formatter),
        }
    }
}
impl std::error::Error for CoordinatedSingletonError {}
impl From<crate::storage_coherence::StorageCoherenceError> for CoordinatedSingletonError {
    fn from(error: crate::storage_coherence::StorageCoherenceError) -> Self {
        Self::Coherence(error)
    }
}
impl From<SingletonError> for CoordinatedSingletonError {
    fn from(error: SingletonError) -> Self {
        Self::Singleton(error)
    }
}
impl From<std::io::Error> for CoordinatedSingletonError {
    fn from(error: std::io::Error) -> Self {
        Self::Singleton(error.into())
    }
}

impl CoordinatedSingletonScope {
    fn directory(self, root: &Path) -> PathBuf {
        match self {
            Self::Runtime => root.join("runtime"),
            Self::Collector => root.join("runtime/collector"),
            Self::SettingsUi => root.join("runtime/settings-ui"),
            Self::DashboardUi => root.join("runtime/dashboard-ui"),
        }
    }
}

impl Singleton {
    pub fn acquire(dir: &Path) -> Result<Self, SingletonError> {
        match Self::acquire_inner(dir, SingletonAcquireMode::Legacy) {
            Ok((singleton, _, _)) => Ok(singleton),
            Err(CoordinatedSingletonError::Singleton(error)) => Err(error),
            Err(CoordinatedSingletonError::Coherence(_)) => {
                unreachable!("legacy singleton acquisition does not use storage coherence")
            }
        }
    }

    pub fn acquire_coordinated(
        root: &Path,
        scope: CoordinatedSingletonScope,
    ) -> Result<CoordinatedSingleton, CoordinatedSingletonError> {
        let barrier = crate::storage_coherence::StorageBarrier::open_existing(root)?;
        let permit = barrier.try_begin_write()?;
        let singleton_path = scope.directory(root);
        let (singleton, directory, metadata) = Self::acquire_inner(
            &singleton_path,
            SingletonAcquireMode::Coordinated {
                permit: &permit,
                #[cfg(test)]
                before_metadata_rename: None,
            },
        )?;
        permit.revalidate()?;
        same_private_directory(&directory, &singleton_path)?;
        validate_private_metadata(&metadata)?;
        same_file(&metadata, singleton.metadata_path())?;
        if read_nonce_from_file(&metadata)? != singleton.boot_nonce {
            return Err(SingletonError::CorruptMetadata.into());
        }
        drop(permit);
        Ok(CoordinatedSingleton {
            singleton,
            barrier,
            directory,
            metadata,
        })
    }

    fn acquire_inner(
        dir: &Path,
        mode: SingletonAcquireMode<'_, '_>,
    ) -> Result<(Self, File, File), CoordinatedSingletonError> {
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
        let mut coordinated_existing_metadata = None;
        let coordinated_directory = match &mode {
            SingletonAcquireMode::Legacy => None,
            SingletonAcquireMode::Coordinated { permit, .. } => {
                let directory = File::open(dir)?;
                validate_coordinated_singleton_scope(permit, &directory, dir, &file, &lock_path)?;
                coordinated_existing_metadata = open_existing_coordinated_metadata(&metadata_path)?;
                Some(directory)
            }
        };
        let mut nonce = [0; 32];
        getrandom::fill(&mut nonce)
            .map_err(|e| SingletonError::Io(std::io::Error::other(e.to_string())))?;
        let temporary = dir.join(format!(".runtime.meta.tmp.{}", std::process::id()));
        if matches!(&mode, SingletonAcquireMode::Legacy) {
            let _ = fs::remove_file(&temporary);
        }
        let mut meta_options = OpenOptions::new();
        meta_options
            .create_new(true)
            .read(!matches!(&mode, SingletonAcquireMode::Legacy))
            .write(true);
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
        if let SingletonAcquireMode::Coordinated {
            permit,
            #[cfg(test)]
            before_metadata_rename,
        } = &mode
        {
            #[cfg(test)]
            if let Some(hook) = before_metadata_rename {
                hook(dir);
            }
            let directory = coordinated_directory
                .as_ref()
                .expect("coordinated acquisition retains its directory descriptor");
            if let Err(error) =
                validate_coordinated_singleton_scope(permit, directory, dir, &file, &lock_path)
                    .and_then(|()| {
                        revalidate_existing_coordinated_metadata(
                            coordinated_existing_metadata.as_ref(),
                            &metadata_path,
                        )
                        .map_err(CoordinatedSingletonError::from)
                    })
            {
                cleanup_coordinated_temporary(directory, &meta, &temporary)?;
                return Err(error);
            }
        }
        fs::rename(&temporary, &metadata_path)?;
        let directory = match coordinated_directory {
            Some(directory) => directory,
            None => File::open(dir)?,
        };
        directory.sync_all()?;
        let remove_metadata_on_drop = matches!(&mode, SingletonAcquireMode::Legacy);
        let singleton = Self {
            file,
            boot_nonce: nonce,
            metadata_path,
            remove_metadata_on_drop,
        };
        Ok((singleton, directory, meta))
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
        parse_nonce(&body)
    }
}
impl Drop for Singleton {
    fn drop(&mut self) {
        if self.remove_metadata_on_drop {
            let _ = fs::remove_file(&self.metadata_path);
        }
        let _ = FileExt::unlock(&self.file);
    }
}

impl CoordinatedSingleton {
    pub fn metadata_path(&self) -> &Path {
        self.singleton.metadata_path()
    }

    pub fn boot_nonce(&self) -> [u8; 32] {
        self.singleton.boot_nonce
    }
}

impl Drop for CoordinatedSingleton {
    fn drop(&mut self) {
        let Ok(permit) = self.barrier.try_begin_write() else {
            return;
        };
        let Some(directory_path) = self.singleton.metadata_path.parent() else {
            return;
        };
        if permit.revalidate().is_err()
            || same_private_directory(&self.directory, directory_path).is_err()
            || validate_private_empty_lock(&self.singleton.file).is_err()
            || same_file(&self.singleton.file, &directory_path.join("runtime.lock")).is_err()
            || validate_private_metadata(&self.metadata).is_err()
            || same_file(&self.metadata, &self.singleton.metadata_path).is_err()
            || read_nonce_from_file(&self.metadata).ok() != Some(self.singleton.boot_nonce)
        {
            return;
        }
        if fs::remove_file(&self.singleton.metadata_path).is_ok() {
            let _ = self.directory.sync_all();
        }
    }
}

impl MutationGuard {
    pub(crate) fn require_root(&self, root: &Path) -> Result<(), SingletonError> {
        let runtime = root.join("runtime");
        if fs::canonicalize(&runtime)? != self.runtime_dir {
            return Err(SingletonError::WrongMutationRoot);
        }
        validate_private_runtime_dir(&runtime)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let held = self.directory.metadata()?;
            let named = fs::metadata(&runtime)?;
            if held.dev() != named.dev() || held.ino() != named.ino() {
                return Err(SingletonError::WrongMutationRoot);
            }
        }
        same_file(&self.file, &runtime.join("mutation.lock"))
    }

    pub fn acquire(runtime_dir: &Path) -> Result<Self, SingletonError> {
        Self::acquire_with(runtime_dir, false)
    }

    pub fn try_acquire(runtime_dir: &Path) -> Result<Self, SingletonError> {
        Self::acquire_with(runtime_dir, true)
    }

    pub fn matches_accounting_lock(
        &self,
        root: &Path,
        relative: &Path,
        candidate: &File,
    ) -> Result<bool, SingletonError> {
        if relative != Path::new("runtime/mutation.lock") {
            return Ok(false);
        }
        self.require_root(root)?;
        validate_private_empty_lock(&self.file)?;
        validate_private_empty_lock(candidate)?;
        same_identity(&self.file, candidate)?;
        Ok(true)
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
        let directory = File::open(runtime_dir)?;
        let runtime_dir = fs::canonicalize(runtime_dir)?;
        same_file(&file, &runtime_dir.join("mutation.lock"))?;
        Ok(Self {
            file,
            directory,
            runtime_dir,
        })
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

fn read_nonce_from_file(file: &File) -> Result<[u8; 32], SingletonError> {
    let mut file = file.try_clone()?;
    file.rewind()?;
    let mut body = String::new();
    file.read_to_string(&mut body)?;
    parse_nonce(&body)
}

fn validate_coordinated_singleton_scope(
    permit: &crate::storage_coherence::StorageWriteGuard<'_>,
    directory: &File,
    directory_path: &Path,
    lock: &File,
    lock_path: &Path,
) -> Result<(), CoordinatedSingletonError> {
    permit.revalidate()?;
    same_private_directory(directory, directory_path)?;
    validate_private_empty_lock(lock)?;
    same_file(lock, lock_path)?;
    Ok(())
}

fn open_existing_coordinated_metadata(path: &Path) -> Result<Option<File>, SingletonError> {
    open_existing_coordinated_metadata_observing(path, || {})
}

fn open_existing_coordinated_metadata_observing(
    path: &Path,
    before_open: impl FnOnce(),
) -> Result<Option<File>, SingletonError> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(SingletonError::Symlink);
        }
        Ok(metadata) if !metadata.is_file() => return Err(SingletonError::CorruptMetadata),
        Ok(_) => {}
    }

    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(no_follow_flag() | nonblocking_flag());
    }
    before_open();
    let file = options.open(path)?;
    revalidate_existing_coordinated_metadata(Some(&file), path)?;
    Ok(Some(file))
}

fn revalidate_existing_coordinated_metadata(
    file: Option<&File>,
    path: &Path,
) -> Result<(), SingletonError> {
    let Some(file) = file else {
        return match fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
            Ok(_) => Err(SingletonError::WrongMutationRoot),
        };
    };
    validate_private_metadata(file)?;
    same_file(file, path)?;
    read_bounded_nonce_from_file(file).map(|_| ())
}

fn read_bounded_nonce_from_file(file: &File) -> Result<[u8; 32], SingletonError> {
    if file.metadata()?.len() > RUNTIME_METADATA_MAX_BYTES_U64 {
        return Err(SingletonError::CorruptMetadata);
    }
    let mut file = file.try_clone()?;
    file.rewind()?;
    let mut body = Vec::with_capacity(RUNTIME_METADATA_MAX_BYTES);
    file.take(RUNTIME_METADATA_MAX_BYTES_U64 + 1)
        .read_to_end(&mut body)?;
    if body.len() > RUNTIME_METADATA_MAX_BYTES {
        return Err(SingletonError::CorruptMetadata);
    }
    let body = std::str::from_utf8(&body).map_err(|_| SingletonError::CorruptMetadata)?;
    parse_nonce(body)
}

fn cleanup_coordinated_temporary(
    directory: &File,
    temporary_file: &File,
    temporary: &Path,
) -> Result<(), SingletonError> {
    validate_private_metadata(temporary_file)?;
    same_file(temporary_file, temporary)?;
    match fs::remove_file(temporary) {
        Ok(()) => directory.sync_all()?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn parse_nonce(body: &str) -> Result<[u8; 32], SingletonError> {
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
pub(crate) fn private_runtime_dir(path: &Path) -> Result<(), SingletonError> {
    use std::os::unix::fs::DirBuilderExt;
    if !path.exists() {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true).mode(0o700);
        builder.create(path)?;
    }
    validate_private_runtime_dir(path)
}

/// Validate an existing directory without creating it, including after removal.
#[cfg(unix)]
pub(crate) fn validate_private_runtime_dir(path: &Path) -> Result<(), SingletonError> {
    use std::os::unix::fs::PermissionsExt;
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
pub(crate) fn private_runtime_dir(_path: &Path) -> Result<(), SingletonError> {
    Err(SingletonError::UnsupportedPlatform)
}

#[cfg(not(unix))]
pub(crate) fn validate_private_runtime_dir(_path: &Path) -> Result<(), SingletonError> {
    Err(SingletonError::UnsupportedPlatform)
}

pub(crate) fn reject_symlink(path: &Path) -> Result<(), SingletonError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(SingletonError::Symlink),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(unix)]
pub(crate) fn private_open_file(file: &File) -> Result<(), SingletonError> {
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

#[cfg(unix)]
fn validate_private_metadata(file: &File) -> Result<(), SingletonError> {
    use std::os::unix::fs::MetadataExt;

    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.nlink() != 1 {
        return Err(SingletonError::WrongMutationRoot);
    }
    if metadata.mode() & 0o7777 != 0o600 {
        return Err(SingletonError::InsecurePermissions);
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_private_metadata(_file: &File) -> Result<(), SingletonError> {
    Err(SingletonError::UnsupportedPlatform)
}

fn validate_private_empty_lock(file: &File) -> Result<(), SingletonError> {
    validate_private_metadata(file)?;
    if file.metadata()?.len() != 0 {
        return Err(SingletonError::WrongMutationRoot);
    }
    Ok(())
}

#[cfg(unix)]
fn same_private_directory(file: &File, path: &Path) -> Result<(), SingletonError> {
    use std::os::unix::fs::MetadataExt;

    validate_private_runtime_dir(path)?;
    let held = file.metadata()?;
    let named = fs::symlink_metadata(path)?;
    if !held.is_dir() || (held.dev(), held.ino()) != (named.dev(), named.ino()) {
        return Err(SingletonError::WrongMutationRoot);
    }
    Ok(())
}

#[cfg(not(unix))]
fn same_private_directory(_file: &File, _path: &Path) -> Result<(), SingletonError> {
    Err(SingletonError::UnsupportedPlatform)
}

#[cfg(unix)]
fn same_identity(first: &File, second: &File) -> Result<(), SingletonError> {
    use std::os::unix::fs::MetadataExt;

    let first = first.metadata()?;
    let second = second.metadata()?;
    if (first.dev(), first.ino()) != (second.dev(), second.ino()) {
        return Err(SingletonError::WrongMutationRoot);
    }
    Ok(())
}

#[cfg(not(unix))]
fn same_identity(_first: &File, _second: &File) -> Result<(), SingletonError> {
    Err(SingletonError::UnsupportedPlatform)
}

#[cfg(not(unix))]
pub(crate) fn private_open_file(_file: &File) -> Result<(), SingletonError> {
    Err(SingletonError::UnsupportedPlatform)
}

#[cfg(any(target_os = "linux", target_os = "android"))]
pub(crate) const fn no_follow_flag() -> i32 {
    0x20_000
}

#[cfg(target_os = "macos")]
pub(crate) const fn no_follow_flag() -> i32 {
    0x100
}

/// Recheck the named lock against the held descriptor; never unlink lock files.
#[cfg(unix)]
pub(crate) fn same_file(file: &File, path: &Path) -> Result<(), SingletonError> {
    use std::os::unix::fs::MetadataExt;
    reject_symlink(path)?;
    private_open_file(file)?;
    let held = file.metadata()?;
    let named = fs::symlink_metadata(path)?;
    if held.dev() != named.dev() || held.ino() != named.ino() || held.nlink() != 1 {
        return Err(SingletonError::WrongMutationRoot);
    }
    Ok(())
}

#[cfg(not(unix))]
pub(crate) fn same_file(_file: &File, _path: &Path) -> Result<(), SingletonError> {
    Err(SingletonError::UnsupportedPlatform)
}

#[cfg(any(target_os = "linux", target_os = "android"))]
pub(crate) const fn nonblocking_flag() -> i32 {
    0x800
}

#[cfg(target_os = "macos")]
pub(crate) const fn nonblocking_flag() -> i32 {
    0x4
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn coordinated_metadata_fifo_is_rejected_without_blocking() {
        const PROBE: &str = "AGENTOBS_SINGLETON_METADATA_FIFO_PROBE";
        if std::env::var_os(PROBE).is_none() {
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "lock::tests::coordinated_metadata_fifo_is_rejected_without_blocking",
                ])
                .env(PROBE, "1")
                .spawn()
                .unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                if let Some(status) = child.try_wait().unwrap() {
                    assert!(status.success());
                    return;
                }
                if std::time::Instant::now() >= deadline {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    panic!("coordinated metadata open blocked on FIFO");
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        let root = std::env::temp_dir().join(format!("singleton-fifo-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let path = root.join("runtime.meta");
        fs::write(&path, b"original").unwrap();
        let result = open_existing_coordinated_metadata_observing(&path, || {
            fs::rename(&path, root.join("original")).unwrap();
            assert!(
                std::process::Command::new("mkfifo")
                    .args(["-m", "600"])
                    .arg(&path)
                    .status()
                    .unwrap()
                    .success()
            );
        });
        assert!(result.is_err());
        assert!(open_existing_coordinated_metadata(&path).is_err());
        assert_eq!(fs::read(root.join("original")).unwrap(), b"original");
        fs::remove_dir_all(root).unwrap();
    }

    fn private_dir(path: &Path) {
        let _ = fs::remove_dir_all(path);
        fs::create_dir_all(path).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
        }
    }

    #[cfg(unix)]
    fn coordinated_fixture(
        name: &str,
    ) -> (
        PathBuf,
        MutationGuard,
        crate::storage_coherence::StorageBarrier,
    ) {
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!(
            "local-runtime-coordinated-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let mutation = MutationGuard::try_acquire(&root.join("runtime")).unwrap();
        let barrier =
            crate::storage_coherence::StorageBarrier::initialize(&root, &mutation).unwrap();
        (root, mutation, barrier)
    }

    #[cfg(unix)]
    #[test]
    fn coordinated_freeze_blocks_singleton_creation_without_creating_scope() {
        let (root, mutation, barrier) = coordinated_fixture("freeze-create");
        let freeze = barrier.try_freeze(&mutation).unwrap();
        let singleton_dir = root.join("runtime/collector");

        assert!(matches!(
            Singleton::acquire_coordinated(&root, CoordinatedSingletonScope::Collector),
            Err(CoordinatedSingletonError::Coherence(
                crate::storage_coherence::StorageCoherenceError::Busy
            ))
        ));
        assert!(!singleton_dir.exists());

        drop(freeze);
        drop(barrier);
        drop(mutation);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn coordinated_active_singleton_does_not_block_freeze() {
        let (root, mutation, barrier) = coordinated_fixture("active-freeze");
        let owner =
            Singleton::acquire_coordinated(&root, CoordinatedSingletonScope::SettingsUi).unwrap();

        let freeze = barrier.try_freeze(&mutation).unwrap();
        freeze.revalidate().unwrap();

        drop(freeze);
        drop(owner);
        drop(barrier);
        drop(mutation);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn coordinated_drop_under_freeze_preserves_metadata_then_creation_recovers_it() {
        let (root, mutation, barrier) = coordinated_fixture("freeze-drop");
        let owner =
            Singleton::acquire_coordinated(&root, CoordinatedSingletonScope::DashboardUi).unwrap();
        let metadata_path = owner.metadata_path().to_path_buf();
        let original = fs::read(&metadata_path).unwrap();
        let first_nonce = owner.boot_nonce();
        let freeze = barrier.try_freeze(&mutation).unwrap();

        drop(owner);
        assert_eq!(fs::read(&metadata_path).unwrap(), original);

        drop(freeze);
        let recovered =
            Singleton::acquire_coordinated(&root, CoordinatedSingletonScope::DashboardUi).unwrap();
        assert_ne!(recovered.boot_nonce(), first_nonce);
        assert_eq!(
            Singleton::read_nonce(recovered.metadata_path()).unwrap(),
            recovered.boot_nonce()
        );
        drop(recovered);
        assert!(!metadata_path.exists());

        drop(barrier);
        drop(mutation);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn coordinated_drop_refuses_replacement_metadata() {
        use std::os::unix::fs::PermissionsExt;

        let (root, mutation, barrier) = coordinated_fixture("metadata-replacement");
        let owner =
            Singleton::acquire_coordinated(&root, CoordinatedSingletonScope::Collector).unwrap();
        let metadata_path = owner.metadata_path().to_path_buf();
        let original_path = metadata_path.with_extension("meta.original");
        fs::rename(&metadata_path, &original_path).unwrap();
        fs::write(&metadata_path, b"replacement").unwrap();
        fs::set_permissions(&metadata_path, fs::Permissions::from_mode(0o600)).unwrap();

        drop(owner);
        assert_eq!(fs::read(&metadata_path).unwrap(), b"replacement");

        drop(barrier);
        drop(mutation);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn coordinated_drop_refuses_replaced_root_identity() {
        use std::os::unix::fs::PermissionsExt;

        let (root, mutation, barrier) = coordinated_fixture("root-replacement");
        let owner =
            Singleton::acquire_coordinated(&root, CoordinatedSingletonScope::Runtime).unwrap();
        let original_root = root.with_extension("original");
        let _ = fs::remove_dir_all(&original_root);
        fs::rename(&root, &original_root).unwrap();
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();

        drop(owner);
        assert!(original_root.join("runtime/runtime.meta").exists());

        drop(barrier);
        drop(mutation);
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(original_root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn coordinated_acquire_preserves_preexisting_temporary_metadata() {
        use std::os::unix::fs::PermissionsExt;

        let (root, mutation, barrier) = coordinated_fixture("temporary-collision");
        let singleton_dir = root.join("runtime/collector");
        fs::create_dir(&singleton_dir).unwrap();
        fs::set_permissions(&singleton_dir, fs::Permissions::from_mode(0o700)).unwrap();
        let temporary = singleton_dir.join(format!(".runtime.meta.tmp.{}", std::process::id()));
        fs::write(&temporary, b"preexisting").unwrap();
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600)).unwrap();

        assert!(
            Singleton::acquire_coordinated(&root, CoordinatedSingletonScope::Collector).is_err()
        );
        assert_eq!(fs::read(&temporary).unwrap(), b"preexisting");
        assert!(!singleton_dir.join("runtime.meta").exists());

        drop(barrier);
        drop(mutation);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn coordinated_acquire_rejects_nonempty_lock_and_preserves_metadata() {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

        let (root, mutation, barrier) = coordinated_fixture("nonempty-lock");
        let singleton_dir = root.join("runtime/collector");
        fs::create_dir(&singleton_dir).unwrap();
        fs::set_permissions(&singleton_dir, fs::Permissions::from_mode(0o700)).unwrap();
        let lock_path = singleton_dir.join("runtime.lock");
        OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&lock_path)
            .unwrap()
            .write_all(b"not-empty")
            .unwrap();
        let metadata_path = singleton_dir.join("runtime.meta");
        let original = b"runtime_metadata.v1\npid=7\nboot_nonce=0707070707070707070707070707070707070707070707070707070707070707\n";
        fs::write(&metadata_path, original).unwrap();
        fs::set_permissions(&metadata_path, fs::Permissions::from_mode(0o600)).unwrap();

        assert!(matches!(
            Singleton::acquire_coordinated(&root, CoordinatedSingletonScope::Collector),
            Err(CoordinatedSingletonError::Singleton(
                SingletonError::WrongMutationRoot
            ))
        ));
        assert_eq!(fs::read(&metadata_path).unwrap(), original);

        drop(barrier);
        drop(mutation);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn coordinated_acquire_rejects_aliased_or_inexact_private_lock() {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

        for (name, alias) in [("aliased-lock", true), ("inexact-lock-mode", false)] {
            let (root, mutation, barrier) = coordinated_fixture(name);
            let singleton_dir = root.join("runtime/collector");
            fs::create_dir(&singleton_dir).unwrap();
            fs::set_permissions(&singleton_dir, fs::Permissions::from_mode(0o700)).unwrap();
            let lock_path = singleton_dir.join("runtime.lock");
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .open(&lock_path)
                .unwrap();
            if alias {
                fs::hard_link(&lock_path, singleton_dir.join("runtime.lock.alias")).unwrap();
            } else {
                fs::set_permissions(&lock_path, fs::Permissions::from_mode(0o700)).unwrap();
            }

            assert!(
                Singleton::acquire_coordinated(&root, CoordinatedSingletonScope::Collector)
                    .is_err()
            );
            assert!(!singleton_dir.join("runtime.meta").exists());

            drop(barrier);
            drop(mutation);
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[cfg(unix)]
    #[test]
    fn coordinated_acquire_rejects_invalid_existing_metadata_without_replacing_it() {
        use std::os::unix::fs::PermissionsExt;

        for (name, contents, mode) in [
            ("corrupt-metadata", b"corrupt".to_vec(), 0o600),
            (
                "oversized-metadata",
                vec![b'x'; RUNTIME_METADATA_MAX_BYTES + 1],
                0o600,
            ),
            (
                "inexact-metadata-mode",
                b"runtime_metadata.v1\npid=7\nboot_nonce=0707070707070707070707070707070707070707070707070707070707070707\n"
                    .to_vec(),
                0o700,
            ),
        ] {
            let (root, mutation, barrier) = coordinated_fixture(name);
            let singleton_dir = root.join("runtime/collector");
            fs::create_dir(&singleton_dir).unwrap();
            fs::set_permissions(&singleton_dir, fs::Permissions::from_mode(0o700)).unwrap();
            let metadata_path = singleton_dir.join("runtime.meta");
            fs::write(&metadata_path, &contents).unwrap();
            fs::set_permissions(&metadata_path, fs::Permissions::from_mode(mode)).unwrap();
            let original = fs::read(&metadata_path).unwrap();

            assert!(Singleton::acquire_coordinated(
                &root,
                CoordinatedSingletonScope::Collector
            )
            .is_err());
            assert_eq!(fs::read(&metadata_path).unwrap(), original);

            drop(barrier);
            drop(mutation);
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[cfg(unix)]
    #[test]
    fn coordinated_acquire_rejects_aliased_existing_metadata_without_replacing_it() {
        use std::os::unix::fs::PermissionsExt;

        let (root, mutation, barrier) = coordinated_fixture("aliased-metadata");
        let singleton_dir = root.join("runtime/collector");
        fs::create_dir(&singleton_dir).unwrap();
        fs::set_permissions(&singleton_dir, fs::Permissions::from_mode(0o700)).unwrap();
        let metadata_path = singleton_dir.join("runtime.meta");
        let original = b"runtime_metadata.v1\npid=7\nboot_nonce=0707070707070707070707070707070707070707070707070707070707070707\n";
        fs::write(&metadata_path, original).unwrap();
        fs::set_permissions(&metadata_path, fs::Permissions::from_mode(0o600)).unwrap();
        let alias = singleton_dir.join("runtime.meta.alias");
        fs::hard_link(&metadata_path, &alias).unwrap();

        assert!(matches!(
            Singleton::acquire_coordinated(&root, CoordinatedSingletonScope::Collector),
            Err(CoordinatedSingletonError::Singleton(
                SingletonError::WrongMutationRoot
            ))
        ));
        assert_eq!(fs::read(&metadata_path).unwrap(), original);
        assert_eq!(fs::read(&alias).unwrap(), original);

        drop(barrier);
        drop(mutation);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn coordinated_acquire_rejects_symlink_lock_without_replacing_metadata() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let (root, mutation, barrier) = coordinated_fixture("symlink-lock");
        let singleton_dir = root.join("runtime/collector");
        fs::create_dir(&singleton_dir).unwrap();
        fs::set_permissions(&singleton_dir, fs::Permissions::from_mode(0o700)).unwrap();
        let target = singleton_dir.join("target");
        fs::write(&target, b"").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
        symlink(&target, singleton_dir.join("runtime.lock")).unwrap();
        let metadata_path = singleton_dir.join("runtime.meta");
        fs::write(&metadata_path, b"preserve-me").unwrap();

        assert!(matches!(
            Singleton::acquire_coordinated(&root, CoordinatedSingletonScope::Collector),
            Err(CoordinatedSingletonError::Singleton(
                SingletonError::Symlink
            ))
        ));
        assert_eq!(fs::read(&metadata_path).unwrap(), b"preserve-me");

        drop(barrier);
        drop(mutation);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn coordinated_acquire_revalidates_named_lock_immediately_before_metadata_rename() {
        use std::os::unix::fs::OpenOptionsExt;

        fn replace_named_lock(directory: &Path) {
            use std::os::unix::fs::OpenOptionsExt;

            let lock_path = directory.join("runtime.lock");
            fs::rename(&lock_path, directory.join("original-runtime.lock")).unwrap();
            OpenOptions::new()
                .create_new(true)
                .read(true)
                .write(true)
                .mode(0o600)
                .open(lock_path)
                .unwrap();
        }

        let (root, mutation, barrier) = coordinated_fixture("named-lock-replacement");
        let singleton_dir = root.join("runtime/collector");
        let unrelated = root.join("runtime/unrelated");
        OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&unrelated)
            .unwrap()
            .write_all(b"preserve-me")
            .unwrap();
        let permit = barrier.try_begin_write().unwrap();

        assert!(matches!(
            Singleton::acquire_inner(
                &singleton_dir,
                SingletonAcquireMode::Coordinated {
                    permit: &permit,
                    before_metadata_rename: Some(replace_named_lock),
                },
            ),
            Err(CoordinatedSingletonError::Singleton(
                SingletonError::WrongMutationRoot
            ))
        ));
        assert!(!singleton_dir.join("runtime.meta").exists());
        assert!(
            !singleton_dir
                .join(format!(".runtime.meta.tmp.{}", std::process::id()))
                .exists()
        );
        assert!(singleton_dir.join("runtime.lock").exists());
        assert!(singleton_dir.join("original-runtime.lock").exists());
        assert_eq!(fs::read(&unrelated).unwrap(), b"preserve-me");

        drop(permit);
        drop(barrier);
        drop(mutation);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn mutation_guard_matches_only_the_exact_accounting_lock() {
        use std::os::unix::fs::OpenOptionsExt;

        let (root, mutation, barrier) = coordinated_fixture("mutation-accounting-match");
        let candidate = OpenOptions::new()
            .read(true)
            .write(true)
            .open(root.join("runtime/mutation.lock"))
            .unwrap();

        assert!(
            mutation
                .matches_accounting_lock(&root, Path::new("runtime/mutation.lock"), &candidate,)
                .unwrap()
        );
        assert!(
            !mutation
                .matches_accounting_lock(
                    &root.join("not-the-root"),
                    Path::new("runtime/runtime.lock"),
                    &candidate,
                )
                .unwrap()
        );
        let unrelated_path = root.join("runtime/unrelated.lock");
        let unrelated = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&unrelated_path)
            .unwrap();
        assert!(matches!(
            mutation
                .matches_accounting_lock(&root, Path::new("runtime/mutation.lock"), &unrelated,),
            Err(SingletonError::WrongMutationRoot)
        ));

        drop(unrelated);
        drop(candidate);
        drop(barrier);
        drop(mutation);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn mutation_guard_rejects_replaced_accounting_lock_identity() {
        use std::os::unix::fs::OpenOptionsExt;

        let (root, mutation, barrier) = coordinated_fixture("mutation-accounting-replaced");
        let path = root.join("runtime/mutation.lock");
        let candidate = OpenOptions::new().read(true).open(&path).unwrap();
        fs::rename(&path, root.join("runtime/original-mutation.lock")).unwrap();
        OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .unwrap();

        assert!(matches!(
            mutation
                .matches_accounting_lock(&root, Path::new("runtime/mutation.lock"), &candidate,),
            Err(SingletonError::WrongMutationRoot)
        ));

        drop(candidate);
        drop(barrier);
        drop(mutation);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn mutation_guard_rejects_nonempty_or_nonexact_private_accounting_lock() {
        use std::os::unix::fs::PermissionsExt;

        let (root, mutation, barrier) = coordinated_fixture("mutation-accounting-shape");
        let path = root.join("runtime/mutation.lock");
        let mut candidate = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        candidate.write_all(b"x").unwrap();
        assert!(matches!(
            mutation
                .matches_accounting_lock(&root, Path::new("runtime/mutation.lock"), &candidate,),
            Err(SingletonError::WrongMutationRoot)
        ));

        candidate.set_len(0).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();
        assert!(matches!(
            mutation
                .matches_accounting_lock(&root, Path::new("runtime/mutation.lock"), &candidate,),
            Err(SingletonError::InsecurePermissions)
        ));

        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        drop(candidate);
        drop(barrier);
        drop(mutation);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn validation_after_observed_directory_removal_does_not_recreate_it() {
        let root =
            std::env::temp_dir().join(format!("runtime-validation-race-{}", std::process::id()));
        private_dir(&root);
        assert!(fs::symlink_metadata(&root).unwrap().is_dir());
        fs::remove_dir(&root).unwrap();
        assert!(validate_private_runtime_dir(&root).is_err());
        assert!(!root.exists());
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
