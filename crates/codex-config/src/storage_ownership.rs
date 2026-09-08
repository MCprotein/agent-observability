//! Bounded, read-only ownership evidence for the private Codex config snapshot.

use super::{
    CodexConfigManager, ConfigError, MAX_SNAPSHOT_BYTES, OwnershipSnapshot, hash,
    validate_private_dir_metadata, validate_private_file,
};
use std::{
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
};

const SNAPSHOT_RELATIVE_PATH: &str = "runtime/integrations/codex/codex-config-ownership-v1.json";
const STATE_RELATIVE_PATH: &str = "runtime/integrations/codex";
const DIRECTORY_RELATIVE_PATHS: [&str; 4] =
    ["", "runtime", "runtime/integrations", STATE_RELATIVE_PATH];

/// Retained evidence for the exact Codex config ownership snapshot.
///
/// The snapshot bytes and bound external config path remain private. Callers must retain their
/// outer storage freeze through capture, matching, final revalidation, and accounting commit.
/// This evidence does not read or repair the external config and does not activate admission.
pub struct CodexConfigSnapshotOwnershipEvidence {
    root: PathBuf,
    config_path: PathBuf,
    directories: Vec<RetainedDirectory>,
    snapshot: Option<RetainedSnapshot>,
}

struct RetainedDirectory {
    relative: PathBuf,
    file: Option<File>,
}

struct RetainedSnapshot {
    file: File,
    hash: String,
}

impl std::fmt::Debug for CodexConfigSnapshotOwnershipEvidence {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CodexConfigSnapshotOwnershipEvidence")
            .finish_non_exhaustive()
    }
}

/// Failure to establish or retain bounded snapshot ownership evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodexConfigSnapshotOwnershipError {
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

impl std::fmt::Display for CodexConfigSnapshotOwnershipError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Io(_) => "Codex config snapshot ownership I/O failure",
            Self::InvalidContent => "Codex config snapshot ownership validation failed",
            Self::RootMismatch => "Codex config snapshot ownership root mismatch",
            Self::Replaced => "Codex config snapshot ownership identity changed",
            Self::InsecurePermissions => "Codex config snapshot ownership path is not private",
            Self::Symlink => "Codex config snapshot ownership refuses symbolic links",
            Self::Hardlink => "Codex config snapshot ownership refuses hard-linked files",
            Self::InvalidType => "Codex config snapshot ownership path has the wrong file type",
            Self::UnsupportedPlatform => "Codex config snapshot ownership is unsupported",
        })
    }
}

impl std::error::Error for CodexConfigSnapshotOwnershipError {}

impl From<io::Error> for CodexConfigSnapshotOwnershipError {
    fn from(error: io::Error) -> Self {
        if error.raw_os_error() == Some(libc::ELOOP) {
            Self::Symlink
        } else {
            Self::Io(error.kind())
        }
    }
}

impl CodexConfigSnapshotOwnershipEvidence {
    /// Captures the exact private snapshot, including bounded absence, without creating paths.
    ///
    /// The manager state directory must be exactly
    /// `<root>/runtime/integrations/codex`. The manager's config path is used only to validate the
    /// snapshot's recorded authority; the external config itself is never opened.
    ///
    /// # Errors
    ///
    /// Returns an error for a mismatched root, unsafe path, malformed snapshot, oversized content,
    /// or any identity change during capture.
    pub fn capture(
        root: &Path,
        manager: &CodexConfigManager,
    ) -> Result<Self, CodexConfigSnapshotOwnershipError> {
        capture(root, manager)
    }

    /// Matches only the exact retained directory paths and snapshot identity.
    ///
    /// Descendants and unknown siblings return `false`. A candidate for an exact retained path
    /// that was absent at capture, or whose identity differs, fails closed.
    ///
    /// # Errors
    ///
    /// Returns an error when the candidate is unsafe or does not have the captured identity.
    pub fn matches_entry(
        &self,
        relative: &Path,
        candidate: &File,
    ) -> Result<bool, CodexConfigSnapshotOwnershipError> {
        if relative == Path::new(SNAPSHOT_RELATIVE_PATH) {
            let Some(snapshot) = &self.snapshot else {
                return Err(CodexConfigSnapshotOwnershipError::Replaced);
            };
            let candidate_metadata = candidate.metadata()?;
            validate_snapshot_metadata(&candidate_metadata)?;
            if !same_identity(&snapshot.file.metadata()?, &candidate_metadata) {
                return Err(CodexConfigSnapshotOwnershipError::Replaced);
            }
            return Ok(true);
        }

        let Some(directory) = self.directories.iter().find(|directory| {
            !directory.relative.as_os_str().is_empty() && directory.relative == relative
        }) else {
            return Ok(false);
        };
        let Some(expected) = &directory.file else {
            return Err(CodexConfigSnapshotOwnershipError::Replaced);
        };
        let candidate_metadata = candidate.metadata()?;
        validate_directory_metadata(&self.root.join(relative), &candidate_metadata)?;
        if !same_identity(&expected.metadata()?, &candidate_metadata) {
            return Err(CodexConfigSnapshotOwnershipError::Replaced);
        }
        Ok(true)
    }

