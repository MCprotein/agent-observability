//! Read-only `LaunchAgent` transaction and lifecycle-lock ownership evidence.

use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io,
    path::{Component, Path, PathBuf},
};

const OWNERSHIP_RELATIVE_PATH: &str = "runtime/integrations/codex/launch-agent-ownership-v1.json";
const LIFECYCLE_LOCK_RELATIVE_PATH: &str = "runtime/integrations/codex/lifecycle/mutation.lock";
const DIRECTORY_RELATIVE_PATHS: [&str; 5] = [
    "",
    "runtime",
    "runtime/integrations",
    "runtime/integrations/codex",
    "runtime/integrations/codex/lifecycle",
];
#[cfg(target_os = "macos")]
const MACOS_O_NONBLOCK: i32 = 0x0000_0004;
#[cfg(target_os = "macos")]
const MACOS_O_NOFOLLOW: i32 = 0x0000_0100;

/// Retained evidence for the exact `LaunchAgent` transaction and lifecycle mutation lock.
///
/// The expected external plist path and transaction digest remain private. The external plist,
/// process table, `launchctl`, and health endpoint are never inspected. Callers must retain their
/// outer storage freeze through capture, matching, final revalidation, and accounting commit.
pub struct LaunchAgentStorageOwnershipEvidence {
    root: PathBuf,
    expected_plist: PathBuf,
    directories: Vec<RetainedDirectory>,
    ownership: Option<RetainedFile>,
    lifecycle_lock: Option<RetainedFile>,
}

struct RetainedDirectory {
    relative: PathBuf,
    file: Option<File>,
}

struct RetainedFile {
    file: File,
    content: RetainedContent,
}

enum RetainedContent {
    TransactionHash([u8; 32]),
    Empty,
}

impl std::fmt::Debug for LaunchAgentStorageOwnershipEvidence {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LaunchAgentStorageOwnershipEvidence")
            .finish_non_exhaustive()
    }
}

/// Failure to establish or retain bounded `LaunchAgent` storage ownership evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LaunchAgentStorageOwnershipError {
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

impl std::fmt::Display for LaunchAgentStorageOwnershipError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Io(_) => "LaunchAgent storage ownership I/O failure",
            Self::InvalidContent => "LaunchAgent storage ownership validation failed",
            Self::RootMismatch => "LaunchAgent storage ownership boundary mismatch",
            Self::Replaced => "LaunchAgent storage ownership identity changed",
            Self::InsecurePermissions => "LaunchAgent storage ownership path is not private",
            Self::Symlink => "LaunchAgent storage ownership refuses symbolic links",
            Self::Hardlink => "LaunchAgent storage ownership refuses hard-linked files",
            Self::InvalidType => "LaunchAgent storage ownership path has the wrong file type",
            Self::UnsupportedPlatform => "LaunchAgent storage ownership is unsupported",
        })
    }
}

impl std::error::Error for LaunchAgentStorageOwnershipError {}

impl From<io::Error> for LaunchAgentStorageOwnershipError {
    fn from(error: io::Error) -> Self {
        Self::Io(error.kind())
    }
}

impl LaunchAgentStorageOwnershipEvidence {
    /// Captures exact private ownership evidence without creating or repairing any path.
    ///
    /// `root` and `home` must be explicit, absolute, normalized boundaries. `home` is used only by
    /// the pure service-label path calculation; no path under it is opened.
    ///
    /// # Errors
    ///
    /// Returns an error for unsupported platforms, invalid boundaries, unsafe paths, invalid
    /// transaction state, nonempty lifecycle locks, or identity changes during capture.
    pub fn capture(root: &Path, home: &Path) -> Result<Self, LaunchAgentStorageOwnershipError> {
        capture(root, home)
    }

