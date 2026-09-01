use crate::policy::{CollectionPolicyV1, PolicyError, RetentionPolicyV1};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

pub const LOCAL_RUNTIME_CONFIG_VERSION: &str = "local_runtime.v2";
const LEGACY_LOCAL_RUNTIME_CONFIG_VERSION: &str = "local_runtime.v1";
static UPDATE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SaveStage {
    Write,
    FileSync,
    Rename,
    ParentSync,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct LocalRuntimeConfigV2 {
    pub schema_version: String,
    pub enabled: bool,
    pub collection: CollectionPolicyV1,
    pub retention: RetentionPolicyV1,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictLocalRuntimeConfigV2 {
    schema_version: String,
    enabled: bool,
    collection: StrictCollectionPolicyV1,
    retention: StrictRetentionPolicyV1,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictCollectionPolicyV1 {
    file_reconcile_interval_ms: u32,
    flush_interval_ms: u32,
    max_batch_records: u16,
    max_batch_bytes: u32,
    active_heartbeat_interval_ms: u32,
    idle_heartbeat_interval_ms: u32,
    local_storage_budget_bytes: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(clippy::struct_field_names)]
struct StrictRetentionPolicyV1 {
    max_record_age_days: u16,
    max_archive_records: u32,
    max_archive_bytes: u64,
}

impl<'de> Deserialize<'de> for LocalRuntimeConfigV2 {
    fn deserialize<Deserializer>(deserializer: Deserializer) -> Result<Self, Deserializer::Error>
    where
        Deserializer: serde::Deserializer<'de>,
    {
        let strict = StrictLocalRuntimeConfigV2::deserialize(deserializer)?;
        Ok(Self {
            schema_version: strict.schema_version,
            enabled: strict.enabled,
            collection: CollectionPolicyV1 {
                file_reconcile_interval_ms: strict.collection.file_reconcile_interval_ms,
                flush_interval_ms: strict.collection.flush_interval_ms,
                max_batch_records: strict.collection.max_batch_records,
                max_batch_bytes: strict.collection.max_batch_bytes,
                active_heartbeat_interval_ms: strict.collection.active_heartbeat_interval_ms,
                idle_heartbeat_interval_ms: strict.collection.idle_heartbeat_interval_ms,
                local_storage_budget_bytes: strict.collection.local_storage_budget_bytes,
            },
            retention: RetentionPolicyV1 {
                max_record_age_days: strict.retention.max_record_age_days,
                max_archive_records: strict.retention.max_archive_records,
                max_archive_bytes: strict.retention.max_archive_bytes,
            },
        })
    }
}

impl Default for LocalRuntimeConfigV2 {
    fn default() -> Self {
        Self {
            schema_version: LOCAL_RUNTIME_CONFIG_VERSION.into(),
            enabled: true,
            collection: CollectionPolicyV1::default(),
            retention: RetentionPolicyV1::default(),
        }
    }
}

impl LocalRuntimeConfigV2 {
    pub fn from_json(input: &str) -> Result<Self, ConfigError> {
        let header: serde_json::Value = serde_json::from_str(input).map_err(ConfigError::Json)?;
        let version = header
            .get("schema_version")
            .and_then(serde_json::Value::as_str)
            .ok_or(ConfigError::UnsupportedVersion)?;
        let config = if version == LEGACY_LOCAL_RUNTIME_CONFIG_VERSION {
            let legacy: LegacyLocalRuntimeConfigV1 =
                serde_json::from_str(input).map_err(ConfigError::Json)?;
            Self {
                schema_version: LOCAL_RUNTIME_CONFIG_VERSION.into(),
                enabled: legacy.enabled,
                collection: legacy.collection,
                retention: RetentionPolicyV1::default(),
            }
        } else {
            serde_json::from_str(input).map_err(ConfigError::Json)?
        };
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.schema_version != LOCAL_RUNTIME_CONFIG_VERSION {
            return Err(ConfigError::UnsupportedVersion);
        }
        self.collection.validate().map_err(ConfigError::Policy)?;
        self.retention.validate().map_err(ConfigError::Policy)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyLocalRuntimeConfigV1 {
    #[serde(rename = "schema_version")]
    _schema_version: String,
    #[serde(default = "enabled_by_default")]
    enabled: bool,
    #[serde(default)]
    collection: CollectionPolicyV1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledLayout {
    pub root: PathBuf,
    pub config: PathBuf,
    pub logs: PathBuf,
    pub queue: PathBuf,
    pub state: PathBuf,
    pub runtime: PathBuf,
}

impl InstalledLayout {
    fn at(root: &Path) -> Self {
        Self {
            root: root.into(),
            config: root.join("config.json"),
            logs: root.join("logs"),
            queue: root.join("queue"),
            state: root.join("state"),
            runtime: root.join("runtime"),
        }
    }
}

#[derive(Debug)]
pub enum ConfigError {
    Io(io::Error),
    Json(serde_json::Error),
    Policy(PolicyError),
    UnsupportedVersion,
    InsecurePermissions,
    InvalidPath,
    Symlink,
    UnsupportedPlatform,
    Conflict,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "local runtime configuration I/O error: {error}"),
            Self::Json(error) => write!(formatter, "invalid local runtime configuration: {error}"),
            Self::Policy(error) => error.fmt(formatter),
            Self::UnsupportedVersion => {
                formatter.write_str("unsupported local runtime config version")
            }
            Self::InsecurePermissions => formatter.write_str("local runtime path is not private"),
            Self::InvalidPath => formatter.write_str("local runtime path has the wrong file type"),
            Self::Symlink => formatter.write_str("local runtime paths must not be symlinks"),
            Self::UnsupportedPlatform => {
                formatter.write_str("private local runtime paths are unsupported on this platform")
            }
            Self::Conflict => formatter.write_str("local runtime configuration changed"),
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Policy(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for ConfigError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

pub fn install(root: &Path) -> Result<InstalledLayout, ConfigError> {
    private_dir(root, true)?;
    let root = fs::canonicalize(root)?;
    let layout = InstalledLayout::at(&root);
    for directory in [&layout.logs, &layout.queue, &layout.state, &layout.runtime] {
        private_dir(directory, true)?;
    }

    if layout.config.exists() {
        let _ = load(&layout.config)?;
        return Ok(layout);
    }

    reject_symlink(&layout.config)?;
    let body =
        serde_json::to_vec_pretty(&LocalRuntimeConfigV2::default()).map_err(ConfigError::Json)?;
    let temporary = layout
        .root
        .join(format!(".config.json.tmp.{}", std::process::id()));
    let _ = fs::remove_file(&temporary);
    let mut file = private_create_new(&temporary)?;
    file.write_all(&body)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    private_open_file(&file)?;
    match fs::hard_link(&temporary, &layout.config) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            fs::remove_file(&temporary)?;
            let _ = load(&layout.config)?;
            return Ok(layout);
        }
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            return Err(error.into());
        }
    }
    fs::remove_file(&temporary)?;
    File::open(&layout.root)?.sync_all()?;
    Ok(layout)
}

pub fn load(path: &Path) -> Result<LocalRuntimeConfigV2, ConfigError> {
    let mut file = open_private_read(path)?;
    let mut body = String::new();
    file.read_to_string(&mut body)?;
    LocalRuntimeConfigV2::from_json(&body)
}

pub fn save(path: &Path, config: &LocalRuntimeConfigV2) -> Result<(), ConfigError> {
    save_with_hook(path, config, None, |_| Ok(()))
}

pub fn save_if_revision(
    path: &Path,
    expected_revision: &str,
    config: &LocalRuntimeConfigV2,
) -> Result<(), ConfigError> {
    save_with_hook(path, config, Some(expected_revision), |_| Ok(()))
}

pub fn revision(config: &LocalRuntimeConfigV2) -> Result<String, ConfigError> {
    let body = serde_json::to_vec(config).map_err(ConfigError::Json)?;
    let digest = Sha256::digest(body);
    Ok(hex(&digest))
}

fn save_with_hook(
    path: &Path,
    config: &LocalRuntimeConfigV2,
    expected_revision: Option<&str>,
    mut before: impl FnMut(SaveStage) -> io::Result<()>,
) -> Result<(), ConfigError> {
    config.validate()?;
    reject_symlink(path)?;
    let parent = path.parent().ok_or(ConfigError::InvalidPath)?;
    private_dir(parent, false)?;
    let _ = open_private_read(path)?;
    ensure_revision(path, expected_revision)?;

    let body = serde_json::to_vec_pretty(config).map_err(ConfigError::Json)?;
    let (temporary, mut file) = private_update_file(parent)?;
    let result = (|| {
        before(SaveStage::Write)?;
        file.write_all(&body)?;
        file.write_all(b"\n")?;
        before(SaveStage::FileSync)?;
        file.sync_all()?;
        private_open_file(&file)?;
        before(SaveStage::Rename)?;
        ensure_revision(path, expected_revision)?;
        fs::rename(&temporary, path)?;
        before(SaveStage::ParentSync)?;
        File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn ensure_revision(path: &Path, expected: Option<&str>) -> Result<(), ConfigError> {
    let Some(expected) = expected else {
        return Ok(());
    };
    if revision(&load(path)?)? == expected {
        Ok(())
    } else {
        Err(ConfigError::Conflict)
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut text, byte| {
            use std::fmt::Write as _;
            write!(text, "{byte:02x}").expect("writing to a String cannot fail");
            text
        })
}

fn private_update_file(parent: &Path) -> Result<(PathBuf, File), ConfigError> {
    for _ in 0..1_024 {
        let sequence = UPDATE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(
            ".config.json.update.{}.{}",
            std::process::id(),
            sequence
        ));
        match private_create_new(&path) {
            Ok(file) => return Ok((path, file)),
            Err(ConfigError::Io(error)) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not allocate a unique config update file",
    )
    .into())
}

const fn enabled_by_default() -> bool {
    true
}

#[cfg(unix)]
fn private_dir(path: &Path, create: bool) -> Result<(), ConfigError> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

    if create && !path.exists() {
        let mut builder = fs::DirBuilder::new();
        builder.mode(0o700);
        match builder.create(path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(ConfigError::Symlink);
    }
    if !metadata.is_dir() {
        return Err(ConfigError::InvalidPath);
    }
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(ConfigError::InsecurePermissions);
    }
    Ok(())
}

#[cfg(not(unix))]
fn private_dir(_path: &Path, _create: bool) -> Result<(), ConfigError> {
    Err(ConfigError::UnsupportedPlatform)
}

fn reject_symlink(path: &Path) -> Result<(), ConfigError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(ConfigError::Symlink),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn private_create_new(path: &Path) -> Result<File, ConfigError> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    Ok(options.open(path)?)
}

#[cfg(unix)]
fn private_open_file(file: &File) -> Result<(), ConfigError> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(ConfigError::InvalidPath);
    }
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(ConfigError::InsecurePermissions);
    }
    Ok(())
}

#[cfg(not(unix))]
fn private_open_file(_file: &File) -> Result<(), ConfigError> {
    Err(ConfigError::UnsupportedPlatform)
}

#[cfg(any(target_os = "linux", target_os = "android", target_os = "macos"))]
fn open_private_read(path: &Path) -> Result<File, ConfigError> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let mut options = OpenOptions::new();
    options.read(true).custom_flags(no_follow_flag());
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(ConfigError::InvalidPath);
    }
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(ConfigError::InsecurePermissions);
    }
    Ok(file)
}

