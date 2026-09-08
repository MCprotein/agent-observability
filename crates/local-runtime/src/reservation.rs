//! One private report-build reservation per accounting root. Metadata survives
//! owner death; only an explicit mutation-guarded recovery may clear it.
#[cfg(unix)]
use crate::lock::{no_follow_flag, nonblocking_flag};
use crate::lock::{private_open_file, reject_symlink, same_file, validate_private_runtime_dir};
use crate::{MutationGuard, SingletonError};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

/// Worst-case allocated metadata block, additional to the requested write ceiling.
pub const REPORT_RESERVATION_METADATA_ALLOWANCE: u64 = 4096;

const FILE_NAME: &str = "report-reservation.lock";
const METADATA_NAME: &str = "report-reservation.meta";
// One fixed slot bounds crash leftovers; cleanup requires both locks.
const TEMP_NAME: &str = ".report-reservation.meta.tmp";
const MAX_METADATA_BYTES: u64 = 512;
const MAX_BYTE_CEILING: u64 = 20 * 1024 * 1024 * 1024;
const STAGING_PARENT: &str = "state/store/report-views.v1";
const STAGING_PREFIX: &str = ".report-view.sqlite3.staging.";

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Metadata {
    version: u8,
    kind: Kind,
    nonce: [u8; 32],
    byte_ceiling: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    staging: Option<StagingBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct StagingBinding {
    root_dev: u64,
    root_ino: u64,
    parent_dev: u64,
    parent_ino: u64,
    file_dev: u64,
    file_ino: u64,
    name: String,
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
    lock: ReservationLock,
    metadata: Metadata,
}

/// Unlock every acquired lock, including validation failures before an owner
/// exists and empty recovery. Closing one descriptor is insufficient when a
/// concurrent fork or duplicate retains the same open-file description.
#[derive(Debug)]
struct ReservationLock {
    file: File,
}

impl Drop for ReservationLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

fn open(root: &Path, create: bool) -> Result<Option<File>, ReservationError> {
    open_named(root, FILE_NAME, create)
}

fn open_named(root: &Path, name: &str, create: bool) -> Result<Option<File>, ReservationError> {
    let runtime = root.join("runtime");
    match validate_private_runtime_dir(&runtime) {
        Ok(()) => (),
        Err(SingletonError::Io(error))
            if !create && error.kind() == std::io::ErrorKind::NotFound =>
        {
            return Ok(None);
        }
        Err(error) => return Err(error.into()),
    }
    let mut directory_options = OpenOptions::new();
    directory_options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        directory_options.custom_flags(no_follow_flag() | nonblocking_flag());
    }
    let directory = directory_options.open(&runtime)?;
    if !directory.metadata()?.is_dir() {
        return Err(ReservationError::Corrupt);
    }
    let path = runtime.join(name);
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
    validate_private_runtime_dir(&runtime)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let held = directory.metadata()?;
        let named = std::fs::symlink_metadata(&runtime)?;
        if held.dev() != named.dev() || held.ino() != named.ino() {
            return Err(SingletonError::WrongMutationRoot.into());
        }
    }
    Ok(Some(file))
}

fn read(file: &mut File) -> Result<Option<Metadata>, ReservationError> {
    let length = file.metadata()?.len();
    if length == 0 {
        return Err(ReservationError::Corrupt);
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
    if !matches!(
        (meta.version, meta.staging.is_some()),
        (1, false) | (2, true)
    ) || meta.nonce == [0; 32]
        || !(1..=MAX_BYTE_CEILING).contains(&meta.byte_ceiling)
        || meta
            .staging
            .as_ref()
            .is_some_and(|binding| !valid_staging_name(&binding.name))
    {
        return Err(ReservationError::Corrupt);
    }
    Ok(Some(meta))
}

fn valid_staging_name(name: &str) -> bool {
    let Some(suffix) = name.strip_prefix(STAGING_PREFIX) else {
        return false;
    };
    let Some((pid, sequence)) = suffix.split_once('.') else {
        return false;
    };
    !pid.is_empty()
        && !sequence.is_empty()
        && !sequence.contains('.')
        && pid
            .parse::<u32>()
            .is_ok_and(|value| value.to_string() == pid)
        && sequence
            .parse::<u64>()
            .is_ok_and(|value| value.to_string() == sequence)
}

#[cfg(unix)]
fn open_private_directory(path: &Path) -> Result<File, ReservationError> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

    reject_symlink(path)?;
    let mut options = OpenOptions::new();
    options
        .read(true)
        .custom_flags(no_follow_flag() | nonblocking_flag());
    let directory = options.open(path)?;
    let held = directory.metadata()?;
    let named = std::fs::symlink_metadata(path)?;
    if !held.is_dir()
        || held.mode() & 0o7777 != 0o700
        || !named.is_dir()
        || named.file_type().is_symlink()
        || (held.dev(), held.ino()) != (named.dev(), named.ino())
    {
        return Err(ReservationError::Corrupt);
    }
    Ok(directory)
}

#[cfg(not(unix))]
fn open_private_directory(_path: &Path) -> Result<File, ReservationError> {
    Err(SingletonError::UnsupportedPlatform.into())
}