    /// Matches only the exact retained directories and two exact owned files.
    ///
    /// Descendants and unknown siblings return `false`. A known path that was absent at capture or
    /// whose identity differs fails closed.
    ///
    /// # Errors
    ///
    /// Returns an error when the candidate has invalid metadata or the wrong captured identity.
    pub fn matches_entry(
        &self,
        relative: &Path,
        candidate: &File,
    ) -> Result<bool, LaunchAgentStorageOwnershipError> {
        if let Some(directory) = self.directories.iter().find(|directory| {
            !directory.relative.as_os_str().is_empty() && directory.relative == relative
        }) {
            let Some(expected) = &directory.file else {
                return Err(LaunchAgentStorageOwnershipError::Replaced);
            };
            let metadata = candidate.metadata()?;
            validate_directory_metadata(&metadata)?;
            if !same_identity(&expected.metadata()?, &metadata) {
                return Err(LaunchAgentStorageOwnershipError::Replaced);
            }
            return Ok(true);
        }

        let expected = if relative == Path::new(OWNERSHIP_RELATIVE_PATH) {
            Some(self.ownership.as_ref())
        } else if relative == Path::new(LIFECYCLE_LOCK_RELATIVE_PATH) {
            Some(self.lifecycle_lock.as_ref())
        } else {
            None
        };
        let Some(expected) = expected else {
            return Ok(false);
        };
        let Some(expected) = expected else {
            return Err(LaunchAgentStorageOwnershipError::Replaced);
        };
        let metadata = candidate.metadata()?;
        validate_file_metadata(&metadata)?;
        if !same_identity(&expected.file.metadata()?, &metadata) {
            return Err(LaunchAgentStorageOwnershipError::Replaced);
        }
        Ok(true)
    }

    /// Revalidates exact presence, names, identities, permissions, and bounded content hashes.
    ///
    /// # Errors
    ///
    /// Returns an error when any retained fact changed or no longer validates.
    pub fn revalidate(&self) -> Result<(), LaunchAgentStorageOwnershipError> {
        self.revalidate_directories()?;
        revalidate_optional_file(
            &self.root.join(OWNERSHIP_RELATIVE_PATH),
            self.ownership.as_ref(),
            &self.expected_plist,
        )?;
        revalidate_optional_file(
            &self.root.join(LIFECYCLE_LOCK_RELATIVE_PATH),
            self.lifecycle_lock.as_ref(),
            &self.expected_plist,
        )?;
        self.revalidate_directories()
    }