    /// Revalidates retained presence, names, identities, permissions, bytes, and snapshot schema.
    ///
    /// # Errors
    ///
    /// Returns an error when any captured path or bounded snapshot fact changed.
    pub fn revalidate(&self) -> Result<(), CodexConfigSnapshotOwnershipError> {
        self.revalidate_directories()?;
        revalidate_snapshot(
            &self.root.join(SNAPSHOT_RELATIVE_PATH),
            self.snapshot.as_ref(),
            &self.config_path,
            &self.root.join(STATE_RELATIVE_PATH),
        )?;
        self.revalidate_directories()
    }

    fn revalidate_directories(&self) -> Result<(), CodexConfigSnapshotOwnershipError> {
        for retained in &self.directories {
            revalidate_optional_directory(
                &self.root.join(&retained.relative),
                retained.file.as_ref(),
            )?;
        }
        Ok(())
    }
}

#[cfg(any(target_os = "linux", target_os = "android", target_os = "macos"))]
fn capture(
    root: &Path,
    manager: &CodexConfigManager,
) -> Result<CodexConfigSnapshotOwnershipEvidence, CodexConfigSnapshotOwnershipError> {
    if !root.is_absolute() || manager.state_dir != root.join(STATE_RELATIVE_PATH) {
        return Err(CodexConfigSnapshotOwnershipError::RootMismatch);
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

    let snapshot = capture_optional_snapshot(&root.join(SNAPSHOT_RELATIVE_PATH), manager)?;
    let evidence = CodexConfigSnapshotOwnershipEvidence {
        root: root.to_path_buf(),
        config_path: manager.config_path.clone(),
        directories,
        snapshot,
    };
    evidence.revalidate()?;
    Ok(evidence)
}

#[cfg(not(any(target_os = "linux", target_os = "android", target_os = "macos")))]
fn capture(
    _root: &Path,
    _manager: &CodexConfigManager,
) -> Result<CodexConfigSnapshotOwnershipEvidence, CodexConfigSnapshotOwnershipError> {
    Err(CodexConfigSnapshotOwnershipError::UnsupportedPlatform)
}

fn capture_optional_directory(
    path: &Path,
) -> Result<Option<File>, CodexConfigSnapshotOwnershipError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            validate_directory_metadata(path, &metadata)?;
            Ok(Some(open_directory(path)?))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn capture_optional_snapshot(
    path: &Path,
    manager: &CodexConfigManager,
) -> Result<Option<RetainedSnapshot>, CodexConfigSnapshotOwnershipError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            validate_snapshot_metadata_named(path, &metadata)?;
            let file = open_file(path)?;
            let bytes = read_bounded(&file, MAX_SNAPSHOT_BYTES)?;
            validate_snapshot_bytes(&bytes, manager)?;
            validate_named_file(path, &file)?;
            Ok(Some(RetainedSnapshot {
                file,
                hash: hash(&bytes),
            }))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn revalidate_optional_directory(
    path: &Path,
    expected: Option<&File>,
) -> Result<(), CodexConfigSnapshotOwnershipError> {
    let Some(expected) = expected else {
        return match fs::symlink_metadata(path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Ok(_) => Err(CodexConfigSnapshotOwnershipError::Replaced),
            Err(error) => Err(error.into()),
        };
    };
    let current = open_directory(path)?;
    if !same_identity(&expected.metadata()?, &current.metadata()?) {
        return Err(CodexConfigSnapshotOwnershipError::Replaced);
    }
    Ok(())
}

fn revalidate_snapshot(
    path: &Path,
    expected: Option<&RetainedSnapshot>,
    config_path: &Path,
    state_dir: &Path,
) -> Result<(), CodexConfigSnapshotOwnershipError> {
    let Some(expected) = expected else {
        return match fs::symlink_metadata(path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Ok(_) => Err(CodexConfigSnapshotOwnershipError::Replaced),
            Err(error) => Err(error.into()),
        };
    };
    let current = open_file(path)?;
    let bytes = read_bounded(&current, MAX_SNAPSHOT_BYTES)?;
    if !same_identity(&expected.file.metadata()?, &current.metadata()?)
        || hash(&bytes) != expected.hash
    {
        return Err(CodexConfigSnapshotOwnershipError::Replaced);
    }
    let manager = CodexConfigManager::from_ownership_snapshot(config_path, state_dir);
    validate_snapshot_bytes(&bytes, &manager)?;
    validate_named_file(path, &current)
}

fn validate_snapshot_bytes(
    bytes: &[u8],
    manager: &CodexConfigManager,
) -> Result<(), CodexConfigSnapshotOwnershipError> {
    let snapshot: OwnershipSnapshot = serde_json::from_slice(bytes)
        .map_err(|_| CodexConfigSnapshotOwnershipError::InvalidContent)?;
    snapshot
        .validate_path_and_prior(manager)
        .map_err(map_snapshot_error)
}

fn map_snapshot_error(_error: ConfigError) -> CodexConfigSnapshotOwnershipError {
    CodexConfigSnapshotOwnershipError::InvalidContent
}

#[cfg(any(target_os = "linux", target_os = "android", target_os = "macos"))]
fn open_directory(path: &Path) -> Result<File, CodexConfigSnapshotOwnershipError> {
    use std::os::unix::fs::OpenOptionsExt;

    let metadata = fs::symlink_metadata(path)?;
    validate_directory_metadata(path, &metadata)?;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let opened = file.metadata()?;
    validate_directory_metadata(path, &opened)?;
    if !same_identity(&metadata, &opened) {
        return Err(CodexConfigSnapshotOwnershipError::Replaced);
    }
    Ok(file)
}

#[cfg(not(any(target_os = "linux", target_os = "android", target_os = "macos")))]
fn open_directory(_path: &Path) -> Result<File, CodexConfigSnapshotOwnershipError> {
    Err(CodexConfigSnapshotOwnershipError::UnsupportedPlatform)
}

#[cfg(any(target_os = "linux", target_os = "android", target_os = "macos"))]
fn open_file(path: &Path) -> Result<File, CodexConfigSnapshotOwnershipError> {
    use std::os::unix::fs::OpenOptionsExt;

    let metadata = fs::symlink_metadata(path)?;
    validate_snapshot_metadata_named(path, &metadata)?;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let opened = file.metadata()?;
    validate_snapshot_metadata(&opened)?;
    if !same_identity(&metadata, &opened) {
        return Err(CodexConfigSnapshotOwnershipError::Replaced);
    }
    Ok(file)
}

#[cfg(not(any(target_os = "linux", target_os = "android", target_os = "macos")))]
fn open_file(_path: &Path) -> Result<File, CodexConfigSnapshotOwnershipError> {
    Err(CodexConfigSnapshotOwnershipError::UnsupportedPlatform)
}

fn validate_directory_metadata(
    path: &Path,
    metadata: &fs::Metadata,
) -> Result<(), CodexConfigSnapshotOwnershipError> {
    if metadata.file_type().is_symlink() {
        return Err(CodexConfigSnapshotOwnershipError::Symlink);
    }
    validate_private_dir_metadata(path, metadata).map_err(|error| match error {
        ConfigError::InsecurePermissions(_) => {
            CodexConfigSnapshotOwnershipError::InsecurePermissions
        }
        ConfigError::NonRegularFile(_) => CodexConfigSnapshotOwnershipError::InvalidType,
        _ => CodexConfigSnapshotOwnershipError::InvalidContent,
    })
}

fn validate_snapshot_metadata_named(
    path: &Path,
    metadata: &fs::Metadata,
) -> Result<(), CodexConfigSnapshotOwnershipError> {
    if metadata.file_type().is_symlink() {
        return Err(CodexConfigSnapshotOwnershipError::Symlink);
    }
    validate_private_file(path, metadata).map_err(|error| match error {
        ConfigError::InsecurePermissions(_) => {
            CodexConfigSnapshotOwnershipError::InsecurePermissions
        }
        ConfigError::NonRegularFile(_) => CodexConfigSnapshotOwnershipError::InvalidType,
        _ => CodexConfigSnapshotOwnershipError::InvalidContent,
    })?;
    validate_snapshot_metadata(metadata)
}

#[cfg(any(target_os = "linux", target_os = "android", target_os = "macos"))]
fn validate_snapshot_metadata(
    metadata: &fs::Metadata,
) -> Result<(), CodexConfigSnapshotOwnershipError> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    if !metadata.is_file() {
        return Err(CodexConfigSnapshotOwnershipError::InvalidType);
    }
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(CodexConfigSnapshotOwnershipError::InsecurePermissions);
    }
    if metadata.nlink() != 1 {
        return Err(CodexConfigSnapshotOwnershipError::Hardlink);
    }
    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "android", target_os = "macos")))]