#[cfg(unix)]
fn directory_identity(directory: &File) -> Result<(u64, u64), ReservationError> {
    use std::os::unix::fs::MetadataExt;

    let metadata = directory.metadata()?;
    Ok((metadata.dev(), metadata.ino()))
}

#[cfg(not(unix))]
fn directory_identity(_directory: &File) -> Result<(u64, u64), ReservationError> {
    Err(SingletonError::UnsupportedPlatform.into())
}

struct StagingDirectories {
    entries: Vec<(PathBuf, File)>,
}

impl StagingDirectories {
    fn open(root: &Path) -> Result<Self, ReservationError> {
        let entries = [
            root.to_path_buf(),
            root.join("state"),
            root.join("state/store"),
            root.join(STAGING_PARENT),
        ]
        .into_iter()
        .map(|path| open_private_directory(&path).map(|file| (path, file)))
        .collect::<Result<Vec<_>, _>>()?;
        let directories = Self { entries };
        directories.revalidate()?;
        Ok(directories)
    }

    fn revalidate(&self) -> Result<(), ReservationError> {
        for (path, directory) in &self.entries {
            let replacement_check = open_private_directory(path)?;
            if directory_identity(directory)? != directory_identity(&replacement_check)? {
                return Err(ReservationError::Corrupt);
            }
        }
        Ok(())
    }

    fn root_identity(&self) -> Result<(u64, u64), ReservationError> {
        directory_identity(&self.entries[0].1)
    }

    fn parent_identity(&self) -> Result<(u64, u64), ReservationError> {
        directory_identity(&self.entries[3].1)
    }

    fn sync_parent(&self) -> Result<(), ReservationError> {
        self.entries[3].1.sync_all()?;
        self.revalidate()
    }
}

fn staging_path(root: &Path, name: &str) -> PathBuf {
    root.join(STAGING_PARENT).join(name)
}

fn inspect_staging(
    root: &Path,
    path: &Path,
    file: &File,
    require_empty: bool,
) -> Result<StagingBinding, ReservationError> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| valid_staging_name(name))
        .ok_or(ReservationError::Corrupt)?;
    if path != staging_path(root, name) {
        return Err(ReservationError::Corrupt);
    }
    let directories = StagingDirectories::open(root)?;
    same_file(file, path)?;
    #[cfg(unix)]
    let (file_dev, file_ino) = {
        use std::os::unix::fs::MetadataExt;

        let metadata = file.metadata()?;
        if metadata.mode() & 0o7777 != 0o600
            || metadata.nlink() != 1
            || (require_empty && metadata.len() != 0)
        {
            return Err(ReservationError::Corrupt);
        }
        (metadata.dev(), metadata.ino())
    };
    #[cfg(not(unix))]
    let (file_dev, file_ino) = return Err(SingletonError::UnsupportedPlatform.into());
    directories.revalidate()?;
    same_file(file, path)?;
    let (root_dev, root_ino) = directories.root_identity()?;
    let (parent_dev, parent_ino) = directories.parent_identity()?;
    Ok(StagingBinding {
        root_dev,
        root_ino,
        parent_dev,
        parent_ino,
        file_dev,
        file_ino,
        name: name.to_owned(),
    })
}

fn validate_recovery_staging(
    root: &Path,
    binding: &StagingBinding,
) -> Result<(), ReservationError> {
    if !valid_staging_name(&binding.name) {
        return Err(ReservationError::Corrupt);
    }
    let directories = StagingDirectories::open(root)?;
    if directories.root_identity()? != (binding.root_dev, binding.root_ino)
        || directories.parent_identity()? != (binding.parent_dev, binding.parent_ino)
    {
        return Err(ReservationError::Corrupt);
    }
    let path = staging_path(root, &binding.name);
    reject_symlink(&path)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(no_follow_flag() | nonblocking_flag());
    }
    let file = match options.open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            directories.revalidate()?;
            return Ok(());
        }
        Err(error) => return Err(error.into()),
    };
    let observed = inspect_staging(root, &path, &file, false)?;
    if &observed != binding {
        return Err(ReservationError::Corrupt);
    }
    Ok(())
}

fn lock(file: File) -> Result<ReservationLock, ReservationError> {
    file.try_lock_exclusive().map_err(|error| {
        if error.kind() == std::io::ErrorKind::WouldBlock {
            ReservationError::Busy
        } else {
            error.into()
        }
    })?;
    Ok(ReservationLock { file })
}

fn read_metadata(root: &Path) -> Result<Option<Metadata>, ReservationError> {
    match open_named(root, METADATA_NAME, false)? {
        Some(mut file) => read(&mut file),
        None => Ok(None),
    }
}

fn validate_lock(file: &File, root: &Path) -> Result<(), ReservationError> {
    same_file(file, &root.join("runtime").join(FILE_NAME))?;
    if file.metadata()?.len() != 0 {
        return Err(ReservationError::Corrupt);
    }
    Ok(())
}

