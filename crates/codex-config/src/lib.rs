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
const AUTH_HEADER_NAME: &str = "x-agent-observability-token";
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
    managed: Option<ManagedValues>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExporterSecurity {
    ca_certificate: String,
    auth_header_value: String,
}

impl ExporterSecurity {
    /// Validates the CA path and private request header used by the Codex OTLP exporter.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::InvalidArgument`] when the path is empty, relative, or not UTF-8,
    /// or when the header value is empty or cannot be represented as an HTTP header value.
    pub fn new(
        ca_certificate: impl Into<PathBuf>,
        auth_header_value: impl Into<String>,
    ) -> Result<Self, ConfigError> {
        let auth_header_value = auth_header_value.into();
        if auth_header_value.is_empty()
            || auth_header_value
                .bytes()
                .any(|byte| !matches!(byte, 0x21..=0x7e))
        {
            return Err(ConfigError::InvalidArgument(
                "collector authentication header value",
            ));
        }
        Ok(Self {
            ca_certificate: managed_path(ca_certificate.into(), "CA certificate path")?,
            auth_header_value,
        })
    }
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pending_rotation: Option<PendingRotation>,
}

#[derive(Debug, Deserialize, Serialize)]
struct PendingRotation {
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
    Rotating,
    Restoring,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SnapshotState {
    PreparedPrior,
    Connected,
    RotationPrepared,
    RotationApplied,
    RestoredPrior,
    Conflict,
}

impl CodexConfigManager {
    /// Creates a manager bound to one config path and one ownership state directory.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::InvalidArgument`] when the binary or runtime path is empty, relative,
    /// or not UTF-8, or when the port is zero. Exporter security values are validated by
    /// [`ExporterSecurity::new`].
    pub fn new(
        config_path: impl Into<PathBuf>,
        state_dir: impl Into<PathBuf>,
        agentobs_binary: impl Into<PathBuf>,
        runtime_root: impl Into<PathBuf>,
        port: u16,
        security: ExporterSecurity,
    ) -> Result<Self, ConfigError> {
        let agentobs_binary = managed_path(agentobs_binary.into(), "agentobs binary path")?;
        let runtime_root = managed_path(runtime_root.into(), "runtime root path")?;
        if port == 0 {
            return Err(ConfigError::InvalidArgument("OTLP port"));
        }
        Ok(Self {
            config_path: config_path.into(),
            state_dir: state_dir.into(),
            managed: Some(ManagedValues {
                binary: agentobs_binary,
                runtime_root,
                endpoint: format!("https://127.0.0.1:{port}/v1/logs"),
                ca_certificate: security.ca_certificate,
                auth_header_value: security.auth_header_value,
            }),
        })
    }

    /// Creates a manager that can inspect and restore an existing ownership snapshot without
    /// collector settings.
    ///
    /// This mode cannot connect or rotate managed values. Status and disconnect trust only the
    /// private ownership snapshot and require the live config to match its exact connected bytes.
    #[must_use]
    pub fn from_ownership_snapshot(
        config_path: impl Into<PathBuf>,
        state_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            config_path: config_path.into(),
            state_dir: state_dir.into(),
            managed: None,
        }
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
        let managed = self.managed_values()?.clone();
        self.ensure_private_state_dir()?;
        let _lock = self.lock()?;
        let snapshot_path = self.snapshot_path();
        if checked_metadata(&snapshot_path)?.is_some()
            && let Some(status) = self.connect_existing(mutations, &snapshot_path)?
        {
            return Ok(status);
        }

