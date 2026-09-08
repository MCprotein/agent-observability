//! Read-only ownership evidence for exact collector runtime files.
//!
//! Callers must hold their outer storage freeze for the complete assessment.
//! This module validates only the collector's semantic ownership of the exact
//! named files; it does not by itself prove writer authority, establish
//! cross-writer coherence, or classify any descendant or sibling path.

use agent_observability_local_runtime::InstalledLayout;
mod tls;
use std::{
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
};
pub use tls::CollectorTlsOwnershipEvidence;

const SETTINGS_RELATIVE_PATH: &str = "runtime/collector.json";
const MIGRATION_RELATIVE_PATH: &str = "runtime/collector-settings-migration.json";
const REPORT_DIRTY_RELATIVE_PATH: &str = "runtime/report-dirty";
const REPORT_DIRTY_CONTENT: &[u8] = b"dirty\n";

/// Retained, read-only evidence for exact collector-owned runtime files.
///
/// Absence is retained as part of the observation. A later candidate at a
/// known path that was absent at capture is therefore an identity change, not
/// an unknown entry.
pub struct CollectorStorageOwnershipEvidence {
    runtime_path: PathBuf,
    runtime: File,
    settings: Option<File>,
    migration: Option<File>,
    report_dirty: Option<File>,
}

impl std::fmt::Debug for CollectorStorageOwnershipEvidence {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CollectorStorageOwnershipEvidence")
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CollectorStorageOwnershipError {
    Io(io::ErrorKind),
    InvalidContent,
    Replaced,
    InsecurePermissions,
    Symlink,
    Hardlink,
    InvalidType,
    UnsupportedPlatform,
}

impl std::fmt::Display for CollectorStorageOwnershipError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Io(_) => "collector storage ownership I/O failure",
            Self::InvalidContent => "collector storage ownership validation failed",
            Self::Replaced => "collector storage ownership identity changed",
            Self::InsecurePermissions => "collector storage ownership path is not private",
            Self::Symlink => "collector storage ownership refuses symbolic links",
            Self::Hardlink => "collector storage ownership refuses hard-linked files",
            Self::InvalidType => "collector storage ownership path has the wrong file type",
            Self::UnsupportedPlatform => "collector storage ownership is unsupported",
        })
    }
}

impl std::error::Error for CollectorStorageOwnershipError {}

impl From<io::Error> for CollectorStorageOwnershipError {
    fn from(error: io::Error) -> Self {
        Self::Io(error.kind())
    }
}

#[derive(Clone, Copy)]
enum OwnedFileKind {
    Settings,
    Migration,
    ReportDirty,
}

impl CollectorStorageOwnershipEvidence {
    /// Capture exact collector runtime-file evidence without creating paths.
    ///
    /// The caller must keep its outer storage freeze held through capture,
    /// matching, revalidation, and the accounting commit that consumes this
    /// evidence.
    pub fn capture(layout: &InstalledLayout) -> Result<Self, CollectorStorageOwnershipError> {
        capture(layout)
    }

    /// Match only the three exact collector-owned runtime files.
    ///
    /// Unknown paths return `false`. A known path whose captured identity is
    /// absent or differs returns [`CollectorStorageOwnershipError::Replaced`].
    pub fn matches_entry(
        &self,
        relative: &Path,
        candidate: &File,
    ) -> Result<bool, CollectorStorageOwnershipError> {
        let expected = if relative == Path::new(SETTINGS_RELATIVE_PATH) {
            Some(self.settings.as_ref())
        } else if relative == Path::new(MIGRATION_RELATIVE_PATH) {
            Some(self.migration.as_ref())
        } else if relative == Path::new(REPORT_DIRTY_RELATIVE_PATH) {
            Some(self.report_dirty.as_ref())
        } else {
            None
        };
        let Some(expected) = expected else {
            return Ok(false);
        };
        let Some(expected) = expected else {
            return Err(CollectorStorageOwnershipError::Replaced);
        };
        let candidate_metadata = candidate.metadata()?;
        validate_file_metadata(&candidate_metadata)?;
        if !same_identity(&expected.metadata()?, &candidate_metadata) {
            return Err(CollectorStorageOwnershipError::Replaced);
        }
        Ok(true)
    }