fn publish_metadata(
    root: &Path,
    guard: &MutationGuard,
    lock: &File,
    metadata: &Metadata,
    phases: [&str; 5],
) -> Result<(), ReservationError> {
    #[cfg(not(test))]
    let _ = phases;
    let body = serde_json::to_vec(metadata).map_err(|_| ReservationError::Corrupt)?;
    if body.len() as u64 > MAX_METADATA_BYTES {
        return Err(ReservationError::Corrupt);
    }
    let temporary = root.join("runtime").join(TEMP_NAME);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(no_follow_flag() | nonblocking_flag());
    }
    let mut temp = options.open(&temporary)?;
    private_open_file(&temp)?;
    #[cfg(test)]
    crash_phase(phases[0]);
    temp.write_all(&body)?;
    #[cfg(test)]
    crash_phase(phases[1]);
    temp.sync_all()?;
    #[cfg(test)]
    crash_phase(phases[2]);
    guard.require_root(root)?;
    validate_lock(lock, root)?;
    same_file(&temp, &temporary)?;
    std::fs::rename(&temporary, root.join("runtime").join(METADATA_NAME))?;
    #[cfg(test)]
    crash_phase(phases[3]);
    File::open(root.join("runtime"))?.sync_all()?;
    #[cfg(test)]
    crash_phase(phases[4]);
    Ok(())
}

// Called only after MutationGuard validation and exclusive lifetime-lock acquisition.
fn cleanup_temp(root: &Path, guard: &MutationGuard, file: &File) -> Result<(), ReservationError> {
    guard.require_root(root)?;
    validate_lock(file, root)?;
    if open_named(root, TEMP_NAME, false)?.is_some() {
        std::fs::remove_file(root.join("runtime").join(TEMP_NAME))?;
        File::open(root.join("runtime"))?.sync_all()?;
    }
    Ok(())
}

pub(crate) fn reserved_bytes(root: &Path) -> Result<u64, ReservationError> {
    let file = open(root, false)?;
    let metadata = read_metadata(root)?;
    match file {
        Some(file) => validate_lock(&file, root)?,
        None if metadata.is_some() => return Err(ReservationError::Corrupt),
        None => (),
    }
    Ok(metadata.map_or(0, |meta| meta.byte_ceiling))
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
        Self::acquire_observing(root, guard, byte_ceiling, |_| {})
    }

    fn acquire_observing(
        root: &Path,
        guard: &MutationGuard,
        byte_ceiling: u64,
        after_lock: impl FnOnce(&File),
    ) -> Result<Self, ReservationError> {
        guard.require_root(root)?;
        if !(1..=MAX_BYTE_CEILING).contains(&byte_ceiling) {
            return Err(ReservationError::Capacity);
        }
        let lock = lock(open(root, true)?.ok_or(ReservationError::Corrupt)?)?;
        after_lock(&lock.file);
        cleanup_temp(root, guard, &lock.file)?;
        if read_metadata(root)?.is_some() {
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
            staging: None,
        };
        publish_metadata(
            root,
            guard,
            &lock.file,
            &metadata,
            ["created", "written", "synced", "renamed", "published"],
        )?;
        Ok(Self { lock, metadata })
    }

    /// Durably binds this exact reservation owner to a newly created report-view staging file.
    pub fn bind_staging(
        &mut self,
        root: &Path,
        guard: &MutationGuard,
        path: &Path,
        file: &File,
    ) -> Result<(), ReservationError> {
        self.validate_owner(root, guard)?;
        if self.metadata.staging.is_some()
            || self.metadata.version != 1
            || self.metadata.byte_ceiling < REPORT_RESERVATION_METADATA_ALLOWANCE
        {
            return Err(ReservationError::Corrupt);
        }
        let binding = inspect_staging(root, path, file, true)?;
        let directories = StagingDirectories::open(root)?;
        if directories.root_identity()? != (binding.root_dev, binding.root_ino)
            || directories.parent_identity()? != (binding.parent_dev, binding.parent_ino)
        {
            return Err(ReservationError::Corrupt);
        }
        same_file(file, path)?;
        directories.sync_parent()?;
        same_file(file, path)?;
        let metadata = Metadata {
            version: 2,
            kind: Kind::ReportBuild,
            nonce: self.metadata.nonce,
            byte_ceiling: self.metadata.byte_ceiling,
            staging: Some(binding),
        };
        publish_metadata(
            root,
            guard,
            &self.lock.file,
            &metadata,
            [
                "binding-created",
                "binding-written",
                "binding-synced",
                "binding-renamed",
                "binding-published",
            ],
        )?;
        self.metadata = metadata;
        Ok(())
    }

    /// Revalidates the bound staging path against the retained create-new descriptor.
    pub fn validate_staging(
        &self,
        root: &Path,
        guard: &MutationGuard,
        path: &Path,
        file: &File,
    ) -> Result<(), ReservationError> {
        self.validate_owner(root, guard)?;
        let expected = self
            .metadata
            .staging
            .as_ref()
            .ok_or(ReservationError::Corrupt)?;
        let observed = inspect_staging(root, path, file, false)?;
        if &observed != expected {
            return Err(ReservationError::Corrupt);
        }
        Ok(())
    }

    pub(crate) fn validate_owner(
        &self,
        root: &Path,
        guard: &MutationGuard,
    ) -> Result<(), ReservationError> {
        guard.require_root(root)?;
        validate_lock(&self.lock.file, root)?;
        if read_metadata(root)?.as_ref() != Some(&self.metadata) {
            return Err(ReservationError::Corrupt);
        }
        Ok(())
    }

    /// Call only after publication or guarded staging cleanup has finished.
    pub fn release(self, root: &Path, guard: &MutationGuard) -> Result<(), ReservationError> {
        self.validate_owner(root, guard)?;
        cleanup_temp(root, guard, &self.lock.file)?;
        std::fs::remove_file(root.join("runtime").join(METADATA_NAME))?;
        #[cfg(test)]
        crash_phase("removed");
        File::open(root.join("runtime"))?.sync_all()?;
        #[cfg(test)]
        crash_phase("cleared");
        Ok(())
    }
}