        let prior = self.read_config()?;
        let mut document = parse_document(&prior.bytes)?;
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
            pending_rotation: None,
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
                SnapshotState::RotationPrepared
                | SnapshotState::RotationApplied
                | SnapshotState::RestoredPrior
                | SnapshotState::Conflict => {
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

    fn connect_existing(
        &self,
        mutations: &impl FileMutations,
        snapshot_path: &Path,
    ) -> Result<Option<ConnectionStatus>, ConfigError> {
        let (mut snapshot, snapshot_file) = self.read_snapshot()?;
        let managed = self.managed_values()?;
        let managed_fingerprint = managed.fingerprint();
        let state = self.snapshot_state(&snapshot)?;
        if snapshot.phase == SnapshotPhase::Rotating {
            if snapshot.pending_managed_fingerprint() != Some(managed_fingerprint.as_str()) {
                return Err(ConfigError::Conflict);
            }
            self.finish_rotation(mutations, &mut snapshot, &snapshot_file, state)?;
            return Ok(Some(ConnectionStatus::Connected));
        }
        if snapshot.managed_fingerprint != managed_fingerprint {
            if state != SnapshotState::Connected {
                return Err(ConfigError::Conflict);
            }
            self.rotate_connected(mutations, &mut snapshot, &snapshot_file, managed)?;
            return Ok(Some(ConnectionStatus::Connected));
        }
        match state {
            SnapshotState::PreparedPrior | SnapshotState::RestoredPrior => {
                mutations
                    .remove_checked(snapshot_path, FileExpectation::present(&snapshot_file))?;
                Ok(None)
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
                Ok(Some(ConnectionStatus::Connected))
            }
            SnapshotState::RotationPrepared | SnapshotState::RotationApplied => {
                Err(ConfigError::InvalidSnapshot)
            }
            SnapshotState::Conflict => Err(ConfigError::Conflict),
        }
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
        let managed_fingerprint = self.managed.as_ref().map(ManagedValues::fingerprint);
        let mut state = self.snapshot_state(&snapshot)?;
        if snapshot.phase == SnapshotPhase::Rotating {
            if managed_fingerprint.as_deref().is_some_and(|fingerprint| {
                snapshot.pending_managed_fingerprint() != Some(fingerprint)
            }) {
                return Err(ConfigError::Conflict);
            }
            snapshot_file =
                self.finish_rotation(mutations, &mut snapshot, &snapshot_file, state)?;
            state = SnapshotState::Connected;
        } else if managed_fingerprint
            .as_deref()
            .is_some_and(|fingerprint| snapshot.managed_fingerprint != fingerprint)
        {
            return Err(ConfigError::Conflict);
        }
        match state {
            SnapshotState::PreparedPrior | SnapshotState::RestoredPrior => {
                mutations
                    .remove_checked(&snapshot_path, FileExpectation::present(&snapshot_file))?;
                return Ok(ConnectionStatus::Disconnected);
            }
            SnapshotState::Connected => {}
            SnapshotState::RotationPrepared | SnapshotState::RotationApplied => {
                return Err(ConfigError::InvalidSnapshot);
            }
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
                SnapshotState::PreparedPrior
                | SnapshotState::RotationPrepared
                | SnapshotState::RotationApplied
                | SnapshotState::Conflict => Err(ConfigError::Conflict),
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
        Ok(self
            .ownership_status()?
            .unwrap_or(ConnectionStatus::Disconnected))
    }

    /// Reports validated ownership separately from the no-snapshot case.
    ///
    /// # Errors
    ///
    /// Returns an error when config or state paths are unsafe, malformed, or unreadable.
    pub fn ownership_status(&self) -> Result<Option<ConnectionStatus>, ConfigError> {
        let Some(state_metadata) = checked_metadata(&self.state_dir)? else {
            return Ok(None);
        };
        validate_private_dir_metadata(&self.state_dir, &state_metadata)?;
        let _lock = self.lock()?;
        let snapshot_path = self.snapshot_path();
        let Some(metadata) = checked_metadata(&snapshot_path)? else {
            return Ok(None);
        };
        validate_private_file(&snapshot_path, &metadata)?;
        let (mut snapshot, mut snapshot_file) = self.read_snapshot()?;
        let managed_fingerprint = self.managed.as_ref().map(ManagedValues::fingerprint);
        let mut state = self.snapshot_state(&snapshot)?;
        if snapshot.phase == SnapshotPhase::Rotating {
            if managed_fingerprint.as_deref().is_some_and(|fingerprint| {
                snapshot.pending_managed_fingerprint() != Some(fingerprint)
            }) {
                return Ok(Some(ConnectionStatus::Conflict));
            }
            if state == SnapshotState::Conflict {
                return Ok(Some(ConnectionStatus::Conflict));
            }
            snapshot_file =
                self.finish_rotation(&FilesystemMutations, &mut snapshot, &snapshot_file, state)?;
            state = SnapshotState::Connected;
        } else if managed_fingerprint
            .as_deref()
            .is_some_and(|fingerprint| snapshot.managed_fingerprint != fingerprint)
        {
            return Ok(Some(ConnectionStatus::Conflict));
        }
        match state {
            SnapshotState::PreparedPrior | SnapshotState::RestoredPrior => {
                remove_checked(&snapshot_path, FileExpectation::present(&snapshot_file))?;
                Ok(None)
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
                Ok(Some(ConnectionStatus::Connected))
            }
            SnapshotState::RotationPrepared | SnapshotState::RotationApplied => {
                Err(ConfigError::InvalidSnapshot)
            }
            SnapshotState::Conflict => Ok(Some(ConnectionStatus::Conflict)),
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
            SnapshotPhase::Rotating if snapshot.matches_connected(&current)? => {
                Ok(SnapshotState::RotationPrepared)
            }
            SnapshotPhase::Rotating if snapshot.matches_pending(&current)? => {
                Ok(SnapshotState::RotationApplied)
            }
            _ if snapshot.matches_connected(&current)? => Ok(SnapshotState::Connected),
            _ => Ok(SnapshotState::Conflict),
        }
    }

    fn rotate_connected(
        &self,
        mutations: &impl FileMutations,
        snapshot: &mut OwnershipSnapshot,
        snapshot_file: &ExistingFile,
        managed: &ManagedValues,
    ) -> Result<ExistingFile, ConfigError> {
        let mut document = parse_document(&snapshot.connected_bytes()?)?;
        replace_managed_values(&mut document, managed);
        let pending_bytes = document.to_string().into_bytes();
        snapshot.phase = SnapshotPhase::Rotating;
        snapshot.pending_rotation = Some(PendingRotation {
            connected_hash: hash(&pending_bytes),
            connected_bytes_hex: hex_encode(&pending_bytes),
            connected_mode: snapshot.connected_mode,
            managed_fingerprint: managed.fingerprint(),
        });
        let rotating_file = self.write_snapshot(mutations, snapshot, snapshot_file)?;
        self.finish_rotation(
            mutations,
            snapshot,
            &rotating_file,
            SnapshotState::RotationPrepared,
        )
    }

    fn finish_rotation(
        &self,
        mutations: &impl FileMutations,
        snapshot: &mut OwnershipSnapshot,
        snapshot_file: &ExistingFile,
        state: SnapshotState,
    ) -> Result<ExistingFile, ConfigError> {
        if state == SnapshotState::RotationPrepared {
            let pending = snapshot
                .pending_rotation
                .as_ref()
                .ok_or(ConfigError::InvalidSnapshot)?;
            mutations.atomic_replace(
                &self.config_path,
                FileExpectation::present_bytes(
                    &snapshot.connected_bytes()?,
                    snapshot.connected_mode,
                ),
                &pending.connected_bytes()?,
                pending.connected_mode,
            )?;
        } else if state != SnapshotState::RotationApplied {
            return Err(if state == SnapshotState::Conflict {
                ConfigError::Conflict
            } else {
                ConfigError::InvalidSnapshot
            });
        }

        let pending = snapshot
            .pending_rotation
            .take()
            .ok_or(ConfigError::InvalidSnapshot)?;
        snapshot.phase = SnapshotPhase::Connected;
        snapshot.connected_bytes_hex = pending.connected_bytes_hex;
        snapshot.connected_hash = pending.connected_hash;
        snapshot.connected_mode = pending.connected_mode;
        snapshot.managed_fingerprint = pending.managed_fingerprint;
        self.write_snapshot(mutations, snapshot, snapshot_file)
    }

    fn write_snapshot(
        &self,
        mutations: &impl FileMutations,
        snapshot: &OwnershipSnapshot,
        current: &ExistingFile,
    ) -> Result<ExistingFile, ConfigError> {
        let bytes = snapshot.to_bytes()?;
        mutations.atomic_replace(
            &self.snapshot_path(),
            FileExpectation::present(current),
            &bytes,
            0o600,
        )?;
        Ok(ExistingFile::present(bytes, 0o600))
    }

    fn transition_snapshot(
        &self,
        mutations: &impl FileMutations,
        snapshot: &mut OwnershipSnapshot,
        current: &ExistingFile,
        phase: SnapshotPhase,
    ) -> Result<ExistingFile, ConfigError> {
        snapshot.phase = phase;
        self.write_snapshot(mutations, snapshot, current)
    }

    fn snapshot_path(&self) -> PathBuf {
        self.state_dir.join(SNAPSHOT_FILE)
    }

    fn lock(&self) -> Result<MutationLock, ConfigError> {
        let lock_path = self.lock_path()?;
        // Every runtime mutates the same user-global Codex config. Keep its lock at that shared
        // boundary so managers with different ownership state directories still serialize.
        MutationLock::acquire(&lock_path)
    }

    fn lock_path(&self) -> Result<PathBuf, ConfigError> {
        Ok(self
            .config_path
            .parent()
            .ok_or(ConfigError::InvalidArgument("Codex config path"))?
            .join(LOCK_FILE))
    }

    fn ensure_private_state_dir(&self) -> Result<(), ConfigError> {
        let parent = self
            .state_dir
            .parent()
            .ok_or(ConfigError::InvalidArgument("ownership state path"))?;
        ensure_private_directory(parent)?;
        ensure_private_directory(&self.state_dir)
    }

    fn managed_values(&self) -> Result<&ManagedValues, ConfigError> {
        self.managed
            .as_ref()
            .ok_or(ConfigError::InvalidArgument("managed Codex values"))
    }
}

fn ensure_private_directory(path: &Path) -> Result<(), ConfigError> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    match builder.create(path) {
        Ok(()) => {
            if let Some(parent) = path.parent() {
                sync_dir(parent)?;
            }
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }
    validate_private_dir(path)
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
        let pending_is_valid = match &self.pending_rotation {
            Some(pending) => {
                let pending_bytes = pending.connected_bytes()?;
                self.phase == SnapshotPhase::Rotating
                    && pending.connected_hash == hash(&pending_bytes)
                    && pending.connected_mode == self.connected_mode
            }
            None => self.phase != SnapshotPhase::Rotating,
        };
        if self.schema_version != SNAPSHOT_VERSION
            || self.config_path != path_identity(&manager.config_path)
            || self.prior_hash != hash(&prior)
            || self.connected_hash != hash(&connected)
            || (!self.prior_existed && !prior.is_empty())
            || self.prior_mode & 0o077 != 0
            || self.connected_mode & 0o077 != 0
            || self.connected_mode != self.prior_mode
            || !pending_is_valid
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

    fn matches_pending(&self, current: &ExistingFile) -> Result<bool, ConfigError> {
        let pending = self
            .pending_rotation
            .as_ref()
            .ok_or(ConfigError::InvalidSnapshot)?;
        Ok(current.matches(&pending.connected_bytes()?, pending.connected_mode))
    }

    fn pending_managed_fingerprint(&self) -> Option<&str> {
        self.pending_rotation
            .as_ref()
            .map(|pending| pending.managed_fingerprint.as_str())
    }
}

impl PendingRotation {
    fn connected_bytes(&self) -> Result<Vec<u8>, ConfigError> {
        hex_decode(&self.connected_bytes_hex).ok_or(ConfigError::InvalidSnapshot)
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

#[derive(Clone, Debug)]
struct ManagedValues {
    binary: String,
    runtime_root: String,
    endpoint: String,
    ca_certificate: String,
    auth_header_value: String,
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
        let mut tls = InlineTable::new();
        tls.insert("ca-certificate", self.ca_certificate.as_str().into());
        let mut headers = InlineTable::new();
        headers.insert(AUTH_HEADER_NAME, self.auth_header_value.as_str().into());
        let mut http = InlineTable::new();
        http.insert("endpoint", self.endpoint.as_str().into());
        http.insert("protocol", "json".into());
        http.insert("tls", Value::InlineTable(tls));
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

fn managed_path(path: PathBuf, argument: &'static str) -> Result<String, ConfigError> {
    if path.as_os_str().is_empty() || !path.is_absolute() {
        return Err(ConfigError::InvalidArgument(argument));
    }
    path.into_os_string()
        .into_string()
        .map_err(|_| ConfigError::InvalidArgument(argument))
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

fn replace_managed_values(document: &mut DocumentMut, managed: &ManagedValues) {
    document["notify"] = value(managed.notify());
    let otel = document["otel"]
        .as_table_like_mut()
        .expect("manager-owned connected snapshot has an otel table");
    otel.insert("exporter", value(managed.exporter()));
    otel.insert("log_user_prompt", value(false));
    otel.insert("environment", value("local"));
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
    let Some(tls) = http.get("tls").and_then(Value::as_inline_table) else {
        return false;
    };
    let Some(headers) = http.get("headers").and_then(Value::as_inline_table) else {
        return false;
    };
    exporter.len() == 1
        && http.len() == 4
        && tls.len() == 1
        && headers.len() == 1
        && http.get("endpoint").and_then(Value::as_str) == Some(managed.endpoint.as_str())
        && http.get("protocol").and_then(Value::as_str) == Some("json")
        && tls.get("ca-certificate").and_then(Value::as_str)
            == Some(managed.ca_certificate.as_str())
        && headers.get(AUTH_HEADER_NAME).and_then(Value::as_str)
            == Some(managed.auth_header_value.as_str())
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
    atomic_replace_with(path, expected, bytes, mode, |_| Ok(()))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AtomicReplaceBoundary {
    TempSynced,
    ExpectationChecked,
    Renamed,
}

fn atomic_replace_with(
    path: &Path,
    expected: FileExpectation<'_>,
    bytes: &[u8],
    mode: u32,
    mut boundary: impl FnMut(AtomicReplaceBoundary) -> Result<(), ConfigError>,
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
        set_mode(&temp, mode)?;
        file.sync_all()?;
        boundary(AtomicReplaceBoundary::TempSynced)?;

        let current = read_optional_private_file(path)?;
        if !expected.matches(&current) {
            return Err(ConfigError::Conflict);
        }
        boundary(AtomicReplaceBoundary::ExpectationChecked)?;

        // Supported writers share the private lock held by the caller. A filesystem has no
        // portable compare-and-swap that can exclude arbitrary non-cooperating open-FD writes
        // between this exact bytes/mode check and rename. Keeping the canonical name in place
        // until this atomic rename means a crash can expose only the complete old or new file.
        fs::rename(&temp, path)?;
        boundary(AtomicReplaceBoundary::Renamed)?;
        sync_dir(parent)?;

        let installed = read_optional_private_file(path)?;
        if !installed.matches(bytes, mode) {
            return Err(ConfigError::Conflict);
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn remove_checked(path: &Path, expected: FileExpectation<'_>) -> Result<(), ConfigError> {
    let parent = path
        .parent()
        .ok_or(ConfigError::InvalidArgument("configuration path"))?;
    let current = read_optional_private_file(path)?;
    if !expected.matches(&current) {
        return Err(ConfigError::Conflict);
    }
    if current.existed {
        fs::remove_file(path)?;
    }
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
        io::{Seek, SeekFrom},
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
        manager_with_port(root, 4318)
    }

    fn manager_with_port(root: &Path, port: u16) -> CodexConfigManager {
        manager_with_security(root, port, "current")
    }

    fn manager_with_security(root: &Path, port: u16, generation: &str) -> CodexConfigManager {
        let tls_root = root.join(format!("tls/{generation}"));
        manager_with_security_values(
            root,
            port,
            tls_root.join("ca-certificate.pem"),
            format!("token-{generation}"),
        )
        .unwrap()
    }

    fn manager_with_security_values(
        root: &Path,
        port: u16,
        ca_certificate: impl Into<PathBuf>,
        auth_header_value: impl Into<String>,
    ) -> Result<CodexConfigManager, ConfigError> {
        let security = ExporterSecurity::new(ca_certificate, auth_header_value)?;
        CodexConfigManager::new(
            root.join("config.toml"),
            root.join("state"),
            root.join("bin/agent-observability"),
            root.join("runtime"),
            port,
            security,
        )
    }

    fn prepare(root: &Path) {
        fs::create_dir_all(root).unwrap();
        set_mode(root, 0o700).unwrap();
    }

    #[test]
    fn emits_exact_server_authenticated_toml_and_removes_new_file_on_disconnect() {
        let root = root("new");
        prepare(&root);
        let manager = manager(&root);
        assert_eq!(manager.status().unwrap(), ConnectionStatus::Disconnected);
        assert_eq!(manager.connect().unwrap(), ConnectionStatus::Connected);
        assert_eq!(manager.status().unwrap(), ConnectionStatus::Connected);
        let text = fs::read_to_string(&manager.config_path).unwrap();
        assert_eq!(
            text,
            format!(
                concat!(
                    "notify = [\"{0}/bin/agent-observability\", \"codex-notify\", ",
                    "\"{0}/runtime\"]\n",
                    "\n[otel]\n",
                    "exporter = {{ otlp-http = {{ endpoint = ",
                    "\"https://127.0.0.1:4318/v1/logs\", protocol = \"json\", tls = {{ ",
                    "ca-certificate = \"{0}/tls/current/ca-certificate.pem\" }}, ",
                    "headers = {{ x-agent-observability-token = \"token-current\" }} }} }}\n",
                    "log_user_prompt = false\n",
                    "environment = \"local\"\n"
                ),
                root.display()
            )
        );
        assert!(!text.contains("client-certificate"));
        assert!(!text.contains("client-private-key"));
        assert_eq!(
            manager.disconnect().unwrap(),
            ConnectionStatus::Disconnected
        );
        assert!(!manager.config_path.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn connect_creates_a_missing_private_state_parent() {
        let root = root("missing-state-parent");
        prepare(&root);
        let state_dir = root.join("runtime/integrations/codex");
        fs::create_dir(root.join("runtime")).unwrap();
        set_mode(&root.join("runtime"), 0o700).unwrap();
        let manager = CodexConfigManager::new(
            root.join("config.toml"),
            &state_dir,
            root.join("bin/agent-observability"),
            root.join("runtime-root"),
            4318,
            ExporterSecurity::new(root.join("tls/ca-certificate.pem"), "private-token").unwrap(),
        )
        .unwrap();

        assert_eq!(manager.connect().unwrap(), ConnectionStatus::Connected);
        assert_eq!(
            unix_mode(&fs::metadata(root.join("runtime/integrations")).unwrap()),
            0o700
        );
        assert_eq!(unix_mode(&fs::metadata(&state_dir).unwrap()), 0o700);
        assert_eq!(
            manager.disconnect().unwrap(),
            ConnectionStatus::Disconnected
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_invalid_exporter_security_values() {
        let root = root("invalid-tls-paths");
        prepare(&root);
        let absolute = root.join("certificate.pem");

        for result in [
            manager_with_security_values(&root, 4318, "", "private-token"),
            manager_with_security_values(&root, 4318, "relative.pem", "private-token"),
            manager_with_security_values(&root, 4318, &absolute, ""),
            manager_with_security_values(&root, 4318, &absolute, "token with spaces"),
            manager_with_security_values(&root, 4318, &absolute, "token\nwith-control"),
        ] {
            assert!(matches!(result, Err(ConfigError::InvalidArgument(_))));
        }
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_non_utf8_ca_path() {
        use std::os::unix::ffi::OsStringExt;

        let root = root("non-utf8-tls-path");
        prepare(&root);
        let mut invalid = root.clone().into_os_string();
        invalid.push(std::ffi::OsString::from_vec(vec![b'/', 0xff]));
        let invalid = PathBuf::from(invalid);
        assert!(matches!(
            manager_with_security_values(&root, 4318, &invalid, "private-token"),
            Err(ConfigError::InvalidArgument(_))
        ));
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
    fn rejects_non_exact_server_authenticated_exporters() {
        for (name, conflict) in [
            ("missing-token-header", "missing-header"),
            ("tls-additional-key", "tls-additional"),
            ("http-additional-header", "http-additional"),
            ("headers-additional-key", "headers-additional"),
            ("wrong-token", "wrong-token"),
        ] {
            let root = root(name);
            prepare(&root);
            let expected = manager(&root).managed_values().unwrap().clone();
            let endpoint = expected.endpoint;
            let ca = expected.ca_certificate;
            let token = expected.auth_header_value;
            let exporter = match conflict {
                "missing-header" => format!(
                    "{{ otlp-http = {{ endpoint = \"{endpoint}\", protocol = \"json\", \
                     tls = {{ ca-certificate = \"{ca}\" }} }} }}"
                ),
                "tls-additional" => format!(
                    "{{ otlp-http = {{ endpoint = \"{endpoint}\", protocol = \"json\", \
                     tls = {{ ca-certificate = \"{ca}\", domain-name = \"localhost\" }}, \
                     headers = {{ x-agent-observability-token = \"{token}\" }} }} }}"
                ),
                "http-additional" => format!(
                    "{{ otlp-http = {{ endpoint = \"{endpoint}\", protocol = \"json\", \
                     tls = {{ ca-certificate = \"{ca}\" }}, headers = {{ \
                     x-agent-observability-token = \"{token}\" }}, timeout = 10 }} }}"
                ),
                "headers-additional" => format!(
                    "{{ otlp-http = {{ endpoint = \"{endpoint}\", protocol = \"json\", \
                     tls = {{ ca-certificate = \"{ca}\" }}, headers = {{ \
                     x-agent-observability-token = \"{token}\", unknown = \"value\" }} }} }}"
                ),
                "wrong-token" => format!(
                    "{{ otlp-http = {{ endpoint = \"{endpoint}\", protocol = \"json\", \
                     tls = {{ ca-certificate = \"{ca}\" }}, headers = {{ \
                     x-agent-observability-token = \"wrong-token\" }} }} }}"
                ),
                _ => unreachable!(),
            };
            let text = format!("[otel]\nexporter = {exporter}\n");
            fs::write(root.join("config.toml"), &text).unwrap();
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

    #[test]
    fn fingerprint_and_matching_cover_ca_path_and_auth_header() {
        let root = root("security-fingerprint");
        let base = manager(&root).managed_values().unwrap().clone();
        let base_exporter = value(base.exporter());
        let tls_root = root.join("tls/current");
        for candidate in [
            manager_with_security_values(
                &root,
                4318,
                root.join("tls/other-ca.pem"),
                "token-current",
            ),
            manager_with_security_values(
                &root,
                4318,
                tls_root.join("ca-certificate.pem"),
                "other-token",
            ),
        ] {
            let candidate = candidate.unwrap().managed_values().unwrap().clone();
            assert_ne!(base.fingerprint(), candidate.fingerprint());
            assert!(!exporter_matches(&base_exporter, &candidate));
        }
    }

    #[test]
    fn reconnect_rotates_owned_security_values_then_restores_original() {
        let root = root("rotate-security");
        prepare(&root);
        let original = b"# retained\nmodel = 'before'\n";
        fs::write(root.join("config.toml"), original).unwrap();
        set_mode(&root.join("config.toml"), 0o400).unwrap();
        manager_with_security(&root, 4318, "first")
            .connect()
            .unwrap();

        let rotated = manager_with_security(&root, 4318, "second");
        assert_eq!(rotated.connect().unwrap(), ConnectionStatus::Connected);
        let connected = fs::read_to_string(&rotated.config_path).unwrap();
        assert!(connected.contains("https://127.0.0.1:4318/v1/logs"));
        assert!(connected.contains("/tls/second/ca-certificate.pem"));
        assert!(connected.contains("x-agent-observability-token = \"token-second\""));
        assert!(!connected.contains("/tls/first/"));
        assert!(connected.contains("# retained"));
        assert!(connected.contains("model = 'before'"));
        assert_eq!(
            unix_mode(&fs::metadata(&rotated.config_path).unwrap()),
            0o400
        );

        assert_eq!(
            rotated.disconnect().unwrap(),
            ConnectionStatus::Disconnected
        );
        assert_eq!(fs::read(&rotated.config_path).unwrap(), original);
        assert_eq!(
            unix_mode(&fs::metadata(&rotated.config_path).unwrap()),
            0o400
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn security_rotation_conflicts_with_an_intervening_user_edit() {
        let root = root("rotate-security-user-edit");
        prepare(&root);
        let original = b"model = 'before'\n";
        fs::write(root.join("config.toml"), original).unwrap();
        set_mode(&root.join("config.toml"), 0o600).unwrap();
        manager_with_security(&root, 4318, "first")
            .connect()
            .unwrap();
        let rotated = manager_with_security(&root, 4318, "second");
        let mut edited = fs::read(&rotated.config_path).unwrap();
        edited.extend_from_slice(b"user_setting = true\n");
        let edit_started = Arc::new(Barrier::new(2));
        let edit_finished = Arc::new(Barrier::new(2));
        let editor_path = rotated.config_path.clone();
        let editor_started = Arc::clone(&edit_started);
        let editor_finished = Arc::clone(&edit_finished);
        let editor_bytes = edited.clone();
        let editor = thread::spawn(move || {
            editor_started.wait();
            fs::write(editor_path, editor_bytes).unwrap();
            editor_finished.wait();
        });
        let mutations = ConcurrentEditMutations {
            config_path: rotated.config_path.clone(),
            edit_started,
            edit_finished,
        };

        assert!(matches!(
            rotated.connect_with(&mutations),
            Err(ConfigError::Conflict)
        ));
        editor.join().unwrap();
        assert_eq!(fs::read(&rotated.config_path).unwrap(), edited);
        assert!(rotated.snapshot_path().exists());
        assert_eq!(rotated.status().unwrap(), ConnectionStatus::Conflict);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn security_rotation_recovers_at_every_durable_mutation_boundary() {
        for failed_call in 1..=3 {
            for apply_first in [false, true] {
                let root = root(&format!("rotate-crash-{failed_call}-{apply_first}"));
                prepare(&root);
                let original = b"# exact prior\nmodel = 'before'\n";
                fs::write(root.join("config.toml"), original).unwrap();
                set_mode(&root.join("config.toml"), 0o400).unwrap();
                manager_with_security(&root, 4318, "first")
                    .connect()
                    .unwrap();
                let rotated = manager_with_security(&root, 4318, "second");

                assert!(matches!(
                    rotated.connect_with(&FaultMutations::new(&[(failed_call, apply_first)])),
                    Err(ConfigError::Io(_))
                ));
                assert_eq!(rotated.connect().unwrap(), ConnectionStatus::Connected);
                let connected = fs::read_to_string(&rotated.config_path).unwrap();
                assert!(connected.contains("/tls/second/ca-certificate.pem"));
                assert!(connected.contains("x-agent-observability-token = \"token-second\""));
                assert!(!connected.contains("/tls/first/"));
                let (snapshot, _) = rotated.read_snapshot().unwrap();
                assert_eq!(snapshot.phase, SnapshotPhase::Connected);
                assert!(snapshot.pending_rotation.is_none());
                assert_eq!(
                    snapshot.managed_fingerprint,
                    rotated.managed_values().unwrap().fingerprint()
                );

                assert_eq!(
                    rotated.disconnect().unwrap(),
                    ConnectionStatus::Disconnected
                );
                assert_eq!(fs::read(&rotated.config_path).unwrap(), original);
                assert_eq!(
                    unix_mode(&fs::metadata(&rotated.config_path).unwrap()),
                    0o400
                );
                let _ = fs::remove_dir_all(root);
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn rotation_config_write_failure_then_snapshot_disconnect_restores_exact_prior() {
        let root = root("rotate-write-failure-disconnect");
        prepare(&root);
        let original = b"# exact prior\nmodel = 'before'\n";
        fs::write(root.join("config.toml"), original).unwrap();
        set_mode(&root.join("config.toml"), 0o400).unwrap();
        manager_with_security(&root, 4318, "first")
            .connect()
            .unwrap();
        let rotated = manager_with_security(&root, 4318, "second");

        assert!(matches!(
            rotated.connect_with(&FaultMutations::new(&[(2, false)])),
            Err(ConfigError::Io(_))
        ));
        let recovery = CodexConfigManager::from_ownership_snapshot(
            root.join("config.toml"),
            root.join("state"),
        );
        assert_eq!(
            recovery.disconnect().unwrap(),
            ConnectionStatus::Disconnected
        );
        assert_eq!(fs::read(root.join("config.toml")).unwrap(), original);
        assert_eq!(
            unix_mode(&fs::metadata(root.join("config.toml")).unwrap()),
            0o400
        );
        assert!(!recovery.snapshot_path().exists());
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

    #[cfg(unix)]
    #[test]
    fn ownership_recovery_restores_exact_prior_without_managed_settings() {
        let root = root("ownership-recovery-exact");
        prepare(&root);
        let original = b"# exact prior\nmodel = 'before'\n";
        fs::write(root.join("config.toml"), original).unwrap();
        set_mode(&root.join("config.toml"), 0o400).unwrap();
        let manager = manager(&root);
        manager.connect().unwrap();

        let recovery = CodexConfigManager::from_ownership_snapshot(
            root.join("config.toml"),
            root.join("state"),
        );
        assert_eq!(
            recovery.ownership_status().unwrap(),
            Some(ConnectionStatus::Connected)
        );
        assert_eq!(
            recovery.disconnect().unwrap(),
            ConnectionStatus::Disconnected
        );
        assert_eq!(fs::read(root.join("config.toml")).unwrap(), original);
        assert_eq!(
            unix_mode(&fs::metadata(root.join("config.toml")).unwrap()),
            0o400
        );
        assert_eq!(recovery.ownership_status().unwrap(), None);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ownership_recovery_without_snapshot_leaves_unrelated_config_untouched() {
        let root = root("ownership-recovery-none");
        prepare(&root);
        let unrelated = b"model = 'unrelated'\n";
        fs::write(root.join("config.toml"), unrelated).unwrap();
        set_mode(&root.join("config.toml"), 0o600).unwrap();
        let recovery = CodexConfigManager::from_ownership_snapshot(
            root.join("config.toml"),
            root.join("state"),
        );

        assert_eq!(recovery.ownership_status().unwrap(), None);
        assert_eq!(
            recovery.disconnect().unwrap(),
            ConnectionStatus::Disconnected
        );
        assert_eq!(fs::read(root.join("config.toml")).unwrap(), unrelated);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ownership_recovery_conflicts_without_overwriting_external_edit() {
        let root = root("ownership-recovery-conflict");
        prepare(&root);
        fs::write(root.join("config.toml"), b"model = 'before'\n").unwrap();
        set_mode(&root.join("config.toml"), 0o600).unwrap();
        manager(&root).connect().unwrap();
        let edited = b"model = 'external'\n";
        fs::write(root.join("config.toml"), edited).unwrap();
        let recovery = CodexConfigManager::from_ownership_snapshot(
            root.join("config.toml"),
            root.join("state"),
        );

        assert_eq!(
            recovery.ownership_status().unwrap(),
            Some(ConnectionStatus::Conflict)
        );
        assert!(matches!(recovery.disconnect(), Err(ConfigError::Conflict)));
        assert_eq!(fs::read(root.join("config.toml")).unwrap(), edited);
        assert!(recovery.snapshot_path().exists());
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
    fn atomic_replace_crash_boundaries_never_strand_the_canonical_file() {
        for boundary in [
            AtomicReplaceBoundary::TempSynced,
            AtomicReplaceBoundary::ExpectationChecked,
            AtomicReplaceBoundary::Renamed,
        ] {
            let root = root(&format!("replace-crash-{boundary:?}"));
            prepare(&root);
            let path = root.join("config.toml");
            let old = b"model = 'old'\n";
            let new = b"model = 'new'\n";
            fs::write(&path, old).unwrap();
            set_mode(&path, 0o400).unwrap();

            let result = atomic_replace_with(
                &path,
                FileExpectation::present_bytes(old, 0o400),
                new,
                0o600,
                |reached| {
                    if reached == boundary {
                        Err(ConfigError::Io(io::Error::other("simulated crash")))
                    } else {
                        Ok(())
                    }
                },
            );

            assert!(matches!(result, Err(ConfigError::Io(_))));
            assert!(path.exists(), "canonical path missing at {boundary:?}");
            if boundary == AtomicReplaceBoundary::Renamed {
                assert_eq!(fs::read(&path).unwrap(), new);
                assert_eq!(unix_mode(&fs::metadata(&path).unwrap()), 0o600);
            } else {
                assert_eq!(fs::read(&path).unwrap(), old);
                assert_eq!(unix_mode(&fs::metadata(&path).unwrap()), 0o400);
            }
            let _ = fs::remove_dir_all(root);
        }
    }

    #[test]
    fn non_cooperating_open_fd_write_cannot_be_portably_cased() {
        let root = root("open-fd-cas-limit");
        prepare(&root);
        let path = root.join("config.toml");
        let old = b"model = 'old'\n";
        let new = b"model = 'new'\n";
        fs::write(&path, old).unwrap();
        set_mode(&path, 0o600).unwrap();
        let mut external = OpenOptions::new().write(true).open(&path).unwrap();

        atomic_replace_with(
            &path,
            FileExpectation::present_bytes(old, 0o600),
            new,
            0o600,
            |boundary| {
                if boundary == AtomicReplaceBoundary::ExpectationChecked {
                    external.seek(SeekFrom::Start(0))?;
                    external.write_all(b"non-cooperating write\n")?;
                    external.set_len(22)?;
                    external.sync_all()?;
                }
                Ok(())
            },
        )
        .unwrap();

        // There is no portable content-CAS rename against an arbitrary writer. The supported
        // writer still never removes the canonical name: its complete replacement wins here.
        assert_eq!(fs::read(&path).unwrap(), new);
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
    fn two_runtime_roots_contend_on_one_user_config_lock() {
        let root = root("two-runtime-global-lock");
        prepare(&root);
        let config_path = root.join("config.toml");
        let first = CodexConfigManager::new(
            &config_path,
            root.join("runtime-one/codex"),
            root.join("bin/agent-observability"),
            root.join("runtime-one"),
            4318,
            ExporterSecurity::new(root.join("runtime-one/tls/ca.pem"), "token-one").unwrap(),
        )
        .unwrap();
        let second = CodexConfigManager::new(
            &config_path,
            root.join("runtime-two/codex"),
            root.join("bin/agent-observability"),
            root.join("runtime-two"),
            4319,
            ExporterSecurity::new(root.join("runtime-two/tls/ca.pem"), "token-two").unwrap(),
        )
        .unwrap();
        first.ensure_private_state_dir().unwrap();
        second.ensure_private_state_dir().unwrap();

        assert_ne!(first.state_dir, second.state_dir);
        assert_eq!(first.lock_path().unwrap(), second.lock_path().unwrap());
        let first_lock = first.lock().unwrap();
        let contender = OpenOptions::new()
            .read(true)
            .write(true)
            .open(second.lock_path().unwrap())
            .unwrap();
        assert!(matches!(
            contender.try_lock(),
            Err(fs::TryLockError::WouldBlock)
        ));
        drop(first_lock);
        contender.lock().unwrap();

        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn concurrent_state_dir_initialization_is_atomic_and_private() {
        let root = root("state-dir-init");
        prepare(&root);
        let manager = Arc::new(manager(&root));
        let start = Arc::new(Barrier::new(17));
        let mut handles = Vec::new();
        for _ in 0..16 {
            let manager = Arc::clone(&manager);
            let start = Arc::clone(&start);
            handles.push(thread::spawn(move || {
                start.wait();
                manager.ensure_private_state_dir()
            }));
        }
        start.wait();
        for handle in handles {
            handle.join().unwrap().unwrap();
        }
        assert_eq!(unix_mode(&fs::metadata(&manager.state_dir).unwrap()), 0o700);
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn state_dir_initialization_revalidates_an_existing_race_winner() {
        let root = root("state-dir-race-winner");
        prepare(&root);
        let manager = manager(&root);
        fs::create_dir(&manager.state_dir).unwrap();
        set_mode(&manager.state_dir, 0o755).unwrap();

        assert!(matches!(
            manager.ensure_private_state_dir(),
            Err(ConfigError::InsecurePermissions(path)) if path == manager.state_dir
        ));
        assert_eq!(unix_mode(&fs::metadata(&manager.state_dir).unwrap()), 0o755);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn already_managed_config_has_unambiguous_ownership() {
        let root = root("already-managed");
        prepare(&root);
        let manager = manager(&root);
        let mut document = DocumentMut::new();
        patch_managed_values(&mut document, manager.managed_values().unwrap());
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
            fs::metadata(manager.lock_path().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert!(
            !fs::read_to_string(manager.snapshot_path())
                .unwrap()
                .contains("token")
        );
        let _ = fs::remove_dir_all(root);
    }
}
