//! One private report-build reservation per accounting root. Metadata survives
//! owner death; only an explicit mutation-guarded recovery may clear it.
#[cfg(unix)]
use crate::lock::{no_follow_flag, nonblocking_flag};
use crate::lock::{private_open_file, private_runtime_dir, reject_symlink, same_file};
use crate::{MutationGuard, SingletonError};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};

const FILE_NAME: &str = "report-reservation.lock";
const MAX_METADATA_BYTES: u64 = 512;
const MAX_BYTE_CEILING: u64 = 20 * 1024 * 1024 * 1024;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Metadata {
    version: u8,
    kind: Kind,
    nonce: [u8; 32],
    byte_ceiling: u64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
enum Kind {
    #[serde(rename = "report_build")]
    ReportBuild,
}

#[derive(Debug)]
pub enum ReservationError {
    Lock(SingletonError),
    Corrupt,
    Capacity,
    Busy,
    Stale,
}

impl From<SingletonError> for ReservationError {
    fn from(error: SingletonError) -> Self {
        Self::Lock(error)
    }
}
impl From<std::io::Error> for ReservationError {
    fn from(error: std::io::Error) -> Self {
        Self::Lock(SingletonError::Io(error))
    }
}
impl std::fmt::Display for ReservationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Lock(_) => "reservation lock or private-file validation failed",
            Self::Corrupt => "reservation metadata is corrupt",
            Self::Capacity => "reservation exceeds available capacity",
            Self::Busy => "report reservation is active",
            Self::Stale => "report reservation requires guarded recovery",
        })
    }
}
impl std::error::Error for ReservationError {}

/// Owns the advisory lock, not the global mutation guard. Dropping leaves a
/// conservatively accounted stale reservation, including after build failure.
#[derive(Debug)]
pub struct WriteReservation {
    file: File,
    metadata: Metadata,
}

fn open(root: &Path, create: bool) -> Result<Option<File>, ReservationError> {
    let runtime = root.join("runtime");
    match std::fs::symlink_metadata(&runtime) {
        Ok(_) => private_runtime_dir(&runtime)?,
        Err(error) if !create && error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    }
    let path = runtime.join(FILE_NAME);
    reject_symlink(&path)?;
    if let Ok(metadata) = std::fs::symlink_metadata(&path)
        && !metadata.is_file()
    {
        return Err(ReservationError::Corrupt);
    }
    let mut options = OpenOptions::new();
    options.read(true).write(create).create(create);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(no_follow_flag() | nonblocking_flag());
    }
    let file = match options.open(&path) {
        Ok(file) => file,
        Err(e) if !create && e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    private_open_file(&file)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if file.metadata()?.permissions().mode() & 0o7777 != 0o600 {
            return Err(SingletonError::InsecurePermissions.into());
        }
    }
    same_file(&file, &path)?;
    Ok(Some(file))
}