    fn revalidate_directories(&self) -> Result<(), LaunchAgentStorageOwnershipError> {
        for retained in &self.directories {
            revalidate_optional_directory(
                &self.root.join(&retained.relative),
                retained.file.as_ref(),
            )?;
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn capture(
    root: &Path,
    home: &Path,
) -> Result<LaunchAgentStorageOwnershipEvidence, LaunchAgentStorageOwnershipError> {
    if !valid_boundary(root) || !valid_boundary(home) {
        return Err(LaunchAgentStorageOwnershipError::RootMismatch);
    }
    let expected_plist = super::expected_launch_agent_plist_path(root, home);
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
    let ownership = capture_optional_file(
        &root.join(OWNERSHIP_RELATIVE_PATH),
        FileKind::Transaction,
        &expected_plist,
    )?;
    let lifecycle_lock = capture_optional_file(
        &root.join(LIFECYCLE_LOCK_RELATIVE_PATH),
        FileKind::EmptyLock,
        &expected_plist,
    )?;
    let evidence = LaunchAgentStorageOwnershipEvidence {
        root: root.to_path_buf(),
        expected_plist,
        directories,
        ownership,
        lifecycle_lock,
    };
    evidence.revalidate()?;
    Ok(evidence)
}

#[cfg(not(target_os = "macos"))]
fn capture(
    _root: &Path,
    _home: &Path,
) -> Result<LaunchAgentStorageOwnershipEvidence, LaunchAgentStorageOwnershipError> {
    Err(LaunchAgentStorageOwnershipError::UnsupportedPlatform)
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

#[derive(Clone, Copy)]
enum FileKind {
    Transaction,
    EmptyLock,
}

fn capture_optional_directory(
    path: &Path,
) -> Result<Option<File>, LaunchAgentStorageOwnershipError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            validate_named_directory_metadata(&metadata)?;
            Ok(Some(open_directory(path)?))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn capture_optional_file(
    path: &Path,
    kind: FileKind,
    expected_plist: &Path,
) -> Result<Option<RetainedFile>, LaunchAgentStorageOwnershipError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            validate_named_file_metadata(&metadata)?;
            let file = open_file(path)?;
            let bytes = read_bounded(&file, maximum_bytes(kind))?;
            let content = validate_content(&bytes, kind, expected_plist)?;
            validate_named_file(path, &file)?;
            Ok(Some(RetainedFile { file, content }))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn revalidate_optional_directory(
    path: &Path,
    expected: Option<&File>,
) -> Result<(), LaunchAgentStorageOwnershipError> {
    let Some(expected) = expected else {
        return validate_absence(path);
    };
    let current = open_directory(path)?;
    if !same_identity(&expected.metadata()?, &current.metadata()?) {
        return Err(LaunchAgentStorageOwnershipError::Replaced);
    }
    Ok(())
}

fn revalidate_optional_file(
    path: &Path,
    expected: Option<&RetainedFile>,
    expected_plist: &Path,
) -> Result<(), LaunchAgentStorageOwnershipError> {
    let Some(expected) = expected else {
        return validate_absence(path);
    };
    let kind = match expected.content {
        RetainedContent::TransactionHash(_) => FileKind::Transaction,
        RetainedContent::Empty => FileKind::EmptyLock,
    };
    let current = open_file(path)?;
    let bytes = read_bounded(&current, maximum_bytes(kind))?;
    if !same_identity(&expected.file.metadata()?, &current.metadata()?) {
        return Err(LaunchAgentStorageOwnershipError::Replaced);
    }
    match expected.content {
        RetainedContent::TransactionHash(expected_hash) => {
            if content_hash(&bytes) != expected_hash {
                return Err(LaunchAgentStorageOwnershipError::Replaced);
            }
            validate_transaction(&bytes, expected_plist)?;
        }
        RetainedContent::Empty => {
            if !bytes.is_empty() {
                return Err(LaunchAgentStorageOwnershipError::Replaced);
            }
        }
    }
    validate_named_file(path, &current)
}

fn validate_absence(path: &Path) -> Result<(), LaunchAgentStorageOwnershipError> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Ok(_) => Err(LaunchAgentStorageOwnershipError::Replaced),
        Err(error) => Err(error.into()),
    }
}

#[cfg(target_os = "macos")]
fn maximum_bytes(kind: FileKind) -> u64 {
    match kind {
        FileKind::Transaction => super::MAX_LAUNCH_AGENT_OWNERSHIP_BYTES,
        FileKind::EmptyLock => 0,
    }
}

#[cfg(not(target_os = "macos"))]
fn maximum_bytes(_kind: FileKind) -> u64 {
    0
}

fn validate_content(
    bytes: &[u8],
    kind: FileKind,
    expected_plist: &Path,
) -> Result<RetainedContent, LaunchAgentStorageOwnershipError> {
    match kind {
        FileKind::Transaction => {
            validate_transaction(bytes, expected_plist)?;
            Ok(RetainedContent::TransactionHash(content_hash(bytes)))
        }
        FileKind::EmptyLock if bytes.is_empty() => Ok(RetainedContent::Empty),
        FileKind::EmptyLock => Err(LaunchAgentStorageOwnershipError::InvalidContent),
    }
}

#[cfg(target_os = "macos")]
fn validate_transaction(
    bytes: &[u8],
    expected_plist: &Path,
) -> Result<(), LaunchAgentStorageOwnershipError> {
    super::decode_launch_agent_transaction(bytes, expected_plist)
        .map(|_| ())
        .map_err(|_| LaunchAgentStorageOwnershipError::InvalidContent)
}

#[cfg(not(target_os = "macos"))]
fn validate_transaction(
    _bytes: &[u8],
    _expected_plist: &Path,
) -> Result<(), LaunchAgentStorageOwnershipError> {
    Err(LaunchAgentStorageOwnershipError::UnsupportedPlatform)
}

fn content_hash(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

#[cfg(target_os = "macos")]
fn open_directory(path: &Path) -> Result<File, LaunchAgentStorageOwnershipError> {
    use std::os::unix::fs::OpenOptionsExt;

    let metadata = fs::symlink_metadata(path)?;
    validate_named_directory_metadata(&metadata)?;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(MACOS_O_NONBLOCK | MACOS_O_NOFOLLOW)
        .open(path)?;
    let opened = file.metadata()?;
    validate_directory_metadata(&opened)?;
    if !same_identity(&metadata, &opened) {
        return Err(LaunchAgentStorageOwnershipError::Replaced);
    }
    Ok(file)
}

#[cfg(not(target_os = "macos"))]
fn open_directory(_path: &Path) -> Result<File, LaunchAgentStorageOwnershipError> {
    Err(LaunchAgentStorageOwnershipError::UnsupportedPlatform)
}

#[cfg(target_os = "macos")]
fn open_file(path: &Path) -> Result<File, LaunchAgentStorageOwnershipError> {
    use std::os::unix::fs::OpenOptionsExt;

    let metadata = fs::symlink_metadata(path)?;
    validate_named_file_metadata(&metadata)?;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(MACOS_O_NONBLOCK | MACOS_O_NOFOLLOW)
        .open(path)?;
    let opened = file.metadata()?;
    validate_file_metadata(&opened)?;
    if !same_identity(&metadata, &opened) {
        return Err(LaunchAgentStorageOwnershipError::Replaced);
    }
    Ok(file)
}

#[cfg(not(target_os = "macos"))]
fn open_file(_path: &Path) -> Result<File, LaunchAgentStorageOwnershipError> {
    Err(LaunchAgentStorageOwnershipError::UnsupportedPlatform)
}

fn validate_named_directory_metadata(
    metadata: &fs::Metadata,
) -> Result<(), LaunchAgentStorageOwnershipError> {
    if metadata.file_type().is_symlink() {
        return Err(LaunchAgentStorageOwnershipError::Symlink);
    }
    validate_directory_metadata(metadata)
}

fn validate_named_file_metadata(
    metadata: &fs::Metadata,
) -> Result<(), LaunchAgentStorageOwnershipError> {
    if metadata.file_type().is_symlink() {
        return Err(LaunchAgentStorageOwnershipError::Symlink);
    }
    validate_file_metadata(metadata)
}

#[cfg(target_os = "macos")]
fn validate_directory_metadata(
    metadata: &fs::Metadata,
) -> Result<(), LaunchAgentStorageOwnershipError> {
    use std::os::unix::fs::PermissionsExt;

    if !metadata.is_dir() {
        return Err(LaunchAgentStorageOwnershipError::InvalidType);
    }
    if metadata.permissions().mode() & 0o7777 != 0o700 {
        return Err(LaunchAgentStorageOwnershipError::InsecurePermissions);
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn validate_directory_metadata(
    _metadata: &fs::Metadata,
) -> Result<(), LaunchAgentStorageOwnershipError> {
    Err(LaunchAgentStorageOwnershipError::UnsupportedPlatform)
}

#[cfg(target_os = "macos")]
fn validate_file_metadata(metadata: &fs::Metadata) -> Result<(), LaunchAgentStorageOwnershipError> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    if !metadata.is_file() {
        return Err(LaunchAgentStorageOwnershipError::InvalidType);
    }
    if metadata.permissions().mode() & 0o7777 != 0o600 {
        return Err(LaunchAgentStorageOwnershipError::InsecurePermissions);
    }
    if metadata.nlink() != 1 {
        return Err(LaunchAgentStorageOwnershipError::Hardlink);
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn validate_file_metadata(
    _metadata: &fs::Metadata,
) -> Result<(), LaunchAgentStorageOwnershipError> {
    Err(LaunchAgentStorageOwnershipError::UnsupportedPlatform)
}

fn validate_named_file(path: &Path, file: &File) -> Result<(), LaunchAgentStorageOwnershipError> {
    let named = fs::symlink_metadata(path)?;
    validate_named_file_metadata(&named)?;
    if !same_identity(&file.metadata()?, &named) {
        return Err(LaunchAgentStorageOwnershipError::Replaced);
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn read_bounded(file: &File, maximum: u64) -> Result<Vec<u8>, LaunchAgentStorageOwnershipError> {
    use std::os::unix::fs::{FileExt, MetadataExt};

    let before = file.metadata()?;
    validate_file_metadata(&before)?;
    if before.len() > maximum {
        return Err(LaunchAgentStorageOwnershipError::InvalidContent);
    }
    let capacity = usize::try_from(maximum)
        .map_err(|_| LaunchAgentStorageOwnershipError::InvalidContent)?
        .checked_add(1)
        .ok_or(LaunchAgentStorageOwnershipError::InvalidContent)?;
    let mut bytes = vec![0_u8; capacity];
    let mut offset = 0_usize;
    while offset < bytes.len() {
        let read = file.read_at(&mut bytes[offset..], offset as u64)?;
        if read == 0 {
            break;
        }
        offset = offset
            .checked_add(read)
            .ok_or(LaunchAgentStorageOwnershipError::InvalidContent)?;
    }
    if u64::try_from(offset).unwrap_or(u64::MAX) > maximum {
        return Err(LaunchAgentStorageOwnershipError::InvalidContent);
    }
    bytes.truncate(offset);
    let after = file.metadata()?;
    validate_file_metadata(&after)?;
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
        return Err(LaunchAgentStorageOwnershipError::Replaced);
    }
    Ok(bytes)
}

#[cfg(not(target_os = "macos"))]
fn read_bounded(_file: &File, _maximum: u64) -> Result<Vec<u8>, LaunchAgentStorageOwnershipError> {
    Err(LaunchAgentStorageOwnershipError::UnsupportedPlatform)
}

#[cfg(target_os = "macos")]
fn same_identity(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    left.dev() == right.dev() && left.ino() == right.ino()
}

#[cfg(not(target_os = "macos"))]
fn same_identity(_left: &fs::Metadata, _right: &fs::Metadata) -> bool {
    false
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use crate::{
        LAUNCH_AGENT_OWNERSHIP_VERSION, LaunchAgentFileState, LaunchAgentOperation,
        LaunchAgentPhase, LaunchAgentTransaction,
    };
    use std::{
        fs,
        os::unix::fs::{FileTypeExt, PermissionsExt, symlink},
        path::{Path, PathBuf},
        process::Command,
        sync::atomic::{AtomicU64, Ordering},
    };

    struct Fixture {
        base: PathBuf,
        root: PathBuf,
        home: PathBuf,
    }

    impl Fixture {
        fn new(complete: bool) -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let base = std::env::temp_dir().join(format!(
                "launch-agent-storage-owner-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            assert!(!base.exists());
            let root = base.join("root");
            let terminal = if complete {
                root.join("runtime/integrations/codex/lifecycle")
            } else {
                root.join("runtime/integrations")
            };
            fs::create_dir_all(&terminal).unwrap();
            for relative in DIRECTORY_RELATIVE_PATHS {
                let path = root.join(relative);
                if path.exists() {
                    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
                }
            }
            fs::set_permissions(&base, fs::Permissions::from_mode(0o700)).unwrap();
            Self {
                home: base.join("home"),
                base,
                root,
            }
        }

        fn ownership(&self) -> PathBuf {
            self.root.join(OWNERSHIP_RELATIVE_PATH)
        }

        fn lock(&self) -> PathBuf {
            self.root.join(LIFECYCLE_LOCK_RELATIVE_PATH)
        }

        fn expected_plist(&self) -> PathBuf {
            crate::expected_launch_agent_plist_path(&self.root, &self.home)
        }

        fn transaction(&self, phase: LaunchAgentPhase) -> LaunchAgentTransaction {
            let missing = LaunchAgentFileState {
                existed: false,
                bytes: Vec::new(),
                mode: 0,
            };
            LaunchAgentTransaction {
                schema_version: LAUNCH_AGENT_OWNERSHIP_VERSION.into(),
                plist_path: self.expected_plist(),
                prior_plist: missing.clone(),
                prior_loaded: false,
                rollback_plist: missing,
                rollback_loaded: false,
                desired_plist: LaunchAgentFileState {
                    existed: true,
                    bytes: b"managed plist".to_vec(),
                    mode: 0o644,
                },
                desired_loaded: true,
                operation: LaunchAgentOperation::Connect,
                phase,
            }
        }

        fn write_private(path: &Path, bytes: &[u8]) {
            fs::write(path, bytes).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
        }

        fn write_transaction(&self, phase: LaunchAgentPhase) -> Vec<u8> {
            let bytes = serde_json::to_vec(&self.transaction(phase)).unwrap();
            Self::write_private(&self.ownership(), &bytes);
            bytes
        }

        fn write_lock(&self) {
            Self::write_private(&self.lock(), b"");
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.base).unwrap();
        }
    }

    #[test]
    fn missing_state_is_noncreating_and_unknown_siblings_are_unowned() {
        let fixture = Fixture::new(false);
        let before = fs::read_dir(fixture.root.join("runtime/integrations"))
            .unwrap()
            .count();
        let evidence =
            LaunchAgentStorageOwnershipEvidence::capture(&fixture.root, &fixture.home).unwrap();
        assert_eq!(
            fs::read_dir(fixture.root.join("runtime/integrations"))
                .unwrap()
                .count(),
            before
        );
        assert!(!fixture.root.join("runtime/integrations/codex").exists());
        let root = File::open(&fixture.root).unwrap();
        assert!(!evidence.matches_entry(Path::new("unknown"), &root).unwrap());
        assert!(
            !evidence
                .matches_entry(Path::new("runtime/integrations/codex/child"), &root)
                .unwrap()
        );
        assert_eq!(
            evidence
                .matches_entry(Path::new(LIFECYCLE_LOCK_RELATIVE_PATH), &root)
                .unwrap_err(),
            LaunchAgentStorageOwnershipError::Replaced
        );
        evidence.revalidate().unwrap();
    }

    #[test]
    fn exact_valid_phases_and_empty_lock_are_owned() {
        for phase in [
            LaunchAgentPhase::Prepared,
            LaunchAgentPhase::ServiceStopped,
            LaunchAgentPhase::PlistWritten,
            LaunchAgentPhase::Bootstrapped,
            LaunchAgentPhase::Applied,
            LaunchAgentPhase::Owned,
            LaunchAgentPhase::Restored,
        ] {
            let fixture = Fixture::new(true);
            fixture.write_transaction(phase);
            fixture.write_lock();
            let evidence =
                LaunchAgentStorageOwnershipEvidence::capture(&fixture.root, &fixture.home).unwrap();
            for relative in DIRECTORY_RELATIVE_PATHS.into_iter().skip(1) {
                let directory = File::open(fixture.root.join(relative)).unwrap();
                assert!(
                    evidence
                        .matches_entry(Path::new(relative), &directory)
                        .unwrap()
                );
            }
            for relative in [OWNERSHIP_RELATIVE_PATH, LIFECYCLE_LOCK_RELATIVE_PATH] {
                let file = File::open(fixture.root.join(relative)).unwrap();
                assert!(evidence.matches_entry(Path::new(relative), &file).unwrap());
            }
            assert_eq!(
                format!("{evidence:?}"),
                "LaunchAgentStorageOwnershipEvidence { .. }"
            );
            evidence.revalidate().unwrap();
        }
    }

    #[test]
    fn malformed_schema_file_state_and_external_path_fail_closed() {
        for scenario in 0..4 {
            let fixture = Fixture::new(true);
            let mut transaction = fixture.transaction(LaunchAgentPhase::Owned);
            match scenario {
                0 => Fixture::write_private(&fixture.ownership(), b"not-json"),
                1 => transaction.schema_version = "wrong".into(),
                2 => {
                    transaction.prior_plist = LaunchAgentFileState {
                        existed: false,
                        bytes: b"invalid".to_vec(),
                        mode: 0,
                    }
                }
                3 => transaction.plist_path.push("wrong"),
                _ => unreachable!(),
            }
            if scenario != 0 {
                Fixture::write_private(
                    &fixture.ownership(),
                    &serde_json::to_vec(&transaction).unwrap(),
                );
            }
            assert_eq!(
                LaunchAgentStorageOwnershipEvidence::capture(&fixture.root, &fixture.home)
                    .unwrap_err(),
                LaunchAgentStorageOwnershipError::InvalidContent
            );
        }
    }

    #[test]
    fn aliases_fifo_permissions_and_nonempty_lock_fail_closed() {
        for scenario in 0..6 {
            let fixture = Fixture::new(true);
            if scenario == 2 {
                assert!(
                    Command::new("mkfifo")
                        .arg(fixture.ownership())
                        .status()
                        .unwrap()
                        .success()
                );
            } else if scenario == 3 {
                let target = fixture.ownership().with_extension("target");
                Fixture::write_private(&target, b"target");
                symlink(target, fixture.ownership()).unwrap();
            } else if scenario == 5 {
                fs::set_permissions(
                    fixture.root.join("runtime/integrations/codex/lifecycle"),
                    fs::Permissions::from_mode(0o755),
                )
                .unwrap();
            } else {
                fixture.write_transaction(LaunchAgentPhase::Owned);
                match scenario {
                    0 => fs::hard_link(
                        fixture.ownership(),
                        fixture.ownership().with_extension("alias"),
                    )
                    .unwrap(),
                    1 => {
                        fs::set_permissions(fixture.ownership(), fs::Permissions::from_mode(0o644))
                            .unwrap();
                    }
                    4 => Fixture::write_private(&fixture.lock(), b"not-empty"),
                    _ => unreachable!(),
                }
            }
            let expected = match scenario {
                0 => LaunchAgentStorageOwnershipError::Hardlink,
                1 | 5 => LaunchAgentStorageOwnershipError::InsecurePermissions,
                2 => LaunchAgentStorageOwnershipError::InvalidType,
                3 => LaunchAgentStorageOwnershipError::Symlink,
                4 => LaunchAgentStorageOwnershipError::InvalidContent,
                _ => unreachable!(),
            };
            assert_eq!(
                LaunchAgentStorageOwnershipEvidence::capture(&fixture.root, &fixture.home)
                    .unwrap_err(),
                expected
            );
        }
    }

    #[test]
    fn replacement_and_wrong_candidate_identity_fail_closed() {
        let fixture = Fixture::new(true);
        fixture.write_transaction(LaunchAgentPhase::Owned);
        fixture.write_lock();
        let evidence =
            LaunchAgentStorageOwnershipEvidence::capture(&fixture.root, &fixture.home).unwrap();
        let wrong = File::open(fixture.lock()).unwrap();
        assert_eq!(
            evidence
                .matches_entry(Path::new(OWNERSHIP_RELATIVE_PATH), &wrong)
                .unwrap_err(),
            LaunchAgentStorageOwnershipError::Replaced
        );
        fs::rename(
            fixture.ownership(),
            fixture.ownership().with_extension("old"),
        )
        .unwrap();
        fixture.write_transaction(LaunchAgentPhase::Prepared);
        assert_eq!(
            evidence.revalidate().unwrap_err(),
            LaunchAgentStorageOwnershipError::Replaced
        );
    }

    #[test]
    fn in_place_transaction_edit_and_ancestor_replacement_fail_closed() {
        let fixture = Fixture::new(true);
        fixture.write_transaction(LaunchAgentPhase::Owned);
        let evidence =
            LaunchAgentStorageOwnershipEvidence::capture(&fixture.root, &fixture.home).unwrap();
        fixture.write_transaction(LaunchAgentPhase::Prepared);
        assert_eq!(
            evidence.revalidate().unwrap_err(),
            LaunchAgentStorageOwnershipError::Replaced
        );

        let ancestor = Fixture::new(true);
        let evidence =
            LaunchAgentStorageOwnershipEvidence::capture(&ancestor.root, &ancestor.home).unwrap();
        let integrations = ancestor.root.join("runtime/integrations");
        fs::rename(
            &integrations,
            ancestor.root.join("runtime/integrations-old"),
        )
        .unwrap();
        fs::create_dir(&integrations).unwrap();
        fs::set_permissions(&integrations, fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(
            evidence.revalidate().unwrap_err(),
            LaunchAgentStorageOwnershipError::Replaced
        );
    }

    #[test]
    fn root_and_home_mismatch_are_rejected() {
        let fixture = Fixture::new(false);
        assert_eq!(
            LaunchAgentStorageOwnershipEvidence::capture(Path::new("relative"), &fixture.home)
                .unwrap_err(),
            LaunchAgentStorageOwnershipError::RootMismatch
        );
        assert_eq!(
            LaunchAgentStorageOwnershipEvidence::capture(&fixture.root, Path::new("relative"))
                .unwrap_err(),
            LaunchAgentStorageOwnershipError::RootMismatch
        );
    }

    #[test]
    fn external_plist_is_never_read_or_changed() {
        let fixture = Fixture::new(true);
        let plist = fixture.expected_plist();
        fs::create_dir_all(plist.parent().unwrap()).unwrap();
        assert!(
            Command::new("mkfifo")
                .arg(&plist)
                .status()
                .unwrap()
                .success()
        );
        fs::set_permissions(&plist, fs::Permissions::from_mode(0o000)).unwrap();
        let transaction = fixture.write_transaction(LaunchAgentPhase::Prepared);
        let evidence =
            LaunchAgentStorageOwnershipEvidence::capture(&fixture.root, &fixture.home).unwrap();
        evidence.revalidate().unwrap();
        assert_eq!(fs::read(fixture.ownership()).unwrap(), transaction);
        assert!(fs::symlink_metadata(plist).unwrap().file_type().is_fifo());
    }

    #[test]
    fn snapshot_bound_is_enforced() {
        let fixture = Fixture::new(true);
        Fixture::write_private(
            &fixture.ownership(),
            &vec![b'x'; usize::try_from(crate::MAX_LAUNCH_AGENT_OWNERSHIP_BYTES).unwrap() + 1],
        );
        assert_eq!(
            LaunchAgentStorageOwnershipEvidence::capture(&fixture.root, &fixture.home).unwrap_err(),
            LaunchAgentStorageOwnershipError::InvalidContent
        );
    }
}
