//! Read-only ownership evidence for the local dashboard capability.

use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io,
    path::{Component, Path, PathBuf},
};

const CAPABILITY_RELATIVE_PATH: &str = "runtime/dashboard-ui/capability";
const DIRECTORY_RELATIVE_PATHS: [&str; 3] = ["", "runtime", "runtime/dashboard-ui"];

/// Retained evidence for the exact dashboard capability and its private directory chain.
///
/// Capability bytes remain private. This observer does not create a token, inspect dashboard or
/// collector status, or activate storage admission. Callers must retain their outer storage freeze
/// through capture, matching, final revalidation, and accounting commit.
pub struct DashboardCapabilityStorageOwnershipEvidence {
    root: PathBuf,
    directories: Vec<RetainedDirectory>,
    capability: Option<RetainedCapability>,
}

struct RetainedDirectory {
    relative: PathBuf,
    file: Option<File>,
}

struct RetainedCapability {
    file: File,
    hash: [u8; 32],
}

impl std::fmt::Debug for DashboardCapabilityStorageOwnershipEvidence {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DashboardCapabilityStorageOwnershipEvidence")
            .finish_non_exhaustive()
    }
}

/// Failure to establish or retain bounded dashboard capability ownership evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DashboardCapabilityStorageOwnershipError {
    Io(io::ErrorKind),
    InvalidContent,
    RootMismatch,
    Replaced,
    InsecurePermissions,
    Symlink,
    Hardlink,
    InvalidType,
    UnsupportedPlatform,
}

impl std::fmt::Display for DashboardCapabilityStorageOwnershipError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Io(_) => "dashboard capability ownership I/O failure",
            Self::InvalidContent => "dashboard capability ownership validation failed",
            Self::RootMismatch => "dashboard capability ownership root mismatch",
            Self::Replaced => "dashboard capability ownership identity changed",
            Self::InsecurePermissions => "dashboard capability ownership path is not private",
            Self::Symlink => "dashboard capability ownership refuses symbolic links",
            Self::Hardlink => "dashboard capability ownership refuses hard-linked files",
            Self::InvalidType => "dashboard capability ownership path has the wrong file type",
            Self::UnsupportedPlatform => "dashboard capability ownership is unsupported",
        })
    }
}

impl std::error::Error for DashboardCapabilityStorageOwnershipError {}

impl From<io::Error> for DashboardCapabilityStorageOwnershipError {
    fn from(error: io::Error) -> Self {
        if error.raw_os_error() == Some(libc::ELOOP) {
            Self::Symlink
        } else {
            Self::Io(error.kind())
        }
    }
}

impl DashboardCapabilityStorageOwnershipEvidence {
    /// Captures exact private directory and capability evidence without creating any path.
    ///
    /// # Errors
    ///
    /// Returns an error for a non-absolute or non-normalized root, unsafe metadata, malformed or
    /// oversized capability content, or any identity change during capture.
    pub fn capture(root: &Path) -> Result<Self, DashboardCapabilityStorageOwnershipError> {
        capture(root)
    }

    /// Matches only the retained root, runtime, dashboard directory, and exact capability.
    ///
    /// Unknown siblings and descendants return `false`. A known path absent at capture or with a
    /// different identity fails closed.
    ///
    /// # Errors
    ///
    /// Returns an error when a known candidate is unsafe or does not have the retained identity.
    pub fn matches_entry(
        &self,
        relative: &Path,
        candidate: &File,
    ) -> Result<bool, DashboardCapabilityStorageOwnershipError> {
        if relative == Path::new(CAPABILITY_RELATIVE_PATH) {
            let Some(expected) = &self.capability else {
                return Err(DashboardCapabilityStorageOwnershipError::Replaced);
            };
            let metadata = candidate.metadata()?;
            validate_capability_metadata(&metadata)?;
            if !same_identity(&expected.file.metadata()?, &metadata) {
                return Err(DashboardCapabilityStorageOwnershipError::Replaced);
            }
            return Ok(true);
        }

        let Some(directory) = self
            .directories
            .iter()
            .find(|directory| directory.relative == relative)
        else {
            return Ok(false);
        };
        let Some(expected) = &directory.file else {
            return Err(DashboardCapabilityStorageOwnershipError::Replaced);
        };
        let metadata = candidate.metadata()?;
        validate_directory_metadata(&metadata)?;
        if !same_identity(&expected.metadata()?, &metadata) {
            return Err(DashboardCapabilityStorageOwnershipError::Replaced);
        }
        Ok(true)
    }