#[cfg(not(any(target_os = "linux", target_os = "android", target_os = "macos")))]
fn open_private_read(_path: &Path) -> Result<File, ConfigError> {
    Err(ConfigError::UnsupportedPlatform)
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

    fn root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "agent-observability-config-{name}-{}",
            std::process::id()
        ))
    }

    #[test]
    fn config_is_strict_and_versioned() {
        let config = LocalRuntimeConfigV2::default();
        config.validate().unwrap();
        assert!(
            LocalRuntimeConfigV2::from_json(
                r#"{"schema_version":"local_runtime.v1","unknown":true}"#
            )
            .is_err()
        );
        assert!(
            LocalRuntimeConfigV2::from_json(r#"{"schema_version":"local_runtime.v3"}"#).is_err()
        );
        let legacy = LocalRuntimeConfigV2::from_json(
            r#"{"schema_version":"local_runtime.v1","enabled":true,"collection":{}}"#,
        )
        .unwrap();
        assert_eq!(legacy.schema_version, LOCAL_RUNTIME_CONFIG_VERSION);
        assert_eq!(legacy.retention, RetentionPolicyV1::default());
    }

    #[test]
    fn versioned_fixture_matches_the_rust_default_and_bounds() {
        let fixture = include_str!("../../../contracts/local-runtime-config-v2.fixture.json");
        assert_eq!(
            LocalRuntimeConfigV2::from_json(fixture).unwrap(),
            LocalRuntimeConfigV2::default()
        );
        let cases: serde_json::Value = serde_json::from_str(include_str!(
            "../../../contracts/local-runtime-config-v2.parity.json"
        ))
        .unwrap();
        for case in cases.as_array().unwrap() {
            let mut document: serde_json::Value = serde_json::from_str(fixture).unwrap();
            apply_parity_case(&mut document, case);
            let accepted = LocalRuntimeConfigV2::from_json(&document.to_string()).is_ok();
            assert_eq!(
                accepted,
                case["valid"].as_bool().unwrap(),
                "{}",
                case["name"]
            );
        }
    }

    fn apply_parity_case(document: &mut serde_json::Value, case: &serde_json::Value) {
        let path = case["path"].as_array().unwrap();
        if path.is_empty() {
            return;
        }
        let mut parent = document;
        for segment in &path[..path.len() - 1] {
            parent = parent.get_mut(segment.as_str().unwrap()).unwrap();
        }
        let field = path.last().unwrap().as_str().unwrap();
        let object = parent.as_object_mut().unwrap();
        match case["operation"].as_str().unwrap() {
            "set" => {
                object.insert(field.into(), case["value"].clone());
            }
            "remove" => {
                object.remove(field);
            }
            operation => panic!("unsupported parity operation: {operation}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn install_is_private_idempotent_and_non_overwriting() {
        use std::os::unix::fs::PermissionsExt;

        let root = root("install");
        let _ = fs::remove_dir_all(&root);
        let first = install(&root).unwrap();
        let original = fs::read(&first.config).unwrap();
        let second = install(&root).unwrap();
        assert_eq!(first, second);
        assert_eq!(original, fs::read(&second.config).unwrap());
        assert_eq!(
            fs::metadata(&second.root).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&second.config).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            load(&second.config).unwrap(),
            LocalRuntimeConfigV2::default()
        );
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn install_rejects_broad_existing_root() {
        use std::os::unix::fs::PermissionsExt;

        let root = root("broad");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(matches!(
            install(&root),
            Err(ConfigError::InsecurePermissions)
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn save_atomically_updates_a_private_valid_config() {
        use std::os::unix::fs::PermissionsExt;

        let root = root("save");
        let _ = fs::remove_dir_all(&root);
        let layout = install(&root).unwrap();
        let mut config = load(&layout.config).unwrap();
        config.retention.max_record_age_days = 90;
        save(&layout.config, &config).unwrap();

        assert_eq!(load(&layout.config).unwrap(), config);
        assert_eq!(
            fs::metadata(&layout.config).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(fs::read_dir(&layout.root).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".update.")
        }));
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn revision_save_rechecks_immediately_before_replace() {
        let root = root("save-revision-conflict");
        let _ = fs::remove_dir_all(&root);
        let layout = install(&root).unwrap();
        let expected = revision(&load(&layout.config).unwrap()).unwrap();
        let mut update = LocalRuntimeConfigV2::default();
        update.retention.max_record_age_days = 90;
        let mut external = LocalRuntimeConfigV2::default();
        external.retention.max_record_age_days = 45;
        let mut external_bytes = serde_json::to_vec_pretty(&external).unwrap();
        external_bytes.push(b'\n');

        let result = save_with_hook(&layout.config, &update, Some(&expected), |stage| {
            if stage == SaveStage::Rename {
                fs::write(&layout.config, &external_bytes)?;
            }
            Ok(())
        });
        assert!(matches!(result, Err(ConfigError::Conflict)));
        assert_eq!(load(&layout.config).unwrap(), external);
        assert!(fs::read_dir(&layout.root).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".update.")
        }));
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn save_rejects_invalid_values_without_changing_the_file() {
        let root = root("save-invalid");
        let _ = fs::remove_dir_all(&root);
        let layout = install(&root).unwrap();
        let original = fs::read(&layout.config).unwrap();
        let mut config = load(&layout.config).unwrap();
        config.retention.max_record_age_days = 0;

        assert!(save(&layout.config, &config).is_err());
        assert_eq!(fs::read(&layout.config).unwrap(), original);
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn save_rejects_symlinks_and_broad_parent_permissions() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let root = root("save-boundaries");
        let _ = fs::remove_dir_all(&root);
        let layout = install(&root).unwrap();
        let backup = layout.root.join("config.backup");
        fs::rename(&layout.config, &backup).unwrap();
        symlink(&backup, &layout.config).unwrap();
        assert!(matches!(
            save(&layout.config, &LocalRuntimeConfigV2::default()),
            Err(ConfigError::Symlink)
        ));

        fs::remove_file(&layout.config).unwrap();
        fs::rename(&backup, &layout.config).unwrap();
        fs::set_permissions(&layout.root, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(matches!(
            save(&layout.config, &LocalRuntimeConfigV2::default()),
            Err(ConfigError::InsecurePermissions)
        ));
        fs::set_permissions(&layout.root, fs::Permissions::from_mode(0o700)).unwrap();
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn concurrent_saves_publish_one_complete_valid_config() {
        use std::sync::{Arc, Barrier};

        let root = root("save-concurrent");
        let _ = fs::remove_dir_all(&root);
        let layout = install(&root).unwrap();
        let path = Arc::new(layout.config.clone());
        let barrier = Arc::new(Barrier::new(3));
        let handles = [30, 90].map(|days| {
            let path = Arc::clone(&path);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                let mut config = LocalRuntimeConfigV2::default();
                config.retention.max_record_age_days = days;
                barrier.wait();
                save(&path, &config)
            })
        });
        barrier.wait();
        for handle in handles {
            handle.join().unwrap().unwrap();
        }

        let stored = load(&layout.config).unwrap();
        assert!(matches!(stored.retention.max_record_age_days, 30 | 90));
        assert!(fs::read_dir(&layout.root).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".update.")
        }));
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn stale_update_file_does_not_block_a_save() {
        use std::os::unix::fs::OpenOptionsExt;

        let root = root("save-stale");
        let _ = fs::remove_dir_all(&root);
        let layout = install(&root).unwrap();
        let stale = layout
            .root
            .join(format!(".config.json.update.{}.0", std::process::id()));
        OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&stale)
            .unwrap();
        let mut config = LocalRuntimeConfigV2::default();
        config.retention.max_record_age_days = 90;
        save(&layout.config, &config).unwrap();
        assert_eq!(load(&layout.config).unwrap(), config);
        assert!(stale.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn pre_rename_io_failures_preserve_config_and_clean_temporary_files() {
        let root = root("save-pre-rename-failures");
        let _ = fs::remove_dir_all(&root);
        let layout = install(&root).unwrap();
        let original = fs::read(&layout.config).unwrap();
        let mut config = LocalRuntimeConfigV2::default();
        config.retention.max_record_age_days = 90;

        for failed_stage in [SaveStage::Write, SaveStage::FileSync, SaveStage::Rename] {
            let result = save_with_hook(&layout.config, &config, None, |stage| {
                if stage == failed_stage {
                    Err(io::Error::other("injected save failure"))
                } else {
                    Ok(())
                }
            });
            assert!(matches!(result, Err(ConfigError::Io(_))));
            assert_eq!(fs::read(&layout.config).unwrap(), original);
            assert!(fs::read_dir(&layout.root).unwrap().all(|entry| {
                !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .contains(".update.")
            }));
        }
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn parent_sync_failure_reports_error_after_complete_replace() {
        let root = root("save-parent-sync-failure");
        let _ = fs::remove_dir_all(&root);
        let layout = install(&root).unwrap();
        let mut config = LocalRuntimeConfigV2::default();
        config.retention.max_record_age_days = 90;

        let result = save_with_hook(&layout.config, &config, None, |stage| {
            if stage == SaveStage::ParentSync {
                Err(io::Error::other("injected parent sync failure"))
            } else {
                Ok(())
            }
        });
        assert!(matches!(result, Err(ConfigError::Io(_))));
        assert_eq!(load(&layout.config).unwrap(), config);
        assert!(fs::read_dir(&layout.root).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".update.")
        }));
        let _ = fs::remove_dir_all(root);
    }
}