/// Claim stale metadata without clearing it. The caller must clean interrupted
/// staging under the mutation guard before releasing the returned owner.
pub(crate) fn recover(
    root: &Path,
    guard: &MutationGuard,
) -> Result<Option<WriteReservation>, ReservationError> {
    recover_observing(root, guard, |_| {})
}

fn recover_observing(
    root: &Path,
    guard: &MutationGuard,
    after_lock: impl FnOnce(&File),
) -> Result<Option<WriteReservation>, ReservationError> {
    guard.require_root(root)?;
    let Some(file) = open(root, false)? else {
        if read_metadata(root)?.is_some() {
            return Err(ReservationError::Corrupt);
        }
        return Ok(None);
    };
    let lock = lock(file)?;
    after_lock(&lock.file);
    let metadata = read_metadata(root)?;
    if let Some(binding) = metadata
        .as_ref()
        .and_then(|metadata| metadata.staging.as_ref())
    {
        validate_recovery_staging(root, binding)?;
    }
    cleanup_temp(root, guard, &lock.file)?;
    Ok(metadata.map(|metadata| WriteReservation { lock, metadata }))
}

#[cfg(test)]
fn crash_phase(phase: &str) {
    if std::env::var("RESERVATION_ATOMIC_CRASH_PHASE").as_deref() == Ok(phase) {
        std::process::exit(73);
    }
}

#[cfg(test)]
mod tests {
    use crate::{Admission, LocalRuntimeConfigV3, MutationGuard, RuntimeControl};
    use std::{
        fs,
        path::{Path, PathBuf},
    };