    /// Revalidates exact presence, names, identities, permissions, and bounded content hash.
    ///
    /// # Errors
    ///
    /// Returns an error when any retained or absent path changed.
    pub fn revalidate(&self) -> Result<(), DashboardCapabilityStorageOwnershipError> {
        self.revalidate_directories()?;
        revalidate_capability(
            &self.root.join(CAPABILITY_RELATIVE_PATH),
            self.capability.as_ref(),
        )?;
        self.revalidate_directories()
    }

    fn revalidate_directories(&self) -> Result<(), DashboardCapabilityStorageOwnershipError> {
        for retained in &self.directories {
            revalidate_optional_directory(
                &self.root.join(&retained.relative),
                retained.file.as_ref(),
            )?;
        }
        Ok(())
    }
}

#[cfg(unix)]
fn capture(
    root: &Path,
) -> Result<DashboardCapabilityStorageOwnershipEvidence, DashboardCapabilityStorageOwnershipError> {
    if !valid_boundary(root) {
        return Err(DashboardCapabilityStorageOwnershipError::RootMismatch);
    }
    let mut directories = Vec::with_capacity(DIRECTORY_RELATIVE_PATHS.len());
    for relative in DIRECTORY_RELATIVE_PATHS {
        let relative = PathBuf::from(relative);
        let path = root.join(&relative);
        let file = if relative.as_os_str().is_empty() {
            Some(open_directory(&path)?)
        } else {
            capture_optional_directory(&path)?
        };
        directories.push(RetainedDirectory { relative, file });
    }
    let capability = capture_optional_capability(&root.join(CAPABILITY_RELATIVE_PATH))?;
    let evidence = DashboardCapabilityStorageOwnershipEvidence {
        root: root.to_path_buf(),
        directories,
        capability,
    };
    evidence.revalidate()?;
    Ok(evidence)
}

#[cfg(not(unix))]
fn capture(
    _root: &Path,
) -> Result<DashboardCapabilityStorageOwnershipEvidence, DashboardCapabilityStorageOwnershipError> {
    Err(DashboardCapabilityStorageOwnershipError::UnsupportedPlatform)
}

fn valid_boundary(path: &Path) -> bool {
    path.is_absolute()
        && path.components().all(|component| {
            matches!(
                component,
                Component::Prefix(_) | Component::RootDir | Component::Normal(_)
            )
        })
}

fn capture_optional_directory(
    path: &Path,
) -> Result<Option<File>, DashboardCapabilityStorageOwnershipError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            validate_named_directory_metadata(&metadata)?;
            Ok(Some(open_directory(path)?))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn capture_optional_capability(
    path: &Path,
) -> Result<Option<RetainedCapability>, DashboardCapabilityStorageOwnershipError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            validate_named_capability_metadata(&metadata)?;
            let file = open_capability(path)?;
            let bytes = read_capability(&file)?;
            validate_named_capability(path, &file)?;
            Ok(Some(RetainedCapability {
                file,
                hash: Sha256::digest(&bytes).into(),
            }))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn revalidate_optional_directory(
    path: &Path,
    expected: Option<&File>,
) -> Result<(), DashboardCapabilityStorageOwnershipError> {
    let Some(expected) = expected else {
        return validate_absence(path);
    };
    let current = open_directory(path)?;
    if !same_identity(&expected.metadata()?, &current.metadata()?) {
        return Err(DashboardCapabilityStorageOwnershipError::Replaced);
    }
    Ok(())
}

fn revalidate_capability(
    path: &Path,
    expected: Option<&RetainedCapability>,
) -> Result<(), DashboardCapabilityStorageOwnershipError> {
    let Some(expected) = expected else {
        return validate_absence(path);
    };
    let current = open_capability(path)?;
    let bytes = read_capability(&current)?;
    if !same_identity(&expected.file.metadata()?, &current.metadata()?)
        || Sha256::digest(&bytes).as_slice() != expected.hash
    {
        return Err(DashboardCapabilityStorageOwnershipError::Replaced);
    }
    validate_named_capability(path, &current)
}