    /// Revalidate exact names, identities, permissions, and bounded semantics.
    pub fn revalidate(&self) -> Result<(), CollectorStorageOwnershipError> {
        let runtime = open_directory(&self.runtime_path)?;
        if !same_identity(&self.runtime.metadata()?, &runtime.metadata()?) {
            return Err(CollectorStorageOwnershipError::Replaced);
        }
        revalidate_optional(
            &self.runtime_path.join("collector.json"),
            self.settings.as_ref(),
            OwnedFileKind::Settings,
        )?;
        revalidate_optional(
            &self.runtime_path.join("collector-settings-migration.json"),
            self.migration.as_ref(),
            OwnedFileKind::Migration,
        )?;
        revalidate_optional(
            &self.runtime_path.join(super::REPORT_DIRTY_FILE_NAME),
            self.report_dirty.as_ref(),
            OwnedFileKind::ReportDirty,
        )?;
        let runtime = open_directory(&self.runtime_path)?;
        if !same_identity(&self.runtime.metadata()?, &runtime.metadata()?) {
            return Err(CollectorStorageOwnershipError::Replaced);
        }
        Ok(())
    }
}

#[cfg(any(target_os = "linux", target_os = "android", target_os = "macos"))]
fn capture(
    layout: &InstalledLayout,
) -> Result<CollectorStorageOwnershipEvidence, CollectorStorageOwnershipError> {
    let runtime_path = layout.runtime.clone();
    let runtime = open_directory(&runtime_path)?;
    let settings = capture_optional(
        &runtime_path.join("collector.json"),
        OwnedFileKind::Settings,
    )?;
    let migration = capture_optional(
        &runtime_path.join("collector-settings-migration.json"),
        OwnedFileKind::Migration,
    )?;
    let report_dirty = capture_optional(
        &runtime_path.join(super::REPORT_DIRTY_FILE_NAME),
        OwnedFileKind::ReportDirty,
    )?;
    let evidence = CollectorStorageOwnershipEvidence {
        runtime_path,
        runtime,
        settings,
        migration,
        report_dirty,
    };
    evidence.revalidate()?;
    Ok(evidence)
}

#[cfg(not(any(target_os = "linux", target_os = "android", target_os = "macos")))]
fn capture(
    _layout: &InstalledLayout,
) -> Result<CollectorStorageOwnershipEvidence, CollectorStorageOwnershipError> {
    Err(CollectorStorageOwnershipError::UnsupportedPlatform)
}

fn capture_optional(
    path: &Path,
    kind: OwnedFileKind,
) -> Result<Option<File>, CollectorStorageOwnershipError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() {
                return Err(CollectorStorageOwnershipError::Symlink);
            }
            let file = open_file(path)?;
            validate_semantics(&file, kind)?;
            let named = fs::symlink_metadata(path)?;
            validate_file_metadata(&named)?;
            if !same_identity(&file.metadata()?, &named) {
                return Err(CollectorStorageOwnershipError::Replaced);
            }
            Ok(Some(file))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn revalidate_optional(
    path: &Path,
    expected: Option<&File>,
    kind: OwnedFileKind,
) -> Result<(), CollectorStorageOwnershipError> {
    let Some(expected) = expected else {
        return match fs::symlink_metadata(path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Ok(_) => Err(CollectorStorageOwnershipError::Replaced),
            Err(error) => Err(error.into()),
        };
    };
    let current = open_file(path)?;
    if !same_identity(&expected.metadata()?, &current.metadata()?) {
        return Err(CollectorStorageOwnershipError::Replaced);
    }
    validate_semantics(&current, kind)
}

#[cfg(any(target_os = "linux", target_os = "android", target_os = "macos"))]
fn open_directory(path: &Path) -> Result<File, CollectorStorageOwnershipError> {
    open_checked(path, true, || {})
}

#[cfg(not(any(target_os = "linux", target_os = "android", target_os = "macos")))]
fn open_directory(_path: &Path) -> Result<File, CollectorStorageOwnershipError> {
    Err(CollectorStorageOwnershipError::UnsupportedPlatform)
}

#[cfg(any(target_os = "linux", target_os = "android", target_os = "macos"))]
fn open_file(path: &Path) -> Result<File, CollectorStorageOwnershipError> {
    open_checked(path, false, || {})
}