    fn setup(name: &str) -> (PathBuf, RuntimeControl) {
        let root = std::env::temp_dir().join(format!("reservation-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        (
            root,
            RuntimeControl::new(&LocalRuntimeConfigV3::default()).unwrap(),
        )
    }

    #[cfg(unix)]
    fn staging_file(root: &Path, sequence: u64) -> (PathBuf, fs::File) {
        use std::os::unix::fs::OpenOptionsExt;

        let directory = root.join("state/store/report-views.v1");
        fs::create_dir_all(&directory).unwrap();
        fs::set_permissions(root.join("state"), fs::Permissions::from_mode(0o700)).unwrap();
        fs::set_permissions(root.join("state/store"), fs::Permissions::from_mode(0o700)).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        let path = directory.join(format!(
            ".report-view.sqlite3.staging.{}.{sequence}",
            std::process::id()
        ));
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .unwrap();
        (path, file)
    }

    #[cfg(unix)]
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    #[cfg(unix)]
    #[test]
    fn binding_accepts_only_the_exact_fresh_private_staging_identity() {
        use std::io::Write;
        use std::os::unix::fs::symlink;

        let (root, control) = setup("bind-identity");
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let guard = MutationGuard::try_acquire(&root.join("runtime")).unwrap();
        let mut reservation = control.reserve_report_build(&root, &guard, 8192).unwrap();
        let (path, file) = staging_file(&root, 1);
        reservation
            .bind_staging(&root, &guard, &path, &file)
            .unwrap();
        reservation
            .validate_staging(&root, &guard, &path, &file)
            .unwrap();
        assert!(
            reservation
                .bind_staging(&root, &guard, &path, &file)
                .is_err()
        );

        let metadata = fs::read(root.join("runtime").join(super::METADATA_NAME)).unwrap();
        assert!(u64::try_from(metadata.len()).unwrap() <= super::MAX_METADATA_BYTES);
        assert!(!String::from_utf8_lossy(&metadata).contains(root.to_str().unwrap()));

        let (other_root, _) = setup("bind-other-root");
        fs::set_permissions(&other_root, fs::Permissions::from_mode(0o700)).unwrap();
        let other_guard = MutationGuard::try_acquire(&other_root.join("runtime")).unwrap();
        assert!(
            reservation
                .validate_staging(&root, &other_guard, &path, &file)
                .is_err()
        );

        fs::hard_link(&path, root.join("staging-alias")).unwrap();
        assert!(
            reservation
                .validate_staging(&root, &guard, &path, &file)
                .is_err()
        );
        fs::remove_file(root.join("staging-alias")).unwrap();

        let saved = root.join("saved-staging");
        fs::rename(&path, &saved).unwrap();
        let replacement = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .unwrap();
        assert!(
            reservation
                .validate_staging(&root, &guard, &path, &file)
                .is_err()
        );
        drop(replacement);
        fs::remove_file(&path).unwrap();
        symlink(&saved, &path).unwrap();
        assert!(
            reservation
                .validate_staging(&root, &guard, &path, &file)
                .is_err()
        );
        fs::remove_file(&path).unwrap();
        fs::rename(&saved, &path).unwrap();

        file.try_clone().unwrap().write_all(b"x").unwrap();
        assert!(
            reservation
                .validate_staging(&root, &guard, &path, &file)
                .is_ok()
        );
        reservation.release(&root, &guard).unwrap();
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(other_root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn binding_rejects_nonzero_wrong_shape_and_nonce_without_releasing_full_promise() {
        use std::io::Write;

        let (root, control) = setup("bind-rejections");
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let guard = MutationGuard::try_acquire(&root.join("runtime")).unwrap();
        let ceiling = 8192;
        let mut reservation = control
            .reserve_report_build(&root, &guard, ceiling)
            .unwrap();
        let before = control.writable_headroom(&root).unwrap();
        let allocated_before = crate::StorageBudget::allocated_tree_bytes(&root).unwrap();
        let (path, mut file) = staging_file(&root, 2);
        file.write_all(b"not-empty").unwrap();
        assert!(
            reservation
                .bind_staging(&root, &guard, &path, &file)
                .is_err()
        );
        assert_eq!(super::reserved_bytes(&root).unwrap(), ceiling);
        let allocated_after = crate::StorageBudget::allocated_tree_bytes(&root).unwrap();
        assert!(allocated_after >= allocated_before);
        assert!(control.writable_headroom(&root).unwrap() <= before);
        drop(file);
        fs::remove_file(&path).unwrap();

        let (bad_path, bad_file) = staging_file(&root, 3);
        let wrong_name = bad_path.with_file_name(".report-view.sqlite3.staging.bad.3");
        fs::rename(&bad_path, &wrong_name).unwrap();
        assert!(
            reservation
                .bind_staging(&root, &guard, &wrong_name, &bad_file)
                .is_err()
        );
        fs::rename(&wrong_name, &bad_path).unwrap();

        let metadata_path = root.join("runtime").join(super::METADATA_NAME);
        let original = fs::read(&metadata_path).unwrap();
        let mut changed: serde_json::Value = serde_json::from_slice(&original).unwrap();
        changed["nonce"] = serde_json::json!(vec![7; 32]);
        fs::write(&metadata_path, serde_json::to_vec(&changed).unwrap()).unwrap();
        assert!(
            reservation
                .bind_staging(&root, &guard, &bad_path, &bad_file)
                .is_err()
        );
        assert_eq!(super::reserved_bytes(&root).unwrap(), ceiling);
        fs::write(&metadata_path, original).unwrap();
        drop(reservation);
        assert_eq!(super::reserved_bytes(&root).unwrap(), ceiling);
        let stale = control
            .claim_stale_report_reservation(&root, &guard)
            .unwrap()
            .unwrap();
        stale.release(&root, &guard).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn bound_stale_recovery_rejects_present_mismatch_but_allows_missing_after_parent_validation() {
        let (root, control) = setup("bound-stale-recovery");
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let guard = MutationGuard::try_acquire(&root.join("runtime")).unwrap();
        let mut reservation = control.reserve_report_build(&root, &guard, 8192).unwrap();
        let (path, file) = staging_file(&root, 4);
        reservation
            .bind_staging(&root, &guard, &path, &file)
            .unwrap();
        drop(reservation);

        fs::remove_file(&path).unwrap();
        let replacement = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .unwrap();
        assert!(
            control
                .claim_stale_report_reservation(&root, &guard)
                .is_err()
        );
        assert_eq!(super::reserved_bytes(&root).unwrap(), 8192);
        drop(replacement);
        fs::remove_file(&path).unwrap();

        let stale = control
            .claim_stale_report_reservation(&root, &guard)
            .unwrap()
            .unwrap();
        drop(stale);
        let parent = path.parent().unwrap();
        let saved_parent = root.join("saved-report-views");
        fs::rename(parent, &saved_parent).unwrap();
        fs::create_dir(parent).unwrap();
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(
            control
                .claim_stale_report_reservation(&root, &guard)
                .is_err()
        );
        fs::remove_dir(parent).unwrap();
        fs::rename(saved_parent, parent).unwrap();
        control
            .claim_stale_report_reservation(&root, &guard)
            .unwrap()
            .unwrap()
            .release(&root, &guard)
            .unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn release_after_staging_rename_does_not_require_the_original_path() {
        let (root, control) = setup("release-after-publish");
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let guard = MutationGuard::try_acquire(&root.join("runtime")).unwrap();
        let mut reservation = control.reserve_report_build(&root, &guard, 8192).unwrap();
        let (path, file) = staging_file(&root, 5);
        reservation
            .bind_staging(&root, &guard, &path, &file)
            .unwrap();
        reservation
            .validate_staging(&root, &guard, &path, &file)
            .unwrap();
        fs::rename(&path, path.with_file_name("report-view-published.sqlite3")).unwrap();
        reservation.release(&root, &guard).unwrap();
        assert_eq!(super::reserved_bytes(&root).unwrap(), 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn v1_unbound_metadata_remains_recoverable_and_can_be_durably_bound() {
        let (root, control) = setup("v1-compatibility");
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let guard = MutationGuard::try_acquire(&root.join("runtime")).unwrap();
        let reservation = control.reserve_report_build(&root, &guard, 8192).unwrap();
        let metadata_path = root.join("runtime").join(super::METADATA_NAME);
        let v1: serde_json::Value =
            serde_json::from_slice(&fs::read(&metadata_path).unwrap()).unwrap();
        assert_eq!(v1["version"], 1);
        assert!(v1.get("staging").is_none());
        drop(reservation);

        let mut recovered = control
            .claim_stale_report_reservation(&root, &guard)
            .unwrap()
            .unwrap();
        let (path, file) = staging_file(&root, 6);
        recovered.bind_staging(&root, &guard, &path, &file).unwrap();
        let v2: serde_json::Value =
            serde_json::from_slice(&fs::read(&metadata_path).unwrap()).unwrap();
        assert_eq!(v2["version"], 2);
        recovered
            .validate_staging(&root, &guard, &path, &file)
            .unwrap();
        recovered.release(&root, &guard).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn worst_case_bound_metadata_stays_within_the_v1_limit() {
        let metadata = super::Metadata {
            version: 2,
            kind: super::Kind::ReportBuild,
            nonce: [u8::MAX; 32],
            byte_ceiling: super::MAX_BYTE_CEILING,
            staging: Some(super::StagingBinding {
                root_dev: u64::MAX,
                root_ino: u64::MAX,
                parent_dev: u64::MAX,
                parent_ino: u64::MAX,
                file_dev: u64::MAX,
                file_ino: u64::MAX,
                name: format!("{}{}.{}", super::STAGING_PREFIX, u32::MAX, u64::MAX),
            }),
        };
        let body = serde_json::to_vec(&metadata).unwrap();
        assert!(body.len() as u64 <= super::MAX_METADATA_BYTES);
        let decoded: super::Metadata = serde_json::from_slice(&body).unwrap();
        assert_eq!(decoded, metadata);
    }

    #[cfg(unix)]
    #[test]
    fn failed_binding_metadata_update_keeps_the_full_promise_for_stale_recovery() {
        let (root, control) = setup("binding-update-failure");
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let guard = MutationGuard::try_acquire(&root.join("runtime")).unwrap();
        let mut reservation = control.reserve_report_build(&root, &guard, 8192).unwrap();
        let (path, file) = staging_file(&root, 7);
        let temporary = root.join("runtime").join(super::TEMP_NAME);
        let blocker = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .unwrap();
        assert!(
            reservation
                .bind_staging(&root, &guard, &path, &file)
                .is_err()
        );
        assert_eq!(super::reserved_bytes(&root).unwrap(), 8192);
        drop(blocker);
        drop(reservation);
        let stale = control
            .claim_stale_report_reservation(&root, &guard)
            .unwrap()
            .unwrap();
        assert!(!temporary.exists());
        assert_eq!(super::reserved_bytes(&root).unwrap(), 8192);
        stale.release(&root, &guard).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn atomic_binding_process_death_preserves_v1_or_v2_full_promise() {
        const ROOT: &str = "RESERVATION_BINDING_CRASH_ROOT";
        if let Some(root) = std::env::var_os(ROOT) {
            let root = PathBuf::from(root);
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).unwrap();
            let control = RuntimeControl::new(&LocalRuntimeConfigV3::default()).unwrap();
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
            let guard = MutationGuard::try_acquire(&root.join("runtime")).unwrap();
            let mut reservation = control.reserve_report_build(&root, &guard, 8192).unwrap();
            let (path, file) = staging_file(&root, 8);
            reservation
                .bind_staging(&root, &guard, &path, &file)
                .unwrap();
            panic!("binding crash phase was not reached");
        }

        for phase in [
            "binding-created",
            "binding-written",
            "binding-synced",
            "binding-renamed",
            "binding-published",
        ] {
            let root = std::env::temp_dir().join(format!(
                "reservation-binding-phase-{phase}-{}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&root);
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "reservation::tests::atomic_binding_process_death_preserves_v1_or_v2_full_promise",
                ])
                .env(ROOT, &root)
                .env("RESERVATION_ATOMIC_CRASH_PHASE", phase)
                .status()
                .unwrap();
            assert_eq!(status.code(), Some(73));
            assert_eq!(super::reserved_bytes(&root).unwrap(), 8192);
            let metadata: serde_json::Value = serde_json::from_slice(
                &fs::read(root.join("runtime").join(super::METADATA_NAME)).unwrap(),
            )
            .unwrap();
            let expected_version = if matches!(phase, "binding-renamed" | "binding-published") {
                2
            } else {
                1
            };
            assert_eq!(metadata["version"], expected_version);
            let guard = MutationGuard::try_acquire(&root.join("runtime")).unwrap();
            let control = RuntimeControl::new(&LocalRuntimeConfigV3::default()).unwrap();
            let stale = control
                .claim_stale_report_reservation(&root, &guard)
                .unwrap()
                .unwrap();
            assert!(!root.join("runtime").join(super::TEMP_NAME).exists());
            assert_eq!(super::reserved_bytes(&root).unwrap(), 8192);
            stale.release(&root, &guard).unwrap();
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn atomic_metadata_process_death_at_each_phase_is_recoverable() {
        const ROOT: &str = "RESERVATION_ATOMIC_CRASH_ROOT";
        if let Some(root) = std::env::var_os(ROOT) {
            let root = PathBuf::from(root);
            let control = RuntimeControl::new(&LocalRuntimeConfigV3::default()).unwrap();
            let guard = MutationGuard::try_acquire(&root.join("runtime")).unwrap();
            let held = control.reserve_report_build(&root, &guard, 8192).unwrap();
            held.release(&root, &guard).unwrap();
            panic!("crash phase was not reached");
        }
        for phase in [
            "created",
            "written",
            "synced",
            "renamed",
            "published",
            "removed",
            "cleared",
        ] {
            let (root, control) = setup(&format!("phase-{phase}"));
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "reservation::tests::atomic_metadata_process_death_at_each_phase_is_recoverable"])
                .env(ROOT, &root)
                .env("RESERVATION_ATOMIC_CRASH_PHASE", phase)
                .status().unwrap();
            assert_eq!(status.code(), Some(73));
            let expected = if matches!(phase, "renamed" | "published") {
                8192
            } else {
                0
            };
            assert_eq!(super::reserved_bytes(&root).unwrap(), expected);
            assert_eq!(
                fs::metadata(root.join("runtime").join(super::FILE_NAME))
                    .unwrap()
                    .len(),
                0
            );
            let guard = MutationGuard::try_acquire(&root.join("runtime")).unwrap();
            if let Some(stale) = control
                .claim_stale_report_reservation(&root, &guard)
                .unwrap()
            {
                stale.release(&root, &guard).unwrap();
            }
            assert!(!root.join("runtime").join(super::TEMP_NAME).exists());
            let held = control.reserve_report_build(&root, &guard, 8192).unwrap();
            held.release(&root, &guard).unwrap();
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[cfg(unix)]
    #[test]
    fn torn_private_temp_is_only_cleaned_with_both_locks_and_cleanup_failure_is_closed() {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, symlink};
        let (root, control) = setup("torn-temp");
        let guard = MutationGuard::try_acquire(&root.join("runtime")).unwrap();
        let held = control.reserve_report_build(&root, &guard, 8192).unwrap();
        let temporary = root.join("runtime").join(super::TEMP_NAME);
        let mut temp = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .unwrap();
        temp.write_all(b"{").unwrap();
        temp.sync_all().unwrap();
        assert!(
            control
                .claim_stale_report_reservation(&root, &guard)
                .is_err()
        );
        assert!(temporary.exists());
        drop(held);
        let stale = control
            .claim_stale_report_reservation(&root, &guard)
            .unwrap()
            .unwrap();
        assert!(!temporary.exists());
        symlink(root.join("unowned"), &temporary).unwrap();
        assert!(stale.release(&root, &guard).is_err());
        assert_eq!(super::reserved_bytes(&root).unwrap(), 8192);
        assert!(
            control
                .claim_stale_report_reservation(&root, &guard)
                .is_err()
        );
        fs::remove_file(&temporary).unwrap();
        control
            .claim_stale_report_reservation(&root, &guard)
            .unwrap()
            .unwrap()
            .release(&root, &guard)
            .unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn metadata_and_runtime_paths_reject_aliases_and_insecure_permissions() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let (root, control) = setup("metadata-private");
        let guard = MutationGuard::try_acquire(&root.join("runtime")).unwrap();
        let held = control.reserve_report_build(&root, &guard, 8192).unwrap();
        let path = root.join("runtime").join(super::METADATA_NAME);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o7777,
            0o600
        );
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(super::reserved_bytes(&root).is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        fs::hard_link(&path, root.join("alias")).unwrap();
        assert!(super::reserved_bytes(&root).is_err());
        fs::remove_file(root.join("alias")).unwrap();
        fs::rename(&path, root.join("saved")).unwrap();
        symlink(root.join("saved"), &path).unwrap();
        assert!(super::reserved_bytes(&root).is_err());
        fs::remove_file(&path).unwrap();
        fs::rename(root.join("saved"), &path).unwrap();
        held.release(&root, &guard).unwrap();
        drop(guard);
        fs::rename(root.join("runtime"), root.join("saved-runtime")).unwrap();
        symlink(root.join("saved-runtime"), root.join("runtime")).unwrap();
        assert!(super::reserved_bytes(&root).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn publication_keeps_lifetime_lock_empty() {
        let (root, control) = setup("atomic-lock");
        let guard = MutationGuard::try_acquire(&root.join("runtime")).unwrap();
        let held = control.reserve_report_build(&root, &guard, 8192).unwrap();
        assert_eq!(
            fs::metadata(root.join("runtime").join(super::FILE_NAME))
                .unwrap()
                .len(),
            0
        );
        held.release(&root, &guard).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn read_only_missing_and_removed_runtime_never_creates_paths() {
        let (root, _) = setup("read-only");
        assert_eq!(super::reserved_bytes(&root).unwrap(), 0);
        assert!(!root.join("runtime").exists());
        let guard = MutationGuard::try_acquire(&root.join("runtime")).unwrap();
        drop(guard);
        fs::remove_dir_all(root.join("runtime")).unwrap();
        assert_eq!(super::reserved_bytes(&root).unwrap(), 0);
        assert!(!root.join("runtime").exists());
        fs::remove_dir_all(&root).unwrap();
        assert_eq!(super::reserved_bytes(&root).unwrap(), 0);
        assert!(!root.exists());
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
    fn config_shrink_preserves_full_active_and_stale_promise_until_release() {
        let (root, control) = setup("config-shrink");
        let guard = MutationGuard::try_acquire(&root.join("runtime")).unwrap();
        let ceiling = 300 * crate::storage::MIB;
        let held = control
            .reserve_report_build(&root, &guard, ceiling)
            .unwrap();
        let mut config = LocalRuntimeConfigV3::default();
        config.collection.local_storage_budget_bytes = 256 * crate::storage::MIB;
        let shrunk = RuntimeControl::new(&config).unwrap();

        assert_eq!(super::reserved_bytes(&root).unwrap(), ceiling);
        assert_eq!(shrunk.writable_headroom(&root).unwrap(), 0);
        assert_eq!(shrunk.admit(&root, 1).unwrap(), Admission::Denied);
        let finalization = shrunk
            .reservation_finalization_headroom(&root, &guard, &held)
            .unwrap();
        assert!(finalization > 0);
        assert!(finalization <= shrunk.storage_budget().writable_limit());
        assert!(finalization < ceiling);

        drop(held);
        assert_eq!(super::reserved_bytes(&root).unwrap(), ceiling);
        assert_eq!(shrunk.admit(&root, 1).unwrap(), Admission::Denied);
        let recovered = shrunk
            .claim_stale_report_reservation(&root, &guard)
            .unwrap()
            .unwrap();
        assert_eq!(super::reserved_bytes(&root).unwrap(), ceiling);
        assert_eq!(shrunk.admit(&root, 1).unwrap(), Admission::Denied);
        recovered.release(&root, &guard).unwrap();
        assert_eq!(super::reserved_bytes(&root).unwrap(), 0);
        assert!(matches!(
            shrunk.admit(&root, 1).unwrap(),
            Admission::Allowed { .. }
        ));
        drop(guard);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn empty_and_corrupt_recovery_unlock_with_a_surviving_duplicate() {
        use std::os::unix::fs::PermissionsExt;
        for corrupt in [false, true] {
            let (root, control) = setup(if corrupt {
                "corrupt-recovery-duplicate"
            } else {
                "empty-recovery-duplicate"
            });
            let guard = MutationGuard::try_acquire(&root.join("runtime")).unwrap();
            control
                .reserve_report_build(&root, &guard, 8192)
                .unwrap()
                .release(&root, &guard)
                .unwrap();
            let metadata_path = root.join("runtime/report-reservation.meta");
            if corrupt {
                fs::write(&metadata_path, b"invalid").unwrap();
                fs::set_permissions(&metadata_path, fs::Permissions::from_mode(0o600)).unwrap();
            }
            let mut duplicate = None;
            let attempt = super::recover_observing(&root, &guard, |file| {
                duplicate = Some(file.try_clone().unwrap());
            });
            if corrupt {
                assert!(matches!(attempt, Err(super::ReservationError::Corrupt)));
                assert_eq!(fs::read(&metadata_path).unwrap(), b"invalid");
            } else {
                assert!(matches!(attempt, Ok(None)));
                assert!(!metadata_path.exists());
            }
            let next = super::lock(super::open(&root, false).unwrap().unwrap()).unwrap();
            assert!(duplicate.is_some());
            drop(next);
            drop(duplicate);
            drop(guard);
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    #[cfg(unix)]
    fn failed_acquire_unlocks_even_while_a_duplicate_descriptor_survives() {
        let (root, control) = setup("failed-acquire-duplicate");
        let guard = MutationGuard::try_acquire(&root.join("runtime")).unwrap();
        let held = control.reserve_report_build(&root, &guard, 8192).unwrap();
        drop(held);
        let metadata_before = fs::read(root.join("runtime/report-reservation.meta")).unwrap();
        let mut duplicate = None;
        let attempt = super::WriteReservation::acquire_observing(&root, &guard, 4096, |file| {
            duplicate = Some(file.try_clone().unwrap());
        });
        assert!(matches!(attempt, Err(super::ReservationError::Stale)));
        assert_eq!(
            fs::read(root.join("runtime/report-reservation.meta")).unwrap(),
            metadata_before
        );
        // A fork/dup shares the open-file description. Closing only the failed
        // attempt's descriptor must not leave its transient lock attached to it.
        let recovered = control
            .claim_stale_report_reservation(&root, &guard)
            .unwrap()
            .unwrap();
        assert_eq!(recovered.byte_ceiling(), 8192);
        assert!(duplicate.is_some());
        recovered.release(&root, &guard).unwrap();
        drop(duplicate);
        drop(guard);
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
        let path = root.join("runtime").join(super::METADATA_NAME);
        let valid: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        drop(held);
        let mut cases = vec![Vec::new(), b"broken".to_vec(), vec![b'x'; 513]];
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
