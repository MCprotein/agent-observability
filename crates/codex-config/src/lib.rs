//! Transactional ownership of the user-level Codex observability settings.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fmt,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use toml_edit::{Array, DocumentMut, InlineTable, Item, Value, value};

const SNAPSHOT_FILE: &str = "codex-config-ownership-v1.json";
const LOCK_FILE: &str = ".codex-config-ownership.lock";
const SNAPSHOT_VERSION: &str = "codex_config_ownership.v1";
const MAX_CONFIG_BYTES: u64 = 1024 * 1024;
const MAX_SNAPSHOT_BYTES: u64 = MAX_CONFIG_BYTES * 4 + 8192;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionStatus {
    Connected,
    Disconnected,
    Conflict,
}

#[derive(Debug)]
pub enum ConfigError {
    Conflict,
    InsecurePermissions(PathBuf),
    InvalidArgument(&'static str),
    InvalidSnapshot,
    InvalidToml(toml_edit::TomlError),
    Io(io::Error),
    NonRegularFile(PathBuf),
    TooLarge(PathBuf),
    RollbackFailed,
    Symlink(PathBuf),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Conflict => formatter.write_str("Codex configuration ownership conflict"),
            Self::InsecurePermissions(path) => {
                write!(formatter, "insecure permissions on {}", path.display())
            }
            Self::InvalidArgument(argument) => write!(formatter, "invalid {argument}"),
            Self::InvalidSnapshot => formatter.write_str("invalid ownership snapshot"),
            Self::InvalidToml(error) => write!(formatter, "invalid Codex TOML: {error}"),
            Self::Io(error) => write!(formatter, "configuration I/O failed: {error}"),
            Self::NonRegularFile(path) => {
                write!(formatter, "not a regular file: {}", path.display())
            }
            Self::TooLarge(path) => {
                write!(formatter, "file exceeds size bound: {}", path.display())
            }
            Self::RollbackFailed => formatter.write_str("configuration rollback failed"),
            Self::Symlink(path) => write!(formatter, "symlink is not allowed: {}", path.display()),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<io::Error> for ConfigError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<toml_edit::TomlError> for ConfigError {
    fn from(error: toml_edit::TomlError) -> Self {
        Self::InvalidToml(error)
    }
}

#[derive(Clone, Debug)]
pub struct CodexConfigManager {
    config_path: PathBuf,
    state_dir: PathBuf,
    agentobs_binary: PathBuf,
    runtime_root: PathBuf,
    port: u16,
    token: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct OwnershipSnapshot {
    schema_version: String,
    config_path: String,
    phase: SnapshotPhase,
    prior_existed: bool,
    prior_bytes_hex: String,
    prior_hash: String,
    prior_mode: u32,
    connected_bytes_hex: String,
    connected_hash: String,
    connected_mode: u32,
    managed_fingerprint: String,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum SnapshotPhase {
    Prepared,
    Connected,
    Restoring,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SnapshotState {
    PreparedPrior,
    Connected,
    RestoredPrior,
    Conflict,
}

impl CodexConfigManager {
    /// Creates a manager bound to one config path and one ownership state directory.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::InvalidArgument`] when either managed path is not absolute,
    /// the port is zero, or the token is empty.
    pub fn new(
        config_path: impl Into<PathBuf>,
        state_dir: impl Into<PathBuf>,
        agentobs_binary: impl Into<PathBuf>,
        runtime_root: impl Into<PathBuf>,
        port: u16,
        token: impl Into<String>,
    ) -> Result<Self, ConfigError> {
        let manager = Self {
            config_path: config_path.into(),
            state_dir: state_dir.into(),
            agentobs_binary: agentobs_binary.into(),
            runtime_root: runtime_root.into(),
            port,
            token: token.into(),
        };
        if !manager.agentobs_binary.is_absolute() {
            return Err(ConfigError::InvalidArgument("agentobs binary path"));
        }
        if !manager.runtime_root.is_absolute() {
            return Err(ConfigError::InvalidArgument("runtime root path"));
        }
        if manager.port == 0 {
            return Err(ConfigError::InvalidArgument("OTLP port"));
        }
        if manager.token.is_empty() {
            return Err(ConfigError::InvalidArgument("OTLP token"));
        }
        Ok(manager)
    }

    /// Applies and takes ownership of the managed Codex values.
    ///
    /// # Errors
    ///
    /// Returns an error for unsafe paths or permissions, invalid TOML, conflicting values,
    /// snapshot corruption, or any I/O or rollback failure.
    pub fn connect(&self) -> Result<ConnectionStatus, ConfigError> {
        self.connect_with(&FilesystemMutations)
    }

    fn connect_with(
        &self,
        mutations: &impl FileMutations,
    ) -> Result<ConnectionStatus, ConfigError> {
        self.ensure_private_state_dir()?;
        let _lock = self.lock()?;
        let snapshot_path = self.snapshot_path();
        if checked_metadata(&snapshot_path)?.is_some() {
            let (mut snapshot, snapshot_file) = self.read_snapshot()?;
            if snapshot.managed_fingerprint != self.managed_values().fingerprint() {
                return Err(ConfigError::Conflict);
            }
            match self.snapshot_state(&snapshot)? {
                SnapshotState::PreparedPrior | SnapshotState::RestoredPrior => {
                    mutations
                        .remove_checked(&snapshot_path, FileExpectation::present(&snapshot_file))?;
                }
                SnapshotState::Connected => {
                    if snapshot.phase != SnapshotPhase::Connected {
                        self.transition_snapshot(
                            mutations,
                            &mut snapshot,
                            &snapshot_file,
                            SnapshotPhase::Connected,
                        )?;
                    }
                    return Ok(ConnectionStatus::Connected);
                }
                SnapshotState::Conflict => return Err(ConfigError::Conflict),
            }
        }

        let prior = self.read_config()?;
        let mut document = parse_document(&prior.bytes)?;
        let managed = self.managed_values();
        ensure_no_managed_conflict(&document, &managed)?;
        patch_managed_values(&mut document, &managed);
        let updated = document.to_string().into_bytes();
        let already_connected = prior.matches(&updated, prior.mode);
        let mut snapshot = OwnershipSnapshot {
            schema_version: SNAPSHOT_VERSION.into(),
            config_path: path_identity(&self.config_path),
            phase: if already_connected {
                SnapshotPhase::Connected
            } else {
                SnapshotPhase::Prepared
            },
            prior_existed: prior.existed,
            prior_bytes_hex: hex_encode(&prior.bytes),
            prior_hash: hash(&prior.bytes),
            prior_mode: prior.mode,
            connected_bytes_hex: hex_encode(&updated),
            connected_hash: hash(&updated),
            connected_mode: prior.mode,
            managed_fingerprint: managed.fingerprint(),
        };
        let snapshot_bytes = snapshot.to_bytes()?;

        mutations.atomic_replace(
            &snapshot_path,
            FileExpectation::Missing,
            &snapshot_bytes,
            0o600,
        )?;
        if already_connected {
            return Ok(ConnectionStatus::Connected);
        }
        if let Err(error) = mutations.atomic_replace(
            &self.config_path,
            FileExpectation::from_existing(&prior),
            &updated,
            prior.mode,
        ) {
            match self.snapshot_state(&snapshot)? {
                SnapshotState::PreparedPrior => {
                    if mutations
                        .remove_checked(
                            &snapshot_path,
                            FileExpectation::present_bytes(&snapshot_bytes, 0o600),
                        )
                        .is_err()
                    {
                        return Err(ConfigError::RollbackFailed);
                    }
                }
                SnapshotState::Connected => {}
                SnapshotState::RestoredPrior | SnapshotState::Conflict => {
                    return Err(ConfigError::Conflict);
                }
            }
            return Err(error);
        }
        let snapshot_file = ExistingFile::present(snapshot_bytes, 0o600);
        self.transition_snapshot(
            mutations,
            &mut snapshot,
            &snapshot_file,
            SnapshotPhase::Connected,
        )?;
        Ok(ConnectionStatus::Connected)
    }

    /// Restores the exact pre-connect bytes when the managed values remain unchanged.
    ///
    /// # Errors
    ///
    /// Returns an error for unsafe paths or permissions, invalid ownership state, managed-value
    /// conflicts, or any I/O or rollback failure.
    pub fn disconnect(&self) -> Result<ConnectionStatus, ConfigError> {
        self.disconnect_with(&FilesystemMutations)
    }

    fn disconnect_with(
        &self,
        mutations: &impl FileMutations,
    ) -> Result<ConnectionStatus, ConfigError> {
        self.ensure_private_state_dir()?;
        let _lock = self.lock()?;
        let snapshot_path = self.snapshot_path();
        if checked_metadata(&snapshot_path)?.is_none() {
            return Ok(ConnectionStatus::Disconnected);
        }
        let (mut snapshot, mut snapshot_file) = self.read_snapshot()?;
        if snapshot.managed_fingerprint != self.managed_values().fingerprint() {
            return Err(ConfigError::Conflict);
        }
        match self.snapshot_state(&snapshot)? {
            SnapshotState::PreparedPrior | SnapshotState::RestoredPrior => {
                mutations
                    .remove_checked(&snapshot_path, FileExpectation::present(&snapshot_file))?;
                return Ok(ConnectionStatus::Disconnected);
            }
            SnapshotState::Connected => {}
            SnapshotState::Conflict => return Err(ConfigError::Conflict),
        }

        if snapshot.phase != SnapshotPhase::Restoring {
            snapshot_file = self.transition_snapshot(
                mutations,
                &mut snapshot,
                &snapshot_file,
                SnapshotPhase::Restoring,
            )?;
        }

        let current = self.read_config()?;
        let prior = snapshot.prior_bytes()?;
        let restore_result = if snapshot.prior_existed {
            mutations.atomic_replace(
                &self.config_path,
                FileExpectation::present(&current),
                &prior,
                snapshot.prior_mode,
            )
        } else {
            mutations.remove_checked(&self.config_path, FileExpectation::present(&current))
        };
        if let Err(error) = restore_result {
            return match self.snapshot_state(&snapshot)? {
                SnapshotState::RestoredPrior => {
                    mutations
                        .remove_checked(&snapshot_path, FileExpectation::present(&snapshot_file))?;
                    Ok(ConnectionStatus::Disconnected)
                }
                SnapshotState::Connected => Err(error),
                SnapshotState::PreparedPrior | SnapshotState::Conflict => {
                    Err(ConfigError::Conflict)
                }
            };
        }

        mutations.remove_checked(&snapshot_path, FileExpectation::present(&snapshot_file))?;
        Ok(ConnectionStatus::Disconnected)
    }

    /// Reports whether this manager owns matching values, has no ownership, or is conflicted.
    ///
    /// # Errors
    ///
    /// Returns an error when config or state paths are unsafe, malformed, or unreadable.
    pub fn status(&self) -> Result<ConnectionStatus, ConfigError> {
        let Some(state_metadata) = checked_metadata(&self.state_dir)? else {
            return Ok(ConnectionStatus::Disconnected);
        };
        validate_private_dir_metadata(&self.state_dir, &state_metadata)?;
        let _lock = self.lock()?;
        let snapshot_path = self.snapshot_path();
        let Some(metadata) = checked_metadata(&snapshot_path)? else {
            return Ok(ConnectionStatus::Disconnected);
        };
        validate_private_file(&snapshot_path, &metadata)?;
        let (mut snapshot, snapshot_file) = self.read_snapshot()?;
        if snapshot.managed_fingerprint != self.managed_values().fingerprint() {
            return Ok(ConnectionStatus::Conflict);
        }
        match self.snapshot_state(&snapshot)? {
            SnapshotState::PreparedPrior | SnapshotState::RestoredPrior => {
                remove_checked(&snapshot_path, FileExpectation::present(&snapshot_file))?;
                Ok(ConnectionStatus::Disconnected)
            }
            SnapshotState::Connected => {
                if snapshot.phase != SnapshotPhase::Connected {
                    self.transition_snapshot(
                        &FilesystemMutations,
                        &mut snapshot,
                        &snapshot_file,
                        SnapshotPhase::Connected,
                    )?;
                }
                Ok(ConnectionStatus::Connected)
            }
            SnapshotState::Conflict => Ok(ConnectionStatus::Conflict),
        }
    }

    fn read_config(&self) -> Result<ExistingFile, ConfigError> {
        read_optional_private_file(&self.config_path)
    }

    fn read_snapshot(&self) -> Result<(OwnershipSnapshot, ExistingFile), ConfigError> {
        let existing = read_optional_private_file(&self.snapshot_path())?;
        if !existing.existed {
            return Err(ConfigError::InvalidSnapshot);
        }
        let snapshot: OwnershipSnapshot =
            serde_json::from_slice(&existing.bytes).map_err(|_| ConfigError::InvalidSnapshot)?;
        snapshot.validate_path_and_prior(self)?;
        Ok((snapshot, existing))
    }

    fn snapshot_state(&self, snapshot: &OwnershipSnapshot) -> Result<SnapshotState, ConfigError> {
        let current = match checked_metadata(&self.config_path)? {
            None => ExistingFile {
                existed: false,
                bytes: Vec::new(),
                mode: 0o600,
            },
            Some(metadata) => {
                if !metadata.is_file() {
                    return Err(ConfigError::NonRegularFile(self.config_path.clone()));
                }
                if unix_mode(&metadata) != snapshot.connected_mode {
                    return Ok(SnapshotState::Conflict);
                }
                self.read_config()?
            }
        };
        match snapshot.phase {
            SnapshotPhase::Prepared if snapshot.matches_prior(&current)? => {
                Ok(SnapshotState::PreparedPrior)
            }
            SnapshotPhase::Restoring if snapshot.matches_prior(&current)? => {
                Ok(SnapshotState::RestoredPrior)
            }
            _ if snapshot.matches_connected(&current)? => Ok(SnapshotState::Connected),
            _ => Ok(SnapshotState::Conflict),
        }
    }

    fn transition_snapshot(
        &self,
        mutations: &impl FileMutations,
        snapshot: &mut OwnershipSnapshot,
        current: &ExistingFile,
        phase: SnapshotPhase,
    ) -> Result<ExistingFile, ConfigError> {
        snapshot.phase = phase;
        let bytes = snapshot.to_bytes()?;
        mutations.atomic_replace(
            &self.snapshot_path(),
            FileExpectation::present(current),
            &bytes,
            0o600,
        )?;
        Ok(ExistingFile::present(bytes, 0o600))
    }

    fn snapshot_path(&self) -> PathBuf {
        self.state_dir.join(SNAPSHOT_FILE)
    }

    fn lock(&self) -> Result<MutationLock, ConfigError> {
        MutationLock::acquire(&self.state_dir.join(LOCK_FILE))
    }

    fn ensure_private_state_dir(&self) -> Result<(), ConfigError> {
        if let Some(metadata) = checked_metadata(&self.state_dir)? {
            validate_private_dir_metadata(&self.state_dir, &metadata)
        } else {
            fs::create_dir_all(&self.state_dir)?;
            set_mode(&self.state_dir, 0o700)?;
            validate_private_dir(&self.state_dir)
        }
    }

    fn managed_values(&self) -> ManagedValues {
        ManagedValues {
            binary: self.agentobs_binary.to_string_lossy().into_owned(),
            runtime_root: self.runtime_root.to_string_lossy().into_owned(),
            endpoint: format!("http://127.0.0.1:{}/v1/logs", self.port),
            token: self.token.clone(),
        }
    }
}

trait FileMutations {
    fn atomic_replace(
        &self,
        path: &Path,
        expected: FileExpectation<'_>,
        bytes: &[u8],
        mode: u32,
    ) -> Result<(), ConfigError>;

    fn remove_checked(&self, path: &Path, expected: FileExpectation<'_>)
    -> Result<(), ConfigError>;
}

struct FilesystemMutations;

impl FileMutations for FilesystemMutations {
    fn atomic_replace(
        &self,
        path: &Path,
        expected: FileExpectation<'_>,
        bytes: &[u8],
        mode: u32,
    ) -> Result<(), ConfigError> {
        atomic_replace(path, expected, bytes, mode)
    }

    fn remove_checked(
        &self,
        path: &Path,
        expected: FileExpectation<'_>,
    ) -> Result<(), ConfigError> {
        remove_checked(path, expected)
    }
}

impl OwnershipSnapshot {
    fn validate_path_and_prior(&self, manager: &CodexConfigManager) -> Result<(), ConfigError> {
        let prior = self.prior_bytes()?;
        let connected = self.connected_bytes()?;
        if self.schema_version != SNAPSHOT_VERSION
            || self.config_path != path_identity(&manager.config_path)
            || self.prior_hash != hash(&prior)
            || self.connected_hash != hash(&connected)
            || (!self.prior_existed && !prior.is_empty())
            || self.prior_mode & 0o077 != 0
            || self.connected_mode & 0o077 != 0
            || self.connected_mode != self.prior_mode
        {
            return Err(ConfigError::InvalidSnapshot);
        }
        Ok(())
    }

    fn prior_bytes(&self) -> Result<Vec<u8>, ConfigError> {
        hex_decode(&self.prior_bytes_hex).ok_or(ConfigError::InvalidSnapshot)
    }

    fn connected_bytes(&self) -> Result<Vec<u8>, ConfigError> {
        hex_decode(&self.connected_bytes_hex).ok_or(ConfigError::InvalidSnapshot)
    }

    fn to_bytes(&self) -> Result<Vec<u8>, ConfigError> {
        let mut bytes =
            serde_json::to_vec_pretty(self).map_err(|_| ConfigError::InvalidSnapshot)?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    fn matches_prior(&self, current: &ExistingFile) -> Result<bool, ConfigError> {
        Ok(if self.prior_existed {
            current.matches(&self.prior_bytes()?, self.prior_mode)
        } else {
            !current.existed
        })
    }

    fn matches_connected(&self, current: &ExistingFile) -> Result<bool, ConfigError> {
        Ok(current.matches(&self.connected_bytes()?, self.connected_mode))
    }
}

#[derive(Debug)]
struct ExistingFile {
    existed: bool,
    bytes: Vec<u8>,
    mode: u32,
}

impl ExistingFile {
    fn present(bytes: Vec<u8>, mode: u32) -> Self {
        Self {
            existed: true,
            bytes,
            mode,
        }
    }

    fn matches(&self, bytes: &[u8], mode: u32) -> bool {
        self.existed && self.bytes == bytes && self.mode == mode
    }
}

#[derive(Clone, Copy, Debug)]
enum FileExpectation<'a> {
    Missing,
    Present { bytes: &'a [u8], mode: u32 },
}

impl<'a> FileExpectation<'a> {
    fn from_existing(file: &'a ExistingFile) -> Self {
        if file.existed {
            Self::present(file)
        } else {
            Self::Missing
        }
    }

    fn present(file: &'a ExistingFile) -> Self {
        Self::present_bytes(&file.bytes, file.mode)
    }

    fn present_bytes(bytes: &'a [u8], mode: u32) -> Self {
        Self::Present { bytes, mode }
    }

    fn matches(self, file: &ExistingFile) -> bool {
        match self {
            Self::Missing => !file.existed,
            Self::Present { bytes, mode } => file.matches(bytes, mode),
        }
    }

    fn matches_metadata(self, path: &Path) -> Result<bool, ConfigError> {
        let metadata = checked_metadata(path)?;
        Ok(match (self, metadata) {
            (Self::Missing, None) => true,
            (Self::Present { mode, .. }, Some(metadata)) => {
                metadata.is_file() && unix_mode(&metadata) == mode
            }
            _ => false,
        })
    }
}

#[derive(Debug)]
struct MutationLock {
    _file: File,
}

impl MutationLock {
    fn acquire(path: &Path) -> Result<Self, ConfigError> {
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        }
        let file = options.open(path)?;
        validate_private_file(path, &file.metadata()?)?;
        file.lock()?;
        Ok(Self { _file: file })
    }
}

#[derive(Debug)]
struct ManagedValues {
    binary: String,
    runtime_root: String,
    endpoint: String,
    token: String,
}

impl ManagedValues {
    fn notify(&self) -> Value {
        let mut notify = Array::new();
        notify.push(self.binary.as_str());
        notify.push("codex-notify");
        notify.push(self.runtime_root.as_str());
        Value::Array(notify)
    }

    fn exporter(&self) -> Value {
        let mut headers = InlineTable::new();
        headers.insert("x-agent-observability-token", self.token.as_str().into());
        let mut http = InlineTable::new();
        http.insert("endpoint", self.endpoint.as_str().into());
        http.insert("protocol", "json".into());
        http.insert("headers", Value::InlineTable(headers));
        let mut exporter = InlineTable::new();
        exporter.insert("otlp-http", Value::InlineTable(http));
        Value::InlineTable(exporter)
    }

    fn fingerprint(&self) -> String {
        let mut bytes = self.notify().to_string().into_bytes();
        bytes.extend_from_slice(self.exporter().to_string().as_bytes());
        bytes.extend_from_slice(b"\0false\0local");
        hash(&bytes)
    }
}

fn parse_document(bytes: &[u8]) -> Result<DocumentMut, ConfigError> {
    let input = std::str::from_utf8(bytes)
        .map_err(|_| ConfigError::InvalidArgument("Codex config encoding"))?;
    if input.is_empty() {
        Ok(DocumentMut::new())
    } else {
        Ok(input.parse()?)
    }
}

fn ensure_no_managed_conflict(
    document: &DocumentMut,
    managed: &ManagedValues,
) -> Result<(), ConfigError> {
    ensure_absent_or_matching(document.get("notify"), |item| notify_matches(item, managed))?;
    if let Some(otel) = document.get("otel") {
        let table = otel.as_table_like().ok_or(ConfigError::Conflict)?;
        ensure_absent_or_matching(table.get("exporter"), |item| {
            exporter_matches(item, managed)
        })?;
        ensure_absent_or_matching(table.get("log_user_prompt"), |item| {
            item.as_bool() == Some(false)
        })?;
        ensure_absent_or_matching(table.get("environment"), |item| {
            item.as_str() == Some("local")
        })?;
    }
    Ok(())
}

fn ensure_absent_or_matching(
    item: Option<&Item>,
    matches: impl FnOnce(&Item) -> bool,
) -> Result<(), ConfigError> {
    match item {
        None | Some(Item::None) => Ok(()),
        Some(item) if matches(item) => Ok(()),
        Some(_) => Err(ConfigError::Conflict),
    }
}

fn patch_managed_values(document: &mut DocumentMut, managed: &ManagedValues) {
    if document.get("notify").is_none() {
        document["notify"] = value(managed.notify());
    }
    if document.get("otel").is_none() {
        document["otel"] = Item::Table(toml_edit::Table::new());
    }
    let otel = document["otel"]
        .as_table_like_mut()
        .expect("validated table");
    if otel.get("exporter").is_none() {
        otel.insert("exporter", value(managed.exporter()));
    }
    if otel.get("log_user_prompt").is_none() {
        otel.insert("log_user_prompt", value(false));
    }
    if otel.get("environment").is_none() {
        otel.insert("environment", value("local"));
    }
}

fn notify_matches(item: &Item, managed: &ManagedValues) -> bool {
    let Some(array) = item.as_array() else {
        return false;
    };
    array.len() == 3
        && array.get(0).and_then(Value::as_str) == Some(managed.binary.as_str())
        && array.get(1).and_then(Value::as_str) == Some("codex-notify")
        && array.get(2).and_then(Value::as_str) == Some(managed.runtime_root.as_str())
}

fn exporter_matches(item: &Item, managed: &ManagedValues) -> bool {
    let Some(exporter) = item.as_inline_table() else {
        return false;
    };
    let Some(http) = exporter.get("otlp-http").and_then(Value::as_inline_table) else {
        return false;
    };
    let Some(headers) = http.get("headers").and_then(Value::as_inline_table) else {
        return false;
    };
    exporter.len() == 1
        && http.len() == 3
        && headers.len() == 1
        && http.get("endpoint").and_then(Value::as_str) == Some(managed.endpoint.as_str())
        && http.get("protocol").and_then(Value::as_str) == Some("json")
        && headers
            .get("x-agent-observability-token")
            .and_then(Value::as_str)
            == Some(managed.token.as_str())
}

fn read_optional_private_file(path: &Path) -> Result<ExistingFile, ConfigError> {
    let Some(metadata) = checked_metadata(path)? else {
        return Ok(ExistingFile {
            existed: false,
            bytes: Vec::new(),
            mode: 0o600,
        });
    };
    validate_private_file(path, &metadata)?;
    let max_bytes = if path.file_name().is_some_and(|name| name == SNAPSHOT_FILE) {
        MAX_SNAPSHOT_BYTES
    } else {
        MAX_CONFIG_BYTES
    };
    if metadata.len() > max_bytes {
        return Err(ConfigError::TooLarge(path.into()));
    }
    let mut file = open_read_no_follow(path)?;
    let opened = file.metadata()?;
    validate_private_file(path, &opened)?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(max_bytes + 1)
        .read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > max_bytes {
        return Err(ConfigError::TooLarge(path.into()));
    }
    Ok(ExistingFile {
        existed: true,
        bytes,
        mode: unix_mode(&opened),
    })
}

fn checked_metadata(path: &Path) -> Result<Option<fs::Metadata>, ConfigError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(ConfigError::Symlink(path.into())),
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn validate_private_file(path: &Path, metadata: &fs::Metadata) -> Result<(), ConfigError> {
    if !metadata.is_file() {
        return Err(ConfigError::NonRegularFile(path.into()));
    }
    validate_private_permissions(path, metadata)
}

fn validate_private_dir(path: &Path) -> Result<(), ConfigError> {
    let metadata = checked_metadata(path)?.ok_or_else(|| {
        ConfigError::Io(io::Error::new(
            io::ErrorKind::NotFound,
            "state directory is missing",
        ))
    })?;
    validate_private_dir_metadata(path, &metadata)
}

fn validate_private_dir_metadata(path: &Path, metadata: &fs::Metadata) -> Result<(), ConfigError> {
    if !metadata.is_dir() {
        return Err(ConfigError::NonRegularFile(path.into()));
    }
    validate_private_permissions(path, metadata)
}

#[cfg(unix)]
fn validate_private_permissions(path: &Path, metadata: &fs::Metadata) -> Result<(), ConfigError> {
    use std::os::unix::fs::PermissionsExt;
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(ConfigError::InsecurePermissions(path.into()));
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_private_permissions(_path: &Path, _metadata: &fs::Metadata) -> Result<(), ConfigError> {
    Ok(())
}

#[cfg(unix)]
fn open_read_no_follow(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
}

#[cfg(not(unix))]
fn open_read_no_follow(path: &Path) -> io::Result<File> {
    File::open(path)
}

fn atomic_replace(
    path: &Path,
    expected: FileExpectation<'_>,
    bytes: &[u8],
    mode: u32,
) -> Result<(), ConfigError> {
    let parent = path
        .parent()
        .ok_or(ConfigError::InvalidArgument("configuration path"))?;
    let temp = unique_sibling(path, "new");
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        }
        let mut file = options.open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        set_mode(&temp, mode)?;
        match expected {
            FileExpectation::Missing => install_without_replace(&temp, path)?,
            FileExpectation::Present { .. } => {
                let displaced = unique_sibling(path, "displaced");
                match fs::rename(path, &displaced) {
                    Ok(()) => {}
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {
                        return Err(ConfigError::Conflict);
                    }
                    Err(error) => return Err(error.into()),
                }

                if !expected.matches_metadata(&displaced)? {
                    restore_displaced(&displaced, path)?;
                    return Err(ConfigError::Conflict);
                }
                let displaced_file = read_optional_private_file(&displaced)?;
                if !expected.matches(&displaced_file) {
                    restore_displaced(&displaced, path)?;
                    return Err(ConfigError::Conflict);
                }
                if let Err(error) = install_without_replace(&temp, path) {
                    restore_displaced(&displaced, path)?;
                    return Err(error);
                }

                let installed = read_optional_private_file(path)?;
                let displaced_after = read_optional_private_file(&displaced)?;
                if !installed.matches(bytes, mode) || !expected.matches(&displaced_after) {
                    return Err(ConfigError::Conflict);
                }
                fs::remove_file(&displaced)?;
            }
        }
        fs::remove_file(&temp)?;
        sync_dir(parent)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn install_without_replace(source: &Path, destination: &Path) -> Result<(), ConfigError> {
    match fs::hard_link(source, destination) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Err(ConfigError::Conflict),
        Err(error) => Err(error.into()),
    }
}

fn restore_displaced(displaced: &Path, path: &Path) -> Result<(), ConfigError> {
    install_without_replace(displaced, path)?;
    fs::remove_file(displaced)?;
    if let Some(parent) = path.parent() {
        sync_dir(parent)?;
    }
    Ok(())
}

fn remove_checked(path: &Path, expected: FileExpectation<'_>) -> Result<(), ConfigError> {
    if matches!(expected, FileExpectation::Missing) {
        return if checked_metadata(path)?.is_none() {
            Ok(())
        } else {
            Err(ConfigError::Conflict)
        };
    }
    let parent = path
        .parent()
        .ok_or(ConfigError::InvalidArgument("configuration path"))?;
    let displaced = unique_sibling(path, "removed");
    match fs::rename(path, &displaced) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(ConfigError::Conflict);
        }
        Err(error) => return Err(error.into()),
    }
    if !expected.matches_metadata(&displaced)? {
        restore_displaced(&displaced, path)?;
        return Err(ConfigError::Conflict);
    }
    let displaced_file = read_optional_private_file(&displaced)?;
    if !expected.matches(&displaced_file) {
        restore_displaced(&displaced, path)?;
        return Err(ConfigError::Conflict);
    }
    fs::remove_file(displaced)?;
    sync_dir(parent)?;
    Ok(())
}

fn unique_sibling(path: &Path, kind: &str) -> PathBuf {
    let mut sibling = path.to_path_buf();
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    sibling.set_file_name(format!(
        ".{}.agentobs.{kind}.{}.{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("config"),
        std::process::id(),
        sequence
    ));
    sibling
}

fn sync_dir(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn unix_mode(metadata: &fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o777
}

#[cfg(not(unix))]
fn unix_mode(_metadata: &fs::Metadata) -> u32 {
    0o600
}

fn path_identity(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn hash(bytes: &[u8]) -> String {
    hex_encode(&Sha256::digest(bytes))
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn hex_decode(input: &str) -> Option<Vec<u8>> {
    if !input.len().is_multiple_of(2) {
        return None;
    }
    let bytes = input.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len() / 2);
    for index in (0..bytes.len()).step_by(2) {
        let high = char::from(bytes[index]).to_digit(16)?;
        let low = char::from(bytes[index + 1]).to_digit(16)?;
        decoded.push(u8::try_from((high << 4) | low).ok()?);
    }
    Some(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        cell::Cell,
        sync::{Arc, Barrier},
        thread,
        time::{SystemTime, UNIX_EPOCH},
    };

    struct FaultMutations<'a> {
        calls: Cell<usize>,
        failures: &'a [(usize, bool)],
    }

    impl FaultMutations<'_> {
        fn new(failures: &[(usize, bool)]) -> FaultMutations<'_> {
            FaultMutations {
                calls: Cell::new(0),
                failures,
            }
        }

        fn mutate(
            &self,
            operation: impl FnOnce() -> Result<(), ConfigError>,
        ) -> Result<(), ConfigError> {
            let call = self.calls.get() + 1;
            self.calls.set(call);
            let failure = self
                .failures
                .iter()
                .find(|(failed_call, _)| *failed_call == call);
            if failure.is_some_and(|(_, apply_first)| *apply_first) {
                operation()?;
            } else if failure.is_none() {
                return operation();
            }
            Err(ConfigError::Io(io::Error::other(
                "injected mutation failure",
            )))
        }
    }

    impl FileMutations for FaultMutations<'_> {
        fn atomic_replace(
            &self,
            path: &Path,
            expected: FileExpectation<'_>,
            bytes: &[u8],
            mode: u32,
        ) -> Result<(), ConfigError> {
            self.mutate(|| atomic_replace(path, expected, bytes, mode))
        }

        fn remove_checked(
            &self,
            path: &Path,
            expected: FileExpectation<'_>,
        ) -> Result<(), ConfigError> {
            self.mutate(|| remove_checked(path, expected))
        }
    }

    struct ConcurrentEditMutations {
        config_path: PathBuf,
        edit_started: Arc<Barrier>,
        edit_finished: Arc<Barrier>,
    }

    impl FileMutations for ConcurrentEditMutations {
        fn atomic_replace(
            &self,
            path: &Path,
            expected: FileExpectation<'_>,
            bytes: &[u8],
            mode: u32,
        ) -> Result<(), ConfigError> {
            if path == self.config_path {
                self.edit_started.wait();
                self.edit_finished.wait();
            }
            atomic_replace(path, expected, bytes, mode)
        }

        fn remove_checked(
            &self,
            path: &Path,
            expected: FileExpectation<'_>,
        ) -> Result<(), ConfigError> {
            remove_checked(path, expected)
        }
    }

    fn root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "agentobs-codex-config-{name}-{}-{nonce}",
            std::process::id()
        ))
    }

    fn manager(root: &Path) -> CodexConfigManager {
        CodexConfigManager::new(
            root.join("config.toml"),
            root.join("state"),
            root.join("bin/agent-observability"),
            root.join("runtime"),
            4318,
            "private-token",
        )
        .unwrap()
    }

    fn prepare(root: &Path) {
        fs::create_dir_all(root).unwrap();
        set_mode(root, 0o700).unwrap();
    }

    #[test]
    fn connects_a_new_file_and_removes_it_on_disconnect() {
        let root = root("new");
        prepare(&root);
        let manager = manager(&root);
        assert_eq!(manager.status().unwrap(), ConnectionStatus::Disconnected);
        assert_eq!(manager.connect().unwrap(), ConnectionStatus::Connected);
        assert_eq!(manager.status().unwrap(), ConnectionStatus::Connected);
        let text = fs::read_to_string(&manager.config_path).unwrap();
        assert!(text.contains("codex-notify"));
        assert!(text.contains("http://127.0.0.1:4318/v1/logs"));
        assert_eq!(
            manager.disconnect().unwrap(),
            ConnectionStatus::Disconnected
        );
        assert!(!manager.config_path.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn preserves_unrelated_keys_and_comments() {
        let root = root("preserve");
        prepare(&root);
        let original =
            b"# retained\nmodel = \"gpt-test\" # inline\n\n[otel]\n# retained otel\nextra = 7\n";
        fs::write(root.join("config.toml"), original).unwrap();
        set_mode(&root.join("config.toml"), 0o600).unwrap();
        let manager = manager(&root);
        manager.connect().unwrap();
        let connected = fs::read_to_string(&manager.config_path).unwrap();
        assert!(connected.contains("# retained\n"));
        assert!(connected.contains("model = \"gpt-test\" # inline"));
        assert!(connected.contains("# retained otel\nextra = 7"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_conflicting_notify_and_otel_values() {
        for (name, text) in [
            ("notify", "notify = [\"other\"]\n"),
            ("otel", "[otel]\nenvironment = \"production\"\n"),
        ] {
            let root = root(name);
            prepare(&root);
            fs::write(root.join("config.toml"), text).unwrap();
            set_mode(&root.join("config.toml"), 0o600).unwrap();
            assert!(matches!(
                manager(&root).connect(),
                Err(ConfigError::Conflict)
            ));
            assert_eq!(fs::read_to_string(root.join("config.toml")).unwrap(), text);
            let _ = fs::remove_dir_all(root);
        }
    }

    #[test]
    fn connect_is_idempotent() {
        let root = root("idempotent");
        prepare(&root);
        let manager = manager(&root);
        manager.connect().unwrap();
        let config = fs::read(&manager.config_path).unwrap();
        let snapshot = fs::read(manager.snapshot_path()).unwrap();
        assert_eq!(manager.connect().unwrap(), ConnectionStatus::Connected);
        assert_eq!(fs::read(&manager.config_path).unwrap(), config);
        assert_eq!(fs::read(manager.snapshot_path()).unwrap(), snapshot);
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinks_and_insecure_modes() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let root = root("security");
        prepare(&root);
        let target = root.join("target.toml");
        fs::write(&target, "").unwrap();
        symlink(&target, root.join("config.toml")).unwrap();
        assert!(matches!(
            manager(&root).connect(),
            Err(ConfigError::Symlink(_))
        ));
        fs::remove_file(root.join("config.toml")).unwrap();
        fs::write(root.join("config.toml"), "").unwrap();
        fs::set_permissions(root.join("config.toml"), fs::Permissions::from_mode(0o644)).unwrap();
        assert!(matches!(
            manager(&root).connect(),
            Err(ConfigError::InsecurePermissions(_))
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn disconnect_restores_exact_prior_bytes() {
        let root = root("restore");
        prepare(&root);
        let original = b"# exact bytes\nmodel='gpt-test'\n";
        fs::write(root.join("config.toml"), original).unwrap();
        set_mode(&root.join("config.toml"), 0o400).unwrap();
        let manager = manager(&root);
        manager.connect().unwrap();
        let (snapshot, _) = manager.read_snapshot().unwrap();
        assert_eq!(snapshot.prior_mode, 0o400);
        assert_eq!(snapshot.connected_mode, 0o400);
        manager.disconnect().unwrap();
        assert_eq!(fs::read(&manager.config_path).unwrap(), original);
        assert_eq!(
            unix_mode(&fs::metadata(&manager.config_path).unwrap()),
            0o400
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn connect_snapshot_write_failure_leaves_config_untouched() {
        let root = root("snapshot-write-failure");
        prepare(&root);
        let original = b"model = 'before'\n";
        fs::write(root.join("config.toml"), original).unwrap();
        set_mode(&root.join("config.toml"), 0o400).unwrap();
        let manager = manager(&root);

        assert!(matches!(
            manager.connect_with(&FaultMutations::new(&[(1, false)])),
            Err(ConfigError::Io(_))
        ));
        assert_eq!(fs::read(&manager.config_path).unwrap(), original);
        assert_eq!(
            unix_mode(&fs::metadata(&manager.config_path).unwrap()),
            0o400
        );
        assert!(!manager.snapshot_path().exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn connect_config_write_failure_cleans_prepared_snapshot() {
        let root = root("config-write-failure");
        prepare(&root);
        let original = b"# exact\nmodel = 'before'\n";
        fs::write(root.join("config.toml"), original).unwrap();
        set_mode(&root.join("config.toml"), 0o400).unwrap();
        let manager = manager(&root);

        assert!(matches!(
            manager.connect_with(&FaultMutations::new(&[(2, false)])),
            Err(ConfigError::Io(_))
        ));
        assert_eq!(fs::read(&manager.config_path).unwrap(), original);
        assert_eq!(
            unix_mode(&fs::metadata(&manager.config_path).unwrap()),
            0o400
        );
        assert!(!manager.snapshot_path().exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn stale_prepared_snapshot_with_prior_current_recovers_disconnected() {
        let root = root("connect-cleanup-failure");
        prepare(&root);
        let original = b"model = 'before'\n";
        fs::write(root.join("config.toml"), original).unwrap();
        set_mode(&root.join("config.toml"), 0o600).unwrap();
        let manager = manager(&root);

        assert!(matches!(
            manager.connect_with(&FaultMutations::new(&[(2, false), (3, false)])),
            Err(ConfigError::RollbackFailed)
        ));
        assert_eq!(fs::read(&manager.config_path).unwrap(), original);
        assert!(manager.snapshot_path().exists());
        assert_eq!(manager.status().unwrap(), ConnectionStatus::Disconnected);
        assert!(!manager.snapshot_path().exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn prepared_snapshot_with_connected_current_recovers_connected() {
        let root = root("connect-after-config-crash");
        prepare(&root);
        fs::write(root.join("config.toml"), b"model = 'before'\n").unwrap();
        set_mode(&root.join("config.toml"), 0o600).unwrap();
        let manager = manager(&root);

        assert!(matches!(
            manager.connect_with(&FaultMutations::new(&[(2, true)])),
            Err(ConfigError::Io(_))
        ));
        assert!(
            fs::read_to_string(&manager.config_path)
                .unwrap()
                .contains("codex-notify")
        );
        assert!(manager.snapshot_path().exists());
        assert_eq!(manager.status().unwrap(), ConnectionStatus::Connected);
        let (snapshot, _) = manager.read_snapshot().unwrap();
        assert_eq!(snapshot.phase, SnapshotPhase::Connected);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn restoring_snapshot_with_connected_current_recovers_connected() {
        let root = root("disconnect-before-restore-crash");
        prepare(&root);
        fs::write(root.join("config.toml"), b"model = 'before'\n").unwrap();
        set_mode(&root.join("config.toml"), 0o400).unwrap();
        let manager = manager(&root);
        manager.connect().unwrap();
        let connected = fs::read(&manager.config_path).unwrap();
        let connected_mode = unix_mode(&fs::metadata(&manager.config_path).unwrap());

        assert!(matches!(
            manager.disconnect_with(&FaultMutations::new(&[(1, true)])),
            Err(ConfigError::Io(_))
        ));
        assert_eq!(fs::read(&manager.config_path).unwrap(), connected);
        assert_eq!(
            unix_mode(&fs::metadata(&manager.config_path).unwrap()),
            connected_mode
        );
        assert!(manager.snapshot_path().exists());
        assert_eq!(manager.status().unwrap(), ConnectionStatus::Connected);
        let (snapshot, _) = manager.read_snapshot().unwrap();
        assert_eq!(snapshot.phase, SnapshotPhase::Connected);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn restoring_snapshot_with_prior_current_finishes_cleanup() {
        let root = root("disconnect-after-restore-crash");
        prepare(&root);
        let original = b"model = 'before'\n";
        fs::write(root.join("config.toml"), original).unwrap();
        set_mode(&root.join("config.toml"), 0o400).unwrap();
        let manager = manager(&root);
        manager.connect().unwrap();

        assert!(matches!(
            manager.disconnect_with(&FaultMutations::new(&[(3, false)])),
            Err(ConfigError::Io(_))
        ));
        assert_eq!(fs::read(&manager.config_path).unwrap(), original);
        assert_eq!(
            unix_mode(&fs::metadata(&manager.config_path).unwrap()),
            0o400
        );
        assert!(manager.snapshot_path().exists());
        assert_eq!(manager.status().unwrap(), ConnectionStatus::Disconnected);
        assert!(!manager.snapshot_path().exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn corrupted_snapshot_fails_closed_without_changing_managed_config() {
        let root = root("corrupted-snapshot");
        prepare(&root);
        let manager = manager(&root);
        manager.connect().unwrap();
        let connected = fs::read(&manager.config_path).unwrap();
        fs::write(manager.snapshot_path(), b"not json").unwrap();
        set_mode(&manager.snapshot_path(), 0o600).unwrap();

        assert!(matches!(
            manager.disconnect(),
            Err(ConfigError::InvalidSnapshot)
        ));
        assert_eq!(fs::read(&manager.config_path).unwrap(), connected);
        assert!(manager.snapshot_path().exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn disconnect_conflicts_after_a_managed_edit() {
        let root = root("edit-conflict");
        prepare(&root);
        let manager = manager(&root);
        manager.connect().unwrap();
        let text = fs::read_to_string(&manager.config_path).unwrap();
        fs::write(
            &manager.config_path,
            text.replace("environment = \"local\"", "environment = \"changed\""),
        )
        .unwrap();
        assert_eq!(manager.status().unwrap(), ConnectionStatus::Conflict);
        assert!(matches!(manager.disconnect(), Err(ConfigError::Conflict)));
        assert!(manager.snapshot_path().exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn disconnect_conflicts_after_an_unrelated_edit_and_preserves_it() {
        let root = root("unrelated-edit-conflict");
        prepare(&root);
        fs::write(root.join("config.toml"), b"model = 'before'\n").unwrap();
        set_mode(&root.join("config.toml"), 0o600).unwrap();
        let manager = manager(&root);
        manager.connect().unwrap();
        let mut edited = fs::read(&manager.config_path).unwrap();
        edited.extend_from_slice(b"unrelated = true\n");
        fs::write(&manager.config_path, &edited).unwrap();

        assert_eq!(manager.status().unwrap(), ConnectionStatus::Conflict);
        assert!(matches!(manager.disconnect(), Err(ConfigError::Conflict)));
        assert_eq!(fs::read(&manager.config_path).unwrap(), edited);
        assert!(manager.snapshot_path().exists());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn disconnect_conflicts_after_a_mode_only_edit_and_preserves_it() {
        let root = root("mode-edit-conflict");
        prepare(&root);
        fs::write(root.join("config.toml"), b"model = 'before'\n").unwrap();
        set_mode(&root.join("config.toml"), 0o600).unwrap();
        let manager = manager(&root);
        manager.connect().unwrap();
        let connected = fs::read(&manager.config_path).unwrap();
        set_mode(&manager.config_path, 0o000).unwrap();

        assert_eq!(manager.status().unwrap(), ConnectionStatus::Conflict);
        assert!(matches!(manager.disconnect(), Err(ConfigError::Conflict)));
        assert_eq!(
            unix_mode(&fs::metadata(&manager.config_path).unwrap()),
            0o000
        );
        set_mode(&manager.config_path, 0o400).unwrap();
        assert_eq!(fs::read(&manager.config_path).unwrap(), connected);
        assert!(manager.snapshot_path().exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn concurrent_external_edit_is_not_overwritten() {
        let root = root("concurrent-external-edit");
        prepare(&root);
        fs::write(root.join("config.toml"), b"model = 'before'\n").unwrap();
        set_mode(&root.join("config.toml"), 0o600).unwrap();
        let manager = manager(&root);
        let edited = b"model = 'edited concurrently'\n".to_vec();
        let edit_started = Arc::new(Barrier::new(2));
        let edit_finished = Arc::new(Barrier::new(2));
        let editor_path = manager.config_path.clone();
        let editor_started = Arc::clone(&edit_started);
        let editor_finished = Arc::clone(&edit_finished);
        let editor_bytes = edited.clone();
        let editor = thread::spawn(move || {
            editor_started.wait();
            fs::write(&editor_path, editor_bytes).unwrap();
            editor_finished.wait();
        });
        let mutations = ConcurrentEditMutations {
            config_path: manager.config_path.clone(),
            edit_started,
            edit_finished,
        };

        assert!(matches!(
            manager.connect_with(&mutations),
            Err(ConfigError::Conflict)
        ));
        editor.join().unwrap();
        assert_eq!(fs::read(&manager.config_path).unwrap(), edited);
        assert!(manager.snapshot_path().exists());
        assert_eq!(manager.status().unwrap(), ConnectionStatus::Conflict);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn concurrent_supported_mutations_are_serialized() {
        let root = root("serialized-mutations");
        prepare(&root);
        let manager = Arc::new(manager(&root));
        let start = Arc::new(Barrier::new(3));
        let mut handles = Vec::new();
        for _ in 0..2 {
            let manager = Arc::clone(&manager);
            let start = Arc::clone(&start);
            handles.push(thread::spawn(move || {
                start.wait();
                manager.connect()
            }));
        }
        start.wait();
        for handle in handles {
            assert_eq!(handle.join().unwrap().unwrap(), ConnectionStatus::Connected);
        }
        assert_eq!(manager.status().unwrap(), ConnectionStatus::Connected);
        manager.disconnect().unwrap();
        assert!(!manager.config_path.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn already_managed_config_has_unambiguous_ownership() {
        let root = root("already-managed");
        prepare(&root);
        let manager = manager(&root);
        let mut document = DocumentMut::new();
        patch_managed_values(&mut document, &manager.managed_values());
        let original = document.to_string().into_bytes();
        fs::write(&manager.config_path, &original).unwrap();
        set_mode(&manager.config_path, 0o600).unwrap();

        assert_eq!(manager.connect().unwrap(), ConnectionStatus::Connected);
        assert_eq!(manager.status().unwrap(), ConnectionStatus::Connected);
        assert_eq!(
            manager.disconnect().unwrap(),
            ConnectionStatus::Disconnected
        );
        assert_eq!(fs::read(&manager.config_path).unwrap(), original);
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn ownership_state_is_private_and_contains_exact_prior_and_connected_state() {
        use std::os::unix::fs::PermissionsExt;
        let root = root("private-state");
        prepare(&root);
        let original = b"model = \"before\"\n";
        fs::write(root.join("config.toml"), original).unwrap();
        set_mode(&root.join("config.toml"), 0o600).unwrap();
        let manager = manager(&root);
        manager.connect().unwrap();
        assert_eq!(
            fs::metadata(&manager.state_dir)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(manager.snapshot_path())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        let (snapshot, _) = manager.read_snapshot().unwrap();
        assert_eq!(snapshot.prior_bytes().unwrap(), original);
        assert_eq!(snapshot.prior_hash, hash(original));
        let connected = fs::read(&manager.config_path).unwrap();
        assert_eq!(snapshot.connected_bytes().unwrap(), connected);
        assert_eq!(snapshot.connected_hash, hash(&connected));
        assert_eq!(snapshot.connected_mode, 0o600);
        assert_eq!(snapshot.phase, SnapshotPhase::Connected);
        assert_eq!(
            fs::metadata(manager.state_dir.join(LOCK_FILE))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert!(
            !fs::read_to_string(manager.snapshot_path())
                .unwrap()
                .contains("private-token")
        );
        let _ = fs::remove_dir_all(root);
    }
}