#[cfg(any(target_os = "linux", target_os = "android", target_os = "macos"))]
fn open_checked(
    path: &Path,
    directory: bool,
    before_open: impl FnOnce(),
) -> Result<File, CollectorStorageOwnershipError> {
    use std::os::unix::fs::OpenOptionsExt;

    let validate = if directory {
        validate_directory_metadata
    } else {
        validate_file_metadata
    };
    let named = fs::symlink_metadata(path)?;
    if named.file_type().is_symlink() {
        return Err(CollectorStorageOwnershipError::Symlink);
    }
    validate(&named)?;
    before_open();
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(
            libc::O_NOFOLLOW | libc::O_NONBLOCK | if directory { libc::O_DIRECTORY } else { 0 },
        )
        .open(path)?;
    let held = file.metadata()?;
    validate(&held)?;
    if !same_identity(&named, &held) {
        return Err(CollectorStorageOwnershipError::Replaced);
    }
    Ok(file)
}

#[cfg(not(any(target_os = "linux", target_os = "android", target_os = "macos")))]
fn open_file(_path: &Path) -> Result<File, CollectorStorageOwnershipError> {
    Err(CollectorStorageOwnershipError::UnsupportedPlatform)
}

fn validate_semantics(
    file: &File,
    kind: OwnedFileKind,
) -> Result<(), CollectorStorageOwnershipError> {
    let maximum = match kind {
        OwnedFileKind::Settings => super::MAX_SETTINGS_BYTES,
        OwnedFileKind::Migration => super::MAX_SETTINGS_MIGRATION_BYTES,
        OwnedFileKind::ReportDirty => REPORT_DIRTY_CONTENT.len() as u64,
    };
    let bytes = read_bounded(file, maximum)?;
    match kind {
        OwnedFileKind::Settings => validate_settings_metadata(&bytes),
        OwnedFileKind::Migration => validate_migration(&bytes),
        OwnedFileKind::ReportDirty if bytes == REPORT_DIRTY_CONTENT => Ok(()),
        OwnedFileKind::ReportDirty => Err(CollectorStorageOwnershipError::InvalidContent),
    }
}

fn validate_settings_metadata(bytes: &[u8]) -> Result<(), CollectorStorageOwnershipError> {
    if super::parse_owned_settings(bytes).is_ok() {
        return Ok(());
    }
    if let Ok(legacy) = serde_json::from_slice::<super::LegacyCollectorSettingsV1>(bytes)
        && super::valid_legacy_v1_metadata(&legacy)
    {
        return Ok(());
    }
    let legacy: super::LegacyCollectorSettingsV2Mtls = serde_json::from_slice(bytes)
        .map_err(|_| CollectorStorageOwnershipError::InvalidContent)?;
    super::validate_legacy_v2_metadata(&legacy)
        .map_err(|_| CollectorStorageOwnershipError::InvalidContent)
}

fn validate_migration(bytes: &[u8]) -> Result<(), CollectorStorageOwnershipError> {
    let migration: super::SettingsMigrationV1 = serde_json::from_slice(bytes)
        .map_err(|_| CollectorStorageOwnershipError::InvalidContent)?;
    if migration.schema_version != super::SETTINGS_MIGRATION_VERSION
        || migration.previous_settings.len()
            > usize::try_from(super::MAX_SETTINGS_BYTES).expect("settings bound fits usize")
        || !super::valid_generation(&migration.replacement_generation)
        || migration
            .previous_generation
            .as_deref()
            .is_some_and(|generation| !super::valid_generation(generation))
        || migration.previous_mode & 0o077 != 0
    {
        return Err(CollectorStorageOwnershipError::InvalidContent);
    }
    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "android", target_os = "macos"))]