fn read(file: &mut File) -> Result<Option<Metadata>, ReservationError> {
    let length = file.metadata()?.len();
    if length == 0 {
        return Ok(None);
    }
    if length > MAX_METADATA_BYTES {
        return Err(ReservationError::Corrupt);
    }
    file.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    file.take(MAX_METADATA_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 != length {
        return Err(ReservationError::Corrupt);
    }
    let meta: Metadata = serde_json::from_slice(&bytes).map_err(|_| ReservationError::Corrupt)?;
    if meta.version != 1
        || meta.nonce == [0; 32]
        || !(1..=MAX_BYTE_CEILING).contains(&meta.byte_ceiling)
    {
        return Err(ReservationError::Corrupt);
    }
    Ok(Some(meta))
}

fn lock(file: &File) -> Result<(), ReservationError> {
    file.try_lock_exclusive().map_err(|error| {
        if error.kind() == std::io::ErrorKind::WouldBlock {
            ReservationError::Busy
        } else {
            error.into()
        }
    })
}

pub(crate) fn reserved_bytes(root: &Path) -> Result<u64, ReservationError> {
    match open(root, false)? {
        Some(mut file) => Ok(read(&mut file)?.map_or(0, |meta| meta.byte_ceiling)),
        None => Ok(0),
    }
}

impl WriteReservation {
    pub fn byte_ceiling(&self) -> u64 {
        self.metadata.byte_ceiling
    }

    pub(crate) fn acquire(
        root: &Path,
        guard: &MutationGuard,
        byte_ceiling: u64,
    ) -> Result<Self, ReservationError> {
        guard.require_root(root)?;
        if !(1..=MAX_BYTE_CEILING).contains(&byte_ceiling) {
            return Err(ReservationError::Capacity);
        }
        let mut file = open(root, true)?.ok_or(ReservationError::Corrupt)?;
        lock(&file)?;
        if read(&mut file)?.is_some() {
            return Err(ReservationError::Stale);
        }
        let mut nonce = [0; 32];
        getrandom::fill(&mut nonce).map_err(|_| ReservationError::Corrupt)?;
        if nonce == [0; 32] {
            return Err(ReservationError::Corrupt);
        }
        let metadata = Metadata {
            version: 1,
            kind: Kind::ReportBuild,
            nonce,
            byte_ceiling,
        };
        let body = serde_json::to_vec(&metadata).map_err(|_| ReservationError::Corrupt)?;
        if body.len() as u64 > MAX_METADATA_BYTES {
            return Err(ReservationError::Corrupt);
        }
        file.seek(SeekFrom::Start(0))?;
        file.write_all(&body)?;
        file.sync_all()?;
        File::open(root.join("runtime"))?.sync_all()?;
        Ok(Self { file, metadata })
    }

    pub(crate) fn validate_owner(
        &self,
        root: &Path,
        guard: &MutationGuard,
    ) -> Result<(), ReservationError> {
        guard.require_root(root)?;
        same_file(&self.file, &root.join("runtime").join(FILE_NAME))?;
        let mut file = open(root, false)?.ok_or(ReservationError::Corrupt)?;
        if read(&mut file)?.as_ref() != Some(&self.metadata) {
            return Err(ReservationError::Corrupt);
        }
        Ok(())
    }

    /// Call only after publication or guarded staging cleanup has finished.
    pub fn release(self, root: &Path, guard: &MutationGuard) -> Result<(), ReservationError> {
        self.validate_owner(root, guard)?;
        self.file.set_len(0)?;
        self.file.sync_all()?;
        Ok(())
    }
}

impl Drop for WriteReservation {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

/// Claim stale metadata without clearing it. The caller must clean interrupted
/// staging under the mutation guard before releasing the returned owner.
pub(crate) fn recover(
    root: &Path,
    guard: &MutationGuard,
) -> Result<Option<WriteReservation>, ReservationError> {
    guard.require_root(root)?;
    let Some(_) = open(root, false)? else {
        return Ok(None);
    };
    let mut file = open(root, true)?.ok_or(ReservationError::Corrupt)?;
    lock(&file)?;
    Ok(read(&mut file)?.map(|metadata| WriteReservation { file, metadata }))
}

#[cfg(test)]
mod tests {
    use crate::{Admission, LocalRuntimeConfigV3, MutationGuard, RuntimeControl};
    use std::{fs, path::PathBuf};

    fn setup(name: &str) -> (PathBuf, RuntimeControl) {
        let root = std::env::temp_dir().join(format!("reservation-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        (
            root,
            RuntimeControl::new(&LocalRuntimeConfigV3::default()).unwrap(),
        )
    }

    #[test]
    fn reservation_blocks_overcommit_but_not_unreserved_writes_and_owner_finalize() {
        let (root, control) = setup("admission");
        let guard = MutationGuard::try_acquire(&root.join("runtime")).unwrap();
        let before = control.writable_headroom(&root).unwrap();
        let held = control
            .reserve_report_build(&root, &guard, 16 * 1024 * 1024)
            .unwrap();
        drop(guard);
        let remaining = control.writable_headroom(&root).unwrap();
        assert!(remaining + held.byte_ceiling() <= before);
        assert_eq!(
            control.admit(&root, remaining + 1).unwrap(),
            Admission::Denied
        );
        assert!(matches!(
            control.admit(&root, 1).unwrap(),
            Admission::Allowed { .. }
        ));
        let guard = MutationGuard::try_acquire(&root.join("runtime")).unwrap();
        assert!(control.reserve_report_build(&root, &guard, 1).is_err());
        assert!(
            control
                .claim_stale_report_reservation(&root, &guard)
                .is_err()
        );
        fs::write(root.join("staging"), vec![0; 4096]).unwrap();
        assert!(
            control
                .reservation_finalization_headroom(&root, &guard, &held)
                .unwrap()
                > 0
        );
        held.release(&root, &guard).unwrap();
        assert!(control.writable_headroom(&root).unwrap() > remaining);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stale_is_counted_until_guarded_recovery_and_wrong_root_is_rejected() {
        let (root, control) = setup("stale");
        let (other, _) = setup("other");
        let guard = MutationGuard::try_acquire(&root.join("runtime")).unwrap();
        let wrong = MutationGuard::try_acquire(&other.join("runtime")).unwrap();
        assert!(control.reserve_report_build(&root, &wrong, 4096).is_err());
        let held = control
            .reserve_report_build(&root, &guard, 1024 * 1024)
            .unwrap();
        let remaining = control.writable_headroom(&root).unwrap();
        drop(held);
        assert_eq!(control.writable_headroom(&root).unwrap(), remaining);
        assert!(control.reserve_report_build(&root, &guard, 4096).is_err());
        assert!(
            control
                .claim_stale_report_reservation(&root, &wrong)
                .is_err()
        );
        let recovered = control
            .claim_stale_report_reservation(&root, &guard)
            .unwrap()
            .unwrap();
        assert_eq!(control.writable_headroom(&root).unwrap(), remaining);
        recovered.release(&root, &guard).unwrap();
        assert!(
            control
                .claim_stale_report_reservation(&root, &guard)
                .unwrap()
                .is_none()
        );
        assert!(control.writable_headroom(&root).unwrap() > remaining);
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(other).unwrap();
    }

    #[test]
    fn malformed_bounds_and_overcommit_fail_closed() {
        let (root, control) = setup("bounds");
        let guard = MutationGuard::try_acquire(&root.join("runtime")).unwrap();
        for bytes in [0, u64::MAX, control.storage_budget().total] {
            assert!(control.reserve_report_build(&root, &guard, bytes).is_err());
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn owner_drop_recovery_and_release_preserve_lock_inode() {
        use std::os::unix::fs::MetadataExt;
        let (root, control) = setup("inode");
        let guard = MutationGuard::try_acquire(&root.join("runtime")).unwrap();
        let held = control.reserve_report_build(&root, &guard, 8192).unwrap();
        let path = root.join("runtime").join(super::FILE_NAME);
        let inode = fs::metadata(&path).unwrap().ino();
        let wrong_root = setup("release-wrong").0;
        let wrong = MutationGuard::try_acquire(&wrong_root.join("runtime")).unwrap();
        assert!(
            control
                .reservation_finalization_headroom(&root, &wrong, &held)
                .is_err()
        );
        assert!(held.release(&root, &wrong).is_err());
        let stale = control
            .claim_stale_report_reservation(&root, &guard)
            .unwrap()
            .unwrap();
        drop(stale); // failed cleanup retains the stale promise
        assert_eq!(super::reserved_bytes(&root).unwrap(), 8192);
        let stale = control
            .claim_stale_report_reservation(&root, &guard)
            .unwrap()
            .unwrap();
        stale.release(&root, &guard).unwrap();
        let next = control.reserve_report_build(&root, &guard, 8192).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().ino(), inode);
        next.release(&root, &guard).unwrap();
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(wrong_root).unwrap();
    }

    #[test]
    fn corrupt_unknown_oversized_metadata_is_never_recovered_or_ignored() {
        let (root, control) = setup("corrupt");
        let guard = MutationGuard::try_acquire(&root.join("runtime")).unwrap();
        let held = control.reserve_report_build(&root, &guard, 8192).unwrap();
        let path = root.join("runtime").join(super::FILE_NAME);
        let valid: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        drop(held);
        let mut cases = vec![b"broken".to_vec(), vec![b'x'; 513]];
        for (key, value) in [
            ("version", serde_json::json!(2)),
            ("kind", serde_json::json!("unknown")),
            ("byte_ceiling", serde_json::json!(0)),
            ("byte_ceiling", serde_json::json!(u64::MAX)),
            ("nonce", serde_json::json!(vec![0; 32])),
            ("raw_content", serde_json::json!("sentinel")),
        ] {
            let mut bad = valid.clone();
            bad[key] = value;
            cases.push(serde_json::to_vec(&bad).unwrap());
        }
        for bad in cases {
            fs::write(&path, bad).unwrap();
            assert!(control.admit(&root, 1).is_err());
            assert!(control.writable_headroom(&root).is_err());
            assert!(control.migration_headroom(&root).is_err());
            assert!(
                control
                    .claim_stale_report_reservation(&root, &guard)
                    .is_err()
            );
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn private_files_reject_permissions_symlink_hardlink_and_replaced_locks() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let (root, control) = setup("private");
        let guard = MutationGuard::try_acquire(&root.join("runtime")).unwrap();
        let held = control.reserve_report_build(&root, &guard, 8192).unwrap();
        let path = root.join("runtime").join(super::FILE_NAME);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(control.admit(&root, 1).is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        fs::hard_link(&path, root.join("alias")).unwrap();
        assert!(control.writable_headroom(&root).is_err());
        fs::remove_file(root.join("alias")).unwrap();
        fs::rename(&path, root.join("old-reservation")).unwrap();
        symlink(root.join("old-reservation"), &path).unwrap();
        assert!(control.writable_headroom(&root).is_err());
        assert!(held.release(&root, &guard).is_err());
        fs::remove_file(&path).unwrap();
        fs::rename(root.join("old-reservation"), &path).unwrap();
        fs::rename(
            root.join("runtime/mutation.lock"),
            root.join("old-mutation"),
        )
        .unwrap();
        let replacement = MutationGuard::try_acquire(&root.join("runtime")).unwrap();
        assert!(
            control
                .claim_stale_report_reservation(&root, &guard)
                .is_err()
        );
        let stale = control
            .claim_stale_report_reservation(&root, &replacement)
            .unwrap()
            .unwrap();
        stale.release(&root, &replacement).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn concurrent_guard_contender_is_nonblocking_and_reservation_spans_guard_drop() {
        let (root, control) = setup("concurrent");
        let guard = MutationGuard::try_acquire(&root.join("runtime")).unwrap();
        let runtime = root.join("runtime");
        std::thread::spawn(move || assert!(MutationGuard::try_acquire(&runtime).is_err()))
            .join()
            .unwrap();
        let held = control.reserve_report_build(&root, &guard, 8192).unwrap();
        drop(guard);
        let other_root = root.clone();
        std::thread::spawn(move || {
            let control = RuntimeControl::new(&LocalRuntimeConfigV3::default()).unwrap();
            let guard = MutationGuard::try_acquire(&other_root.join("runtime")).unwrap();
            assert!(
                control
                    .claim_stale_report_reservation(&other_root, &guard)
                    .is_err()
            );
            assert!(
                control
                    .reserve_report_build(&other_root, &guard, 8192)
                    .is_err()
            );
        })
        .join()
        .unwrap();
        let guard = MutationGuard::try_acquire(&root.join("runtime")).unwrap();
        held.release(&root, &guard).unwrap();
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn process_exit_leaves_stale_promise_until_explicit_cleanup_release() {
        const CHILD_ROOT: &str = "RUNTIME_RESERVATION_CRASH_TEST_ROOT";
        if let Some(root) = std::env::var_os(CHILD_ROOT) {
            let root = PathBuf::from(root);
            let control = RuntimeControl::new(&LocalRuntimeConfigV3::default()).unwrap();
            let guard = MutationGuard::try_acquire(&root.join("runtime")).unwrap();
            let _held = control.reserve_report_build(&root, &guard, 8192).unwrap();
            std::process::exit(0); // intentionally no Rust destructors
        }
        let (root, control) = setup("process-exit");
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "reservation::tests::process_exit_leaves_stale_promise_until_explicit_cleanup_release"])
            .env(CHILD_ROOT, &root).status().unwrap();
        assert!(status.success());
        assert_eq!(super::reserved_bytes(&root).unwrap(), 8192);
        let guard = MutationGuard::try_acquire(&root.join("runtime")).unwrap();
        let recovered = control
            .claim_stale_report_reservation(&root, &guard)
            .unwrap()
            .unwrap();
        assert_eq!(super::reserved_bytes(&root).unwrap(), 8192);
        recovered.release(&root, &guard).unwrap();
        assert_eq!(super::reserved_bytes(&root).unwrap(), 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn replaced_runtime_directory_cannot_reuse_a_guard_even_with_same_lock_inode() {
        let (root, control) = setup("directory-replaced");
        let runtime = root.join("runtime");
        let guard = MutationGuard::try_acquire(&runtime).unwrap();
        fs::rename(&runtime, root.join("old-runtime")).unwrap();
        let replacement = MutationGuard::try_acquire(&runtime).unwrap();
        drop(replacement);
        fs::remove_file(runtime.join("mutation.lock")).unwrap();
        fs::rename(
            root.join("old-runtime/mutation.lock"),
            runtime.join("mutation.lock"),
        )
        .unwrap();
        assert!(control.reserve_report_build(&root, &guard, 8192).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn metadata_allowance_is_reserved_before_any_file_write() {
        let (root, control) = setup("metadata-allowance");
        let guard = MutationGuard::try_acquire(&root.join("runtime")).unwrap();
        let before = control.writable_headroom(&root).unwrap();
        assert!(
            control
                .reserve_report_build(&root, &guard, before - 4095)
                .is_err()
        );
        assert!(!root.join("runtime").join(super::FILE_NAME).exists());
        let held = control
            .reserve_report_build(&root, &guard, before - 8192)
            .unwrap();
        let allocated = crate::StorageBudget::allocated_tree_bytes(&root).unwrap();
        assert!(allocated + held.byte_ceiling() <= control.storage_budget().writable_limit());
        assert!(
            control.migration_headroom(&root).unwrap()
                <= control.storage_budget().total - allocated - held.byte_ceiling()
        );
        assert!(
            control.writable_headroom(&root).unwrap()
                <= fs2::available_space(&root)
                    .unwrap()
                    .saturating_sub(held.byte_ceiling())
        );
        assert_eq!(control.admit(&root, u64::MAX).unwrap(), Admission::Denied);
        held.release(&root, &guard).unwrap();
        fs::remove_dir_all(root).unwrap();
    }
}