fn validate_snapshot_metadata(
    _metadata: &fs::Metadata,
) -> Result<(), CodexConfigSnapshotOwnershipError> {
    Err(CodexConfigSnapshotOwnershipError::UnsupportedPlatform)
}

fn validate_named_file(path: &Path, file: &File) -> Result<(), CodexConfigSnapshotOwnershipError> {
    let named = fs::symlink_metadata(path)?;
    validate_snapshot_metadata_named(path, &named)?;
    if !same_identity(&file.metadata()?, &named) {
        return Err(CodexConfigSnapshotOwnershipError::Replaced);
    }
    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "android", target_os = "macos"))]
fn read_bounded(file: &File, maximum: u64) -> Result<Vec<u8>, CodexConfigSnapshotOwnershipError> {
    use std::os::unix::fs::{FileExt, MetadataExt};

    let before = file.metadata()?;
    validate_snapshot_metadata(&before)?;
    if before.len() > maximum {
        return Err(CodexConfigSnapshotOwnershipError::InvalidContent);
    }
    let capacity = usize::try_from(maximum)
        .map_err(|_| CodexConfigSnapshotOwnershipError::InvalidContent)?
        .checked_add(1)
        .ok_or(CodexConfigSnapshotOwnershipError::InvalidContent)?;
    let mut bytes = vec![0_u8; capacity];
    let mut offset = 0_usize;
    while offset < bytes.len() {
        let read = file.read_at(&mut bytes[offset..], offset as u64)?;
        if read == 0 {
            break;
        }
        offset = offset
            .checked_add(read)
            .ok_or(CodexConfigSnapshotOwnershipError::InvalidContent)?;
    }
    if u64::try_from(offset).unwrap_or(u64::MAX) > maximum {
        return Err(CodexConfigSnapshotOwnershipError::InvalidContent);
    }
    bytes.truncate(offset);
    let after = file.metadata()?;
    validate_snapshot_metadata(&after)?;
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
        return Err(CodexConfigSnapshotOwnershipError::Replaced);
    }
    Ok(bytes)
}