fn validate_absence(path: &Path) -> Result<(), DashboardCapabilityStorageOwnershipError> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Ok(_) => Err(DashboardCapabilityStorageOwnershipError::Replaced),
        Err(error) => Err(error.into()),
    }
}

#[cfg(unix)]
fn open_directory(path: &Path) -> Result<File, DashboardCapabilityStorageOwnershipError> {
    use std::os::unix::fs::OpenOptionsExt;

    let named = fs::symlink_metadata(path)?;
    validate_named_directory_metadata(&named)?;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let opened = file.metadata()?;
    validate_directory_metadata(&opened)?;
    if !same_identity(&named, &opened) {
        return Err(DashboardCapabilityStorageOwnershipError::Replaced);
    }
    Ok(file)
}

#[cfg(not(unix))]
fn open_directory(_path: &Path) -> Result<File, DashboardCapabilityStorageOwnershipError> {
    Err(DashboardCapabilityStorageOwnershipError::UnsupportedPlatform)
}

#[cfg(unix)]
fn open_capability(path: &Path) -> Result<File, DashboardCapabilityStorageOwnershipError> {
    use std::os::unix::fs::OpenOptionsExt;

    let named = fs::symlink_metadata(path)?;
    validate_named_capability_metadata(&named)?;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let opened = file.metadata()?;
    validate_capability_metadata(&opened)?;
    if !same_identity(&named, &opened) {
        return Err(DashboardCapabilityStorageOwnershipError::Replaced);
    }
    Ok(file)
}

#[cfg(not(unix))]
fn open_capability(_path: &Path) -> Result<File, DashboardCapabilityStorageOwnershipError> {
    Err(DashboardCapabilityStorageOwnershipError::UnsupportedPlatform)
}

fn validate_named_directory_metadata(
    metadata: &fs::Metadata,
) -> Result<(), DashboardCapabilityStorageOwnershipError> {
    if metadata.file_type().is_symlink() {
        return Err(DashboardCapabilityStorageOwnershipError::Symlink);
    }
    validate_directory_metadata(metadata)
}

fn validate_named_capability_metadata(
    metadata: &fs::Metadata,
) -> Result<(), DashboardCapabilityStorageOwnershipError> {
    if metadata.file_type().is_symlink() {
        return Err(DashboardCapabilityStorageOwnershipError::Symlink);
    }
    validate_capability_metadata(metadata)
}

