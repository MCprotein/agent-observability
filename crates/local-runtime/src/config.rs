use crate::policy::{CollectionPolicyV1, PolicyError, RetentionPolicyV1};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

pub const LOCAL_RUNTIME_CONFIG_VERSION: &str = "local_runtime.v2";
const LEGACY_LOCAL_RUNTIME_CONFIG_VERSION: &str = "local_runtime.v1";
static UPDATE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LocalRuntimeConfigV2 {
    pub schema_version: String,
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
    #[serde(default)]
    pub collection: CollectionPolicyV1,
    #[serde(default)]
    pub retention: RetentionPolicyV1,
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
    config.validate()?;
    reject_symlink(path)?;
    let parent = path.parent().ok_or(ConfigError::InvalidPath)?;
    private_dir(parent, false)?;
    let _ = open_private_read(path)?;

    let body = serde_json::to_vec_pretty(config).map_err(ConfigError::Json)?;
    let (temporary, mut file) = private_update_file(parent)?;
    let result = (|| {
        file.write_all(&body)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        private_open_file(&file)?;
        fs::rename(&temporary, path)?;
        File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
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
}