#[cfg(not(any(target_os = "linux", target_os = "android", target_os = "macos")))]
fn read_bounded(_file: &File, _maximum: u64) -> Result<Vec<u8>, CodexConfigSnapshotOwnershipError> {
    Err(CodexConfigSnapshotOwnershipError::UnsupportedPlatform)
}

#[cfg(any(target_os = "linux", target_os = "android", target_os = "macos"))]
fn same_identity(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    left.dev() == right.dev() && left.ino() == right.ino()
}

#[cfg(not(any(target_os = "linux", target_os = "android", target_os = "macos")))]
fn same_identity(_left: &fs::Metadata, _right: &fs::Metadata) -> bool {
    false
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::{PendingRotation, SNAPSHOT_VERSION, SnapshotPhase, hash, hex_encode};
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
        process::Command,
        sync::atomic::{AtomicU64, Ordering},
    };

    struct Fixture {
        base: PathBuf,
        root: PathBuf,
        manager: CodexConfigManager,
    }

    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let base = std::env::temp_dir().join(format!(
                "codex-config-snapshot-owner-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            assert!(!base.exists());
            let root = base.join("agent-observability");
            let state_dir = root.join(STATE_RELATIVE_PATH);
            fs::create_dir_all(&state_dir).unwrap();
            for directory in [
                &base,
                &root,
                &root.join("runtime"),
                &root.join("runtime/integrations"),
                &state_dir,
            ] {
                fs::set_permissions(directory, fs::Permissions::from_mode(0o700)).unwrap();
            }
            let manager = CodexConfigManager::from_ownership_snapshot(
                base.join("external-codex/config.toml"),
                state_dir,
            );
            Self {
                base,
                root,
                manager,
            }
        }

        fn snapshot_path(&self) -> PathBuf {
            self.root.join(SNAPSHOT_RELATIVE_PATH)
        }

        fn snapshot(&self, phase: SnapshotPhase) -> OwnershipSnapshot {
            let prior = b"model = 'prior'\n";
            let connected = b"model = 'connected'\n";
            let pending = b"model = 'pending'\n";
            OwnershipSnapshot {
                schema_version: SNAPSHOT_VERSION.into(),
                config_path: self.manager.config_path.to_string_lossy().into_owned(),
                phase,
                prior_existed: true,
                prior_bytes_hex: hex_encode(prior),
                prior_hash: hash(prior),
                prior_mode: 0o600,
                connected_bytes_hex: hex_encode(connected),
                connected_hash: hash(connected),
                connected_mode: 0o600,
                owns_notify: true,
                managed_fingerprint: "managed".into(),
                pending_rotation: (phase == SnapshotPhase::Rotating).then(|| PendingRotation {
                    connected_bytes_hex: hex_encode(pending),
                    connected_hash: hash(pending),
                    connected_mode: 0o600,
                    managed_fingerprint: "pending".into(),
                }),
            }
        }

        fn write(&self, bytes: &[u8], mode: u32) {
            fs::write(self.snapshot_path(), bytes).unwrap();
            fs::set_permissions(self.snapshot_path(), fs::Permissions::from_mode(mode)).unwrap();
        }

        fn write_snapshot(&self, phase: SnapshotPhase) -> Vec<u8> {
            let bytes = self.snapshot(phase).to_bytes().unwrap();
            self.write(&bytes, 0o600);
            bytes
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.base).unwrap();
        }
    }

    #[test]
    fn missing_snapshot_and_missing_state_suffix_are_noncreating() {
        for remove_suffix in [false, true] {
            let fixture = Fixture::new();
            if remove_suffix {
                fs::remove_dir(&fixture.manager.state_dir).unwrap();
            }
            let evidence =
                CodexConfigSnapshotOwnershipEvidence::capture(&fixture.root, &fixture.manager)
                    .unwrap();
            assert!(!fixture.snapshot_path().exists());
            assert_eq!(fixture.manager.state_dir.exists(), !remove_suffix);
            evidence.revalidate().unwrap();
        }
    }

    #[test]
    fn valid_snapshot_phases_have_matching_read_only_evidence() {
        for phase in [
            SnapshotPhase::Prepared,
            SnapshotPhase::Connected,
            SnapshotPhase::Rotating,
            SnapshotPhase::Restoring,
        ] {
            let fixture = Fixture::new();
            let bytes = fixture.write_snapshot(phase);
            let evidence =
                CodexConfigSnapshotOwnershipEvidence::capture(&fixture.root, &fixture.manager)
                    .unwrap();
            let candidate = File::open(fixture.snapshot_path()).unwrap();
            assert!(
                evidence
                    .matches_entry(Path::new(SNAPSHOT_RELATIVE_PATH), &candidate)
                    .unwrap()
            );
            assert!(
                !evidence
                    .matches_entry(Path::new("runtime/integrations/codex/unknown"), &candidate)
                    .unwrap()
            );
            for relative in ["runtime", "runtime/integrations", STATE_RELATIVE_PATH] {
                let directory = File::open(fixture.root.join(relative)).unwrap();
                assert!(
                    evidence
                        .matches_entry(Path::new(relative), &directory)
                        .unwrap()
                );
            }
            evidence.revalidate().unwrap();
            assert_eq!(fs::read(fixture.snapshot_path()).unwrap(), bytes);
            assert_eq!(
                format!("{evidence:?}"),
                "CodexConfigSnapshotOwnershipEvidence { .. }"
            );
        }
    }

    #[test]
    fn malformed_hash_path_and_phase_fail_closed() {
        for scenario in 0..5 {
            let fixture = Fixture::new();
            let mut snapshot = fixture.snapshot(SnapshotPhase::Connected);
            match scenario {
                0 => fixture.write(b"not-json", 0o600),
                1 => snapshot.prior_hash = "bad".into(),
                2 => snapshot.connected_hash = "bad".into(),
                3 => snapshot.config_path.push_str(".other"),
                4 => snapshot.phase = SnapshotPhase::Rotating,
                _ => unreachable!(),
            }
            if scenario != 0 {
                fixture.write(&snapshot.to_bytes().unwrap(), 0o600);
            }
            assert_eq!(
                CodexConfigSnapshotOwnershipEvidence::capture(&fixture.root, &fixture.manager)
                    .unwrap_err(),
                CodexConfigSnapshotOwnershipError::InvalidContent
            );
        }
    }

    #[test]
    fn replacement_and_known_absence_fail_closed() {
        let fixture = Fixture::new();
        fixture.write_snapshot(SnapshotPhase::Connected);
        let evidence =
            CodexConfigSnapshotOwnershipEvidence::capture(&fixture.root, &fixture.manager).unwrap();
        let replacement = fixture
            .snapshot(SnapshotPhase::Prepared)
            .to_bytes()
            .unwrap();
        fs::rename(
            fixture.snapshot_path(),
            fixture.snapshot_path().with_extension("old"),
        )
        .unwrap();
        fixture.write(&replacement, 0o600);
        assert_eq!(
            evidence.revalidate().unwrap_err(),
            CodexConfigSnapshotOwnershipError::Replaced
        );
        let absent = Fixture::new();
        let evidence =
            CodexConfigSnapshotOwnershipEvidence::capture(&absent.root, &absent.manager).unwrap();
        absent.write_snapshot(SnapshotPhase::Connected);
        let candidate = File::open(absent.snapshot_path()).unwrap();
        assert_eq!(
            evidence
                .matches_entry(Path::new(SNAPSHOT_RELATIVE_PATH), &candidate)
                .unwrap_err(),
            CodexConfigSnapshotOwnershipError::Replaced
        );
        assert_eq!(
            evidence.revalidate().unwrap_err(),
            CodexConfigSnapshotOwnershipError::Replaced
        );
    }

    #[test]
    fn wrong_candidate_in_place_edit_and_ancestor_replacement_fail_closed() {
        let fixture = Fixture::new();
        fixture.write_snapshot(SnapshotPhase::Connected);
        let evidence =
            CodexConfigSnapshotOwnershipEvidence::capture(&fixture.root, &fixture.manager).unwrap();
        let other = fixture.root.join("other-private-file");
        fs::write(&other, b"other").unwrap();
        fs::set_permissions(&other, fs::Permissions::from_mode(0o600)).unwrap();
        let candidate = File::open(other).unwrap();
        assert_eq!(
            evidence
                .matches_entry(Path::new(SNAPSHOT_RELATIVE_PATH), &candidate)
                .unwrap_err(),
            CodexConfigSnapshotOwnershipError::Replaced
        );
        fixture.write_snapshot(SnapshotPhase::Prepared);
        assert_eq!(
            evidence.revalidate().unwrap_err(),
            CodexConfigSnapshotOwnershipError::Replaced
        );

        let ancestor = Fixture::new();
        let evidence =
            CodexConfigSnapshotOwnershipEvidence::capture(&ancestor.root, &ancestor.manager)
                .unwrap();
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
            CodexConfigSnapshotOwnershipError::Replaced
        );
    }

    #[test]
    fn directory_matching_rejects_aliases_types_and_captured_absence() {
        let fixture = Fixture::new();
        let evidence =
            CodexConfigSnapshotOwnershipEvidence::capture(&fixture.root, &fixture.manager).unwrap();
        let wrong_directory = File::open(&fixture.root).unwrap();
        assert_eq!(
            evidence
                .matches_entry(Path::new(STATE_RELATIVE_PATH), &wrong_directory)
                .unwrap_err(),
            CodexConfigSnapshotOwnershipError::Replaced
        );
        let regular = fixture.root.join("private-regular");
        fs::write(&regular, b"regular").unwrap();
        fs::set_permissions(&regular, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            evidence
                .matches_entry(
                    Path::new(STATE_RELATIVE_PATH),
                    &File::open(regular).unwrap()
                )
                .unwrap_err(),
            CodexConfigSnapshotOwnershipError::InvalidType
        );
        assert!(
            !evidence
                .matches_entry(
                    Path::new("runtime/integrations/codex/child"),
                    &wrong_directory,
                )
                .unwrap()
        );

        let absent = Fixture::new();
        fs::remove_dir(&absent.manager.state_dir).unwrap();
        let evidence =
            CodexConfigSnapshotOwnershipEvidence::capture(&absent.root, &absent.manager).unwrap();
        assert_eq!(
            evidence
                .matches_entry(Path::new(STATE_RELATIVE_PATH), &wrong_directory)
                .unwrap_err(),
            CodexConfigSnapshotOwnershipError::Replaced
        );
    }

    #[test]
    fn hardlink_fifo_and_public_permissions_are_rejected() {
        for scenario in 0..4 {
            let fixture = Fixture::new();
            if scenario == 1 {
                assert!(
                    Command::new("mkfifo")
                        .arg(fixture.snapshot_path())
                        .status()
                        .unwrap()
                        .success()
                );
            } else if scenario == 3 {
                let target = fixture.snapshot_path().with_extension("target");
                fs::write(&target, b"target").unwrap();
                std::os::unix::fs::symlink(target, fixture.snapshot_path()).unwrap();
            } else {
                fixture.write_snapshot(SnapshotPhase::Connected);
                if scenario == 0 {
                    fs::hard_link(
                        fixture.snapshot_path(),
                        fixture.snapshot_path().with_extension("link"),
                    )
                    .unwrap();
                } else {
                    fs::set_permissions(fixture.snapshot_path(), fs::Permissions::from_mode(0o644))
                        .unwrap();
                }
            }
            let expected = match scenario {
                0 => CodexConfigSnapshotOwnershipError::Hardlink,
                1 => CodexConfigSnapshotOwnershipError::InvalidType,
                2 => CodexConfigSnapshotOwnershipError::InsecurePermissions,
                3 => CodexConfigSnapshotOwnershipError::Symlink,
                _ => unreachable!(),
            };
            assert_eq!(
                CodexConfigSnapshotOwnershipEvidence::capture(&fixture.root, &fixture.manager)
                    .unwrap_err(),
                expected
            );
        }
    }

    #[test]
    fn root_mismatch_is_rejected_before_filesystem_access() {
        let fixture = Fixture::new();
        assert_eq!(
            CodexConfigSnapshotOwnershipEvidence::capture(
                &fixture.root.join("other"),
                &fixture.manager
            )
            .unwrap_err(),
            CodexConfigSnapshotOwnershipError::RootMismatch
        );
        assert_eq!(
            CodexConfigSnapshotOwnershipEvidence::capture(Path::new("relative"), &fixture.manager)
                .unwrap_err(),
            CodexConfigSnapshotOwnershipError::RootMismatch
        );
    }

    #[test]
    fn external_config_is_never_read_and_capture_has_no_side_effects() {
        let fixture = Fixture::new();
        fs::create_dir(fixture.manager.config_path.parent().unwrap()).unwrap();
        assert!(
            Command::new("mkfifo")
                .arg(&fixture.manager.config_path)
                .status()
                .unwrap()
                .success()
        );
        fs::set_permissions(
            &fixture.manager.config_path,
            fs::Permissions::from_mode(0o000),
        )
        .unwrap();
        let snapshot = fixture.write_snapshot(SnapshotPhase::Prepared);
        let before = fs::read_dir(&fixture.manager.state_dir).unwrap().count();
        let evidence =
            CodexConfigSnapshotOwnershipEvidence::capture(&fixture.root, &fixture.manager).unwrap();
        evidence.revalidate().unwrap();
        assert_eq!(fs::read(fixture.snapshot_path()).unwrap(), snapshot);
        assert_eq!(
            fs::read_dir(&fixture.manager.state_dir).unwrap().count(),
            before
        );
        assert!(
            !fixture
                .manager
                .config_path
                .parent()
                .unwrap()
                .join(super::super::LOCK_FILE)
                .exists()
        );
    }

    #[test]
    fn snapshot_size_limit_is_enforced() {
        let fixture = Fixture::new();
        let oversized = vec![b' '; usize::try_from(MAX_SNAPSHOT_BYTES).unwrap() + 1];
        fixture.write(&oversized, 0o600);
        assert_eq!(
            CodexConfigSnapshotOwnershipEvidence::capture(&fixture.root, &fixture.manager)
                .unwrap_err(),
            CodexConfigSnapshotOwnershipError::InvalidContent
        );
    }
}