#[cfg(unix)]
fn validate_directory_metadata(
    metadata: &fs::Metadata,
) -> Result<(), DashboardCapabilityStorageOwnershipError> {
    use std::os::unix::fs::PermissionsExt;

    if !metadata.is_dir() {
        return Err(DashboardCapabilityStorageOwnershipError::InvalidType);
    }
    if metadata.permissions().mode() & 0o7777 != 0o700 {
        return Err(DashboardCapabilityStorageOwnershipError::InsecurePermissions);
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_directory_metadata(
    _metadata: &fs::Metadata,
) -> Result<(), DashboardCapabilityStorageOwnershipError> {
    Err(DashboardCapabilityStorageOwnershipError::UnsupportedPlatform)
}

#[cfg(unix)]
fn validate_capability_metadata(
    metadata: &fs::Metadata,
) -> Result<(), DashboardCapabilityStorageOwnershipError> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    if !metadata.is_file() {
        return Err(DashboardCapabilityStorageOwnershipError::InvalidType);
    }
    if metadata.permissions().mode() & 0o7777 != 0o600 {
        return Err(DashboardCapabilityStorageOwnershipError::InsecurePermissions);
    }
    if metadata.nlink() != 1 {
        return Err(DashboardCapabilityStorageOwnershipError::Hardlink);
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_capability_metadata(
    _metadata: &fs::Metadata,
) -> Result<(), DashboardCapabilityStorageOwnershipError> {
    Err(DashboardCapabilityStorageOwnershipError::UnsupportedPlatform)
}

fn validate_named_capability(
    path: &Path,
    file: &File,
) -> Result<(), DashboardCapabilityStorageOwnershipError> {
    let named = fs::symlink_metadata(path)?;
    validate_named_capability_metadata(&named)?;
    if !same_identity(&file.metadata()?, &named) {
        return Err(DashboardCapabilityStorageOwnershipError::Replaced);
    }
    Ok(())
}

#[cfg(unix)]
fn read_capability(file: &File) -> Result<Vec<u8>, DashboardCapabilityStorageOwnershipError> {
    use std::os::unix::fs::{FileExt, MetadataExt};

    let before = file.metadata()?;
    validate_capability_metadata(&before)?;
    if before.len() > super::DASHBOARD_CAPABILITY_MAX_BYTES {
        return Err(DashboardCapabilityStorageOwnershipError::InvalidContent);
    }
    let capacity = usize::try_from(super::DASHBOARD_CAPABILITY_MAX_BYTES + 1)
        .map_err(|_| DashboardCapabilityStorageOwnershipError::InvalidContent)?;
    let mut bytes = vec![0_u8; capacity];
    let mut offset = 0_usize;
    while offset < bytes.len() {
        let read = file.read_at(&mut bytes[offset..], offset as u64)?;
        if read == 0 {
            break;
        }
        offset = offset
            .checked_add(read)
            .ok_or(DashboardCapabilityStorageOwnershipError::InvalidContent)?;
    }
    if u64::try_from(offset).unwrap_or(u64::MAX) > super::DASHBOARD_CAPABILITY_MAX_BYTES {
        return Err(DashboardCapabilityStorageOwnershipError::InvalidContent);
    }
    bytes.truncate(offset);
    let value = std::str::from_utf8(&bytes)
        .map_err(|_| DashboardCapabilityStorageOwnershipError::InvalidContent)?;
    super::validated_dashboard_token(value)
        .ok_or(DashboardCapabilityStorageOwnershipError::InvalidContent)?;
    let after = file.metadata()?;
    validate_capability_metadata(&after)?;
    if before.dev() != after.dev()
        || before.ino() != after.ino()
        || before.len() != after.len()
        || before.mode() != after.mode()
        || before.nlink() != after.nlink()
        || before.mtime() != after.mtime()
        || before.mtime_nsec() != after.mtime_nsec()
        || before.ctime() != after.ctime()
        || before.ctime_nsec() != after.ctime_nsec()
    {
        return Err(DashboardCapabilityStorageOwnershipError::Replaced);
    }
    Ok(bytes)
}

#[cfg(not(unix))]
fn read_capability(_file: &File) -> Result<Vec<u8>, DashboardCapabilityStorageOwnershipError> {
    Err(DashboardCapabilityStorageOwnershipError::UnsupportedPlatform)
}

#[cfg(unix)]
fn same_identity(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    left.dev() == right.dev() && left.ino() == right.ino()
}

#[cfg(not(unix))]
fn same_identity(_left: &fs::Metadata, _right: &fs::Metadata) -> bool {
    false
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{
        fs::{self, File, OpenOptions},
        io::Write,
        os::unix::fs::{OpenOptionsExt, PermissionsExt},
        path::{Path, PathBuf},
        process::Command,
        sync::atomic::{AtomicU64, Ordering},
        time::{Duration, Instant},
    };

    static NEXT: AtomicU64 = AtomicU64::new(0);

    struct Fixture {
        root: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "dashboard-capability-owner-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            assert!(!root.exists());
            fs::create_dir(&root).unwrap();
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
            Self { root }
        }

        fn dashboard(&self) -> PathBuf {
            self.root.join("runtime/dashboard-ui")
        }

        fn capability(&self) -> PathBuf {
            self.dashboard().join("capability")
        }

        fn create_dashboard(&self) {
            fs::create_dir_all(self.dashboard()).unwrap();
            for directory in [self.root.join("runtime"), self.dashboard()] {
                fs::set_permissions(directory, fs::Permissions::from_mode(0o700)).unwrap();
            }
        }

        fn write_capability(&self, bytes: &[u8]) {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .open(self.capability())
                .unwrap();
            file.write_all(bytes).unwrap();
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.root).unwrap();
        }
    }

    fn token(byte: u8, newline: bool) -> Vec<u8> {
        let mut bytes = vec![byte; 64];
        if newline {
            bytes.push(b'\n');
        }
        bytes
    }

    #[test]
    fn missing_optional_paths_are_noncreating_and_unknown_siblings_are_unowned() {
        for depth in 0..3 {
            let fixture = Fixture::new();
            if depth > 0 {
                fs::create_dir(fixture.root.join("runtime")).unwrap();
                fs::set_permissions(
                    fixture.root.join("runtime"),
                    fs::Permissions::from_mode(0o700),
                )
                .unwrap();
            }
            if depth > 1 {
                fs::create_dir(fixture.dashboard()).unwrap();
                fs::set_permissions(fixture.dashboard(), fs::Permissions::from_mode(0o700))
                    .unwrap();
            }
            let evidence =
                DashboardCapabilityStorageOwnershipEvidence::capture(&fixture.root).unwrap();
            assert_eq!(fixture.root.join("runtime").exists(), depth > 0);
            assert_eq!(fixture.dashboard().exists(), depth > 1);
            assert!(!fixture.capability().exists());
            let root = File::open(&fixture.root).unwrap();
            assert!(evidence.matches_entry(Path::new(""), &root).unwrap());
            for relative in ["other", "runtime/other", "runtime/dashboard-ui/other"] {
                assert!(!evidence.matches_entry(Path::new(relative), &root).unwrap());
            }
            evidence.revalidate().unwrap();
        }
    }

    #[test]
    fn valid_capability_and_exact_directories_match_without_exposing_token() {
        for newline in [false, true] {
            let fixture = Fixture::new();
            fixture.create_dashboard();
            let token = token(b'a', newline);
            fixture.write_capability(&token);
            let evidence =
                DashboardCapabilityStorageOwnershipEvidence::capture(&fixture.root).unwrap();
            for relative in ["", "runtime", "runtime/dashboard-ui"] {
                assert!(
                    evidence
                        .matches_entry(
                            Path::new(relative),
                            &File::open(fixture.root.join(relative)).unwrap()
                        )
                        .unwrap()
                );
            }
            assert!(
                evidence
                    .matches_entry(
                        Path::new("runtime/dashboard-ui/capability"),
                        &File::open(fixture.capability()).unwrap(),
                    )
                    .unwrap()
            );
            let debug = format!("{evidence:?}");
            assert!(!debug.contains(&"a".repeat(64)));
            assert!(!debug.contains(fixture.root.to_str().unwrap()));
            evidence.revalidate().unwrap();
            assert_eq!(fs::read(fixture.capability()).unwrap(), token);
        }
    }

    #[test]
    fn malformed_and_oversized_capabilities_fail_closed() {
        for bytes in [
            b"short".to_vec(),
            token(b'A', false),
            vec![b'g'; 64],
            vec![b'a'; 66],
        ] {
            let fixture = Fixture::new();
            fixture.create_dashboard();
            fixture.write_capability(&bytes);
            assert_eq!(
                DashboardCapabilityStorageOwnershipEvidence::capture(&fixture.root).unwrap_err(),
                DashboardCapabilityStorageOwnershipError::InvalidContent,
            );
        }
    }

    #[test]
    fn replacement_in_place_change_and_known_absence_fail_closed() {
        for scenario in 0..3 {
            let fixture = Fixture::new();
            fixture.create_dashboard();
            if scenario != 2 {
                fixture.write_capability(&token(b'a', true));
            }
            let evidence =
                DashboardCapabilityStorageOwnershipEvidence::capture(&fixture.root).unwrap();
            if scenario == 0 {
                fs::write(fixture.capability(), token(b'b', true)).unwrap();
            } else if scenario == 1 {
                fs::rename(
                    fixture.capability(),
                    fixture.dashboard().join("old-capability"),
                )
                .unwrap();
                fixture.write_capability(&token(b'a', true));
            } else {
                fixture.write_capability(&token(b'a', true));
                assert_eq!(
                    evidence
                        .matches_entry(
                            Path::new("runtime/dashboard-ui/capability"),
                            &File::open(fixture.capability()).unwrap(),
                        )
                        .unwrap_err(),
                    DashboardCapabilityStorageOwnershipError::Replaced,
                );
            }
            assert_eq!(
                evidence.revalidate().unwrap_err(),
                DashboardCapabilityStorageOwnershipError::Replaced,
            );
        }
    }

    #[test]
    fn broad_permissions_hardlinks_and_wrong_candidates_are_rejected() {
        for hardlink in [false, true] {
            let fixture = Fixture::new();
            fixture.create_dashboard();
            fixture.write_capability(&token(b'a', true));
            if hardlink {
                fs::hard_link(fixture.capability(), fixture.dashboard().join("alias")).unwrap();
            } else {
                fs::set_permissions(fixture.capability(), fs::Permissions::from_mode(0o640))
                    .unwrap();
            }
            assert!(DashboardCapabilityStorageOwnershipEvidence::capture(&fixture.root).is_err());
        }

        let fixture = Fixture::new();
        fixture.create_dashboard();
        fixture.write_capability(&token(b'a', true));
        let evidence = DashboardCapabilityStorageOwnershipEvidence::capture(&fixture.root).unwrap();
        let other = fixture.dashboard().join("other");
        fs::write(&other, token(b'a', true)).unwrap();
        fs::set_permissions(&other, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            evidence
                .matches_entry(
                    Path::new("runtime/dashboard-ui/capability"),
                    &File::open(other).unwrap()
                )
                .unwrap_err(),
            DashboardCapabilityStorageOwnershipError::Replaced,
        );
    }

    #[test]
    fn wrong_roots_and_replaced_directories_fail_closed() {
        let fixture = Fixture::new();
        assert_eq!(
            DashboardCapabilityStorageOwnershipEvidence::capture(Path::new("relative"))
                .unwrap_err(),
            DashboardCapabilityStorageOwnershipError::RootMismatch,
        );
        fixture.create_dashboard();
        let evidence = DashboardCapabilityStorageOwnershipEvidence::capture(&fixture.root).unwrap();
        fs::rename(
            fixture.dashboard(),
            fixture.root.join("retained-dashboard-ui"),
        )
        .unwrap();
        fs::create_dir(fixture.dashboard()).unwrap();
        fs::set_permissions(fixture.dashboard(), fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(
            evidence.revalidate().unwrap_err(),
            DashboardCapabilityStorageOwnershipError::Replaced,
        );

        let absent = Fixture::new();
        let evidence = DashboardCapabilityStorageOwnershipEvidence::capture(&absent.root).unwrap();
        fs::create_dir(absent.root.join("runtime")).unwrap();
        fs::set_permissions(
            absent.root.join("runtime"),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        assert_eq!(
            evidence.revalidate().unwrap_err(),
            DashboardCapabilityStorageOwnershipError::Replaced,
        );
    }

    #[test]
    fn broad_directory_permissions_fail_closed() {
        for relative in ["", "runtime", "runtime/dashboard-ui"] {
            let fixture = Fixture::new();
            fixture.create_dashboard();
            fs::set_permissions(
                fixture.root.join(relative),
                fs::Permissions::from_mode(0o750),
            )
            .unwrap();
            assert_eq!(
                DashboardCapabilityStorageOwnershipEvidence::capture(&fixture.root).unwrap_err(),
                DashboardCapabilityStorageOwnershipError::InsecurePermissions,
            );
            fs::set_permissions(
                fixture.root.join(relative),
                fs::Permissions::from_mode(0o700),
            )
            .unwrap();
        }
    }

    #[test]
    fn symlinks_are_not_followed() {
        let fixture = Fixture::new();
        fixture.create_dashboard();
        let target = fixture.dashboard().join("target");
        fs::write(&target, token(b'a', true)).unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
        std::os::unix::fs::symlink(&target, fixture.capability()).unwrap();
        assert_eq!(
            DashboardCapabilityStorageOwnershipEvidence::capture(&fixture.root).unwrap_err(),
            DashboardCapabilityStorageOwnershipError::Symlink,
        );
    }

    #[test]
    fn fifo_is_rejected_without_blocking() {
        const PROBE: &str = "AGENTOBS_DASHBOARD_CAPABILITY_OWNER_FIFO";
        if std::env::var_os(PROBE).is_none() {
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "storage_ownership::tests::fifo_is_rejected_without_blocking",
                ])
                .env(PROBE, "1")
                .spawn()
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                if let Some(status) = child.try_wait().unwrap() {
                    assert!(status.success());
                    return;
                }
                if Instant::now() >= deadline {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    panic!("dashboard capability ownership blocked on FIFO");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        let fixture = Fixture::new();
        fixture.create_dashboard();
        assert!(
            Command::new("mkfifo")
                .args(["-m", "600"])
                .arg(fixture.capability())
                .status()
                .unwrap()
                .success()
        );
        assert_eq!(
            DashboardCapabilityStorageOwnershipEvidence::capture(&fixture.root).unwrap_err(),
            DashboardCapabilityStorageOwnershipError::InvalidType,
        );
    }
}
