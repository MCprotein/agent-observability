use crate::{
    lock::{MutationGuard, SingletonError},
    policy::{
        CollectionPolicyV1, PolicyError, RetentionPolicyV1, StorageBudgetMode,
        StorageBudgetPolicyV1, StorageLifecyclePolicyV1,
    },
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

pub const LOCAL_RUNTIME_CONFIG_VERSION: &str = "local_runtime.v5";
const LEGACY_LOCAL_RUNTIME_CONFIG_VERSION: &str = "local_runtime.v1";
const PRIOR_LOCAL_RUNTIME_CONFIG_VERSION: &str = "local_runtime.v2";
const PREVIOUS_LOCAL_RUNTIME_CONFIG_VERSION: &str = "local_runtime.v3";
const LIFECYCLE_LOCAL_RUNTIME_CONFIG_VERSION: &str = "local_runtime.v4";
static UPDATE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SaveStage {
    Write,
    FileSync,
    Rename,
    ParentSync,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct LocalRuntimeConfigV5 {
    pub schema_version: String,
    pub enabled: bool,
    pub capture_private_codex_turn_details: bool,
    pub collection: CollectionPolicyV1,
    pub retention: RetentionPolicyV1,
    pub lifecycle: StorageLifecyclePolicyV1,
    pub storage_budget: StorageBudgetPolicyV1,
}

/// Compatibility alias for source callers; serialized output uses v5.
pub type LocalRuntimeConfigV3 = LocalRuntimeConfigV5;
/// Compatibility alias for source callers; serialized output uses v5.
pub type LocalRuntimeConfigV4 = LocalRuntimeConfigV5;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictLocalRuntimeConfigV5 {
    schema_version: String,
    enabled: bool,
    capture_private_codex_turn_details: bool,
    collection: StrictCollectionPolicyV1,
    retention: StrictRetentionPolicyV1,
    lifecycle: StrictStorageLifecyclePolicyV1,
    storage_budget: StorageBudgetPolicyV1,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictLocalRuntimeConfigV4 {
    #[serde(rename = "schema_version")]
    _schema_version: String,
    enabled: bool,
    capture_private_codex_turn_details: bool,
    collection: StrictCollectionPolicyV1,
    retention: StrictRetentionPolicyV1,
    lifecycle: StrictStorageLifecyclePolicyV1,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictLocalRuntimeConfigV3 {
    #[serde(rename = "schema_version")]
    _schema_version: String,
    enabled: bool,
    capture_private_codex_turn_details: bool,
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictStorageLifecyclePolicyV1 {
    enabled: bool,
    hot_days: u16,
    warm_days: u16,
    delete_after_days: u16,
    private_raw_days: u16,
    maintenance_interval_seconds: u32,
    max_traces_per_pass: u16,
}

impl<'de> Deserialize<'de> for LocalRuntimeConfigV5 {
    fn deserialize<Deserializer>(deserializer: Deserializer) -> Result<Self, Deserializer::Error>
    where
        Deserializer: serde::Deserializer<'de>,
    {
        let strict = StrictLocalRuntimeConfigV5::deserialize(deserializer)?;
        Ok(Self {
            schema_version: strict.schema_version,
            enabled: strict.enabled,
            capture_private_codex_turn_details: strict.capture_private_codex_turn_details,
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
            lifecycle: StorageLifecyclePolicyV1 {
                enabled: strict.lifecycle.enabled,
                hot_days: strict.lifecycle.hot_days,
                warm_days: strict.lifecycle.warm_days,
                delete_after_days: strict.lifecycle.delete_after_days,
                private_raw_days: strict.lifecycle.private_raw_days,
                maintenance_interval_seconds: strict.lifecycle.maintenance_interval_seconds,
                max_traces_per_pass: strict.lifecycle.max_traces_per_pass,
            },
            storage_budget: strict.storage_budget,
        })
    }
}

impl Default for LocalRuntimeConfigV5 {
    fn default() -> Self {
        Self {
            schema_version: LOCAL_RUNTIME_CONFIG_VERSION.into(),
            enabled: true,
            capture_private_codex_turn_details: false,
            collection: CollectionPolicyV1::default(),
            retention: RetentionPolicyV1::default(),
            lifecycle: StorageLifecyclePolicyV1::default(),
            storage_budget: StorageBudgetPolicyV1::default(),
        }
    }
}

impl LocalRuntimeConfigV5 {
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
                capture_private_codex_turn_details: false,
                collection: legacy.collection,
                retention: RetentionPolicyV1::default(),
                lifecycle: StorageLifecyclePolicyV1::default(),
                storage_budget: StorageBudgetPolicyV1::default(),
            }
        } else if version == PRIOR_LOCAL_RUNTIME_CONFIG_VERSION {
            let prior: LegacyLocalRuntimeConfigV2 =
                serde_json::from_str(input).map_err(ConfigError::Json)?;
            Self {
                schema_version: LOCAL_RUNTIME_CONFIG_VERSION.into(),
                enabled: prior.enabled,
                capture_private_codex_turn_details: false,
                collection: prior.collection.into(),
                retention: prior.retention.into(),
                lifecycle: StorageLifecyclePolicyV1::default(),
                storage_budget: StorageBudgetPolicyV1::default(),
            }
        } else if version == PREVIOUS_LOCAL_RUNTIME_CONFIG_VERSION {
            let previous: StrictLocalRuntimeConfigV3 =
                serde_json::from_str(input).map_err(ConfigError::Json)?;
            Self {
                schema_version: LOCAL_RUNTIME_CONFIG_VERSION.into(),
                enabled: previous.enabled,
                capture_private_codex_turn_details: previous.capture_private_codex_turn_details,
                collection: previous.collection.into(),
                retention: previous.retention.into(),
                lifecycle: StorageLifecyclePolicyV1::default(),
                storage_budget: StorageBudgetPolicyV1::default(),
            }
        } else if version == LIFECYCLE_LOCAL_RUNTIME_CONFIG_VERSION {
            let previous: StrictLocalRuntimeConfigV4 =
                serde_json::from_str(input).map_err(ConfigError::Json)?;
            Self {
                schema_version: LOCAL_RUNTIME_CONFIG_VERSION.into(),
                enabled: previous.enabled,
                capture_private_codex_turn_details: previous.capture_private_codex_turn_details,
                collection: previous.collection.into(),
                retention: previous.retention.into(),
                lifecycle: previous.lifecycle.into(),
                storage_budget: StorageBudgetPolicyV1::default(),
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
        self.retention.validate().map_err(ConfigError::Policy)?;
        self.lifecycle.validate().map_err(ConfigError::Policy)?;
        self.storage_budget.validate().map_err(ConfigError::Policy)
    }

    /// P1 accepts the new contract but must not activate unfinished admission logic.
    pub fn require_operational_storage_policy(&self) -> Result<(), ConfigError> {
        match self.storage_budget.mode {
            StorageBudgetMode::Legacy => Ok(()),
            StorageBudgetMode::Separated => Err(ConfigError::StoragePolicyUnavailable),
        }
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyLocalRuntimeConfigV2 {
    #[serde(rename = "schema_version")]
    _schema_version: String,
    enabled: bool,
    collection: StrictCollectionPolicyV1,
    retention: StrictRetentionPolicyV1,
}

impl From<StrictCollectionPolicyV1> for CollectionPolicyV1 {
    fn from(strict: StrictCollectionPolicyV1) -> Self {
        Self {
            file_reconcile_interval_ms: strict.file_reconcile_interval_ms,
            flush_interval_ms: strict.flush_interval_ms,
            max_batch_records: strict.max_batch_records,
            max_batch_bytes: strict.max_batch_bytes,
            active_heartbeat_interval_ms: strict.active_heartbeat_interval_ms,
            idle_heartbeat_interval_ms: strict.idle_heartbeat_interval_ms,
            local_storage_budget_bytes: strict.local_storage_budget_bytes,
        }
    }
}

impl From<StrictRetentionPolicyV1> for RetentionPolicyV1 {
    fn from(strict: StrictRetentionPolicyV1) -> Self {
        Self {
            max_record_age_days: strict.max_record_age_days,
            max_archive_records: strict.max_archive_records,
            max_archive_bytes: strict.max_archive_bytes,
        }
    }
}

impl From<StrictStorageLifecyclePolicyV1> for StorageLifecyclePolicyV1 {
    fn from(strict: StrictStorageLifecyclePolicyV1) -> Self {
        Self {
            enabled: strict.enabled,
            hot_days: strict.hot_days,
            warm_days: strict.warm_days,
            delete_after_days: strict.delete_after_days,
            private_raw_days: strict.private_raw_days,
            maintenance_interval_seconds: strict.maintenance_interval_seconds,
            max_traces_per_pass: strict.max_traces_per_pass,
        }
    }
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

#[derive(Clone, Debug)]
pub struct LocalConfigService {
    layout: InstalledLayout,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VersionedLocalConfig {
    pub config: LocalRuntimeConfigV3,
    pub revision: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigServiceError {
    Busy,
    Conflict,
    Invalid,
    Unavailable,
}

impl LocalConfigService {
    pub fn new(layout: &InstalledLayout) -> Self {
        Self {
            layout: InstalledLayout::at(&layout.root),
        }
    }

    pub fn read(&self) -> Result<VersionedLocalConfig, ConfigServiceError> {
        let config = load(&self.layout.config).map_err(|_| ConfigServiceError::Unavailable)?;
        let revision = revision(&config).map_err(|_| ConfigServiceError::Unavailable)?;
        Ok(VersionedLocalConfig { config, revision })
    }

    pub fn save(
        &self,
        expected_revision: &str,
        config: &LocalRuntimeConfigV3,
    ) -> Result<VersionedLocalConfig, ConfigServiceError> {
        config.validate().map_err(|_| ConfigServiceError::Invalid)?;
        let mutation = ConfigMutationGuard::acquire(&self.layout).map_err(|error| match error {
            SingletonError::AlreadyRunning => ConfigServiceError::Busy,
            _ => ConfigServiceError::Unavailable,
        })?;
        save_if_revision(&mutation, expected_revision, config).map_err(|error| match error {
            ConfigError::Conflict => ConfigServiceError::Conflict,
            ConfigError::Policy(_)
            | ConfigError::UnsupportedVersion
            | ConfigError::StoragePolicyUnavailable => ConfigServiceError::Invalid,
            _ => ConfigServiceError::Unavailable,
        })?;
        self.read()
    }
}

#[derive(Debug)]
pub struct ConfigMutationGuard {
    _mutation: MutationGuard,
    config_path: PathBuf,
}

impl ConfigMutationGuard {
    pub fn acquire(layout: &InstalledLayout) -> Result<Self, SingletonError> {
        let canonical = InstalledLayout::at(&layout.root);
        MutationGuard::try_acquire(&canonical.runtime).map(|mutation| Self {
            _mutation: mutation,
            config_path: canonical.config,
        })
    }
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

/// Resolves and validates an already-installed private runtime without creating or updating it.
pub fn inspect(root: &Path) -> Result<InstalledLayout, ConfigError> {
    private_dir(root, false)?;
    let root = fs::canonicalize(root)?;
    let layout = InstalledLayout::at(&root);
    for directory in [&layout.logs, &layout.queue, &layout.state, &layout.runtime] {
        private_dir(directory, false)?;
    }
    let _ = load(&layout.config)?;
    Ok(layout)
}

#[derive(Debug)]
pub enum ConfigError {
    Io(io::Error),
    Json(serde_json::Error),
    Policy(PolicyError),
    UnsupportedVersion,
    StoragePolicyUnavailable,
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
            Self::StoragePolicyUnavailable => formatter.write_str(
                "separated storage policy is not available in this development checkpoint",
            ),
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
        serde_json::to_vec_pretty(&LocalRuntimeConfigV3::default()).map_err(ConfigError::Json)?;
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

pub fn load(path: &Path) -> Result<LocalRuntimeConfigV3, ConfigError> {
    let mut file = open_private_read(path)?;
    let mut body = String::new();
    file.read_to_string(&mut body)?;
    let config = LocalRuntimeConfigV3::from_json(&body)?;
    config.require_operational_storage_policy()?;
    Ok(config)
}

pub fn save(guard: &ConfigMutationGuard, config: &LocalRuntimeConfigV3) -> Result<(), ConfigError> {
    save_with_hook(&guard.config_path, config, None, |_| Ok(()))
}

pub fn save_if_revision(
    guard: &ConfigMutationGuard,
    expected_revision: &str,
    config: &LocalRuntimeConfigV3,
) -> Result<(), ConfigError> {
    save_with_hook(&guard.config_path, config, Some(expected_revision), |_| {
        Ok(())
    })
}

pub fn revision(config: &LocalRuntimeConfigV3) -> Result<String, ConfigError> {
    let body = serde_json::to_vec(config).map_err(ConfigError::Json)?;
    let digest = Sha256::digest(body);
    Ok(hex(&digest))
}

fn save_with_hook(
    path: &Path,
    config: &LocalRuntimeConfigV3,
    expected_revision: Option<&str>,
    mut before: impl FnMut(SaveStage) -> io::Result<()>,
) -> Result<(), ConfigError> {
    config.validate()?;
    config.require_operational_storage_policy()?;
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

    #[test]
    fn v4_migration_preserves_all_existing_values_and_selects_legacy() {
        let mut input: serde_json::Value = serde_json::from_str(include_str!(
            "../../../contracts/local-runtime-config-v4.fixture.json"
        ))
        .unwrap();
        input["enabled"] = false.into();
        input["capture_private_codex_turn_details"] = true.into();
        input["collection"]["local_storage_budget_bytes"] = 536_870_912_u64.into();
        input["lifecycle"]["enabled"] = true.into();
        let config = LocalRuntimeConfigV3::from_json(&input.to_string()).unwrap();
        let output = serde_json::to_value(&config).unwrap();
        assert_eq!(output["schema_version"], "local_runtime.v5");
        assert_eq!(output["storage_budget"]["mode"], "legacy");
        for field in [
            "enabled",
            "capture_private_codex_turn_details",
            "collection",
            "retention",
            "lifecycle",
        ] {
            assert_eq!(output[field], input[field], "{field}");
        }
        assert_eq!(
            output["storage_budget"]["retained_target_bytes"],
            1_073_741_824_u64
        );
    }

    #[test]
    fn historical_migrations_preserve_nondefault_collection_and_retention() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../contracts/local-runtime-config-v4.fixture.json"
        ))
        .unwrap();
        for version in 1..=4 {
            let mut input = fixture.clone();
            input["schema_version"] = format!("local_runtime.v{version}").into();
            input["enabled"] = false.into();
            input["collection"]["local_storage_budget_bytes"] = 2_147_483_648_u64.into();
            input["collection"]["flush_interval_ms"] = 12_000.into();
            input["retention"]["max_record_age_days"] = 234.into();
            if version < 4 {
                input.as_object_mut().unwrap().remove("lifecycle");
            }
            if version < 3 {
                input
                    .as_object_mut()
                    .unwrap()
                    .remove("capture_private_codex_turn_details");
            }
            if version < 2 {
                input.as_object_mut().unwrap().remove("retention");
            }
            let migrated = LocalRuntimeConfigV5::from_json(&input.to_string()).unwrap();
            let output = serde_json::to_value(&migrated).unwrap();
            for (field, value) in input.as_object().unwrap() {
                if field != "schema_version" {
                    assert_eq!(&output[field], value, "v{version}/{field}");
                }
            }
            assert_eq!(migrated.storage_budget, StorageBudgetPolicyV1::default());
            input["storage_budget"] =
                serde_json::to_value(StorageBudgetPolicyV1::default()).unwrap();
            assert!(
                LocalRuntimeConfigV5::from_json(&input.to_string()).is_err(),
                "v{version} must reject new fields"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn reading_v4_does_not_rewrite_config_or_change_revision_between_reads() {
        let root = root("p1-read-migration");
        let layout = install(&root).unwrap();
        let fixture = include_str!("../../../contracts/local-runtime-config-v4.fixture.json");
        fs::write(&layout.config, fixture).unwrap();
        let service = LocalConfigService::new(&layout);
        let first = service.read().unwrap();
        assert_eq!(first, service.read().unwrap());
        assert_eq!(fs::read_to_string(&layout.config).unwrap(), fixture);
        let second = service.save(&first.revision, &first.config).unwrap();
        assert_eq!(first, second);
        assert!(
            fs::read_to_string(&layout.config)
                .unwrap()
                .contains("local_runtime.v5")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn v5_storage_fields_are_required_strict_and_bounded_even_when_inactive() {
        let fixture = serde_json::to_value(LocalRuntimeConfigV5::default()).unwrap();
        let mut missing = fixture.clone();
        missing.as_object_mut().unwrap().remove("storage_budget");
        assert!(LocalRuntimeConfigV5::from_json(&missing.to_string()).is_err());
        for mode in ["legacy", "separated"] {
            for field in [
                "retained_target_bytes",
                "workspace_budget_bytes",
                "minimum_free_bytes",
            ] {
                for (bytes, accepted) in [
                    (268_435_455_u64, false),
                    (268_435_456, true),
                    (21_474_836_480, true),
                    (21_474_836_481, false),
                    (u64::MAX, false),
                ] {
                    let mut input = fixture.clone();
                    input["storage_budget"]["mode"] = mode.into();
                    input["storage_budget"][field] = bytes.into();
                    assert_eq!(
                        LocalRuntimeConfigV5::from_json(&input.to_string()).is_ok(),
                        accepted,
                        "{mode}/{field}/{bytes}"
                    );
                }
                let mut input = fixture.clone();
                input["storage_budget"]
                    .as_object_mut()
                    .unwrap()
                    .remove(field);
                assert!(LocalRuntimeConfigV5::from_json(&input.to_string()).is_err());
                for value in [
                    serde_json::json!(-1),
                    serde_json::json!(268_435_456.5),
                    serde_json::json!("1073741824"),
                    serde_json::Value::Null,
                ] {
                    input["storage_budget"][field] = value;
                    assert!(LocalRuntimeConfigV5::from_json(&input.to_string()).is_err());
                }
            }
        }
        for value in [
            serde_json::json!("auto"),
            serde_json::json!("LEGACY"),
            serde_json::Value::Null,
        ] {
            let mut input = fixture.clone();
            input["storage_budget"]["mode"] = value;
            assert!(LocalRuntimeConfigV5::from_json(&input.to_string()).is_err());
        }
        let mut unknown = fixture.clone();
        unknown["storage_budget"]["extra"] = true.into();
        assert!(LocalRuntimeConfigV5::from_json(&unknown.to_string()).is_err());
        let mut missing_mode = fixture;
        missing_mode["storage_budget"]
            .as_object_mut()
            .unwrap()
            .remove("mode");
        assert!(LocalRuntimeConfigV5::from_json(&missing_mode.to_string()).is_err());
    }

    #[test]
    fn storage_mode_round_trip_preserves_inactive_values_and_changes_revision() {
        let mut config = LocalRuntimeConfigV5::default();
        config.collection.local_storage_budget_bytes = 536_870_912;
        config.storage_budget.retained_target_bytes = 2_147_483_648;
        config.storage_budget.workspace_budget_bytes = 805_306_368;
        config.storage_budget.minimum_free_bytes = 1_610_612_736;
        config.lifecycle.enabled = true;
        let original = config.clone();
        let original_revision = revision(&config).unwrap();
        config.storage_budget.mode = StorageBudgetMode::Separated;
        assert_ne!(revision(&config).unwrap(), original_revision);
        let serialized = serde_json::to_string(&config).unwrap();
        let mut reopened = LocalRuntimeConfigV5::from_json(&serialized).unwrap();
        assert_eq!(reopened, config);
        reopened.storage_budget.mode = StorageBudgetMode::Legacy;
        assert_eq!(reopened, original);
        assert_eq!(revision(&reopened).unwrap(), original_revision);
    }

    #[cfg(unix)]
    #[test]
    fn unfinished_separated_policy_cannot_save_load_or_create_runtime_control() {
        let root = root("p1-activation-gate");
        let layout = install(&root).unwrap();
        let service = LocalConfigService::new(&layout);
        let current = service.read().unwrap();
        let original = fs::read(&layout.config).unwrap();
        let mut candidate = current.config.clone();
        candidate.storage_budget.mode = StorageBudgetMode::Separated;
        assert!(candidate.validate().is_ok());
        assert_eq!(
            service.save(&current.revision, &candidate),
            Err(ConfigServiceError::Invalid)
        );
        assert_eq!(fs::read(&layout.config).unwrap(), original);
        assert!(matches!(
            crate::RuntimeControl::new(&candidate),
            Err(crate::ControlError::Config(
                ConfigError::StoragePolicyUnavailable
            ))
        ));
        // Simulate a hand-edited config only inside this disposable private fixture.
        fs::write(&layout.config, serde_json::to_vec(&candidate).unwrap()).unwrap();
        assert!(matches!(
            load(&layout.config),
            Err(ConfigError::StoragePolicyUnavailable)
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn v5_inactive_budget_save_preserves_values_and_rejects_stale_revision() {
        let root = root("p1-revision");
        let layout = install(&root).unwrap();
        let service = LocalConfigService::new(&layout);
        let before = service.read().unwrap();
        let mut candidate = before.config.clone();
        candidate.storage_budget.workspace_budget_bytes = 805_306_368;
        candidate.lifecycle.enabled = true;
        let after = service.save(&before.revision, &candidate).unwrap();
        assert_eq!(after.config, candidate);
        assert_ne!(after.revision, before.revision);
        assert_eq!(
            service.save(&before.revision, &before.config),
            Err(ConfigServiceError::Conflict)
        );
        assert_eq!(service.read().unwrap(), after);
        fs::remove_dir_all(root).unwrap();
    }

    fn root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "agent-observability-config-{name}-{}",
            std::process::id()
        ))
    }

    #[test]
    fn config_is_strict_and_versioned() {
        let config = LocalRuntimeConfigV4::default();
        config.validate().unwrap();
        assert!(
            LocalRuntimeConfigV3::from_json(
                r#"{"schema_version":"local_runtime.v1","unknown":true}"#
            )
            .is_err()
        );
        assert!(
            LocalRuntimeConfigV4::from_json(r#"{"schema_version":"local_runtime.v6"}"#).is_err()
        );
        let legacy = LocalRuntimeConfigV3::from_json(
            r#"{"schema_version":"local_runtime.v1","enabled":true,"collection":{}}"#,
        )
        .unwrap();
        assert_eq!(legacy.schema_version, LOCAL_RUNTIME_CONFIG_VERSION);
        assert!(!legacy.capture_private_codex_turn_details);
        assert_eq!(legacy.retention, RetentionPolicyV1::default());
        assert_eq!(legacy.lifecycle, StorageLifecyclePolicyV1::default());
        assert_eq!(legacy.storage_budget, StorageBudgetPolicyV1::default());

        let prior_v2 = LocalRuntimeConfigV3::from_json(
            r#"{"schema_version":"local_runtime.v2","enabled":true,"collection":{"file_reconcile_interval_ms":5000,"flush_interval_ms":5000,"max_batch_records":100,"max_batch_bytes":524288,"active_heartbeat_interval_ms":60000,"idle_heartbeat_interval_ms":300000,"local_storage_budget_bytes":1073741824},"retention":{"max_record_age_days":30,"max_archive_records":10000,"max_archive_bytes":16777216}}"#,
        )
        .unwrap();
        assert!(!prior_v2.capture_private_codex_turn_details);
        assert_eq!(prior_v2.lifecycle, StorageLifecyclePolicyV1::default());
        assert_eq!(prior_v2.storage_budget, StorageBudgetPolicyV1::default());

        let mut previous_fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../contracts/local-runtime-config-v3.fixture.json"
        ))
        .unwrap();
        previous_fixture["enabled"] = false.into();
        previous_fixture["capture_private_codex_turn_details"] = true.into();
        previous_fixture["collection"]["max_batch_records"] = 321.into();
        previous_fixture["retention"]["max_record_age_days"] = 123.into();
        let previous_v3 = LocalRuntimeConfigV4::from_json(&previous_fixture.to_string()).unwrap();
        assert_eq!(previous_v3.schema_version, LOCAL_RUNTIME_CONFIG_VERSION);
        assert!(!previous_v3.enabled);
        assert!(previous_v3.capture_private_codex_turn_details);
        assert_eq!(previous_v3.collection.max_batch_records, 321);
        assert_eq!(previous_v3.retention.max_record_age_days, 123);
        assert_eq!(previous_v3.lifecycle, StorageLifecyclePolicyV1::default());
        assert_eq!(previous_v3.storage_budget, StorageBudgetPolicyV1::default());

        let mut malformed_v3 = previous_fixture.clone();
        malformed_v3["collection"]
            .as_object_mut()
            .unwrap()
            .remove("flush_interval_ms");
        assert!(LocalRuntimeConfigV4::from_json(&malformed_v3.to_string()).is_err());
        let mut unknown_v3 = previous_fixture;
        unknown_v3["unexpected"] = true.into();
        assert!(LocalRuntimeConfigV4::from_json(&unknown_v3.to_string()).is_err());

        let mut opted_in: serde_json::Value = serde_json::from_str(include_str!(
            "../../../contracts/local-runtime-config-v4.fixture.json"
        ))
        .unwrap();
        opted_in["capture_private_codex_turn_details"] = serde_json::Value::Bool(true);
        assert!(
            LocalRuntimeConfigV3::from_json(&opted_in.to_string())
                .unwrap()
                .capture_private_codex_turn_details
        );
    }

    #[test]
    fn versioned_fixture_matches_the_rust_default_and_bounds() {
        let fixture = include_str!("../../../contracts/local-runtime-config-v4.fixture.json");
        assert_eq!(
            LocalRuntimeConfigV3::from_json(fixture).unwrap(),
            LocalRuntimeConfigV4::default()
        );
        let cases: serde_json::Value = serde_json::from_str(include_str!(
            "../../../contracts/local-runtime-config-v4.parity.json"
        ))
        .unwrap();
        for case in cases.as_array().unwrap() {
            let mut document: serde_json::Value = serde_json::from_str(fixture).unwrap();
            apply_parity_case(&mut document, case);
            let accepted = LocalRuntimeConfigV3::from_json(&document.to_string()).is_ok();
            assert_eq!(
                accepted,
                case["valid"].as_bool().unwrap(),
                "{}",
                case["name"]
            );
        }
    }

    #[test]
    fn v5_fixture_and_shared_parity_match_rust() {
        let fixture = include_str!("../../../contracts/local-runtime-config-v5.fixture.json");
        assert_eq!(
            LocalRuntimeConfigV5::from_json(fixture).unwrap(),
            LocalRuntimeConfigV5::default()
        );
        let cases: serde_json::Value = serde_json::from_str(include_str!(
            "../../../contracts/local-runtime-config-v5.parity.json"
        ))
        .unwrap();
        for case in cases.as_array().unwrap() {
            let mut document: serde_json::Value = serde_json::from_str(fixture).unwrap();
            apply_parity_case(&mut document, case);
            assert_eq!(
                LocalRuntimeConfigV5::from_json(&document.to_string()).is_ok(),
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
            LocalRuntimeConfigV3::default()
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
    fn inspect_validates_existing_layout_without_creating_missing_paths() {
        let root = root("inspect");
        let _ = fs::remove_dir_all(&root);
        assert!(inspect(&root).is_err());
        assert!(!root.exists());

        let layout = install(&root).unwrap();
        assert_eq!(inspect(&root).unwrap(), layout);
        fs::remove_dir(&layout.queue).unwrap();
        assert!(inspect(&root).is_err());
        assert!(!layout.queue.exists());
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
        save_with_hook(&layout.config, &config, None, |_| Ok(())).unwrap();

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
    fn config_mutation_guard_serializes_supported_writers() {
        let root = root("config-mutation-guard");
        let _ = fs::remove_dir_all(&root);
        let layout = install(&root).unwrap();

        let first = ConfigMutationGuard::acquire(&layout).unwrap();
        assert!(matches!(
            ConfigMutationGuard::acquire(&layout),
            Err(SingletonError::AlreadyRunning)
        ));
        drop(first);
        ConfigMutationGuard::acquire(&layout).unwrap();

        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn config_mutation_guard_binds_save_to_its_installed_layout() {
        let first_root = root("config-mutation-bound-first");
        let second_root = root("config-mutation-bound-second");
        let _ = fs::remove_dir_all(&first_root);
        let _ = fs::remove_dir_all(&second_root);
        let first_layout = install(&first_root).unwrap();
        let second_layout = install(&second_root).unwrap();
        let mut mismatched_layout = first_layout.clone();
        mismatched_layout.config = second_layout.config.clone();
        mismatched_layout.runtime = second_layout.runtime.clone();
        let guard = ConfigMutationGuard::acquire(&mismatched_layout).unwrap();
        let mut update = LocalRuntimeConfigV3::default();
        update.retention.max_record_age_days = 90;

        save(&guard, &update).unwrap();

        assert_eq!(load(&first_layout.config).unwrap(), update);
        assert_eq!(
            load(&second_layout.config).unwrap(),
            LocalRuntimeConfigV3::default()
        );
        drop(guard);
        let _ = fs::remove_dir_all(first_root);
        let _ = fs::remove_dir_all(second_root);
    }

    #[cfg(unix)]
    #[test]
    fn revision_save_rechecks_immediately_before_replace() {
        let root = root("save-revision-conflict");
        let _ = fs::remove_dir_all(&root);
        let layout = install(&root).unwrap();
        let expected = revision(&load(&layout.config).unwrap()).unwrap();
        let mut update = LocalRuntimeConfigV3::default();
        update.retention.max_record_age_days = 90;
        let mut external = LocalRuntimeConfigV3::default();
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

        assert!(save_with_hook(&layout.config, &config, None, |_| Ok(())).is_err());
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
            save_with_hook(
                &layout.config,
                &LocalRuntimeConfigV3::default(),
                None,
                |_| Ok(())
            ),
            Err(ConfigError::Symlink)
        ));

        fs::remove_file(&layout.config).unwrap();
        fs::rename(&backup, &layout.config).unwrap();
        fs::set_permissions(&layout.root, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(matches!(
            save_with_hook(
                &layout.config,
                &LocalRuntimeConfigV3::default(),
                None,
                |_| Ok(())
            ),
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
                let mut config = LocalRuntimeConfigV3::default();
                config.retention.max_record_age_days = days;
                barrier.wait();
                save_with_hook(&path, &config, None, |_| Ok(()))
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
        let mut config = LocalRuntimeConfigV3::default();
        config.retention.max_record_age_days = 90;
        save_with_hook(&layout.config, &config, None, |_| Ok(())).unwrap();
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
        let mut config = LocalRuntimeConfigV3::default();
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
        let mut config = LocalRuntimeConfigV3::default();
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