fn read_bounded(file: &File, maximum: u64) -> Result<Vec<u8>, CollectorStorageOwnershipError> {
    use std::os::unix::fs::{FileExt, MetadataExt};

    let before = file.metadata()?;
    validate_file_metadata(&before)?;
    if before.len() > maximum {
        return Err(CollectorStorageOwnershipError::InvalidContent);
    }
    let capacity = usize::try_from(maximum)
        .map_err(|_| CollectorStorageOwnershipError::InvalidContent)?
        .checked_add(1)
        .ok_or(CollectorStorageOwnershipError::InvalidContent)?;
    let mut bytes = vec![0_u8; capacity];
    let mut offset = 0_usize;
    while offset < bytes.len() {
        let read = file.read_at(&mut bytes[offset..], offset as u64)?;
        if read == 0 {
            break;
        }
        offset = offset
            .checked_add(read)
            .ok_or(CollectorStorageOwnershipError::InvalidContent)?;
    }
    if u64::try_from(offset).unwrap_or(u64::MAX) > maximum {
        return Err(CollectorStorageOwnershipError::InvalidContent);
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
        return Err(CollectorStorageOwnershipError::Replaced);
    }
    Ok(bytes)
}

#[cfg(not(any(target_os = "linux", target_os = "android", target_os = "macos")))]
fn read_bounded(_file: &File, _maximum: u64) -> Result<Vec<u8>, CollectorStorageOwnershipError> {
    Err(CollectorStorageOwnershipError::UnsupportedPlatform)
}

#[cfg(any(target_os = "linux", target_os = "android", target_os = "macos"))]
fn validate_directory_metadata(
    metadata: &fs::Metadata,
) -> Result<(), CollectorStorageOwnershipError> {
    use std::os::unix::fs::PermissionsExt;

    if !metadata.is_dir() {
        return Err(CollectorStorageOwnershipError::InvalidType);
    }
    if metadata.permissions().mode() & 0o7777 != 0o700 {
        return Err(CollectorStorageOwnershipError::InsecurePermissions);
    }
    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "android", target_os = "macos")))]
fn validate_directory_metadata(
    _metadata: &fs::Metadata,
) -> Result<(), CollectorStorageOwnershipError> {
    Err(CollectorStorageOwnershipError::UnsupportedPlatform)
}

#[cfg(any(target_os = "linux", target_os = "android", target_os = "macos"))]
fn validate_file_metadata(metadata: &fs::Metadata) -> Result<(), CollectorStorageOwnershipError> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    if !metadata.is_file() {
        return Err(CollectorStorageOwnershipError::InvalidType);
    }
    if metadata.permissions().mode() & 0o7777 != 0o600 {
        return Err(CollectorStorageOwnershipError::InsecurePermissions);
    }
    if metadata.nlink() != 1 {
        return Err(CollectorStorageOwnershipError::Hardlink);
    }
    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "android", target_os = "macos")))]
fn validate_file_metadata(_metadata: &fs::Metadata) -> Result<(), CollectorStorageOwnershipError> {
    Err(CollectorStorageOwnershipError::UnsupportedPlatform)
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

#[cfg(all(
    test,
    any(target_os = "linux", target_os = "android", target_os = "macos")
))]
mod tests {
    use super::*;
    use std::{
        fs,
        io::Write,
        sync::atomic::{AtomicU64, Ordering},
    };

    static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    fn fixture() -> (PathBuf, InstalledLayout) {
        let root = std::env::temp_dir().join(format!(
            "collector-storage-ownership-{}-{}",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        set_mode(&root, 0o700);
        let layout = InstalledLayout {
            root: root.clone(),
            config: root.join("config.json"),
            logs: root.join("logs"),
            queue: root.join("queue"),
            state: root.join("state"),
            runtime: root.join("runtime"),
        };
        fs::create_dir(&layout.runtime).unwrap();
        set_mode(&layout.runtime, 0o700);
        (root, layout)
    }

    fn write_private(path: &Path, bytes: &[u8]) {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)
            .unwrap();
        file.write_all(bytes).unwrap();
        drop(file);
        set_mode(path, 0o600);
    }

    fn settings_bytes() -> Vec<u8> {
        serde_json::to_vec(&super::super::CollectorSettings {
            schema_version: super::super::COLLECTOR_SETTINGS_VERSION.into(),
            generation: "a".repeat(64),
            port: 4318,
            transport: super::super::COLLECTOR_TRANSPORT.into(),
            auth_token: "b".repeat(64),
            credentials: super::super::CredentialMetadata {
                ca_certificate: format!(
                    "{}/{}/{}",
                    super::super::TLS_DIRECTORY,
                    "a".repeat(64),
                    super::super::CA_CERTIFICATE_NAME
                ),
                server_certificate: format!(
                    "{}/{}/{}",
                    super::super::TLS_DIRECTORY,
                    "a".repeat(64),
                    super::super::SERVER_CERTIFICATE_NAME
                ),
                server_private_key: format!(
                    "{}/{}/{}",
                    super::super::TLS_DIRECTORY,
                    "a".repeat(64),
                    super::super::SERVER_PRIVATE_KEY_NAME
                ),
                expires_at_unix_ms: 1,
            },
        })
        .unwrap()
    }

    fn migration_bytes() -> Vec<u8> {
        serde_json::to_vec(&super::super::SettingsMigrationV1 {
            schema_version: super::super::SETTINGS_MIGRATION_VERSION.into(),
            phase: super::super::SettingsMigrationPhase::Pending,
            previous_settings: settings_bytes(),
            previous_mode: 0o600,
            previous_generation: Some("c".repeat(64)),
            replacement_generation: "d".repeat(64),
        })
        .unwrap()
    }

    #[cfg(unix)]
    fn set_mode(path: &Path, mode: u32) {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    }

    #[test]
    fn opens_reject_fifo_replacement_without_waiting() {
        const PROBE: &str = "AGENTOBS_COLLECTOR_OWNERSHIP_FIFO_PROBE";
        if std::env::var_os(PROBE).is_none() {
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "storage_ownership::tests::opens_reject_fifo_replacement_without_waiting",
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
                    panic!("ownership open blocked on a substituted FIFO");
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        for directory in [false, true] {
            let (root, layout) = fixture();
            let path = layout.runtime.join("candidate");
            if directory {
                fs::create_dir(&path).unwrap();
                set_mode(&path, 0o700);
            } else {
                write_private(&path, b"");
            }
            let result = open_checked(&path, directory, || {
                fs::rename(&path, layout.runtime.join("original")).unwrap();
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
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn legacy_settings_remain_owned_without_loading_credential_contents() {
        let generation = "a".repeat(64);
        let prefix = format!("{}/{generation}/", super::super::TLS_DIRECTORY);
        let legacy_states = [
            serde_json::json!({
                "schema_version": "local_collector.v1", "port": 4318,
                "token": "b".repeat(64), "source_generation": super::super::SOURCE_GENERATION,
            }),
            serde_json::json!({
                "schema_version": "local_collector.v2", "port": 4318,
                "generation": generation, "transport": "mtls",
                "credentials": {
                    "ca_certificate": format!("{prefix}{}", super::super::CA_CERTIFICATE_NAME),
                    "server_certificate": format!("{prefix}{}", super::super::SERVER_CERTIFICATE_NAME),
                    "server_private_key": format!("{prefix}{}", super::super::SERVER_PRIVATE_KEY_NAME),
                    "client_certificate": format!("{prefix}{}", super::super::LEGACY_CLIENT_CERTIFICATE_NAME),
                    "client_private_key": format!("{prefix}{}", super::super::LEGACY_CLIENT_PRIVATE_KEY_NAME),
                    "expires_at_unix_ms": 1,
                },
            }),
        ];
        for mut legacy in legacy_states {
            let (root, layout) = fixture();
            let settings = layout.runtime.join("collector.json");
            write_private(&settings, &serde_json::to_vec(&legacy).unwrap());
            let evidence = CollectorStorageOwnershipEvidence::capture(&layout).unwrap();
            assert!(
                evidence
                    .matches_entry(
                        Path::new(SETTINGS_RELATIVE_PATH),
                        &File::open(&settings).unwrap()
                    )
                    .unwrap()
            );
            evidence.revalidate().unwrap();
            assert!(!layout.runtime.join(super::super::TLS_DIRECTORY).exists());
            fs::remove_file(&settings).unwrap();
            legacy["port"] = serde_json::json!(0);
            write_private(&settings, &serde_json::to_vec(&legacy).unwrap());
            assert_eq!(
                CollectorStorageOwnershipEvidence::capture(&layout).unwrap_err(),
                CollectorStorageOwnershipError::InvalidContent
            );
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn capture_is_noncreating_and_unknown_zero_file_is_not_owned() {
        let (root, layout) = fixture();
        let evidence = CollectorStorageOwnershipEvidence::capture(&layout).unwrap();
        assert!(
            !layout
                .runtime
                .join(super::super::REPORT_DIRTY_FILE_NAME)
                .exists()
        );
        let unknown = layout.runtime.join("unknown-zero");
        write_private(&unknown, b"");
        assert!(
            !evidence
                .matches_entry(
                    Path::new("runtime/unknown-zero"),
                    &File::open(unknown).unwrap()
                )
                .unwrap()
        );
        let settings = layout.runtime.join("collector.json");
        write_private(&settings, &settings_bytes());
        assert_eq!(
            evidence.matches_entry(
                Path::new(SETTINGS_RELATIVE_PATH),
                &File::open(settings).unwrap()
            ),
            Err(CollectorStorageOwnershipError::Replaced)
        );
        assert_eq!(
            format!("{evidence:?}"),
            "CollectorStorageOwnershipEvidence { .. }"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn exact_settings_and_dirty_marker_match_but_replacement_fails_closed() {
        let (root, layout) = fixture();
        let settings = layout.runtime.join("collector.json");
        let migration = layout.runtime.join("collector-settings-migration.json");
        let dirty = layout.runtime.join(super::super::REPORT_DIRTY_FILE_NAME);
        write_private(&settings, &settings_bytes());
        write_private(&migration, &migration_bytes());
        write_private(&dirty, REPORT_DIRTY_CONTENT);
        let evidence = CollectorStorageOwnershipEvidence::capture(&layout).unwrap();
        assert!(
            evidence
                .matches_entry(
                    Path::new(SETTINGS_RELATIVE_PATH),
                    &File::open(&settings).unwrap()
                )
                .unwrap()
        );
        assert!(
            evidence
                .matches_entry(
                    Path::new(MIGRATION_RELATIVE_PATH),
                    &File::open(&migration).unwrap()
                )
                .unwrap()
        );
        assert!(
            evidence
                .matches_entry(
                    Path::new(REPORT_DIRTY_RELATIVE_PATH),
                    &File::open(&dirty).unwrap()
                )
                .unwrap()
        );
        fs::rename(&settings, layout.runtime.join("old-settings")).unwrap();
        write_private(&settings, &settings_bytes());
        assert_eq!(
            evidence.matches_entry(
                Path::new(SETTINGS_RELATIVE_PATH),
                &File::open(&settings).unwrap()
            ),
            Err(CollectorStorageOwnershipError::Replaced)
        );
        assert_eq!(
            evidence.revalidate(),
            Err(CollectorStorageOwnershipError::Replaced)
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn aliases_are_rejected() {
        use std::os::unix::fs::symlink;

        let (root, layout) = fixture();
        let outside = root.join("outside");
        write_private(&outside, &settings_bytes());
        let settings = layout.runtime.join("collector.json");
        fs::hard_link(&outside, &settings).unwrap();
        assert_eq!(
            CollectorStorageOwnershipEvidence::capture(&layout).unwrap_err(),
            CollectorStorageOwnershipError::Hardlink
        );
        fs::remove_file(&settings).unwrap();
        symlink(&outside, &settings).unwrap();
        assert_eq!(
            CollectorStorageOwnershipEvidence::capture(&layout).unwrap_err(),
            CollectorStorageOwnershipError::Symlink
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn semantically_invalid_known_files_fail_closed() {
        let (root, layout) = fixture();
        write_private(&layout.runtime.join("collector.json"), b"{}");
        assert_eq!(
            CollectorStorageOwnershipEvidence::capture(&layout).unwrap_err(),
            CollectorStorageOwnershipError::InvalidContent
        );
        fs::remove_dir_all(root).unwrap();

        let (root, layout) = fixture();
        write_private(
            &layout.runtime.join(super::super::REPORT_DIRTY_FILE_NAME),
            b"clean\n",
        );
        assert_eq!(
            CollectorStorageOwnershipEvidence::capture(&layout).unwrap_err(),
            CollectorStorageOwnershipError::InvalidContent
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn private_permissions_are_required() {
        let (root, layout) = fixture();
        let settings = layout.runtime.join("collector.json");
        write_private(&settings, &settings_bytes());
        set_mode(&settings, 0o640);
        assert_eq!(
            CollectorStorageOwnershipEvidence::capture(&layout).unwrap_err(),
            CollectorStorageOwnershipError::InsecurePermissions
        );
        fs::remove_dir_all(root).unwrap();
    }
}
