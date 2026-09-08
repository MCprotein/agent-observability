#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc)]

use agent_observability_codex_config::{
    CodexConfigManager, ConfigError, ConnectionStatus as ConfigConnectionStatus, ExporterSecurity,
    NotifyOwnership,
};
use agent_observability_contracts::CODEX_INTEGRATION_STATUS_VERSION;
pub use agent_observability_contracts::{
    CodexConnectionStatusV1 as ConnectionStatus,
    CodexIntegrationStatusV1 as CodexIntegrationStatus, CodexNotifyStatusV1 as NotifyStatus,
    CollectorStatusV1 as CollectorStatus,
};
use agent_observability_local_collector::{
    CollectorError, CollectorSettings, HealthOutcome, check_health, check_health_details,
    commit_settings_migration, install_settings, load_settings, recover_occupied_persisted_port,
    rollback_settings_migration, settings_migration_pending,
};
use agent_observability_local_runtime::storage_coherence::{
    StorageBarrier, StorageCoherenceError, StorageMutationWriter,
};
use agent_observability_local_runtime::{InstalledLayout, MutationGuard, install};
#[cfg(target_os = "macos")]
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    env, fmt, fs,
    path::{Component, Path, PathBuf},
    time::Duration,
};
#[cfg(target_os = "macos")]
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};

const COLLECTOR_READY_TIMEOUT: Duration = Duration::from_secs(10);
#[cfg(target_os = "macos")]
const LAUNCH_AGENT_OWNERSHIP_VERSION: &str = "agent-observability.launch-agent-ownership.v1";
#[cfg(target_os = "macos")]
const MAX_LAUNCH_AGENT_PLIST_BYTES: u64 = 1024 * 1024;
#[cfg(target_os = "macos")]
const MAX_LAUNCH_AGENT_OWNERSHIP_BYTES: u64 = MAX_LAUNCH_AGENT_PLIST_BYTES * 12 + 8192;
#[cfg(target_os = "macos")]
// Darwin's O_NOFOLLOW value from <sys/fcntl.h>; all uses are macOS-only.
const MACOS_O_NOFOLLOW: i32 = 0x0000_0100;
#[cfg(target_os = "macos")]
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Reports whether a local Codex installation is available for automatic setup.
///
/// Detection is read-only. Explicit `connect codex` retains its existing behavior and may create
/// the default Codex home when the user deliberately requests that integration.
pub fn codex_detected() -> Result<bool, IntegrationError> {
    let codex_home = codex_home_path()?;
    codex_detected_at(&codex_home, env::var_os("PATH").as_deref())
}

fn codex_detected_at(
    codex_home: &Path,
    executable_path: Option<&std::ffi::OsStr>,
) -> Result<bool, IntegrationError> {
    match fs::symlink_metadata(codex_home) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => Err(
            IntegrationError::Runtime("Codex home must be a real directory".into()),
        ),
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(command_on_path("codex", executable_path))
        }
        Err(error) => Err(error.into()),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LaunchAgentOwnershipStatus {
    Absent,
    Owned,
    Conflict,
}

#[derive(Debug)]
pub enum IntegrationError {
    Collector(CollectorError),
    Config(ConfigError),
    Io(std::io::Error),
    Runtime(String),
    Storage(StorageCoherenceError),
    // Completion refers to this file-writing scope, not the entire integration lifecycle.
    StorageWriteUnverified {
        operation_completed: bool,
        primary: Option<Box<IntegrationError>>,
        verification: StorageCoherenceError,
    },
    RecoveryFailed {
        primary: Box<IntegrationError>,
        recovery: Box<IntegrationError>,
    },
    ConnectCommittedSettingsFinalizationUnverified {
        finalization: CollectorError,
    },
    DisconnectCommittedSettingsFinalizationUnverified {
        finalization: CollectorError,
    },
    SettingsRollbackFailed {
        primary: Box<IntegrationError>,
        rollback: CollectorError,
    },
    SettingsRollbackUnverified {
        primary: Box<IntegrationError>,
        rollback: CollectorError,
    },
}

impl fmt::Display for IntegrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Collector(error) => error.fmt(formatter),
            Self::Config(error) => error.fmt(formatter),
            Self::Io(error) => error.fmt(formatter),
            Self::Runtime(message) => formatter.write_str(message),
            Self::Storage(error) => error.fmt(formatter),
            Self::StorageWriteUnverified {
                operation_completed,
                ..
            } => write!(
                formatter,
                "integration storage write unverified (operation completed: {operation_completed}); fresh guarded recovery required"
            ),
            Self::RecoveryFailed { primary, recovery } => {
                if is_storage_write_unverified(recovery) {
                    write!(
                        formatter,
                        "{primary}; recovery final state unverified: {recovery}"
                    )
                } else {
                    write!(formatter, "{primary}; rollback failed: {recovery}")
                }
            }
            Self::ConnectCommittedSettingsFinalizationUnverified { finalization } => write!(
                formatter,
                "Codex integration connect committed; settings finalization unverified: {finalization}"
            ),
            Self::DisconnectCommittedSettingsFinalizationUnverified { finalization } => write!(
                formatter,
                "Codex integration disconnect committed; settings finalization unverified: {finalization}"
            ),
            Self::SettingsRollbackFailed { primary, rollback } => {
                write!(formatter, "{primary}; settings rollback failed: {rollback}")
            }
            Self::SettingsRollbackUnverified { primary, rollback } => write!(
                formatter,
                "{primary}; settings rollback final state unverified: {rollback}"
            ),
        }
    }
}

impl std::error::Error for IntegrationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Collector(error) => Some(error),
            Self::Config(error) => Some(error),
            Self::Io(error) => Some(error),
            Self::Runtime(_) => None,
            Self::Storage(error) => Some(error),
            Self::StorageWriteUnverified {
                primary,
                verification,
                ..
            } => primary
                .as_deref()
                .map(|error| error as &(dyn std::error::Error + 'static))
                .or(Some(verification)),
            Self::RecoveryFailed { primary, .. } => Some(primary.as_ref()),
            Self::ConnectCommittedSettingsFinalizationUnverified { finalization }
            | Self::DisconnectCommittedSettingsFinalizationUnverified { finalization } => {
                Some(finalization)
            }
            Self::SettingsRollbackFailed { rollback, .. }
            | Self::SettingsRollbackUnverified { rollback, .. } => Some(rollback),
        }
    }
}

impl From<CollectorError> for IntegrationError {
    fn from(error: CollectorError) -> Self {
        Self::Collector(error)
    }
}

impl From<ConfigError> for IntegrationError {
    fn from(error: ConfigError) -> Self {
        Self::Config(error)
    }
}

impl From<std::io::Error> for IntegrationError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

pub fn connect(root: &Path, executable: &Path) -> Result<CodexIntegrationStatus, IntegrationError> {
    let layout = install(root).map_err(runtime_error)?;
    with_lifecycle_lock(&layout, || {
        let result = (|| {
            let settings = install_settings(&layout.root)?;
            let (_, restart) = recover_connect_settings(&layout.root, &settings)
                .map_err(|error| rollback_migration_without_service(&layout.root, error))?;
            connect_with_reloaded_settings(
                &layout.root,
                executable,
                &SystemLifecycle,
                restart,
                || load_settings(&layout.root).map_err(Into::into),
                |settings| codex_config_manager(&layout, executable, settings),
            )
        })();
        finish_settings_migration(&layout.root, result)
    })
}

fn finish_settings_migration(
    root: &Path,
    result: Result<CodexIntegrationStatus, IntegrationError>,
) -> Result<CodexIntegrationStatus, IntegrationError> {
    finish_settings_migration_with(root, result, commit_settings_migration)
}

fn finish_settings_migration_with(
    root: &Path,
    result: Result<CodexIntegrationStatus, IntegrationError>,
    finalize: impl FnOnce(&Path) -> Result<(), CollectorError>,
) -> Result<CodexIntegrationStatus, IntegrationError> {
    match result {
        Ok(status) => {
            finalize(root).map_err(|finalization| {
                IntegrationError::ConnectCommittedSettingsFinalizationUnverified { finalization }
            })?;
            Ok(status)
        }
        Err(error) => Err(error),
    }
}

fn settle_settings_migration_for_status(
    root: &Path,
    config: ConfigConnectionStatus,
) -> Result<bool, IntegrationError> {
    match config {
        ConfigConnectionStatus::Connected => {
            commit_settings_migration(root)?;
            Ok(false)
        }
        ConfigConnectionStatus::Disconnected if settings_migration_pending(root)? => {
            rollback_settings_migration(root)?;
            Ok(true)
        }
        ConfigConnectionStatus::Disconnected | ConfigConnectionStatus::Conflict => Ok(false),
    }
}

fn recover_connect_settings(
    root: &Path,
    settings: &CollectorSettings,
) -> Result<(CollectorSettings, bool), IntegrationError> {
    recover_connect_settings_with(root, settings, check_health)
}

fn recover_connect_settings_with(
    root: &Path,
    settings: &CollectorSettings,
    health: impl FnOnce(&Path) -> HealthOutcome,
) -> Result<(CollectorSettings, bool), IntegrationError> {
    if matches!(health(root), HealthOutcome::Ready | HealthOutcome::Degraded) {
        return Ok((settings.clone(), false));
    }
    let recovered = recover_occupied_persisted_port(root, settings)?;
    Ok((recovered, true))
}

fn connect_with_reloaded_settings<C: ConfigLifecycle>(
    root: &Path,
    executable: &Path,
    lifecycle: &impl CollectorLifecycle,
    restart: bool,
    load_after_ready: impl FnOnce() -> Result<CollectorSettings, IntegrationError>,
    config_from_settings: impl FnOnce(&CollectorSettings) -> Result<C, IntegrationError>,
) -> Result<CodexIntegrationStatus, IntegrationError> {
    let service = if restart {
        lifecycle.restart(root, executable)
    } else {
        lifecycle.install(root, executable)
    }?;
    let collector = lifecycle
        .wait_until_ready(root)
        .map_err(|error| rollback_migration_install(root, lifecycle, &service, error))?;
    let settings = load_after_ready()
        .map_err(|error| rollback_migration_install(root, lifecycle, &service, error))?;
    let manager = config_from_settings(&settings)
        .map_err(|error| rollback_migration_install(root, lifecycle, &service, error))?;
    let (config, notify) = match manager.connect() {
        Ok((config, notify)) => (connection_status(config), notify.map(notify_status)),
        Err(error) => {
            return Err(recover_failed_config_connect(
                root, &manager, lifecycle, &service, error,
            ));
        }
    };
    lifecycle.commit_install(&service)?;
    Ok(CodexIntegrationStatus {
        schema_version: CODEX_INTEGRATION_STATUS_VERSION.into(),
        collector_degradation_reasons: Vec::new(),
        config,
        notify,
        collector,
        endpoint: Some(settings.endpoint()),
        service: Some(service.label),
        data_retained: true,
    })
}

#[cfg(test)]
fn connect_prepared(
    root: &Path,
    executable: &Path,
    endpoint: &str,
    config: &impl ConfigLifecycle,
    lifecycle: &impl CollectorLifecycle,
) -> Result<CodexIntegrationStatus, IntegrationError> {
    let service = lifecycle.install(root, executable)?;
    let collector = lifecycle
        .wait_until_ready(root)
        .map_err(|error| rollback_install(lifecycle, &service, error))?;
    let (config_status, notify) = match config.connect() {
        Ok((status, notify)) => (connection_status(status), notify.map(notify_status)),
        Err(error) => return Err(rollback_install(lifecycle, &service, error)),
    };
    lifecycle.commit_install(&service)?;
    Ok(CodexIntegrationStatus {
        schema_version: CODEX_INTEGRATION_STATUS_VERSION.into(),
        collector_degradation_reasons: Vec::new(),
        config: config_status,
        notify,
        collector,
        endpoint: Some(endpoint.into()),
        service: Some(service.label),
        data_retained: true,
    })
}

pub fn disconnect(
    root: &Path,
    executable: &Path,
) -> Result<CodexIntegrationStatus, IntegrationError> {
    let layout = install(root).map_err(runtime_error)?;
    with_lifecycle_lock(&layout, || {
        if let Some(status) = settle_pending_migration_before_disconnect(&layout)? {
            return Ok(status);
        }
        let result = match load_settings(&layout.root) {
            Ok(settings) => {
                disconnect_with_settings(&layout, executable, &settings, &SystemLifecycle)
            }
            Err(CollectorError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                disconnect_without_settings(&layout, executable, &SystemLifecycle)
            }
            Err(error) => Err(error.into()),
        };
        finish_disconnect_settings_migration(&layout.root, result)
    })
}

fn finish_disconnect_settings_migration(
    root: &Path,
    result: Result<CodexIntegrationStatus, IntegrationError>,
) -> Result<CodexIntegrationStatus, IntegrationError> {
    finish_disconnect_settings_migration_with(root, result, rollback_settings_migration)
}

fn finish_disconnect_settings_migration_with(
    root: &Path,
    result: Result<CodexIntegrationStatus, IntegrationError>,
    finalize: impl FnOnce(&Path) -> Result<(), CollectorError>,
) -> Result<CodexIntegrationStatus, IntegrationError> {
    match result {
        Ok(status) => {
            finalize(root).map_err(|finalization| {
                IntegrationError::DisconnectCommittedSettingsFinalizationUnverified { finalization }
            })?;
            Ok(status)
        }
        Err(error) => Err(error),
    }
}

fn disconnect_with_settings(
    layout: &InstalledLayout,
    executable: &Path,
    settings: &CollectorSettings,
    lifecycle: &impl CollectorLifecycle,
) -> Result<CodexIntegrationStatus, IntegrationError> {
    let manager = codex_config_ownership_manager(layout)?;
    disconnect_prepared(
        &layout.root,
        executable,
        &settings.endpoint(),
        &manager,
        lifecycle,
    )
}

fn disconnect_without_settings(
    layout: &InstalledLayout,
    executable: &Path,
    lifecycle: &impl CollectorLifecycle,
) -> Result<CodexIntegrationStatus, IntegrationError> {
    let manager = codex_config_ownership_manager(layout)?;
    let config_ownership = manager.ownership_status()?;
    let service_ownership = launch_agent_ownership_status(&layout.root)?;
    if config_ownership.is_none() && service_ownership == LaunchAgentOwnershipStatus::Absent {
        return Ok(disconnected_status());
    }
    if config_ownership == Some(ConfigConnectionStatus::Conflict)
        || service_ownership == LaunchAgentOwnershipStatus::Conflict
    {
        return Err(ConfigError::Conflict.into());
    }
    disconnect_owned_prepared(&layout.root, executable, None, &manager, lifecycle)
}

fn disconnect_prepared(
    root: &Path,
    executable: &Path,
    endpoint: &str,
    config: &impl ConfigLifecycle,
    lifecycle: &impl CollectorLifecycle,
) -> Result<CodexIntegrationStatus, IntegrationError> {
    disconnect_owned_prepared(root, executable, Some(endpoint), config, lifecycle)
}

fn disconnect_owned_prepared(
    root: &Path,
    executable: &Path,
    endpoint: Option<&str>,
    config: &impl ConfigLifecycle,
    lifecycle: &impl CollectorLifecycle,
) -> Result<CodexIntegrationStatus, IntegrationError> {
    let service = lifecycle.service(root)?;
    if config.status()? == ConfigConnectionStatus::Conflict {
        return Err(ConfigError::Conflict.into());
    }
    lifecycle.uninstall(&service)?;
    let status = match config.disconnect() {
        Ok(status) => {
            lifecycle.commit_uninstall(&service)?;
            status
        }
        Err(error) if is_storage_write_unverified(&error) => return Err(error),
        Err(error) => match config.status() {
            Ok(ConfigConnectionStatus::Connected) => {
                let reinstall = lifecycle
                    .rollback_uninstall(&service, root, executable)
                    .and_then(|()| lifecycle.wait_until_ready(root).map(|_| ()));
                return match reinstall {
                    Ok(()) => Err(error),
                    Err(reinstall) => Err(rollback_error(error, reinstall)),
                };
            }
            Ok(ConfigConnectionStatus::Disconnected | ConfigConnectionStatus::Conflict) => {
                if let Err(commit) = lifecycle.commit_uninstall(&service) {
                    return Err(rollback_error(error, commit));
                }
                return Err(error);
            }
            Err(status_error) => return Err(rollback_error(error, status_error)),
        },
    };
    Ok(CodexIntegrationStatus {
        schema_version: CODEX_INTEGRATION_STATUS_VERSION.into(),
        collector_degradation_reasons: Vec::new(),
        config: connection_status(status),
        notify: None,
        collector: CollectorStatus::Unavailable,
        endpoint: endpoint.map(str::to_owned),
        service: Some(service.label),
        data_retained: true,
    })
}

trait ConfigLifecycle {
    fn status(&self) -> Result<ConfigConnectionStatus, IntegrationError>;
    fn connect(
        &self,
    ) -> Result<(ConfigConnectionStatus, Option<NotifyOwnership>), IntegrationError>;
    fn disconnect(&self) -> Result<ConfigConnectionStatus, IntegrationError>;
}

#[cfg(test)]
impl ConfigLifecycle for CodexConfigManager {
    fn status(&self) -> Result<ConfigConnectionStatus, IntegrationError> {
        self.status().map_err(Into::into)
    }

    fn connect(
        &self,
    ) -> Result<(ConfigConnectionStatus, Option<NotifyOwnership>), IntegrationError> {
        let status = CodexConfigManager::connect(self)?;
        let notify = CodexConfigManager::notify_ownership(self)?;
        Ok((status, notify))
    }

    fn disconnect(&self) -> Result<ConfigConnectionStatus, IntegrationError> {
        self.disconnect().map_err(Into::into)
    }
}

// The root is supplied by the composition boundary, never recovered from a filename.
struct StorageConfigManager {
    root: PathBuf,
    manager: CodexConfigManager,
}

impl StorageConfigManager {
    fn ownership_status(&self) -> Result<Option<ConfigConnectionStatus>, IntegrationError> {
        with_storage_writer(&self.root, || {
            self.manager.ownership_status().map_err(Into::into)
        })
    }

    fn notify_ownership(&self) -> Result<Option<NotifyOwnership>, IntegrationError> {
        with_storage_writer(&self.root, || {
            self.manager.notify_ownership().map_err(Into::into)
        })
    }
}

impl ConfigLifecycle for StorageConfigManager {
    fn status(&self) -> Result<ConfigConnectionStatus, IntegrationError> {
        with_storage_writer(&self.root, || self.manager.status().map_err(Into::into))
    }

    fn connect(
        &self,
    ) -> Result<(ConfigConnectionStatus, Option<NotifyOwnership>), IntegrationError> {
        with_storage_writer(&self.root, || {
            let status = self.manager.connect()?;
            let notify = self.manager.notify_ownership()?;
            Ok((status, notify))
        })
    }

    fn disconnect(&self) -> Result<ConfigConnectionStatus, IntegrationError> {
        with_storage_writer(&self.root, || self.manager.disconnect().map_err(Into::into))
    }
}

fn notify_status(ownership: NotifyOwnership) -> NotifyStatus {
    match ownership {
        NotifyOwnership::AgentobsOwned => NotifyStatus::AgentobsOwned,
        NotifyOwnership::ExternalPreserved => NotifyStatus::ExternalPreserved,
    }
}

trait CollectorLifecycle {
    fn install(&self, root: &Path, executable: &Path)
    -> Result<CollectorService, IntegrationError>;
    fn service(&self, root: &Path) -> Result<CollectorService, IntegrationError>;
    fn restart(
        &self,
        root: &Path,
        executable: &Path,
    ) -> Result<CollectorService, IntegrationError> {
        self.install(root, executable)
    }
    fn wait_until_ready(&self, root: &Path) -> Result<CollectorStatus, IntegrationError>;
    fn commit_install(&self, _service: &CollectorService) -> Result<(), IntegrationError> {
        Ok(())
    }
    fn rollback_install(&self, service: &CollectorService) -> Result<(), IntegrationError> {
        self.uninstall(service)
    }
    fn uninstall(&self, service: &CollectorService) -> Result<(), IntegrationError>;
    fn commit_uninstall(&self, _service: &CollectorService) -> Result<(), IntegrationError> {
        Ok(())
    }
    fn rollback_uninstall(
        &self,
        _service: &CollectorService,
        root: &Path,
        executable: &Path,
    ) -> Result<(), IntegrationError> {
        self.install(root, executable).map(|_| ())
    }
}

struct SystemLifecycle;

impl CollectorLifecycle for SystemLifecycle {
    fn install(
        &self,
        root: &Path,
        executable: &Path,
    ) -> Result<CollectorService, IntegrationError> {
        install_collector_service(root, executable)
    }

    fn service(&self, root: &Path) -> Result<CollectorService, IntegrationError> {
        collector_service(root)
    }

    fn restart(
        &self,
        root: &Path,
        executable: &Path,
    ) -> Result<CollectorService, IntegrationError> {
        restart_collector_service(root, executable)
    }

    fn wait_until_ready(&self, root: &Path) -> Result<CollectorStatus, IntegrationError> {
        wait_for_collector(root)
    }

    fn commit_install(&self, service: &CollectorService) -> Result<(), IntegrationError> {
        commit_collector_service_install(service)
    }

    fn rollback_install(&self, service: &CollectorService) -> Result<(), IntegrationError> {
        rollback_collector_service_install(service)
    }

    fn uninstall(&self, service: &CollectorService) -> Result<(), IntegrationError> {
        uninstall_collector_service(service)
    }

    fn commit_uninstall(&self, service: &CollectorService) -> Result<(), IntegrationError> {
        commit_collector_service_uninstall(service)
    }

    fn rollback_uninstall(
        &self,
        service: &CollectorService,
        _root: &Path,
        _executable: &Path,
    ) -> Result<(), IntegrationError> {
        rollback_collector_service_uninstall(service)
    }
}

#[cfg(test)]
fn rollback_install(
    lifecycle: &impl CollectorLifecycle,
    service: &CollectorService,
    error: IntegrationError,
) -> IntegrationError {
    if is_storage_write_unverified(&error) {
        return error;
    }
    match lifecycle.rollback_install(service) {
        Ok(()) => error,
        Err(rollback) => rollback_error(error, rollback),
    }
}

fn rollback_migration_install(
    root: &Path,
    lifecycle: &impl CollectorLifecycle,
    service: &CollectorService,
    error: IntegrationError,
) -> IntegrationError {
    if is_storage_write_unverified(&error) {
        return error;
    }
    if let Err(rollback) = lifecycle.rollback_install(service) {
        return rollback_error(error, rollback);
    }
    rollback_migration_without_service(root, error)
}

fn recover_failed_config_connect(
    root: &Path,
    config: &impl ConfigLifecycle,
    lifecycle: &impl CollectorLifecycle,
    service: &CollectorService,
    error: IntegrationError,
) -> IntegrationError {
    if is_storage_write_unverified(&error) {
        return error;
    }
    match config.status() {
        Ok(ConfigConnectionStatus::Disconnected) => {
            rollback_migration_install(root, lifecycle, service, error)
        }
        Ok(ConfigConnectionStatus::Connected | ConfigConnectionStatus::Conflict) => error,
        Err(status) => rollback_error(error, status),
    }
}

fn rollback_migration_without_service(root: &Path, error: IntegrationError) -> IntegrationError {
    if is_storage_write_unverified(&error) {
        return error;
    }
    rollback_migration_without_service_with(root, error, rollback_settings_migration)
}

fn rollback_migration_without_service_with(
    root: &Path,
    error: IntegrationError,
    rollback_settings: impl FnOnce(&Path) -> Result<(), CollectorError>,
) -> IntegrationError {
    match rollback_settings(root) {
        Ok(()) => error,
        Err(rollback @ CollectorError::StorageWriteUnverified { .. }) => {
            IntegrationError::SettingsRollbackUnverified {
                primary: Box::new(error),
                rollback,
            }
        }
        Err(rollback) => IntegrationError::SettingsRollbackFailed {
            primary: Box::new(error),
            rollback,
        },
    }
}

fn is_storage_write_unverified(error: &IntegrationError) -> bool {
    match error {
        IntegrationError::Collector(CollectorError::StorageWriteUnverified { .. })
        | IntegrationError::StorageWriteUnverified { .. } => true,
        IntegrationError::RecoveryFailed { primary, recovery } => {
            is_storage_write_unverified(primary) || is_storage_write_unverified(recovery)
        }
        _ => false,
    }
}

fn rollback_error(error: IntegrationError, rollback: IntegrationError) -> IntegrationError {
    IntegrationError::RecoveryFailed {
        primary: Box::new(error),
        recovery: Box::new(rollback),
    }
}

pub fn status(root: &Path, executable: &Path) -> Result<CodexIntegrationStatus, IntegrationError> {
    let layout = install(root).map_err(runtime_error)?;
    with_lifecycle_lock(&layout, || {
        if let Some(status) = settle_pending_migration_before_status(&layout)? {
            return Ok(status);
        }
        let settings = match load_settings(&layout.root) {
            Ok(settings) => settings,
            Err(CollectorError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                return status_without_settings(&layout);
            }
            Err(error) => return Err(error.into()),
        };
        let manager = codex_config_manager(&layout, executable, &settings)?;
        let config = manager.status()?;
        let notify = if config == ConfigConnectionStatus::Connected {
            manager.notify_ownership()?.map(notify_status)
        } else {
            None
        };
        reconcile_collector_service(&layout.root, config)?;
        if settle_settings_migration_for_status(&layout.root, config)? {
            return status_without_settings(&layout);
        }
        let health = check_health_details(&layout.root);
        Ok(CodexIntegrationStatus {
            schema_version: CODEX_INTEGRATION_STATUS_VERSION.into(),
            collector_degradation_reasons: health.degradation_reasons,
            config: connection_status(config),
            notify,
            collector: collector_status(health.outcome),
            endpoint: Some(settings.endpoint()),
            service: Some(service_label(&layout.root)),
            data_retained: true,
        })
    })
}

fn settle_pending_migration_before_status(
    layout: &InstalledLayout,
) -> Result<Option<CodexIntegrationStatus>, IntegrationError> {
    if !settings_migration_pending(&layout.root)? {
        return Ok(None);
    }
    let config = codex_config_ownership_manager(layout)?.ownership_status()?;
    match config {
        Some(ConfigConnectionStatus::Connected) => {
            reconcile_collector_service(&layout.root, ConfigConnectionStatus::Connected)?;
            commit_settings_migration(&layout.root)?;
            Ok(None)
        }
        Some(ConfigConnectionStatus::Conflict) => status_without_settings(layout).map(Some),
        Some(ConfigConnectionStatus::Disconnected) | None => {
            reconcile_collector_service(&layout.root, ConfigConnectionStatus::Disconnected)?;
            rollback_settings_migration(&layout.root)?;
            status_without_settings(layout).map(Some)
        }
    }
}

fn settle_pending_migration_before_disconnect(
    layout: &InstalledLayout,
) -> Result<Option<CodexIntegrationStatus>, IntegrationError> {
    if !settings_migration_pending(&layout.root)? {
        return Ok(None);
    }
    let config = codex_config_ownership_manager(layout)?.ownership_status()?;
    match config {
        Some(ConfigConnectionStatus::Connected) => Ok(None),
        Some(ConfigConnectionStatus::Conflict) => Err(ConfigError::Conflict.into()),
        Some(ConfigConnectionStatus::Disconnected) | None => {
            reconcile_collector_service(&layout.root, ConfigConnectionStatus::Disconnected)?;
            rollback_settings_migration(&layout.root)?;
            status_without_settings(layout).map(Some)
        }
    }
}

fn status_without_settings(
    layout: &InstalledLayout,
) -> Result<CodexIntegrationStatus, IntegrationError> {
    let manager = codex_config_ownership_manager(layout)?;
    let config = manager.ownership_status()?;
    let notify = if config == Some(ConfigConnectionStatus::Connected) {
        manager.notify_ownership()?.map(notify_status)
    } else {
        None
    };
    let service = launch_agent_ownership_status(&layout.root)?;
    Ok(missing_settings_status(
        &layout.root,
        config,
        notify,
        service,
    ))
}

fn missing_settings_status(
    root: &Path,
    config: Option<ConfigConnectionStatus>,
    notify: Option<NotifyStatus>,
    service: LaunchAgentOwnershipStatus,
) -> CodexIntegrationStatus {
    if config.is_none() && service == LaunchAgentOwnershipStatus::Absent {
        return disconnected_status();
    }
    let config = if service == LaunchAgentOwnershipStatus::Conflict
        || (config.is_none() && service == LaunchAgentOwnershipStatus::Owned)
    {
        ConnectionStatus::Conflict
    } else {
        connection_status(config.unwrap_or(ConfigConnectionStatus::Conflict))
    };
    CodexIntegrationStatus {
        schema_version: CODEX_INTEGRATION_STATUS_VERSION.into(),
        collector_degradation_reasons: Vec::new(),
        config,
        notify,
        collector: CollectorStatus::Unavailable,
        endpoint: None,
        service: (service != LaunchAgentOwnershipStatus::Absent).then(|| service_label(root)),
        data_retained: true,
    }
}

fn disconnected_status() -> CodexIntegrationStatus {
    CodexIntegrationStatus {
        schema_version: CODEX_INTEGRATION_STATUS_VERSION.into(),
        collector_degradation_reasons: Vec::new(),
        config: ConnectionStatus::Disconnected,
        notify: None,
        collector: CollectorStatus::Unavailable,
        endpoint: None,
        service: None,
        data_retained: true,
    }
}

fn connection_status(status: ConfigConnectionStatus) -> ConnectionStatus {
    match status {
        ConfigConnectionStatus::Connected => ConnectionStatus::Connected,
        ConfigConnectionStatus::Disconnected => ConnectionStatus::Disconnected,
        ConfigConnectionStatus::Conflict => ConnectionStatus::Conflict,
    }
}

fn collector_status(health: HealthOutcome) -> CollectorStatus {
    match health {
        HealthOutcome::Ready => CollectorStatus::Ready,
        HealthOutcome::Degraded => CollectorStatus::Degraded,
        HealthOutcome::Unavailable => CollectorStatus::Unavailable,
    }
}

fn runtime_error(error: impl fmt::Display) -> IntegrationError {
    IntegrationError::Runtime(error.to_string())
}

fn with_storage_writer<T>(
    root: &Path,
    operation: impl FnOnce() -> Result<T, IntegrationError>,
) -> Result<T, IntegrationError> {
    let barrier = StorageBarrier::open_if_initialized(root).map_err(IntegrationError::Storage)?;
    let writer = StorageMutationWriter::acquire_exclusive(root, barrier.as_ref())
        .map_err(IntegrationError::Storage)?;
    let result = operation();
    let verification = writer.revalidate();
    match verification {
        Ok(()) => result,
        Err(verification) => Err(IntegrationError::StorageWriteUnverified {
            operation_completed: result.is_ok()
                || matches!(
                    &result,
                    Err(IntegrationError::StorageWriteUnverified {
                        operation_completed: true,
                        ..
                    })
                ),
            primary: result.err().map(Box::new),
            verification,
        }),
    }
}

fn acquire_lifecycle_lock(layout: &InstalledLayout) -> Result<MutationGuard, IntegrationError> {
    with_storage_writer(&layout.root, || acquire_lifecycle_lock_unchecked(layout))
}

fn acquire_lifecycle_lock_unchecked(
    layout: &InstalledLayout,
) -> Result<MutationGuard, IntegrationError> {
    ensure_private_runtime_directory(&layout.runtime.join("integrations"))?;
    ensure_private_runtime_directory(&layout.runtime.join("integrations/codex"))?;
    let lock_dir = layout.runtime.join("integrations/codex/lifecycle");
    ensure_private_runtime_directory(&lock_dir)?;
    MutationGuard::try_acquire(&lock_dir).map_err(|error| {
        IntegrationError::Runtime(format!("Codex integration lifecycle is busy: {error}"))
    })
}

fn with_lifecycle_lock<T>(
    layout: &InstalledLayout,
    operation: impl FnOnce() -> Result<T, IntegrationError>,
) -> Result<T, IntegrationError> {
    let _lifecycle = acquire_lifecycle_lock(layout)?;
    operation()
}

fn codex_config_manager(
    layout: &InstalledLayout,
    executable: &Path,
    settings: &CollectorSettings,
) -> Result<StorageConfigManager, IntegrationError> {
    let config_path = codex_config_path()?;
    let codex_home = config_path
        .parent()
        .ok_or_else(|| IntegrationError::Runtime("Codex config path has no parent".into()))?;
    with_storage_writer(&layout.root, || ensure_codex_home(codex_home))?;
    let security = exporter_security(layout, settings)?;
    let manager = CodexConfigManager::new(
        config_path,
        layout.runtime.join("integrations/codex"),
        executable,
        &layout.root,
        settings.port,
        security,
    )?;
    Ok(StorageConfigManager {
        root: layout.root.clone(),
        manager,
    })
}

fn exporter_security(
    layout: &InstalledLayout,
    settings: &CollectorSettings,
) -> Result<ExporterSecurity, IntegrationError> {
    let absolute = |relative: &str| {
        let relative = Path::new(relative);
        if relative.as_os_str().is_empty()
            || relative
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(IntegrationError::Runtime(
                "invalid local collector credential path".into(),
            ));
        }
        Ok(layout.runtime.join(relative))
    };
    ExporterSecurity::new(
        absolute(&settings.credentials.ca_certificate)?,
        settings.auth_token.clone(),
    )
    .map_err(Into::into)
}

fn codex_config_ownership_manager(
    layout: &InstalledLayout,
) -> Result<StorageConfigManager, IntegrationError> {
    Ok(StorageConfigManager {
        root: layout.root.clone(),
        manager: CodexConfigManager::from_ownership_snapshot(
            codex_config_path()?,
            layout.runtime.join("integrations/codex"),
        ),
    })
}

fn codex_config_path() -> Result<PathBuf, IntegrationError> {
    Ok(codex_home_path()?.join("config.toml"))
}

fn codex_home_path() -> Result<PathBuf, IntegrationError> {
    env::var_os("CODEX_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .map_or_else(
            || {
                env::var_os("HOME")
                    .filter(|value| !value.is_empty())
                    .map(PathBuf::from)
                    .map(|home| home.join(".codex"))
                    .ok_or_else(|| IntegrationError::Runtime("HOME is not set".into()))
            },
            Ok,
        )
}

fn command_on_path(command: &str, executable_path: Option<&std::ffi::OsStr>) -> bool {
    executable_path.is_some_and(|path| {
        env::split_paths(path).any(|directory| {
            let candidate = directory.join(command);
            fs::metadata(candidate)
                .is_ok_and(|metadata| metadata.is_file() && executable_by_someone(&metadata))
        })
    })
}

#[cfg(unix)]
fn executable_by_someone(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;

    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn executable_by_someone(_metadata: &fs::Metadata) -> bool {
    true
}

fn ensure_codex_home(path: &Path) -> Result<(), IntegrationError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(IntegrationError::Runtime(
                "Codex home must be a real directory".into(),
            ));
        }
        Ok(_) => return Ok(()),
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
        Err(_) => {}
    }

    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    match builder.create(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let metadata = fs::symlink_metadata(path)?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                Err(IntegrationError::Runtime(
                    "Codex home must be a real directory".into(),
                ))
            } else {
                Ok(())
            }
        }
        Err(error) => Err(IntegrationError::Runtime(format!(
            "cannot create Codex home: {error}"
        ))),
    }
}

#[derive(Clone, Debug)]
struct CollectorService {
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    root: PathBuf,
    label: String,
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    plist: PathBuf,
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    target: String,
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    ownership: PathBuf,
}

fn service_label(root: &Path) -> String {
    let mut digest = Sha256::new();
    digest.update(root.as_os_str().as_encoded_bytes());
    let suffix = digest
        .finalize()
        .iter()
        .take(6)
        .fold(String::new(), |mut output, byte| {
            use std::fmt::Write as _;
            write!(output, "{byte:02x}").expect("writing to String cannot fail");
            output
        });
    format!("io.agent-observability.collector.{suffix}")
}

fn collector_service(root: &Path) -> Result<CollectorService, IntegrationError> {
    let home = env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| IntegrationError::Runtime("HOME is not set".into()))?;
    let label = service_label(root);
    let uid = current_uid()?;
    Ok(CollectorService {
        root: root.to_path_buf(),
        plist: home
            .join("Library/LaunchAgents")
            .join(format!("{label}.plist")),
        target: format!("gui/{uid}/{label}"),
        ownership: root.join("runtime/integrations/codex/launch-agent-ownership-v1.json"),
        label,
    })
}

#[cfg(target_os = "macos")]
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct LaunchAgentFileState {
    existed: bool,
    bytes: Vec<u8>,
    mode: u32,
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug)]
enum LaunchAgentFileExpectation<'a> {
    Missing,
    Present { bytes: &'a [u8], mode: u32 },
}

#[cfg(target_os = "macos")]
impl<'a> LaunchAgentFileExpectation<'a> {
    fn from_state(state: &'a LaunchAgentFileState) -> Self {
        if state.existed {
            Self::Present {
                bytes: &state.bytes,
                mode: state.mode,
            }
        } else {
            Self::Missing
        }
    }

    fn matches(self, state: &LaunchAgentFileState) -> bool {
        match self {
            Self::Missing => !state.existed,
            Self::Present { bytes, mode } => {
                state.existed && state.bytes == bytes && state.mode == mode
            }
        }
    }
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LaunchAgentMutationBoundary {
    ReadyToRevalidate,
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum LaunchAgentOperation {
    Connect,
    Reconnect,
    Disconnect,
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum LaunchAgentPhase {
    Prepared,
    ServiceStopped,
    PlistWritten,
    Bootstrapped,
    Applied,
    Owned,
    Restored,
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
enum LaunchAgentRecovery {
    Connect,
    Disconnect,
    Status(ConfigConnectionStatus),
}

#[cfg(target_os = "macos")]
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct LaunchAgentTransaction {
    schema_version: String,
    plist_path: PathBuf,
    prior_plist: LaunchAgentFileState,
    prior_loaded: bool,
    rollback_plist: LaunchAgentFileState,
    rollback_loaded: bool,
    desired_plist: LaunchAgentFileState,
    desired_loaded: bool,
    operation: LaunchAgentOperation,
    phase: LaunchAgentPhase,
}

#[cfg(target_os = "macos")]
fn install_collector_service(
    root: &Path,
    executable: &Path,
) -> Result<CollectorService, IntegrationError> {
    let service = collector_service(root)?;
    install_collector_service_with(service, root, executable, &SystemLaunchctl, false)
}

#[cfg(target_os = "macos")]
fn restart_collector_service(
    root: &Path,
    executable: &Path,
) -> Result<CollectorService, IntegrationError> {
    let service = collector_service(root)?;
    install_collector_service_with(service, root, executable, &SystemLaunchctl, true)
}

#[cfg(target_os = "macos")]
fn install_collector_service_with(
    service: CollectorService,
    root: &Path,
    executable: &Path,
    launchctl: &impl Launchctl,
    force_reconnect: bool,
) -> Result<CollectorService, IntegrationError> {
    if service.root != root {
        return Err(IntegrationError::Storage(
            StorageCoherenceError::WrongMutationRoot,
        ));
    }
    let owned =
        recover_launch_agent_transaction(&service, launchctl, LaunchAgentRecovery::Connect)?;
    let desired = LaunchAgentFileState {
        existed: true,
        bytes: launch_agent_body(&service.label, executable, root).into_bytes(),
        mode: 0o644,
    };
    if !force_reconnect
        && owned
            .as_ref()
            .is_some_and(|transaction| transaction.desired_plist == desired)
    {
        return Ok(service);
    }

    let (mut transaction, expected_plist) = if let Some(owned) = owned {
        let expected_plist = owned.desired_plist.clone();
        (
            LaunchAgentTransaction {
                rollback_plist: owned.desired_plist.clone(),
                rollback_loaded: owned.desired_loaded,
                desired_plist: desired,
                desired_loaded: true,
                operation: LaunchAgentOperation::Reconnect,
                phase: LaunchAgentPhase::Prepared,
                ..owned
            },
            expected_plist,
        )
    } else {
        let prior_plist = read_launch_agent_file(&service.plist)?;
        let prior_loaded = launchctl.is_loaded(&service.target)?;
        if prior_loaded && !prior_plist.existed {
            return Err(IntegrationError::Runtime(format!(
                "loaded LaunchAgent {} has no restorable plist",
                service.target
            )));
        }
        (
            LaunchAgentTransaction {
                schema_version: LAUNCH_AGENT_OWNERSHIP_VERSION.into(),
                plist_path: service.plist.clone(),
                prior_plist: prior_plist.clone(),
                prior_loaded,
                rollback_plist: prior_plist.clone(),
                rollback_loaded: prior_loaded,
                desired_plist: desired,
                desired_loaded: true,
                operation: LaunchAgentOperation::Connect,
                phase: LaunchAgentPhase::Prepared,
            },
            prior_plist,
        )
    };
    save_launch_agent_transaction(&service, &transaction)?;
    if let Err(error) =
        apply_launch_agent_desired(&service, launchctl, &mut transaction, &expected_plist)
    {
        return Err(rollback_launch_agent_operation(&service, launchctl, error));
    }
    transaction.phase = LaunchAgentPhase::Applied;
    save_launch_agent_transaction(&service, &transaction)?;
    Ok(service)
}

#[cfg(not(target_os = "macos"))]
fn install_collector_service(
    _root: &Path,
    _executable: &Path,
) -> Result<CollectorService, IntegrationError> {
    Err(IntegrationError::Runtime(
        "automatic local collection currently supports macOS".into(),
    ))
}

#[cfg(not(target_os = "macos"))]
fn restart_collector_service(
    _root: &Path,
    _executable: &Path,
) -> Result<CollectorService, IntegrationError> {
    Err(IntegrationError::Runtime(
        "automatic local collection currently supports macOS".into(),
    ))
}

#[cfg(target_os = "macos")]
fn uninstall_collector_service(service: &CollectorService) -> Result<(), IntegrationError> {
    uninstall_collector_service_with(service, &SystemLaunchctl)
}

#[cfg(target_os = "macos")]
fn uninstall_collector_service_with(
    service: &CollectorService,
    launchctl: &impl Launchctl,
) -> Result<(), IntegrationError> {
    let Some(mut transaction) =
        recover_launch_agent_transaction(service, launchctl, LaunchAgentRecovery::Disconnect)?
    else {
        return Ok(());
    };
    if transaction.operation == LaunchAgentOperation::Disconnect
        && transaction.phase == LaunchAgentPhase::Restored
    {
        return Ok(());
    }
    let expected_plist = transaction.desired_plist.clone();
    transaction.rollback_plist = transaction.desired_plist.clone();
    transaction.rollback_loaded = transaction.desired_loaded;
    transaction.desired_plist = transaction.prior_plist.clone();
    transaction.desired_loaded = transaction.prior_loaded;
    transaction.operation = LaunchAgentOperation::Disconnect;
    transaction.phase = LaunchAgentPhase::Prepared;
    save_launch_agent_transaction(service, &transaction)?;
    apply_launch_agent_desired(service, launchctl, &mut transaction, &expected_plist)?;
    transaction.phase = LaunchAgentPhase::Restored;
    save_launch_agent_transaction(service, &transaction)
}

#[cfg(target_os = "macos")]
fn reconcile_collector_service(
    root: &Path,
    config: ConfigConnectionStatus,
) -> Result<(), IntegrationError> {
    let service = collector_service(root)?;
    recover_launch_agent_transaction(
        &service,
        &SystemLaunchctl,
        LaunchAgentRecovery::Status(config),
    )
    .map(|_| ())
}

#[cfg(target_os = "macos")]
fn launch_agent_ownership_status(
    root: &Path,
) -> Result<LaunchAgentOwnershipStatus, IntegrationError> {
    let service = collector_service(root)?;
    let Some(transaction) = load_launch_agent_transaction(&service)? else {
        return Ok(LaunchAgentOwnershipStatus::Absent);
    };
    if launch_agent_transaction_conflicts(&service, &transaction)? {
        return Ok(LaunchAgentOwnershipStatus::Conflict);
    }
    Ok(LaunchAgentOwnershipStatus::Owned)
}

#[cfg(not(target_os = "macos"))]
#[allow(clippy::unnecessary_wraps)] // Keep the cross-platform lifecycle contract uniform.
fn reconcile_collector_service(
    _root: &Path,
    _config: ConfigConnectionStatus,
) -> Result<(), IntegrationError> {
    Ok(())
}

#[cfg(not(target_os = "macos"))]
#[allow(clippy::unnecessary_wraps)] // Keep the cross-platform lifecycle contract uniform.
fn launch_agent_ownership_status(
    _root: &Path,
) -> Result<LaunchAgentOwnershipStatus, IntegrationError> {
    Ok(LaunchAgentOwnershipStatus::Absent)
}

#[cfg(target_os = "macos")]
fn commit_collector_service_install(service: &CollectorService) -> Result<(), IntegrationError> {
    let Some(mut transaction) = load_launch_agent_transaction(service)? else {
        return Err(IntegrationError::Runtime(
            "LaunchAgent install ownership state is missing".into(),
        ));
    };
    if transaction.operation == LaunchAgentOperation::Connect
        && transaction.phase == LaunchAgentPhase::Owned
    {
        return Ok(());
    }
    if !matches!(
        transaction.operation,
        LaunchAgentOperation::Connect | LaunchAgentOperation::Reconnect
    ) || transaction.phase != LaunchAgentPhase::Applied
    {
        return Err(IntegrationError::Runtime(
            "LaunchAgent install is not ready to commit".into(),
        ));
    }
    transaction.operation = LaunchAgentOperation::Connect;
    transaction.phase = LaunchAgentPhase::Owned;
    save_launch_agent_transaction(service, &transaction)
}

#[cfg(not(target_os = "macos"))]
#[allow(clippy::unnecessary_wraps)] // Keep the cross-platform lifecycle contract uniform.
fn commit_collector_service_install(_service: &CollectorService) -> Result<(), IntegrationError> {
    Ok(())
}

#[cfg(target_os = "macos")]
fn rollback_collector_service_install(service: &CollectorService) -> Result<(), IntegrationError> {
    recover_launch_agent_transaction(service, &SystemLaunchctl, LaunchAgentRecovery::Connect)
        .map(|_| ())
}

#[cfg(not(target_os = "macos"))]
fn rollback_collector_service_install(_service: &CollectorService) -> Result<(), IntegrationError> {
    Err(IntegrationError::Runtime(
        "automatic local collection currently supports macOS".into(),
    ))
}

#[cfg(target_os = "macos")]
fn commit_collector_service_uninstall(service: &CollectorService) -> Result<(), IntegrationError> {
    let Some(transaction) = load_launch_agent_transaction(service)? else {
        return Ok(());
    };
    if transaction.operation != LaunchAgentOperation::Disconnect
        || transaction.phase != LaunchAgentPhase::Restored
    {
        return Err(IntegrationError::Runtime(
            "LaunchAgent disconnect is not ready to commit".into(),
        ));
    }
    remove_launch_agent_transaction(service)
}

#[cfg(not(target_os = "macos"))]
#[allow(clippy::unnecessary_wraps)] // Keep the cross-platform lifecycle contract uniform.
fn commit_collector_service_uninstall(_service: &CollectorService) -> Result<(), IntegrationError> {
    Ok(())
}

#[cfg(target_os = "macos")]
fn rollback_collector_service_uninstall(
    service: &CollectorService,
) -> Result<(), IntegrationError> {
    let Some(mut transaction) = load_launch_agent_transaction(service)? else {
        return Err(IntegrationError::Runtime(
            "LaunchAgent disconnect ownership state is missing".into(),
        ));
    };
    if transaction.operation != LaunchAgentOperation::Disconnect {
        return Err(IntegrationError::Runtime(
            "LaunchAgent ownership state is not disconnecting".into(),
        ));
    }
    let expected_plist = validated_launch_agent_file(service, &transaction)?;
    transaction.desired_plist = transaction.rollback_plist.clone();
    transaction.desired_loaded = transaction.rollback_loaded;
    transaction.phase = LaunchAgentPhase::Prepared;
    save_launch_agent_transaction(service, &transaction)?;
    apply_launch_agent_desired(service, &SystemLaunchctl, &mut transaction, &expected_plist)?;
    transaction.operation = LaunchAgentOperation::Connect;
    transaction.phase = LaunchAgentPhase::Owned;
    save_launch_agent_transaction(service, &transaction)
}

#[cfg(not(target_os = "macos"))]
fn rollback_collector_service_uninstall(
    _service: &CollectorService,
) -> Result<(), IntegrationError> {
    Err(IntegrationError::Runtime(
        "automatic local collection currently supports macOS".into(),
    ))
}

#[cfg(target_os = "macos")]
fn sync_launch_agent_parent(path: &Path, action: &str) -> Result<(), IntegrationError> {
    let parent = path
        .parent()
        .ok_or_else(|| IntegrationError::Runtime("LaunchAgent path has no parent".into()))?;
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| {
            IntegrationError::Runtime(format!("LaunchAgent {action} sync failed: {error}"))
        })
}

#[cfg(target_os = "macos")]
fn recover_launch_agent_transaction(
    service: &CollectorService,
    launchctl: &impl Launchctl,
    recovery: LaunchAgentRecovery,
) -> Result<Option<LaunchAgentTransaction>, IntegrationError> {
    let Some(mut transaction) = load_launch_agent_transaction(service)? else {
        return Ok(None);
    };
    let expected_plist = validated_launch_agent_file(service, &transaction)?;
    if transaction.phase == LaunchAgentPhase::Owned {
        if launchctl.is_loaded(&service.target)? != transaction.desired_loaded {
            transaction.operation = LaunchAgentOperation::Reconnect;
            transaction.rollback_plist = transaction.desired_plist.clone();
            transaction.rollback_loaded = transaction.desired_loaded;
            transaction.phase = LaunchAgentPhase::Prepared;
            save_launch_agent_transaction(service, &transaction)?;
            apply_launch_agent_desired(service, launchctl, &mut transaction, &expected_plist)?;
            transaction.phase = LaunchAgentPhase::Owned;
            save_launch_agent_transaction(service, &transaction)?;
        }
        return Ok(Some(transaction));
    }

    match transaction.operation {
        LaunchAgentOperation::Connect => {
            if let LaunchAgentRecovery::Status(ConfigConnectionStatus::Connected) = recovery {
                transaction.phase = LaunchAgentPhase::Owned;
                save_launch_agent_transaction(service, &transaction)?;
                Ok(Some(transaction))
            } else {
                transaction.desired_plist = transaction.rollback_plist.clone();
                transaction.desired_loaded = transaction.rollback_loaded;
                apply_launch_agent_desired(service, launchctl, &mut transaction, &expected_plist)?;
                remove_launch_agent_transaction(service)?;
                Ok(None)
            }
        }
        LaunchAgentOperation::Reconnect => {
            if let LaunchAgentRecovery::Status(ConfigConnectionStatus::Connected) = recovery {
                transaction.operation = LaunchAgentOperation::Connect;
                transaction.phase = LaunchAgentPhase::Owned;
                save_launch_agent_transaction(service, &transaction)?;
                Ok(Some(transaction))
            } else {
                transaction.desired_plist = transaction.rollback_plist.clone();
                transaction.desired_loaded = transaction.rollback_loaded;
                apply_launch_agent_desired(service, launchctl, &mut transaction, &expected_plist)?;
                transaction.operation = LaunchAgentOperation::Connect;
                transaction.phase = LaunchAgentPhase::Owned;
                save_launch_agent_transaction(service, &transaction)?;
                Ok(Some(transaction))
            }
        }
        LaunchAgentOperation::Disconnect => match recovery {
            LaunchAgentRecovery::Connect
            | LaunchAgentRecovery::Status(ConfigConnectionStatus::Connected) => {
                transaction.desired_plist = transaction.rollback_plist.clone();
                transaction.desired_loaded = transaction.rollback_loaded;
                apply_launch_agent_desired(service, launchctl, &mut transaction, &expected_plist)?;
                transaction.operation = LaunchAgentOperation::Connect;
                transaction.phase = LaunchAgentPhase::Owned;
                save_launch_agent_transaction(service, &transaction)?;
                Ok(Some(transaction))
            }
            LaunchAgentRecovery::Disconnect
            | LaunchAgentRecovery::Status(ConfigConnectionStatus::Disconnected) => {
                apply_launch_agent_desired(service, launchctl, &mut transaction, &expected_plist)?;
                transaction.phase = LaunchAgentPhase::Restored;
                save_launch_agent_transaction(service, &transaction)?;
                if matches!(recovery, LaunchAgentRecovery::Status(_)) {
                    remove_launch_agent_transaction(service)?;
                    Ok(None)
                } else {
                    Ok(Some(transaction))
                }
            }
            LaunchAgentRecovery::Status(ConfigConnectionStatus::Conflict) => {
                Err(IntegrationError::Runtime(
                    "cannot reconcile LaunchAgent with conflicting Codex config".into(),
                ))
            }
        },
    }
}

#[cfg(target_os = "macos")]
fn launch_agent_transaction_conflicts(
    service: &CollectorService,
    transaction: &LaunchAgentTransaction,
) -> Result<bool, IntegrationError> {
    let current = read_launch_agent_file(&service.plist)?;
    if transaction.phase == LaunchAgentPhase::Owned {
        return Ok(current != transaction.desired_plist);
    }
    Ok(current != transaction.rollback_plist && current != transaction.desired_plist)
}

#[cfg(target_os = "macos")]
fn validated_launch_agent_file(
    service: &CollectorService,
    transaction: &LaunchAgentTransaction,
) -> Result<LaunchAgentFileState, IntegrationError> {
    let current = read_launch_agent_file(&service.plist)?;
    if (transaction.phase == LaunchAgentPhase::Owned && current != transaction.desired_plist)
        || (transaction.phase != LaunchAgentPhase::Owned
            && current != transaction.rollback_plist
            && current != transaction.desired_plist)
    {
        return Err(IntegrationError::Runtime(
            "LaunchAgent plist changed outside the owned transaction".into(),
        ));
    }
    Ok(current)
}

#[cfg(target_os = "macos")]
fn rollback_launch_agent_operation(
    service: &CollectorService,
    launchctl: &impl Launchctl,
    error: IntegrationError,
) -> IntegrationError {
    if is_storage_write_unverified(&error) {
        return error;
    }
    match recover_launch_agent_transaction(service, launchctl, LaunchAgentRecovery::Connect) {
        Ok(_) => error,
        Err(rollback) => rollback_error(error, rollback),
    }
}

#[cfg(target_os = "macos")]
fn apply_launch_agent_desired(
    service: &CollectorService,
    launchctl: &impl Launchctl,
    transaction: &mut LaunchAgentTransaction,
    expected_plist: &LaunchAgentFileState,
) -> Result<(), IntegrationError> {
    apply_launch_agent_desired_with(service, launchctl, transaction, expected_plist, |_| Ok(()))
}

#[cfg(target_os = "macos")]
fn apply_launch_agent_desired_with(
    service: &CollectorService,
    launchctl: &impl Launchctl,
    transaction: &mut LaunchAgentTransaction,
    expected_plist: &LaunchAgentFileState,
    boundary: impl FnMut(LaunchAgentMutationBoundary) -> Result<(), IntegrationError>,
) -> Result<(), IntegrationError> {
    stop_launch_agent(launchctl, service)?;
    transaction.phase = LaunchAgentPhase::ServiceStopped;
    save_launch_agent_transaction(service, transaction)?;

    replace_launch_agent_file_with(
        &service.plist,
        LaunchAgentFileExpectation::from_state(expected_plist),
        &transaction.desired_plist,
        boundary,
    )?;
    transaction.phase = LaunchAgentPhase::PlistWritten;
    save_launch_agent_transaction(service, transaction)?;

    if transaction.desired_loaded {
        if !transaction.desired_plist.existed {
            return Err(IntegrationError::Runtime(
                "cannot load a LaunchAgent without a plist".into(),
            ));
        }
        let domain = service
            .target
            .rsplit_once('/')
            .map(|(domain, _)| domain)
            .ok_or_else(|| IntegrationError::Runtime("invalid LaunchAgent target".into()))?;
        launchctl.bootstrap(domain, &service.plist)?;
        transaction.phase = LaunchAgentPhase::Bootstrapped;
        save_launch_agent_transaction(service, transaction)?;
        if !launchctl.is_loaded(&service.target)? {
            launchctl.kickstart(&service.target)?;
        }
        if !launchctl.is_loaded(&service.target)? {
            return Err(IntegrationError::Runtime(format!(
                "LaunchAgent start unconfirmed for {}",
                service.target
            )));
        }
    } else if launchctl.is_loaded(&service.target)? {
        return Err(IntegrationError::Runtime(format!(
            "LaunchAgent termination unconfirmed for {}",
            service.target
        )));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn stop_launch_agent(
    launchctl: &impl Launchctl,
    service: &CollectorService,
) -> Result<(), IntegrationError> {
    stop_launch_agent_with(launchctl, service, 40, std::thread::sleep)
}

#[cfg(target_os = "macos")]
fn stop_launch_agent_with(
    launchctl: &impl Launchctl,
    service: &CollectorService,
    max_checks: usize,
    mut wait: impl FnMut(Duration),
) -> Result<(), IntegrationError> {
    if !launchctl.is_loaded(&service.target)? {
        return Ok(());
    }
    let bootout = launchctl.bootout(&service.target);
    for check in 0..max_checks.max(1) {
        match launchctl.is_loaded(&service.target) {
            Ok(false) => return Ok(()),
            Ok(true) if check + 1 < max_checks.max(1) => {
                wait(Duration::from_millis(25));
            }
            Ok(true) => break,
            Err(status_error) => {
                return match bootout {
                    Ok(_) => Err(status_error),
                    Err(error) => Err(rollback_error(error, status_error)),
                };
            }
        }
    }
    match bootout {
        Ok(_) => Err(IntegrationError::Runtime(format!(
            "LaunchAgent stop failed for {}",
            service.target
        ))),
        Err(error) => Err(error),
    }
}

#[cfg(target_os = "macos")]
fn open_bounded_regular_file(
    path: &Path,
    max_bytes: u64,
    require_private: bool,
    name: &str,
) -> Result<Option<(File, fs::Metadata)>, IntegrationError> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(IntegrationError::Runtime(format!(
            "{name} must be a regular file"
        )));
    }
    if require_private && metadata.permissions().mode() & 0o077 != 0 {
        return Err(IntegrationError::Runtime(format!(
            "{name} is not a private regular file"
        )));
    }
    if metadata.len() > max_bytes {
        return Err(IntegrationError::Runtime(format!(
            "{name} exceeds size bound"
        )));
    }

    let file = OpenOptions::new()
        .read(true)
        .custom_flags(MACOS_O_NOFOLLOW)
        .open(path)?;
    let opened = file.metadata()?;
    if !opened.is_file() {
        return Err(IntegrationError::Runtime(format!(
            "{name} must be a regular file"
        )));
    }
    if require_private && opened.permissions().mode() & 0o077 != 0 {
        return Err(IntegrationError::Runtime(format!(
            "{name} is not a private regular file"
        )));
    }
    if opened.len() > max_bytes {
        return Err(IntegrationError::Runtime(format!(
            "{name} exceeds size bound"
        )));
    }
    Ok(Some((file, opened)))
}

#[cfg(target_os = "macos")]
fn read_launch_agent_file(path: &Path) -> Result<LaunchAgentFileState, IntegrationError> {
    use std::os::unix::fs::PermissionsExt;

    let Some((mut file, metadata)) = open_bounded_regular_file(
        path,
        MAX_LAUNCH_AGENT_PLIST_BYTES,
        false,
        "LaunchAgent plist",
    )?
    else {
        return Ok(LaunchAgentFileState {
            existed: false,
            bytes: Vec::new(),
            mode: 0,
        });
    };
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(MAX_LAUNCH_AGENT_PLIST_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_LAUNCH_AGENT_PLIST_BYTES {
        return Err(IntegrationError::Runtime(
            "LaunchAgent plist exceeds size bound".into(),
        ));
    }
    Ok(LaunchAgentFileState {
        existed: true,
        bytes,
        mode: metadata.permissions().mode() & 0o777,
    })
}

#[cfg(target_os = "macos")]
fn replace_launch_agent_file_with(
    path: &Path,
    expected: LaunchAgentFileExpectation<'_>,
    desired: &LaunchAgentFileState,
    mut boundary: impl FnMut(LaunchAgentMutationBoundary) -> Result<(), IntegrationError>,
) -> Result<(), IntegrationError> {
    let parent = path
        .parent()
        .ok_or_else(|| IntegrationError::Runtime("LaunchAgent path has no parent".into()))?;
    fs::create_dir_all(parent).map_err(|error| {
        IntegrationError::Runtime(format!("LaunchAgent directory failed: {error}"))
    })?;
    if desired.existed {
        atomic_write_checked(
            path,
            expected,
            &desired.bytes,
            desired.mode,
            "LaunchAgent",
            &mut boundary,
        )
    } else {
        boundary(LaunchAgentMutationBoundary::ReadyToRevalidate)?;
        let current = read_launch_agent_file(path)?;
        if !expected.matches(&current) {
            return Err(launch_agent_conflict());
        }
        if current.existed {
            fs::remove_file(path).map_err(|error| {
                IntegrationError::Runtime(format!("LaunchAgent removal failed: {error}"))
            })?;
        }
        sync_launch_agent_parent(path, "removal")
    }
}

#[cfg(target_os = "macos")]
fn launch_agent_conflict() -> IntegrationError {
    IntegrationError::Runtime("LaunchAgent plist changed outside the owned transaction".into())
}

#[cfg(target_os = "macos")]
fn load_launch_agent_transaction(
    service: &CollectorService,
) -> Result<Option<LaunchAgentTransaction>, IntegrationError> {
    validate_service_storage_path(service)?;
    let Some((mut file, _)) = open_bounded_regular_file(
        &service.ownership,
        MAX_LAUNCH_AGENT_OWNERSHIP_BYTES,
        true,
        "LaunchAgent ownership state",
    )?
    else {
        return Ok(None);
    };
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(MAX_LAUNCH_AGENT_OWNERSHIP_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_LAUNCH_AGENT_OWNERSHIP_BYTES {
        return Err(IntegrationError::Runtime(
            "LaunchAgent ownership state exceeds size bound".into(),
        ));
    }
    let transaction: LaunchAgentTransaction = serde_json::from_slice(&bytes).map_err(|error| {
        IntegrationError::Runtime(format!("invalid LaunchAgent ownership state: {error}"))
    })?;
    validate_launch_agent_transaction_file_states(&transaction)?;
    if transaction.schema_version != LAUNCH_AGENT_OWNERSHIP_VERSION
        || transaction.plist_path != service.plist
    {
        return Err(IntegrationError::Runtime(
            "LaunchAgent ownership state does not match this service".into(),
        ));
    }
    Ok(Some(transaction))
}

#[cfg(target_os = "macos")]
fn save_launch_agent_transaction(
    service: &CollectorService,
    transaction: &LaunchAgentTransaction,
) -> Result<(), IntegrationError> {
    with_storage_writer(&service.root, || {
        save_launch_agent_transaction_guarded(service, transaction)
    })
}

#[cfg(target_os = "macos")]
fn save_launch_agent_transaction_guarded(
    service: &CollectorService,
    transaction: &LaunchAgentTransaction,
) -> Result<(), IntegrationError> {
    validate_launch_agent_transaction_file_states(transaction)?;
    validate_service_storage_path(service)?;
    let integrations = service.root.join("runtime/integrations");
    ensure_private_runtime_directory(&integrations)?;
    ensure_private_runtime_directory(&integrations.join("codex"))?;
    let mut bytes = serde_json::to_vec(transaction).map_err(|error| {
        IntegrationError::Runtime(format!("LaunchAgent state encode failed: {error}"))
    })?;
    bytes.push(b'\n');
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_LAUNCH_AGENT_OWNERSHIP_BYTES {
        return Err(IntegrationError::Runtime(
            "LaunchAgent ownership state exceeds size bound".into(),
        ));
    }
    atomic_write(
        &service.ownership,
        &bytes,
        0o600,
        "LaunchAgent ownership state",
    )
}

#[cfg(target_os = "macos")]
fn validate_launch_agent_transaction_file_states(
    transaction: &LaunchAgentTransaction,
) -> Result<(), IntegrationError> {
    for state in [
        &transaction.prior_plist,
        &transaction.rollback_plist,
        &transaction.desired_plist,
    ] {
        if u64::try_from(state.bytes.len()).unwrap_or(u64::MAX) > MAX_LAUNCH_AGENT_PLIST_BYTES {
            return Err(IntegrationError::Runtime(
                "LaunchAgent ownership state contains an oversized plist".into(),
            ));
        }
        if (!state.existed && (!state.bytes.is_empty() || state.mode != 0))
            || state.mode & !0o777 != 0
        {
            return Err(IntegrationError::Runtime(
                "LaunchAgent ownership state contains an invalid plist state".into(),
            ));
        }
    }
    Ok(())
}

fn ensure_private_runtime_directory(path: &Path) -> Result<(), IntegrationError> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    match builder.create(path) {
        Ok(()) => {
            let parent = path.parent().ok_or_else(|| {
                IntegrationError::Runtime("private runtime directory has no parent".into())
            })?;
            fs::File::open(parent)?.sync_all()?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }
    let metadata = fs::symlink_metadata(path)?;
    #[cfg(unix)]
    let private = {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode().trailing_zeros() >= 6
    };
    #[cfg(not(unix))]
    let private = true;
    if metadata.file_type().is_symlink() || !metadata.is_dir() || !private {
        return Err(IntegrationError::Runtime(
            "integration state directory must be private and real".into(),
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn remove_launch_agent_transaction(service: &CollectorService) -> Result<(), IntegrationError> {
    with_storage_writer(&service.root, || {
        validate_service_storage_path(service)?;
        match fs::remove_file(&service.ownership) {
            Ok(()) => sync_launch_agent_parent(&service.ownership, "state removal"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    })
}

#[cfg(target_os = "macos")]
fn validate_service_storage_path(service: &CollectorService) -> Result<(), IntegrationError> {
    if service.ownership
        != service
            .root
            .join("runtime/integrations/codex/launch-agent-ownership-v1.json")
    {
        return Err(IntegrationError::Storage(
            StorageCoherenceError::WrongMutationRoot,
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn atomic_write(path: &Path, bytes: &[u8], mode: u32, name: &str) -> Result<(), IntegrationError> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let parent = path
        .parent()
        .ok_or_else(|| IntegrationError::Runtime(format!("{name} path has no parent")))?;
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(
        ".{}.agentobs.{}.{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("state"),
        std::process::id(),
        sequence
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .custom_flags(MACOS_O_NOFOLLOW)
            .open(&temporary)?;
        file.write_all(bytes)?;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(mode))?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        File::open(path)?.sync_all()?;
        sync_launch_agent_parent(path, "write")
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(target_os = "macos")]
fn atomic_write_checked(
    path: &Path,
    expected: LaunchAgentFileExpectation<'_>,
    bytes: &[u8],
    mode: u32,
    name: &str,
    boundary: &mut impl FnMut(LaunchAgentMutationBoundary) -> Result<(), IntegrationError>,
) -> Result<(), IntegrationError> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let parent = path
        .parent()
        .ok_or_else(|| IntegrationError::Runtime(format!("{name} path has no parent")))?;
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(
        ".{}.agentobs.{}.{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("state"),
        std::process::id(),
        sequence
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .custom_flags(MACOS_O_NOFOLLOW)
            .open(&temporary)?;
        file.write_all(bytes)?;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(mode))?;
        file.sync_all()?;
        boundary(LaunchAgentMutationBoundary::ReadyToRevalidate)?;

        let current = read_launch_agent_file(path)?;
        if !expected.matches(&current) {
            return Err(launch_agent_conflict());
        }
        fs::rename(&temporary, path)?;
        sync_launch_agent_parent(path, "write")?;

        let installed = read_launch_agent_file(path)?;
        if installed
            != (LaunchAgentFileState {
                existed: true,
                bytes: bytes.to_vec(),
                mode,
            })
        {
            return Err(launch_agent_conflict());
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(not(target_os = "macos"))]
fn uninstall_collector_service(_service: &CollectorService) -> Result<(), IntegrationError> {
    Err(IntegrationError::Runtime(
        "automatic local collection currently supports macOS".into(),
    ))
}

fn wait_for_collector(root: &Path) -> Result<CollectorStatus, IntegrationError> {
    let started = std::time::Instant::now();
    loop {
        let status = collector_status(check_health(root));
        if matches!(status, CollectorStatus::Ready | CollectorStatus::Degraded) {
            return Ok(status);
        }
        if started.elapsed() >= COLLECTOR_READY_TIMEOUT {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(IntegrationError::Runtime(
        "local collector did not become ready within 10 seconds".into(),
    ))
}

#[cfg(target_os = "macos")]
fn current_uid() -> Result<String, IntegrationError> {
    let output = Command::new("id")
        .arg("-u")
        .output()
        .map_err(|error| IntegrationError::Runtime(format!("cannot determine user id: {error}")))?;
    if !output.status.success() {
        return Err(IntegrationError::Runtime("cannot determine user id".into()));
    }
    String::from_utf8(output.stdout)
        .map(|uid| uid.trim().to_owned())
        .map_err(|_| IntegrationError::Runtime("invalid user id output".into()))
}

#[cfg(not(target_os = "macos"))]
fn current_uid() -> Result<String, IntegrationError> {
    Err(IntegrationError::Runtime(
        "automatic local collection currently supports macOS".into(),
    ))
}

#[cfg(target_os = "macos")]
trait Launchctl {
    fn bootout(&self, target: &str) -> Result<bool, IntegrationError>;
    fn is_loaded(&self, _target: &str) -> Result<bool, IntegrationError> {
        Ok(true)
    }
    fn bootstrap(&self, domain: &str, plist: &Path) -> Result<(), IntegrationError>;
    fn kickstart(&self, target: &str) -> Result<(), IntegrationError>;
}

#[cfg(target_os = "macos")]
struct SystemLaunchctl;

#[cfg(target_os = "macos")]
impl Launchctl for SystemLaunchctl {
    fn bootout(&self, target: &str) -> Result<bool, IntegrationError> {
        Command::new("launchctl")
            .args(["bootout", target])
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .map_err(|error| IntegrationError::Runtime(format!("LaunchAgent stop failed: {error}")))
    }

    fn is_loaded(&self, target: &str) -> Result<bool, IntegrationError> {
        Command::new("launchctl")
            .args(["print", target])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .map_err(|error| {
                IntegrationError::Runtime(format!("LaunchAgent status check failed: {error}"))
            })
    }

    fn bootstrap(&self, domain: &str, plist: &Path) -> Result<(), IntegrationError> {
        command_success(
            Command::new("launchctl")
                .arg("bootstrap")
                .arg(domain)
                .arg(plist),
            "LaunchAgent bootstrap",
        )
    }

    fn kickstart(&self, target: &str) -> Result<(), IntegrationError> {
        command_success(
            Command::new("launchctl").args(["kickstart", target]),
            "LaunchAgent start",
        )
    }
}

#[cfg(target_os = "macos")]
fn command_success(command: &mut Command, name: &str) -> Result<(), IntegrationError> {
    let status = command
        .status()
        .map_err(|error| IntegrationError::Runtime(format!("{name} failed: {error}")))?;
    if !status.success() {
        return Err(IntegrationError::Runtime(format!(
            "{name} failed with status {status}"
        )));
    }
    Ok(())
}

#[cfg(any(target_os = "macos", test))]
fn launch_agent_body(label: &str, executable: &Path, root: &Path) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict>\n<key>Label</key><string>{}</string>\n<key>ProgramArguments</key><array><string>{}</string><string>collector-serve</string><string>{}</string></array>\n<key>RunAtLoad</key><true/>\n<key>KeepAlive</key><true/>\n<key>ProcessType</key><string>Background</string>\n</dict></plist>\n",
        xml_escape(label),
        xml_escape(&executable.display().to_string()),
        xml_escape(&root.display().to_string()),
    )
}

#[cfg(any(target_os = "macos", test))]
fn xml_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::{
        CodexIntegrationStatus, CollectorLifecycle, CollectorService, CollectorStatus,
        ConfigConnectionStatus, ConfigError, ConfigLifecycle, ConnectionStatus, IntegrationError,
        LaunchAgentOwnershipStatus, NotifyOwnership, NotifyStatus, codex_detected_at,
        collector_status, connect_prepared, connect_with_reloaded_settings,
        disconnect_owned_prepared, disconnect_prepared, disconnected_status, ensure_codex_home,
        exporter_security, finish_disconnect_settings_migration,
        finish_disconnect_settings_migration_with, finish_settings_migration,
        finish_settings_migration_with, launch_agent_body, missing_settings_status,
        recover_connect_settings, recover_connect_settings_with,
        rollback_migration_without_service_with, service_label,
        settle_pending_migration_before_disconnect, settle_pending_migration_before_status,
        settle_settings_migration_for_status, status, with_lifecycle_lock,
    };
    #[cfg(target_os = "macos")]
    use super::{
        LaunchAgentFileState, LaunchAgentMutationBoundary, LaunchAgentOperation, LaunchAgentPhase,
        LaunchAgentRecovery, LaunchAgentTransaction, Launchctl, MAX_LAUNCH_AGENT_OWNERSHIP_BYTES,
        MAX_LAUNCH_AGENT_PLIST_BYTES, apply_launch_agent_desired_with,
        commit_collector_service_install, commit_collector_service_uninstall,
        install_collector_service_with, load_launch_agent_transaction, read_launch_agent_file,
        recover_launch_agent_transaction, save_launch_agent_transaction, stop_launch_agent_with,
        uninstall_collector_service_with,
    };
    use agent_observability_codex_config::{CodexConfigManager, ExporterSecurity};
    use agent_observability_local_collector::{
        HealthOutcome, install_settings, load_settings, rollback_settings_migration,
    };
    use agent_observability_local_runtime::{LocalConfigService, install};
    #[cfg(target_os = "macos")]
    use std::collections::VecDeque;
    use std::{
        cell::{Cell, RefCell},
        fs,
        net::TcpListener,
        path::{Path, PathBuf},
        sync::{Arc, Mutex, mpsc},
        thread,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    fn temporary_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "agentobs-codex-integration-{name}-{}-{nonce}",
            std::process::id()
        ))
    }

    fn test_exporter_security(root: &Path) -> ExporterSecurity {
        ExporterSecurity::new(root.join("ca-certificate.pem"), "private-token").unwrap()
    }

    #[test]
    fn lifecycle_creation_respects_held_accounting_permit() {
        use agent_observability_local_runtime::{MutationGuard, storage_coherence::StorageBarrier};
        let root = temporary_root("lifecycle-accounting");
        let layout = install(&root).unwrap();
        let mutation = MutationGuard::try_acquire(&layout.runtime).unwrap();
        let barrier = StorageBarrier::initialize(&root, &mutation).unwrap();
        drop(mutation);
        let permit = barrier.try_begin_write().unwrap();
        let result = super::acquire_lifecycle_lock(&layout);
        let created = layout.runtime.join("integrations/codex/lifecycle").exists();
        assert!(
            result.is_err(),
            "accounting must exclude lifecycle creation"
        );
        assert!(!created);
        drop(permit);
        super::acquire_lifecycle_lock(&layout).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn lifecycle_creation_does_not_recreate_missing_root() {
        let root = temporary_root("lifecycle-missing-root");
        let layout = install(&root).unwrap();
        fs::remove_dir_all(&root).unwrap();
        let result = super::acquire_lifecycle_lock(&layout);
        assert!(result.is_err());
        assert!(!root.exists());
    }

    #[test]
    fn storage_writer_postchecks_success_and_primary_failure() {
        for coordinated in [false, true] {
            for completed in [false, true] {
                let root = temporary_root("integration-postcheck");
                let layout = install(&root).unwrap();
                if coordinated {
                    initialize_accounting(&root);
                }
                let result = super::with_storage_writer(&root, || {
                    fs::write(root.join("runtime/published-fixture"), b"retained").unwrap();
                    fs::remove_file(layout.runtime.join("mutation.lock")).unwrap();
                    if completed {
                        Ok(())
                    } else {
                        Err(IntegrationError::Config(ConfigError::Conflict))
                    }
                });
                let error = result.unwrap_err();
                assert!(matches!(&error, IntegrationError::StorageWriteUnverified {
                    operation_completed, primary, ..
                } if *operation_completed == completed && primary.is_none() == completed));
                if !completed {
                    assert!(matches!(&error, IntegrationError::StorageWriteUnverified {
                        primary: Some(primary), ..
                    } if matches!(primary.as_ref(), IntegrationError::Config(ConfigError::Conflict))));
                }
                assert_eq!(
                    fs::read(root.join("runtime/published-fixture")).unwrap(),
                    b"retained"
                );
                let retained = super::rollback_migration_without_service(&root, error);
                assert!(matches!(
                    retained,
                    IntegrationError::StorageWriteUnverified { .. }
                ));
                fs::remove_dir_all(root).unwrap();
            }
        }
    }

    fn initialize_accounting(root: &Path) -> super::StorageBarrier {
        let mutation = super::MutationGuard::try_acquire(&root.join("runtime")).unwrap();
        super::StorageBarrier::initialize(root, &mutation).unwrap()
    }

    #[test]
    fn integration_uncertainty_skips_status_probe_and_service_compensation() {
        let config = FakeConfig::disconnected();
        let lifecycle = FakeLifecycle::ready();
        for completed in [false, true] {
            let error = IntegrationError::StorageWriteUnverified {
                operation_completed: completed,
                primary: (!completed)
                    .then(|| Box::new(IntegrationError::Config(ConfigError::Conflict))),
                verification: super::StorageCoherenceError::InvalidIdentity,
            };
            let error = super::recover_failed_config_connect(
                Path::new("/unused"),
                &config,
                &lifecycle,
                &FakeLifecycle::service(),
                error,
            );
            let error = super::rollback_migration_install(
                Path::new("/unused"),
                &lifecycle,
                &FakeLifecycle::service(),
                error,
            );
            assert!(matches!(
                error,
                IntegrationError::StorageWriteUnverified { .. }
            ));
        }
        assert!(config.events.borrow().is_empty());
        assert!(lifecycle.events.borrow().is_empty());
    }

    #[test]
    fn connect_releases_storage_before_fake_start_and_health() {
        let root = temporary_root("connect-storage-release");
        let layout = install(&root).unwrap();
        initialize_accounting(&root);
        let settings = install_settings(&root).unwrap();
        let lifecycle = FakeLifecycle {
            storage_probe_root: Some(root.clone()),
            ..FakeLifecycle::ready()
        };
        with_lifecycle_lock(&layout, || {
            connect_with_reloaded_settings(
                &root,
                Path::new("/bin/agentobs"),
                &lifecycle,
                false,
                || Ok(settings.clone()),
                |_| {
                    Ok(super::StorageConfigManager {
                        root: root.clone(),
                        manager: CodexConfigManager::new(
                            root.join("config.toml"),
                            layout.runtime.join("integrations/codex"),
                            Path::new("/bin/agentobs"),
                            &root,
                            settings.port,
                            test_exporter_security(&root),
                        )?,
                    })
                },
            )
        })
        .unwrap();
        assert_eq!(*lifecycle.events.borrow(), ["install", "health", "commit"]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn config_writers_and_recovery_respect_accounting_and_legacy() {
        for coordinated in [false, true] {
            let root = temporary_root("config-writer-accounting");
            let layout = install(&root).unwrap();
            let manager = super::StorageConfigManager {
                root: root.clone(),
                manager: CodexConfigManager::new(
                    root.join("config.toml"),
                    layout.runtime.join("integrations/codex"),
                    Path::new("/bin/agentobs"),
                    &root,
                    4318,
                    test_exporter_security(&root),
                )
                .unwrap(),
            };
            let barrier = coordinated.then(|| initialize_accounting(&root));
            if let Some(barrier) = &barrier {
                let permit = barrier.try_begin_write().unwrap();
                assert!(manager.notify_ownership().is_err());
                assert!(!layout.runtime.join("integrations/codex").exists());
                drop(permit);
            }
            manager.connect().unwrap();
            let snapshot_path = layout
                .runtime
                .join("integrations/codex/codex-config-ownership-v1.json");
            let mut pending: serde_json::Value =
                serde_json::from_slice(&fs::read(&snapshot_path).unwrap()).unwrap();
            pending["phase"] = "prepared".into();
            fs::write(&snapshot_path, serde_json::to_vec(&pending).unwrap()).unwrap();
            let snapshot = fs::read(&snapshot_path).unwrap();
            let config = fs::read(root.join("config.toml")).unwrap();
            if let Some(barrier) = &barrier {
                let permit = barrier.try_begin_write().unwrap();
                assert!(matches!(
                    manager.connect(),
                    Err(IntegrationError::Storage(
                        super::StorageCoherenceError::Busy
                    ))
                ));
                assert!(manager.disconnect().is_err());
                assert!(manager.status().is_err());
                assert!(manager.ownership_status().is_err());
                assert!(manager.notify_ownership().is_err());
                assert_eq!(fs::read(&snapshot_path).unwrap(), snapshot);
                assert_eq!(fs::read(root.join("config.toml")).unwrap(), config);
                drop(permit);
            }
            assert_eq!(manager.status().unwrap(), ConfigConnectionStatus::Connected);
            let recovered: serde_json::Value =
                serde_json::from_slice(&fs::read(&snapshot_path).unwrap()).unwrap();
            assert_eq!(recovered["phase"], "connected");
            assert!(manager.notify_ownership().unwrap().is_some());
            manager.disconnect().unwrap();
            assert!(!snapshot_path.exists());
            assert_eq!(
                layout.runtime.join("storage-accounting.lock").exists(),
                coordinated
            );
            fs::remove_dir_all(root).unwrap();
        }
    }

    fn connected_status() -> CodexIntegrationStatus {
        CodexIntegrationStatus {
            schema_version: super::CODEX_INTEGRATION_STATUS_VERSION.into(),
            collector_degradation_reasons: Vec::new(),
            config: ConnectionStatus::Connected,
            notify: Some(NotifyStatus::AgentobsOwned),
            collector: CollectorStatus::Ready,
            endpoint: Some("https://127.0.0.1:4318/v1/logs".into()),
            service: Some("io.agent-observability.collector.test".into()),
            data_retained: true,
        }
    }

    #[test]
    fn exporter_security_uses_absolute_ca_path_and_settings_token() {
        let root = temporary_root("exporter-security");
        let layout = agent_observability_local_runtime::install(&root).unwrap();
        let settings = install_settings(&root).unwrap();
        let config_path = root.join("codex-config.toml");
        let manager = CodexConfigManager::new(
            &config_path,
            layout.runtime.join("integrations/codex-test"),
            root.join("bin/agentobs"),
            &root,
            settings.port,
            exporter_security(&layout, &settings).unwrap(),
        )
        .unwrap();
        manager.connect().unwrap();
        let config = fs::read_to_string(&config_path).unwrap();
        assert!(
            config.contains(
                &layout
                    .runtime
                    .join(&settings.credentials.ca_certificate)
                    .display()
                    .to_string()
            )
        );
        assert!(config.contains(&settings.auth_token));
        assert!(!config.contains("client-certificate"));
        assert!(!config.contains("client-private-key"));

        let mut traversal = settings;
        traversal.credentials.ca_certificate = "../ca-certificate.pem".into();
        assert!(exporter_security(&layout, &traversal).is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn new_codex_home_is_private_and_existing_directory_is_preserved() {
        use std::os::unix::fs::PermissionsExt;

        let root = temporary_root("codex-home");
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let codex_home = root.join(".codex");

        ensure_codex_home(&codex_home).unwrap();
        assert_eq!(
            fs::metadata(&codex_home).unwrap().permissions().mode() & 0o777,
            0o700
        );
        ensure_codex_home(&codex_home).unwrap();

        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn codex_detection_is_read_only_and_accepts_home_or_path_evidence() {
        use std::os::unix::fs::PermissionsExt;

        let root = temporary_root("codex-detection");
        let codex_home = root.join(".codex");
        let bin = root.join("bin");
        fs::create_dir_all(&bin).unwrap();

        assert!(!codex_detected_at(&codex_home, Some(bin.as_os_str())).unwrap());
        assert!(!codex_home.exists());

        let codex = bin.join("codex");
        fs::write(&codex, b"#!/bin/sh\n").unwrap();
        fs::set_permissions(&codex, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(codex_detected_at(&codex_home, Some(bin.as_os_str())).unwrap());
        assert!(!codex_home.exists());

        fs::create_dir(&codex_home).unwrap();
        assert!(codex_detected_at(&codex_home, None).unwrap());
        let _ = fs::remove_dir_all(root);
    }

    struct FakeConfig {
        state: Cell<ConfigConnectionStatus>,
        connect_error: Cell<bool>,
        connect_error_after_apply: Cell<bool>,
        disconnect_error: Cell<bool>,
        disconnect_error_after_restore: Cell<bool>,
        events: RefCell<Vec<&'static str>>,
    }

    impl FakeConfig {
        fn connected() -> Self {
            Self {
                state: Cell::new(ConfigConnectionStatus::Connected),
                connect_error: Cell::new(false),
                connect_error_after_apply: Cell::new(false),
                disconnect_error: Cell::new(false),
                disconnect_error_after_restore: Cell::new(false),
                events: RefCell::new(Vec::new()),
            }
        }

        fn disconnected() -> Self {
            Self {
                state: Cell::new(ConfigConnectionStatus::Disconnected),
                ..Self::connected()
            }
        }
    }

    impl ConfigLifecycle for FakeConfig {
        fn status(&self) -> Result<ConfigConnectionStatus, IntegrationError> {
            self.events.borrow_mut().push("config-status");
            Ok(self.state.get())
        }

        fn connect(
            &self,
        ) -> Result<(ConfigConnectionStatus, Option<NotifyOwnership>), IntegrationError> {
            self.events.borrow_mut().push("config-connect");
            if self.connect_error.get() {
                return Err(IntegrationError::Runtime("config connect failed".into()));
            }
            self.state.set(ConfigConnectionStatus::Connected);
            if self.connect_error_after_apply.get() {
                return Err(IntegrationError::Runtime(
                    "config connect failed after apply".into(),
                ));
            }
            Ok((
                ConfigConnectionStatus::Connected,
                Some(NotifyOwnership::AgentobsOwned),
            ))
        }

        fn disconnect(&self) -> Result<ConfigConnectionStatus, IntegrationError> {
            self.events.borrow_mut().push("config-disconnect");
            if self.disconnect_error.get() {
                return Err(IntegrationError::Runtime("config disconnect failed".into()));
            }
            self.state.set(ConfigConnectionStatus::Disconnected);
            if self.disconnect_error_after_restore.get() {
                return Err(IntegrationError::Runtime(
                    "config cleanup failed after restore".into(),
                ));
            }
            Ok(ConfigConnectionStatus::Disconnected)
        }
    }

    struct FakeLifecycle {
        storage_probe_root: Option<PathBuf>,
        wait_error: bool,
        uninstall_error: bool,
        commit_error: bool,
        install_error: Cell<bool>,
        collector_status: CollectorStatus,
        events: RefCell<Vec<&'static str>>,
    }

    impl FakeLifecycle {
        fn ready() -> Self {
            Self {
                storage_probe_root: None,
                wait_error: false,
                uninstall_error: false,
                commit_error: false,
                install_error: Cell::new(false),
                collector_status: CollectorStatus::Ready,
                events: RefCell::new(Vec::new()),
            }
        }

        fn service() -> CollectorService {
            CollectorService {
                root: PathBuf::from("/runtime"),
                label: "test-service".into(),
                plist: PathBuf::from("/tmp/test-service.plist"),
                target: "gui/1/test-service".into(),
                ownership: PathBuf::from("/tmp/launch-agent-ownership-v1.json"),
            }
        }

        fn check_storage_released(&self) {
            if let Some(root) = &self.storage_probe_root {
                super::with_storage_writer(root, || Ok(()))
                    .expect("storage guard held across process start or health");
            }
        }
    }

    impl CollectorLifecycle for FakeLifecycle {
        fn install(
            &self,
            _root: &Path,
            _executable: &Path,
        ) -> Result<CollectorService, IntegrationError> {
            self.events.borrow_mut().push("install");
            self.check_storage_released();
            if self.install_error.get() {
                return Err(IntegrationError::Runtime("install failed".into()));
            }
            Ok(Self::service())
        }

        fn service(&self, _root: &Path) -> Result<CollectorService, IntegrationError> {
            self.events.borrow_mut().push("service");
            Ok(Self::service())
        }

        fn restart(
            &self,
            _root: &Path,
            _executable: &Path,
        ) -> Result<CollectorService, IntegrationError> {
            self.events.borrow_mut().push("restart");
            self.check_storage_released();
            if self.install_error.get() {
                return Err(IntegrationError::Runtime("install failed".into()));
            }
            Ok(Self::service())
        }

        fn wait_until_ready(&self, _root: &Path) -> Result<CollectorStatus, IntegrationError> {
            self.events.borrow_mut().push("health");
            self.check_storage_released();
            if self.wait_error {
                Err(IntegrationError::Runtime("health failed".into()))
            } else {
                Ok(self.collector_status)
            }
        }

        fn commit_install(&self, _service: &CollectorService) -> Result<(), IntegrationError> {
            self.events.borrow_mut().push("commit");
            if self.commit_error {
                Err(IntegrationError::Runtime("commit failed".into()))
            } else {
                Ok(())
            }
        }

        fn uninstall(&self, _service: &CollectorService) -> Result<(), IntegrationError> {
            self.events.borrow_mut().push("uninstall");
            if self.uninstall_error {
                Err(IntegrationError::Runtime("uninstall failed".into()))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn status_is_serializable_with_stable_values() {
        let status = CodexIntegrationStatus {
            schema_version: super::CODEX_INTEGRATION_STATUS_VERSION.into(),
            collector_degradation_reasons: Vec::new(),
            config: ConnectionStatus::Conflict,
            notify: None,
            collector: CollectorStatus::Unavailable,
            endpoint: Some("https://127.0.0.1:43181/v1/logs".into()),
            service: Some("io.agent-observability.collector.example".into()),
            data_retained: true,
        };
        let value = serde_json::to_value(status).unwrap();
        assert_eq!(value["config"], "conflict");
        assert_eq!(value["collector"], "unavailable");
        assert_eq!(value["data_retained"], true);
    }

    #[cfg(unix)]
    #[test]
    fn connect_coordinator_commits_success_and_preserves_failed_compensation_state() {
        use std::os::unix::fs::PermissionsExt;

        let legacy = br#"{"schema_version":"local_collector.v1","port":4318,"token":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","source_generation":"codex-otel-v1"}"#;
        let status = CodexIntegrationStatus {
            schema_version: super::CODEX_INTEGRATION_STATUS_VERSION.into(),
            collector_degradation_reasons: Vec::new(),
            config: ConnectionStatus::Connected,
            notify: Some(NotifyStatus::AgentobsOwned),
            collector: CollectorStatus::Ready,
            endpoint: Some("https://127.0.0.1:4318/v1/logs".into()),
            service: Some("io.agent-observability.collector.test".into()),
            data_retained: true,
        };

        let success_root = temporary_root("settings-migration-commit");
        let success_layout = install(&success_root).unwrap();
        let success_path = success_layout.runtime.join("collector.json");
        fs::write(&success_path, legacy).unwrap();
        fs::set_permissions(&success_path, fs::Permissions::from_mode(0o600)).unwrap();
        let committed_settings = install_settings(&success_root).unwrap();
        assert_eq!(
            finish_settings_migration(&success_root, Ok(status.clone())).unwrap(),
            status
        );
        assert_eq!(load_settings(&success_root).unwrap(), committed_settings);
        assert!(
            !success_layout
                .runtime
                .join("collector-settings-migration.json")
                .exists()
        );

        let failure_root = temporary_root("settings-migration-rollback");
        let failure_layout = install(&failure_root).unwrap();
        let failure_path = failure_layout.runtime.join("collector.json");
        fs::write(&failure_path, legacy).unwrap();
        fs::set_permissions(&failure_path, fs::Permissions::from_mode(0o600)).unwrap();
        let replacement = install_settings(&failure_root).unwrap();
        let error = finish_settings_migration(
            &failure_root,
            Err(IntegrationError::Runtime("connect failed".into())),
        )
        .unwrap_err();
        assert!(error.to_string().contains("connect failed"));
        assert_eq!(load_settings(&failure_root).unwrap(), replacement);
        assert!(
            failure_layout
                .runtime
                .join("integrations/codex/tls")
                .join(&replacement.generation)
                .exists()
        );
        assert!(
            failure_layout
                .runtime
                .join("collector-settings-migration.json")
                .exists()
        );
        rollback_settings_migration(&failure_root).unwrap();
        assert_eq!(fs::read(&failure_path).unwrap(), legacy);

        let _ = fs::remove_dir_all(success_root);
        let _ = fs::remove_dir_all(failure_root);
    }

    #[cfg(unix)]
    #[test]
    fn committed_connect_reports_unverified_settings_finalization_without_mutating_fixture() {
        use agent_observability_local_collector::CollectorError;
        use std::os::unix::fs::PermissionsExt;

        let root = temporary_root("connect-finalization-unverified");
        let layout = install(&root).unwrap();
        let settings_path = layout.runtime.join("collector.json");
        let legacy = br#"{"schema_version":"local_collector.v1","port":4318,"token":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","source_generation":"codex-otel-v1"}"#;
        fs::write(&settings_path, legacy).unwrap();
        fs::set_permissions(&settings_path, fs::Permissions::from_mode(0o600)).unwrap();
        let replacement = install_settings(&root).unwrap();
        let journal_path = layout.runtime.join("collector-settings-migration.json");
        let settings_bytes = fs::read(&settings_path).unwrap();
        let journal_bytes = fs::read(&journal_path).unwrap();

        let error = finish_settings_migration_with(&root, Ok(connected_status()), |_| {
            Err(CollectorError::StorageWriteUnverified {
                operation_completed: true,
                primary: None,
                verification: Box::new(CollectorError::Runtime(
                    "settings finalization postcheck failed".into(),
                )),
            })
        })
        .unwrap_err();

        assert!(matches!(
            error,
            IntegrationError::ConnectCommittedSettingsFinalizationUnverified {
                finalization: CollectorError::StorageWriteUnverified {
                    operation_completed: true,
                    ..
                }
            }
        ));
        assert_eq!(fs::read(settings_path).unwrap(), settings_bytes);
        assert_eq!(fs::read(journal_path).unwrap(), journal_bytes);
        assert!(
            layout
                .runtime
                .join("integrations/codex/tls")
                .join(replacement.generation)
                .exists()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn committed_disconnect_reports_unverified_settings_finalization_without_mutating_fixture() {
        use agent_observability_local_collector::CollectorError;
        use std::os::unix::fs::PermissionsExt;

        let root = temporary_root("disconnect-finalization-unverified");
        let layout = install(&root).unwrap();
        let settings_path = layout.runtime.join("collector.json");
        let legacy = br#"{"schema_version":"local_collector.v1","port":4318,"token":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","source_generation":"codex-otel-v1"}"#;
        fs::write(&settings_path, legacy).unwrap();
        fs::set_permissions(&settings_path, fs::Permissions::from_mode(0o600)).unwrap();
        let replacement = install_settings(&root).unwrap();
        let journal_path = layout.runtime.join("collector-settings-migration.json");
        let mut journal: serde_json::Value =
            serde_json::from_slice(&fs::read(&journal_path).unwrap()).unwrap();
        journal["phase"] = serde_json::Value::String("integration_committed".into());
        fs::write(&journal_path, serde_json::to_vec(&journal).unwrap()).unwrap();
        let settings_bytes = fs::read(&settings_path).unwrap();
        let journal_bytes = fs::read(&journal_path).unwrap();

        let error =
            finish_disconnect_settings_migration_with(&root, Ok(disconnected_status()), |_| {
                Err(CollectorError::StorageWriteUnverified {
                    operation_completed: true,
                    primary: None,
                    verification: Box::new(CollectorError::Runtime(
                        "settings finalization postcheck failed".into(),
                    )),
                })
            })
            .unwrap_err();

        assert!(matches!(
            error,
            IntegrationError::DisconnectCommittedSettingsFinalizationUnverified {
                finalization: CollectorError::StorageWriteUnverified {
                    operation_completed: true,
                    ..
                }
            }
        ));
        assert_eq!(fs::read(settings_path).unwrap(), settings_bytes);
        assert_eq!(fs::read(journal_path).unwrap(), journal_bytes);
        assert!(
            layout
                .runtime
                .join("integrations/codex/tls")
                .join(replacement.generation)
                .exists()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn settings_rollback_uncertainty_preserves_typed_primary_and_verification() {
        use agent_observability_local_collector::CollectorError;

        let error = rollback_migration_without_service_with(
            Path::new("/runtime"),
            IntegrationError::Config(ConfigError::Conflict),
            |_| {
                Err(CollectorError::StorageWriteUnverified {
                    operation_completed: true,
                    primary: None,
                    verification: Box::new(CollectorError::Runtime(
                        "settings rollback postcheck failed".into(),
                    )),
                })
            },
        );

        assert!(matches!(
            error,
            IntegrationError::SettingsRollbackUnverified {
                primary,
                rollback: CollectorError::StorageWriteUnverified {
                    operation_completed: true,
                    ..
                }
            } if matches!(*primary, IntegrationError::Config(ConfigError::Conflict))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn status_settles_pending_settings_from_observed_config_state() {
        use std::os::unix::fs::PermissionsExt;

        let legacy = br#"{"schema_version":"local_collector.v1","port":4318,"token":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","source_generation":"codex-otel-v1"}"#;

        let connected_root = temporary_root("settings-status-connected");
        let connected_layout = install(&connected_root).unwrap();
        let connected_path = connected_layout.runtime.join("collector.json");
        fs::write(&connected_path, legacy).unwrap();
        fs::set_permissions(&connected_path, fs::Permissions::from_mode(0o600)).unwrap();
        let replacement = install_settings(&connected_root).unwrap();
        assert!(
            !settle_settings_migration_for_status(
                &connected_root,
                ConfigConnectionStatus::Connected,
            )
            .unwrap()
        );
        assert_eq!(load_settings(&connected_root).unwrap(), replacement);
        assert!(
            !connected_layout
                .runtime
                .join("collector-settings-migration.json")
                .exists()
        );

        let disconnected_root = temporary_root("settings-status-disconnected");
        let disconnected_layout = install(&disconnected_root).unwrap();
        let disconnected_path = disconnected_layout.runtime.join("collector.json");
        fs::write(&disconnected_path, legacy).unwrap();
        fs::set_permissions(&disconnected_path, fs::Permissions::from_mode(0o600)).unwrap();
        install_settings(&disconnected_root).unwrap();
        fs::write(&disconnected_path, legacy).unwrap();
        let status = settle_pending_migration_before_status(&disconnected_layout)
            .unwrap()
            .unwrap();
        assert_eq!(status.config, ConnectionStatus::Disconnected);
        assert_eq!(fs::read(&disconnected_path).unwrap(), legacy);
        assert!(
            !disconnected_layout
                .runtime
                .join("collector-settings-migration.json")
                .exists()
        );

        let _ = fs::remove_dir_all(connected_root);
        let _ = fs::remove_dir_all(disconnected_root);
    }

    #[cfg(unix)]
    #[test]
    fn disconnect_settles_pending_and_committed_settings_without_crossing_phases() {
        use std::os::unix::fs::PermissionsExt;

        let legacy = br#"{"schema_version":"local_collector.v1","port":4318,"token":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","source_generation":"codex-otel-v1"}"#;
        let status = CodexIntegrationStatus {
            config: ConnectionStatus::Disconnected,
            schema_version: super::CODEX_INTEGRATION_STATUS_VERSION.into(),
            collector_degradation_reasons: Vec::new(),
            notify: None,
            collector: CollectorStatus::Unavailable,
            endpoint: None,
            service: None,
            data_retained: true,
        };

        let pending_root = temporary_root("disconnect-pending-migration");
        let pending_layout = install(&pending_root).unwrap();
        let pending_path = pending_layout.runtime.join("collector.json");
        fs::write(&pending_path, legacy).unwrap();
        fs::set_permissions(&pending_path, fs::Permissions::from_mode(0o600)).unwrap();
        install_settings(&pending_root).unwrap();
        fs::write(&pending_path, legacy).unwrap();
        assert_eq!(
            settle_pending_migration_before_disconnect(&pending_layout)
                .unwrap()
                .unwrap(),
            status
        );
        assert_eq!(fs::read(&pending_path).unwrap(), legacy);

        let committed_root = temporary_root("disconnect-committed-migration");
        let committed_layout = install(&committed_root).unwrap();
        let committed_path = committed_layout.runtime.join("collector.json");
        fs::write(&committed_path, legacy).unwrap();
        fs::set_permissions(&committed_path, fs::Permissions::from_mode(0o600)).unwrap();
        let replacement = install_settings(&committed_root).unwrap();
        let journal_path = committed_layout
            .runtime
            .join("collector-settings-migration.json");
        let mut journal: serde_json::Value =
            serde_json::from_slice(&fs::read(&journal_path).unwrap()).unwrap();
        journal["phase"] = serde_json::Value::String("integration_committed".into());
        fs::write(&journal_path, serde_json::to_vec(&journal).unwrap()).unwrap();
        assert_eq!(
            finish_disconnect_settings_migration(&committed_root, Ok(status.clone())).unwrap(),
            status
        );
        assert_eq!(load_settings(&committed_root).unwrap(), replacement);
        assert!(!journal_path.exists());

        let _ = fs::remove_dir_all(pending_root);
        let _ = fs::remove_dir_all(committed_root);
    }

    #[test]
    fn degraded_status_serializes_without_becoming_ready() {
        let status = CodexIntegrationStatus {
            schema_version: super::CODEX_INTEGRATION_STATUS_VERSION.into(),
            collector_degradation_reasons: vec![
                agent_observability_contracts::CollectorDegradationReasonV1::StoragePressure,
            ],
            config: ConnectionStatus::Connected,
            notify: Some(NotifyStatus::ExternalPreserved),
            collector: CollectorStatus::Degraded,
            endpoint: None,
            service: None,
            data_retained: true,
        };

        let value = serde_json::to_value(status).unwrap();

        assert_eq!(value["collector"], "degraded");
        assert_eq!(
            value["collector_degradation_reasons"][0],
            "storage_pressure"
        );
    }

    #[test]
    fn missing_settings_reports_owned_config_as_unavailable() {
        let root = Path::new("/runtime");

        let status = missing_settings_status(
            root,
            Some(ConfigConnectionStatus::Connected),
            Some(NotifyStatus::ExternalPreserved),
            LaunchAgentOwnershipStatus::Absent,
        );

        assert_eq!(status.config, ConnectionStatus::Connected);
        assert_eq!(status.notify, Some(NotifyStatus::ExternalPreserved));
        assert_eq!(status.collector, CollectorStatus::Unavailable);
        assert_eq!(status.endpoint, None);
        assert_eq!(status.service, None);
    }

    #[test]
    fn missing_settings_reports_launch_agent_only_as_conflict() {
        let root = Path::new("/runtime");

        let status = missing_settings_status(root, None, None, LaunchAgentOwnershipStatus::Owned);

        assert_eq!(status.config, ConnectionStatus::Conflict);
        assert_eq!(status.collector, CollectorStatus::Unavailable);
        assert_eq!(status.endpoint, None);
        assert_eq!(
            status.service.as_deref(),
            Some(service_label(root).as_str())
        );
    }

    #[test]
    fn missing_settings_without_ownership_remains_disconnected() {
        let status = missing_settings_status(
            Path::new("/runtime"),
            None,
            None,
            LaunchAgentOwnershipStatus::Absent,
        );

        assert_eq!(status.config, ConnectionStatus::Disconnected);
        assert_eq!(status.collector, CollectorStatus::Unavailable);
        assert_eq!(status.endpoint, None);
        assert_eq!(status.service, None);
    }

    #[test]
    fn collector_health_outcome_maps_to_public_status() {
        assert_eq!(
            collector_status(HealthOutcome::Ready),
            CollectorStatus::Ready
        );
        assert_eq!(
            collector_status(HealthOutcome::Degraded),
            CollectorStatus::Degraded
        );
        assert_eq!(
            collector_status(HealthOutcome::Unavailable),
            CollectorStatus::Unavailable
        );
    }

    #[test]
    fn explicit_connect_recovery_does_not_rotate_a_healthy_collector() {
        let root = temporary_root("healthy-no-port-recovery");
        let original = install_settings(&root).unwrap();
        let original_bytes = fs::read(root.join("runtime/collector.json")).unwrap();
        let (settings, restart) = recover_connect_settings_with(&root, &original, |probe_root| {
            assert_eq!(probe_root, root);
            HealthOutcome::Ready
        })
        .unwrap();

        assert_eq!(settings, original);
        assert!(!restart);
        assert_eq!(
            fs::read(root.join("runtime/collector.json")).unwrap(),
            original_bytes
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn explicit_connect_recovery_does_not_rotate_a_degraded_collector() {
        let root = temporary_root("degraded-no-port-recovery");
        let original = install_settings(&root).unwrap();
        let original_bytes = fs::read(root.join("runtime/collector.json")).unwrap();
        let (settings, restart) =
            recover_connect_settings_with(&root, &original, |_| HealthOutcome::Degraded).unwrap();

        assert_eq!(settings, original);
        assert!(!restart);
        assert_eq!(
            fs::read(root.join("runtime/collector.json")).unwrap(),
            original_bytes
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn explicit_connect_restarts_unavailable_collector_without_rotating_free_port() {
        let root = temporary_root("free-port-restart");
        let original = install_settings(&root).unwrap();
        let original_bytes = fs::read(root.join("runtime/collector.json")).unwrap();

        let (settings, restart) = recover_connect_settings(&root, &original).unwrap();

        assert_eq!(settings, original);
        assert!(restart);
        assert_eq!(
            fs::read(root.join("runtime/collector.json")).unwrap(),
            original_bytes
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn explicit_connect_preserves_renewed_credentials_while_selecting_restart() {
        let root = temporary_root("renewed-restart");
        let original = install_settings(&root).unwrap();
        let mut expired = original.clone();
        expired.credentials.expires_at_unix_ms = 1;
        fs::write(
            root.join("runtime/collector.json"),
            serde_json::to_vec(&expired).unwrap(),
        )
        .unwrap();

        let renewed = install_settings(&root).unwrap();
        let renewed_bytes = fs::read(root.join("runtime/collector.json")).unwrap();
        let (recovered, restart) = recover_connect_settings(&root, &renewed).unwrap();

        assert!(restart);
        assert_eq!(recovered, renewed);
        assert_ne!(recovered.generation, original.generation);
        assert_eq!(
            fs::read(root.join("runtime/collector.json")).unwrap(),
            renewed_bytes
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn lifecycle_lock_rejects_peer_interleaving_without_blocking_runtime_config() {
        const CHANNEL_TIMEOUT: Duration = Duration::from_secs(10);

        let root = temporary_root("lifecycle-lock");
        let layout = agent_observability_local_runtime::install(&root).unwrap();
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let events = Arc::new(Mutex::new(Vec::new()));
        let worker_layout = layout.clone();
        let worker_events = Arc::clone(&events);
        let worker = thread::spawn(move || {
            with_lifecycle_lock(&worker_layout, || {
                worker_events
                    .lock()
                    .unwrap()
                    .extend(["service", "settings", "health", "config"]);
                entered_tx.send(()).map_err(|error| {
                    IntegrationError::Runtime(format!("signal lifecycle entry: {error}"))
                })?;
                release_rx.recv_timeout(CHANNEL_TIMEOUT).map_err(|error| {
                    IntegrationError::Runtime(format!("wait for lifecycle release: {error}"))
                })?;
                worker_events.lock().unwrap().push("rollback");
                Ok(())
            })
        });

        let entered = entered_rx.recv_timeout(CHANNEL_TIMEOUT);
        let config_save = entered.as_ref().ok().map(|()| {
            let config = LocalConfigService::new(&layout);
            config
                .read()
                .and_then(|versioned| config.save(&versioned.revision, &versioned.config))
        });
        let contender = entered.as_ref().ok().map(|()| {
            let contender_events = Arc::clone(&events);
            with_lifecycle_lock(&layout, || {
                contender_events.lock().unwrap().push("interleaved");
                Ok(())
            })
        });
        let released = release_tx.send(());
        let worker_result = worker.join();
        let observed_events = events.lock().map(|events| events.clone());
        let _ = fs::remove_dir_all(root);

        assert!(
            entered.is_ok(),
            "worker did not enter lifecycle: {entered:?}"
        );
        assert!(matches!(config_save, Some(Ok(_))));
        assert!(matches!(
            &contender,
            Some(Err(error)) if error.to_string().contains("lifecycle is busy")
        ));
        assert!(released.is_ok(), "worker release failed: {released:?}");
        assert!(
            matches!(&worker_result, Ok(Ok(()))),
            "worker failed: {worker_result:?}"
        );
        assert_eq!(
            observed_events.unwrap(),
            ["service", "settings", "health", "config", "rollback"]
        );
    }

    #[test]
    fn disconnected_status_does_not_invent_endpoint_or_service() {
        let root = std::env::temp_dir().join(format!(
            "agent-observability-codex-integration-status-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let result = status(&root, Path::new("/usr/local/bin/agentobs")).unwrap();
        assert_eq!(result.config, ConnectionStatus::Disconnected);
        assert_eq!(result.collector, CollectorStatus::Unavailable);
        assert_eq!(result.endpoint, None);
        assert_eq!(result.service, None);
        assert!(result.data_retained);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn status_fails_closed_on_invalid_collector_settings() {
        let root = std::env::temp_dir().join(format!(
            "agent-observability-codex-integration-invalid-status-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let layout = agent_observability_local_runtime::install(&root).unwrap();
        fs::write(layout.runtime.join("collector.json"), b"not json").unwrap();
        assert!(status(&root, Path::new("/usr/local/bin/agentobs")).is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn service_label_is_deterministic_and_root_specific() {
        assert_eq!(
            service_label(Path::new("/one")),
            service_label(Path::new("/one"))
        );
        assert_ne!(
            service_label(Path::new("/one")),
            service_label(Path::new("/two"))
        );
        assert!(service_label(Path::new("/one")).starts_with("io.agent-observability.collector."));
    }

    #[test]
    fn launch_agent_body_escapes_all_dynamic_values() {
        let body = launch_agent_body(
            "label<&\"'",
            Path::new("/Applications/A&B/agentobs"),
            Path::new("/tmp/<runtime>"),
        );
        assert!(body.contains("label&lt;&amp;&quot;&apos;"));
        assert!(body.contains("/Applications/A&amp;B/agentobs"));
        assert!(body.contains("/tmp/&lt;runtime&gt;"));
        assert!(!body.contains("label<&"));
    }

    #[test]
    fn connect_preserves_degraded_health_while_accepting_ingest_readiness() {
        let config = FakeConfig::disconnected();
        let lifecycle = FakeLifecycle {
            collector_status: CollectorStatus::Degraded,
            ..FakeLifecycle::ready()
        };

        let status = connect_prepared(
            Path::new("/runtime"),
            Path::new("/bin/agentobs"),
            "https://127.0.0.1:43181/v1/logs",
            &config,
            &lifecycle,
        )
        .unwrap();

        assert_eq!(status.config, ConnectionStatus::Connected);
        assert_eq!(status.collector, CollectorStatus::Degraded);
        assert_eq!(*lifecycle.events.borrow(), ["install", "health", "commit"]);
        assert_eq!(*config.events.borrow(), ["config-connect"]);
    }

    #[test]
    fn uncertain_connect_commit_failure_retains_connected_state() {
        let config = FakeConfig::disconnected();
        let lifecycle = FakeLifecycle {
            commit_error: true,
            ..FakeLifecycle::ready()
        };

        let error = connect_prepared(
            Path::new("/runtime"),
            Path::new("/bin/agentobs"),
            "https://127.0.0.1:43181/v1/logs",
            &config,
            &lifecycle,
        )
        .unwrap_err();

        assert!(error.to_string().contains("commit failed"));
        assert_eq!(config.state.get(), ConfigConnectionStatus::Connected);
        assert_eq!(*config.events.borrow(), ["config-connect"]);
        assert_eq!(*lifecycle.events.borrow(), ["install", "health", "commit"]);
    }

    #[cfg(unix)]
    #[test]
    fn unverified_settings_failure_does_not_trigger_blind_migration_rollback() {
        use agent_observability_local_collector::CollectorError;
        use std::os::unix::fs::PermissionsExt;

        for operation_completed in [false, true] {
            let root = temporary_root("settings-unverified-no-rollback");
            let layout = install(&root).unwrap();
            let settings_path = layout.runtime.join("collector.json");
            let legacy = br#"{"schema_version":"local_collector.v1","port":4318,"token":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","source_generation":"codex-otel-v1"}"#;
            fs::write(&settings_path, legacy).unwrap();
            fs::set_permissions(&settings_path, fs::Permissions::from_mode(0o600)).unwrap();
            let replacement = install_settings(&root).unwrap();
            let journal = layout.runtime.join("collector-settings-migration.json");
            let journal_bytes = fs::read(&journal).unwrap();
            let error = IntegrationError::Collector(CollectorError::StorageWriteUnverified {
                operation_completed,
                primary: (!operation_completed)
                    .then(|| Box::new(CollectorError::Runtime("primary".into()))),
                verification: Box::new(CollectorError::Runtime("storage identity changed".into())),
            });
            let result = super::rollback_migration_without_service(&root, error);
            assert!(matches!(
                result,
                IntegrationError::Collector(CollectorError::StorageWriteUnverified { .. })
            ));
            assert_eq!(load_settings(&root).unwrap(), replacement);
            assert_eq!(fs::read(journal).unwrap(), journal_bytes);
            assert!(
                layout
                    .runtime
                    .join("integrations/codex/tls")
                    .join(replacement.generation)
                    .exists()
            );
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[cfg(unix)]
    #[test]
    fn production_connect_commit_failure_retains_recoverable_v3_state() {
        use std::os::unix::fs::PermissionsExt;

        let root = temporary_root("migration-commit-failure");
        let layout = install(&root).unwrap();
        let settings_path = layout.runtime.join("collector.json");
        let legacy = br#"{"schema_version":"local_collector.v1","port":4318,"token":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","source_generation":"codex-otel-v1"}"#;
        fs::write(&settings_path, legacy).unwrap();
        fs::set_permissions(&settings_path, fs::Permissions::from_mode(0o600)).unwrap();
        let replacement = install_settings(&root).unwrap();
        let config_path = root.join("codex-config.toml");
        let lifecycle = FakeLifecycle {
            commit_error: true,
            ..FakeLifecycle::ready()
        };

        let error = connect_with_reloaded_settings(
            &root,
            Path::new("/bin/agentobs"),
            &lifecycle,
            false,
            || load_settings(&root).map_err(Into::into),
            |settings| {
                CodexConfigManager::new(
                    &config_path,
                    layout.runtime.join("integrations/codex"),
                    Path::new("/bin/agentobs"),
                    &root,
                    settings.port,
                    exporter_security(&layout, settings)?,
                )
                .map_err(Into::into)
            },
        )
        .unwrap_err();

        assert!(error.to_string().contains("commit failed"));
        assert_eq!(load_settings(&root).unwrap(), replacement);
        assert!(config_path.exists());
        assert!(
            layout
                .runtime
                .join("collector-settings-migration.json")
                .exists()
        );
        assert!(
            layout
                .runtime
                .join("integrations/codex/tls")
                .join(&replacement.generation)
                .exists()
        );
        assert_eq!(*lifecycle.events.borrow(), ["install", "health", "commit"]);

        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn uncertain_config_connect_retains_service_and_v3_until_status_recovery() {
        use std::os::unix::fs::PermissionsExt;

        let root = temporary_root("uncertain-config-connect");
        let layout = install(&root).unwrap();
        let settings_path = layout.runtime.join("collector.json");
        let legacy = br#"{"schema_version":"local_collector.v1","port":4318,"token":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","source_generation":"codex-otel-v1"}"#;
        fs::write(&settings_path, legacy).unwrap();
        fs::set_permissions(&settings_path, fs::Permissions::from_mode(0o600)).unwrap();
        let replacement = install_settings(&root).unwrap();
        let lifecycle = FakeLifecycle::ready();

        let error = connect_with_reloaded_settings(
            &root,
            Path::new("/bin/agentobs"),
            &lifecycle,
            false,
            || load_settings(&root).map_err(Into::into),
            |_| {
                let config = FakeConfig::disconnected();
                config.connect_error_after_apply.set(true);
                Ok(config)
            },
        )
        .unwrap_err();

        assert!(error.to_string().contains("failed after apply"));
        assert_eq!(load_settings(&root).unwrap(), replacement);
        assert!(
            layout
                .runtime
                .join("collector-settings-migration.json")
                .exists()
        );
        assert!(
            layout
                .runtime
                .join("integrations/codex/tls")
                .join(&replacement.generation)
                .exists()
        );
        assert_eq!(*lifecycle.events.borrow(), ["install", "health"]);

        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn uncertain_service_install_or_restart_retains_v3_for_lifecycle_recovery() {
        use std::os::unix::fs::PermissionsExt;

        let legacy = br#"{"schema_version":"local_collector.v1","port":4318,"token":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","source_generation":"codex-otel-v1"}"#;
        for (restart, event) in [(false, "install"), (true, "restart")] {
            let root = temporary_root(&format!("uncertain-service-{event}"));
            let layout = install(&root).unwrap();
            let settings_path = layout.runtime.join("collector.json");
            fs::write(&settings_path, legacy).unwrap();
            fs::set_permissions(&settings_path, fs::Permissions::from_mode(0o600)).unwrap();
            let replacement = install_settings(&root).unwrap();
            let lifecycle = FakeLifecycle::ready();
            lifecycle.install_error.set(true);

            let error = connect_with_reloaded_settings(
                &root,
                Path::new("/bin/agentobs"),
                &lifecycle,
                restart,
                || load_settings(&root).map_err(Into::into),
                |_| Ok(FakeConfig::disconnected()),
            )
            .unwrap_err();

            assert!(error.to_string().contains("install failed"));
            assert_eq!(load_settings(&root).unwrap(), replacement);
            assert!(
                layout
                    .runtime
                    .join("collector-settings-migration.json")
                    .exists()
            );
            assert!(
                layout
                    .runtime
                    .join("integrations/codex/tls")
                    .join(&replacement.generation)
                    .exists()
            );
            assert_eq!(*lifecycle.events.borrow(), [event]);

            let _ = fs::remove_dir_all(root);
        }
    }

    #[test]
    fn connect_builds_config_from_settings_reloaded_after_collector_ready() {
        let root = temporary_root("reloaded-settings");
        let mut renewed = install_settings(&root).unwrap();
        renewed.port = 49_321;
        let lifecycle = FakeLifecycle::ready();
        let manager_port = Cell::new(0);
        let status = connect_with_reloaded_settings(
            &root,
            Path::new("/bin/agentobs"),
            &lifecycle,
            false,
            || {
                assert_eq!(*lifecycle.events.borrow(), ["install", "health"]);
                Ok(renewed)
            },
            |settings| {
                manager_port.set(settings.port);
                Ok(FakeConfig::disconnected())
            },
        )
        .unwrap();

        assert_eq!(manager_port.get(), 49_321);
        assert_eq!(
            status.endpoint.as_deref(),
            Some("https://127.0.0.1:49321/v1/logs")
        );
        assert_eq!(status.config, ConnectionStatus::Connected);
        assert_eq!(*lifecycle.events.borrow(), ["install", "health", "commit"]);
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn automatic_setup_connect_rebases_only_unowned_config_edits() {
        use std::os::unix::fs::PermissionsExt;

        let root = temporary_root("automatic-setup-unowned-rebase");
        let layout = install(&root).unwrap();
        let settings = install_settings(&root).unwrap();
        let config_path = root.join("codex-config.toml");
        let original = b"notify = ['/external/notify']\nmodel = 'before'\n";
        fs::write(&config_path, original).unwrap();
        fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600)).unwrap();

        CodexConfigManager::new(
            &config_path,
            layout.runtime.join("integrations/codex"),
            Path::new("/bin/agentobs"),
            &root,
            settings.port,
            exporter_security(&layout, &settings).unwrap(),
        )
        .unwrap()
        .connect()
        .unwrap();

        let mut edited = fs::read_to_string(&config_path)
            .unwrap()
            .replace("model = 'before'", "model = 'after'")
            .into_bytes();
        edited.extend_from_slice(b"\n[features]\nnew_unowned_flag = true\n");
        fs::write(&config_path, &edited).unwrap();

        let lifecycle = FakeLifecycle::ready();
        let status = connect_with_reloaded_settings(
            &root,
            Path::new("/bin/agentobs"),
            &lifecycle,
            false,
            || load_settings(&root).map_err(Into::into),
            |settings| {
                CodexConfigManager::new(
                    &config_path,
                    layout.runtime.join("integrations/codex"),
                    Path::new("/bin/agentobs"),
                    &root,
                    settings.port,
                    exporter_security(&layout, settings)?,
                )
                .map_err(Into::into)
            },
        )
        .unwrap();

        assert_eq!(status.config, ConnectionStatus::Connected);
        assert_eq!(status.notify, Some(NotifyStatus::ExternalPreserved));
        assert_eq!(fs::read(&config_path).unwrap(), edited);
        assert_eq!(*lifecycle.events.borrow(), ["install", "health", "commit"]);

        let manager = CodexConfigManager::new(
            &config_path,
            layout.runtime.join("integrations/codex"),
            Path::new("/bin/agentobs"),
            &root,
            settings.port,
            exporter_security(&layout, &settings).unwrap(),
        )
        .unwrap();
        assert_eq!(manager.status().unwrap(), ConfigConnectionStatus::Connected);
        assert_eq!(
            manager.disconnect().unwrap(),
            ConfigConnectionStatus::Disconnected
        );
        let disconnected = fs::read_to_string(&config_path).unwrap();
        assert!(disconnected.contains("notify = ['/external/notify']"));
        assert!(disconnected.contains("model = 'after'"));
        assert!(disconnected.contains("[features]\nnew_unowned_flag = true"));
        assert!(!disconnected.contains("[otel]"));

        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn automatic_setup_connect_rejects_owned_otel_edits_without_rewriting_config() {
        use std::os::unix::fs::PermissionsExt;

        let root = temporary_root("automatic-setup-owned-conflict");
        let layout = install(&root).unwrap();
        let settings = install_settings(&root).unwrap();
        let config_path = root.join("codex-config.toml");
        fs::write(&config_path, b"model = 'before'\n").unwrap();
        fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600)).unwrap();

        CodexConfigManager::new(
            &config_path,
            layout.runtime.join("integrations/codex"),
            Path::new("/bin/agentobs"),
            &root,
            settings.port,
            exporter_security(&layout, &settings).unwrap(),
        )
        .unwrap()
        .connect()
        .unwrap();

        let edited = fs::read_to_string(&config_path)
            .unwrap()
            .replace("environment = \"local\"", "environment = \"changed\"")
            .into_bytes();
        fs::write(&config_path, &edited).unwrap();

        let lifecycle = FakeLifecycle::ready();
        let error = connect_with_reloaded_settings(
            &root,
            Path::new("/bin/agentobs"),
            &lifecycle,
            false,
            || load_settings(&root).map_err(Into::into),
            |settings| {
                CodexConfigManager::new(
                    &config_path,
                    layout.runtime.join("integrations/codex"),
                    Path::new("/bin/agentobs"),
                    &root,
                    settings.port,
                    exporter_security(&layout, settings)?,
                )
                .map_err(Into::into)
            },
        )
        .unwrap_err();

        assert!(matches!(
            error,
            IntegrationError::Config(ConfigError::Conflict)
        ));
        assert_eq!(fs::read(&config_path).unwrap(), edited);
        assert_eq!(*lifecycle.events.borrow(), ["install", "health"]);

        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn recovered_connect_rotates_config_and_disconnect_restores_exact_prior() {
        use std::os::unix::fs::PermissionsExt;

        let root = temporary_root("recover-and-restore-config");
        let layout = agent_observability_local_runtime::install(&root).unwrap();
        let codex_home = root.join("codex-home");
        fs::create_dir(&codex_home).unwrap();
        let config_path = codex_home.join("config.toml");
        let original = b"# exact prior\nmodel = 'before'\n";
        fs::write(&config_path, original).unwrap();
        fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600)).unwrap();
        let settings = install_settings(&root).unwrap();
        let occupied = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, settings.port)).unwrap();
        let (recovered, restart) = recover_connect_settings(&root, &settings).unwrap();
        assert!(restart);
        assert_ne!(recovered.port, settings.port);
        let lifecycle = FakeLifecycle::ready();

        let connected = connect_with_reloaded_settings(
            &root,
            Path::new("/bin/agentobs"),
            &lifecycle,
            restart,
            || load_settings(&root).map_err(Into::into),
            |settings| {
                let security = exporter_security(&layout, settings)?;
                CodexConfigManager::new(
                    &config_path,
                    layout.runtime.join("integrations/codex"),
                    Path::new("/bin/agentobs"),
                    &root,
                    settings.port,
                    security,
                )
                .map_err(Into::into)
            },
        )
        .unwrap();
        assert_eq!(
            connected.endpoint.as_deref(),
            Some(recovered.endpoint().as_str())
        );
        assert_eq!(*lifecycle.events.borrow(), ["restart", "health", "commit"]);
        assert!(
            fs::read_to_string(&config_path)
                .unwrap()
                .contains(&recovered.endpoint())
        );
        let connected_config = fs::read_to_string(&config_path).unwrap();
        assert!(
            connected_config.contains(
                &layout
                    .runtime
                    .join(&recovered.credentials.ca_certificate)
                    .display()
                    .to_string()
            )
        );
        assert!(connected_config.contains(&recovered.auth_token));
        assert!(!connected_config.contains("client-certificate"));
        assert!(!connected_config.contains("client-private-key"));

        let security = exporter_security(&layout, &recovered).unwrap();
        let manager = CodexConfigManager::new(
            &config_path,
            layout.runtime.join("integrations/codex"),
            Path::new("/bin/agentobs"),
            &root,
            recovered.port,
            security,
        )
        .unwrap();
        let disconnected = disconnect_prepared(
            &root,
            Path::new("/bin/agentobs"),
            &recovered.endpoint(),
            &manager,
            &lifecycle,
        )
        .unwrap();

        assert_eq!(disconnected.config, ConnectionStatus::Disconnected);
        assert_eq!(fs::read(config_path).unwrap(), original);
        drop(occupied);
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn missing_settings_disconnect_restores_from_config_ownership_snapshot() {
        use std::os::unix::fs::PermissionsExt;

        let root = temporary_root("missing-settings-exact-restore");
        let layout = agent_observability_local_runtime::install(&root).unwrap();
        let config_path = root.join("config.toml");
        let original = b"# exact prior\nmodel = 'before'\n";
        fs::write(&config_path, original).unwrap();
        fs::set_permissions(&config_path, fs::Permissions::from_mode(0o400)).unwrap();
        let state_dir = layout.runtime.join("integrations/codex");
        CodexConfigManager::new(
            &config_path,
            &state_dir,
            Path::new("/bin/agentobs"),
            &root,
            43_181,
            test_exporter_security(&root),
        )
        .unwrap()
        .connect()
        .unwrap();
        let recovery = CodexConfigManager::from_ownership_snapshot(&config_path, &state_dir);
        let lifecycle = FakeLifecycle::ready();

        let status = disconnect_owned_prepared(
            &root,
            Path::new("/bin/agentobs"),
            None,
            &recovery,
            &lifecycle,
        )
        .unwrap();

        assert_eq!(status.config, ConnectionStatus::Disconnected);
        assert_eq!(status.endpoint, None);
        assert_eq!(fs::read(&config_path).unwrap(), original);
        assert_eq!(
            fs::metadata(&config_path).unwrap().permissions().mode() & 0o777,
            0o400
        );
        assert_eq!(*lifecycle.events.borrow(), ["service", "uninstall"]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn missing_settings_disconnect_conflict_does_not_remove_service_or_edit_config() {
        use std::os::unix::fs::PermissionsExt;

        let root = temporary_root("missing-settings-conflict");
        let layout = agent_observability_local_runtime::install(&root).unwrap();
        let config_path = root.join("config.toml");
        fs::write(&config_path, b"model = 'before'\n").unwrap();
        fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600)).unwrap();
        let state_dir = layout.runtime.join("integrations/codex");
        CodexConfigManager::new(
            &config_path,
            &state_dir,
            Path::new("/bin/agentobs"),
            &root,
            43_181,
            test_exporter_security(&root),
        )
        .unwrap()
        .connect()
        .unwrap();
        let edited = b"model = 'external'\n";
        fs::write(&config_path, edited).unwrap();
        let recovery = CodexConfigManager::from_ownership_snapshot(&config_path, &state_dir);
        let lifecycle = FakeLifecycle::ready();

        assert!(matches!(
            disconnect_owned_prepared(
                &root,
                Path::new("/bin/agentobs"),
                None,
                &recovery,
                &lifecycle,
            ),
            Err(IntegrationError::Config(
                agent_observability_codex_config::ConfigError::Conflict
            ))
        ));
        assert_eq!(fs::read(&config_path).unwrap(), edited);
        assert_eq!(*lifecycle.events.borrow(), ["service"]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn recovered_connect_config_failure_rolls_back_service_without_partial_config() {
        let root = temporary_root("recovered-connect-config-failure");
        let settings = install_settings(&root).unwrap();
        let occupied = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, settings.port)).unwrap();
        let (recovered, restart) = recover_connect_settings(&root, &settings).unwrap();
        let lifecycle = FakeLifecycle::ready();
        let config = FakeConfig::disconnected();
        config.connect_error.set(true);

        assert!(
            connect_with_reloaded_settings(
                &root,
                Path::new("/bin/agentobs"),
                &lifecycle,
                restart,
                || load_settings(&root).map_err(Into::into),
                |_| Ok(config),
            )
            .is_err()
        );
        assert_eq!(
            *lifecycle.events.borrow(),
            ["restart", "health", "uninstall"]
        );
        let persisted = load_settings(&root).unwrap();
        assert_eq!(persisted, recovered);
        assert_eq!(persisted.generation, settings.generation);
        assert_eq!(persisted.credentials, settings.credentials);
        drop(occupied);
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn recovered_connect_restart_failure_then_disconnect_restores_prior_ownership() {
        use std::os::unix::fs::PermissionsExt;

        let root = temporary_root("recovered-restart-failure-disconnect");
        let layout = agent_observability_local_runtime::install(&root).unwrap();
        let config_path = root.join("config.toml");
        let original = b"# exact prior\nmodel = 'before'\n";
        fs::write(&config_path, original).unwrap();
        fs::set_permissions(&config_path, fs::Permissions::from_mode(0o400)).unwrap();
        let original_settings = install_settings(&root).unwrap();
        let security = exporter_security(&layout, &original_settings).unwrap();
        CodexConfigManager::new(
            &config_path,
            layout.runtime.join("integrations/codex"),
            Path::new("/bin/agentobs"),
            &root,
            original_settings.port,
            security,
        )
        .unwrap()
        .connect()
        .unwrap();
        let occupied =
            TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, original_settings.port)).unwrap();
        let (recovered, restart) = recover_connect_settings(&root, &original_settings).unwrap();
        assert!(restart);
        assert_ne!(recovered.port, original_settings.port);
        let recovery = CodexConfigManager::from_ownership_snapshot(
            &config_path,
            layout.runtime.join("integrations/codex"),
        );
        let lifecycle = FakeLifecycle::ready();
        lifecycle.install_error.set(true);

        assert!(
            connect_with_reloaded_settings(
                &root,
                Path::new("/bin/agentobs"),
                &lifecycle,
                restart,
                || load_settings(&root).map_err(Into::into),
                |_| Ok(FakeConfig::disconnected()),
            )
            .is_err()
        );
        lifecycle.install_error.set(false);
        let disconnected = disconnect_prepared(
            &root,
            Path::new("/bin/agentobs"),
            &recovered.endpoint(),
            &recovery,
            &lifecycle,
        )
        .unwrap();

        assert_eq!(disconnected.config, ConnectionStatus::Disconnected);
        assert_eq!(disconnected.endpoint, Some(recovered.endpoint()));
        assert_eq!(fs::read(&config_path).unwrap(), original);
        assert_eq!(
            fs::metadata(&config_path).unwrap().permissions().mode() & 0o777,
            0o400
        );
        assert_eq!(
            *lifecycle.events.borrow(),
            ["restart", "service", "uninstall"]
        );
        drop(occupied);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn connect_health_failure_uninstalls_service_before_config_mutation() {
        let config = FakeConfig::disconnected();
        let lifecycle = FakeLifecycle {
            wait_error: true,
            ..FakeLifecycle::ready()
        };

        assert!(
            connect_prepared(
                Path::new("/runtime"),
                Path::new("/bin/agentobs"),
                "https://127.0.0.1:43181/v1/logs",
                &config,
                &lifecycle,
            )
            .is_err()
        );
        assert_eq!(
            *lifecycle.events.borrow(),
            ["install", "health", "uninstall"]
        );
        assert!(config.events.borrow().is_empty());
        assert_eq!(config.state.get(), ConfigConnectionStatus::Disconnected);
    }

    #[test]
    fn connect_config_failure_uninstalls_service() {
        let config = FakeConfig::disconnected();
        config.connect_error.set(true);
        let lifecycle = FakeLifecycle::ready();

        assert!(
            connect_prepared(
                Path::new("/runtime"),
                Path::new("/bin/agentobs"),
                "https://127.0.0.1:43181/v1/logs",
                &config,
                &lifecycle,
            )
            .is_err()
        );
        assert_eq!(
            *lifecycle.events.borrow(),
            ["install", "health", "uninstall"]
        );
        assert_eq!(*config.events.borrow(), ["config-connect"]);
        assert_eq!(config.state.get(), ConfigConnectionStatus::Disconnected);
    }

    #[test]
    fn disconnect_config_failure_reinstalls_service_after_confirmed_removal() {
        let config = FakeConfig::connected();
        config.disconnect_error.set(true);
        let lifecycle = FakeLifecycle::ready();

        assert!(
            disconnect_prepared(
                Path::new("/runtime"),
                Path::new("/bin/agentobs"),
                "https://127.0.0.1:43181/v1/logs",
                &config,
                &lifecycle,
            )
            .is_err()
        );
        assert_eq!(
            *lifecycle.events.borrow(),
            ["service", "uninstall", "install", "health"]
        );
        assert_eq!(
            *config.events.borrow(),
            ["config-status", "config-disconnect", "config-status"]
        );
        assert_eq!(config.state.get(), ConfigConnectionStatus::Connected);
    }

    #[test]
    fn disconnect_cleanup_error_after_restore_does_not_reinstall_orphan_collector() {
        let config = FakeConfig::connected();
        config.disconnect_error_after_restore.set(true);
        let lifecycle = FakeLifecycle::ready();

        assert!(
            disconnect_prepared(
                Path::new("/runtime"),
                Path::new("/bin/agentobs"),
                "https://127.0.0.1:43181/v1/logs",
                &config,
                &lifecycle,
            )
            .is_err()
        );

        assert_eq!(*lifecycle.events.borrow(), ["service", "uninstall"]);
        assert_eq!(
            *config.events.borrow(),
            ["config-status", "config-disconnect", "config-status"]
        );
        assert_eq!(config.state.get(), ConfigConnectionStatus::Disconnected);
    }

    #[test]
    fn disconnect_service_failure_does_not_mutate_config() {
        let config = FakeConfig::connected();
        let lifecycle = FakeLifecycle {
            uninstall_error: true,
            ..FakeLifecycle::ready()
        };

        assert!(
            disconnect_prepared(
                Path::new("/runtime"),
                Path::new("/bin/agentobs"),
                "https://127.0.0.1:43181/v1/logs",
                &config,
                &lifecycle,
            )
            .is_err()
        );
        assert_eq!(*lifecycle.events.borrow(), ["service", "uninstall"]);
        assert_eq!(*config.events.borrow(), ["config-status"]);
        assert_eq!(config.state.get(), ConfigConnectionStatus::Connected);
    }

    #[test]
    fn disconnect_service_failure_preserves_prior_disconnected_state() {
        let config = FakeConfig::disconnected();
        let lifecycle = FakeLifecycle {
            uninstall_error: true,
            ..FakeLifecycle::ready()
        };

        assert!(
            disconnect_prepared(
                Path::new("/runtime"),
                Path::new("/bin/agentobs"),
                "https://127.0.0.1:43181/v1/logs",
                &config,
                &lifecycle,
            )
            .is_err()
        );
        assert_eq!(*config.events.borrow(), ["config-status"]);
        assert_eq!(config.state.get(), ConfigConnectionStatus::Disconnected);
    }

    #[test]
    fn disconnect_reports_failed_service_reinstall() {
        let config = FakeConfig::connected();
        config.disconnect_error.set(true);
        let lifecycle = FakeLifecycle::ready();
        lifecycle.install_error.set(true);

        let error = disconnect_prepared(
            Path::new("/runtime"),
            Path::new("/bin/agentobs"),
            "https://127.0.0.1:43181/v1/logs",
            &config,
            &lifecycle,
        )
        .unwrap_err();
        assert!(error.to_string().contains("rollback failed"));
        assert_eq!(config.state.get(), ConfigConnectionStatus::Connected);
        assert_eq!(
            *lifecycle.events.borrow(),
            ["service", "uninstall", "install"]
        );
    }

    #[test]
    fn disconnect_recovery_is_idempotent_after_failed_reinstall() {
        let config = FakeConfig::connected();
        config.disconnect_error.set(true);
        let lifecycle = FakeLifecycle::ready();
        lifecycle.install_error.set(true);

        assert!(
            disconnect_prepared(
                Path::new("/runtime"),
                Path::new("/bin/agentobs"),
                "https://127.0.0.1:43181/v1/logs",
                &config,
                &lifecycle,
            )
            .is_err()
        );

        config.disconnect_error.set(false);
        lifecycle.install_error.set(false);
        let status = disconnect_prepared(
            Path::new("/runtime"),
            Path::new("/bin/agentobs"),
            "https://127.0.0.1:43181/v1/logs",
            &config,
            &lifecycle,
        )
        .unwrap();

        assert_eq!(status.config, ConnectionStatus::Disconnected);
        assert_eq!(config.state.get(), ConfigConnectionStatus::Disconnected);
        assert_eq!(
            *lifecycle.events.borrow(),
            ["service", "uninstall", "install", "service", "uninstall"]
        );
    }

    #[cfg(target_os = "macos")]
    struct FakeLaunchctl {
        storage_probe_root: Option<PathBuf>,
        loaded: Cell<bool>,
        pending_unload_checks: Cell<Option<usize>>,
        bootout_unload_delay_checks: Cell<usize>,
        bootstrap_marks_loaded: Cell<bool>,
        bootstrap_results: RefCell<VecDeque<Result<(), &'static str>>>,
        kickstart_results: RefCell<VecDeque<Result<(), &'static str>>>,
        bootout_results: RefCell<VecDeque<Result<bool, &'static str>>>,
        events: RefCell<Vec<&'static str>>,
    }

    #[cfg(target_os = "macos")]
    impl FakeLaunchctl {
        fn new(loaded: bool) -> Self {
            Self {
                storage_probe_root: None,
                loaded: Cell::new(loaded),
                pending_unload_checks: Cell::new(None),
                bootout_unload_delay_checks: Cell::new(0),
                bootstrap_marks_loaded: Cell::new(true),
                bootstrap_results: RefCell::new(VecDeque::new()),
                kickstart_results: RefCell::new(VecDeque::new()),
                bootout_results: RefCell::new(VecDeque::new()),
                events: RefCell::new(Vec::new()),
            }
        }

        fn check_storage_released(&self) {
            if let Some(root) = &self.storage_probe_root {
                super::with_storage_writer(root, || Ok(()))
                    .expect("storage guard held across launchctl");
            }
        }
    }

    #[cfg(target_os = "macos")]
    impl Launchctl for FakeLaunchctl {
        fn bootout(&self, _target: &str) -> Result<bool, IntegrationError> {
            self.check_storage_released();
            self.events.borrow_mut().push("bootout");
            let result = self
                .bootout_results
                .borrow_mut()
                .pop_front()
                .unwrap_or(Ok(true));
            if matches!(result, Ok(true)) {
                let delay = self.bootout_unload_delay_checks.get();
                if delay == 0 {
                    self.loaded.set(false);
                } else {
                    self.pending_unload_checks.set(Some(delay));
                }
            }
            result.map_err(|message| IntegrationError::Runtime(message.into()))
        }

        fn is_loaded(&self, _target: &str) -> Result<bool, IntegrationError> {
            self.check_storage_released();
            self.events.borrow_mut().push("is-loaded");
            if let Some(remaining) = self.pending_unload_checks.get() {
                if remaining == 0 {
                    self.pending_unload_checks.set(None);
                    self.loaded.set(false);
                } else {
                    self.pending_unload_checks.set(Some(remaining - 1));
                }
            }
            Ok(self.loaded.get())
        }

        fn bootstrap(&self, _domain: &str, _plist: &Path) -> Result<(), IntegrationError> {
            self.check_storage_released();
            self.events.borrow_mut().push("bootstrap");
            let result = self
                .bootstrap_results
                .borrow_mut()
                .pop_front()
                .unwrap_or(Ok(()));
            if result.is_ok() && self.bootstrap_marks_loaded.get() {
                self.loaded.set(true);
            }
            result.map_err(|message| IntegrationError::Runtime(message.into()))
        }

        fn kickstart(&self, _target: &str) -> Result<(), IntegrationError> {
            self.check_storage_released();
            self.events.borrow_mut().push("kickstart");
            let result = self
                .kickstart_results
                .borrow_mut()
                .pop_front()
                .unwrap_or(Ok(()));
            if result.is_ok() {
                self.loaded.set(true);
            }
            result.map_err(|message| IntegrationError::Runtime(message.into()))
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn launch_agent_stop_waits_for_asynchronous_bootout_convergence() {
        let root = temporary_root("asynchronous-bootout");
        let service = test_service(&root);
        let launchctl = FakeLaunchctl::new(true);
        launchctl.bootout_unload_delay_checks.set(2);

        stop_launch_agent_with(&launchctl, &service, 4, |_| {}).unwrap();

        assert!(!launchctl.loaded.get());
        assert_eq!(
            *launchctl.events.borrow(),
            [
                "is-loaded",
                "bootout",
                "is-loaded",
                "is-loaded",
                "is-loaded"
            ]
        );
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn launch_agent_stop_fails_closed_after_bounded_checks() {
        let root = temporary_root("asynchronous-bootout-timeout");
        let service = test_service(&root);
        let launchctl = FakeLaunchctl::new(true);
        launchctl.bootout_unload_delay_checks.set(3);

        let error = stop_launch_agent_with(&launchctl, &service, 2, |_| {}).unwrap_err();

        assert!(error.to_string().contains("LaunchAgent stop failed"));
        assert!(launchctl.loaded.get());
        assert_eq!(
            *launchctl.events.borrow(),
            ["is-loaded", "bootout", "is-loaded", "is-loaded"]
        );
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(target_os = "macos")]
    fn test_service(root: &Path) -> CollectorService {
        install(root).unwrap();
        CollectorService {
            root: root.to_path_buf(),
            label: "test-service".into(),
            plist: root.join("LaunchAgents/test-service.plist"),
            target: "gui/1/test-service".into(),
            ownership: root.join("runtime/integrations/codex/launch-agent-ownership-v1.json"),
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn launch_agent_writers_exclude_accounting_and_release_before_process_calls() {
        let root = temporary_root("launch-agent-storage");
        let service = test_service(&root);
        let layout = install(&root).unwrap();
        let barrier = initialize_accounting(&root);
        let launchctl = FakeLaunchctl {
            storage_probe_root: Some(root.clone()),
            ..FakeLaunchctl::new(false)
        };
        with_lifecycle_lock(&layout, || {
            install_collector_service_with(
                service.clone(),
                &root,
                Path::new("/bin/agentobs"),
                &launchctl,
                false,
            )?;
            let transaction = load_launch_agent_transaction(&service)?.unwrap();
            let before = fs::read(&service.ownership)?;
            let permit = barrier.try_begin_write().unwrap();
            assert!(save_launch_agent_transaction(&service, &transaction).is_err());
            assert!(super::remove_launch_agent_transaction(&service).is_err());
            assert_eq!(fs::read(&service.ownership)?, before);
            drop(permit);
            commit_collector_service_install(&service)?;
            uninstall_collector_service_with(&service, &launchctl)?;
            commit_collector_service_uninstall(&service)?;
            Ok(())
        })
        .unwrap();
        assert!(!service.ownership.exists());
        assert!(launchctl.events.borrow().contains(&"bootstrap"));
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn launch_agent_postcheck_failure_retains_transaction_without_process_rollback() {
        let root = temporary_root("launch-agent-postcheck");
        let service = test_service(&root);
        initialize_accounting(&root);
        let launchctl = FakeLaunchctl::new(false);
        install_collector_service_with(
            service.clone(),
            &root,
            Path::new("/bin/agentobs"),
            &launchctl,
            false,
        )
        .unwrap();
        let transaction = load_launch_agent_transaction(&service).unwrap().unwrap();
        let events = launchctl.events.borrow().clone();
        let error = super::with_storage_writer(&root, || {
            super::save_launch_agent_transaction_guarded(&service, &transaction)?;
            fs::remove_file(root.join("runtime/storage-accounting.lock"))?;
            Ok(())
        })
        .unwrap_err();
        let bytes = fs::read(&service.ownership).unwrap();
        let error = super::rollback_launch_agent_operation(&service, &launchctl, error);
        assert!(matches!(
            error,
            IntegrationError::StorageWriteUnverified {
                operation_completed: true,
                ..
            }
        ));
        assert_eq!(fs::read(&service.ownership).unwrap(), bytes);
        assert_eq!(*launchctl.events.borrow(), events);
        fs::remove_dir_all(&root).unwrap();
        assert!(save_launch_agent_transaction(&service, &transaction).is_err());
        assert!(super::remove_launch_agent_transaction(&service).is_err());
        assert!(!root.exists());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn install_and_disconnect_restore_inherited_plist_mode_and_loaded_state() {
        use std::os::unix::fs::PermissionsExt;

        let root = temporary_root("restore-inherited");
        let service = test_service(&root);
        fs::create_dir_all(service.plist.parent().unwrap()).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(&service.plist, b"inherited plist").unwrap();
        fs::set_permissions(&service.plist, fs::Permissions::from_mode(0o640)).unwrap();
        let launchctl = FakeLaunchctl::new(true);

        install_collector_service_with(
            test_service(&root),
            &root,
            Path::new("/bin/agentobs"),
            &launchctl,
            false,
        )
        .unwrap();
        commit_collector_service_install(&service).unwrap();
        assert_ne!(fs::read(&service.plist).unwrap(), b"inherited plist");
        assert!(service.ownership.is_file());
        assert_eq!(
            fs::metadata(&service.ownership)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );

        uninstall_collector_service_with(&service, &launchctl).unwrap();
        commit_collector_service_uninstall(&service).unwrap();

        assert_eq!(fs::read(&service.plist).unwrap(), b"inherited plist");
        assert_eq!(
            fs::metadata(&service.plist).unwrap().permissions().mode() & 0o777,
            0o640
        );
        assert!(launchctl.loaded.get());
        assert!(!service.ownership.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn install_failure_restores_inherited_working_service() {
        use std::os::unix::fs::PermissionsExt;

        let root = temporary_root("install-failure-inherited");
        let service = test_service(&root);
        fs::create_dir_all(service.plist.parent().unwrap()).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(&service.plist, b"working plist").unwrap();
        fs::set_permissions(&service.plist, fs::Permissions::from_mode(0o600)).unwrap();
        let launchctl = FakeLaunchctl::new(true);
        launchctl
            .bootstrap_results
            .borrow_mut()
            .extend([Err("new bootstrap failed"), Ok(())]);

        let error = install_collector_service_with(
            service.clone(),
            &root,
            Path::new("/bin/agentobs"),
            &launchctl,
            false,
        )
        .unwrap_err();

        assert!(error.to_string().contains("new bootstrap failed"));
        assert_eq!(fs::read(&service.plist).unwrap(), b"working plist");
        assert_eq!(
            fs::metadata(&service.plist).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(launchctl.loaded.get());
        assert!(!service.ownership.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn fresh_install_kickstart_failure_removes_only_new_service() {
        let root = temporary_root("fresh-kickstart-failure");
        let service = test_service(&root);
        let launchctl = FakeLaunchctl::new(false);
        launchctl.bootstrap_marks_loaded.set(false);
        launchctl
            .kickstart_results
            .borrow_mut()
            .push_back(Err("kickstart failed"));

        let error = install_collector_service_with(
            service.clone(),
            &root,
            Path::new("/bin/agentobs"),
            &launchctl,
            false,
        )
        .unwrap_err();

        assert!(error.to_string().contains("kickstart failed"));
        assert!(!service.plist.exists());
        assert!(!service.ownership.exists());
        assert!(!launchctl.loaded.get());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn reconnect_failure_then_disconnect_restores_exact_prior_service() {
        use std::os::unix::fs::PermissionsExt;

        let root = temporary_root("reconnect-failure");
        let service = test_service(&root);
        fs::create_dir_all(service.plist.parent().unwrap()).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(&service.plist, b"inherited plist").unwrap();
        fs::set_permissions(&service.plist, fs::Permissions::from_mode(0o640)).unwrap();
        let launchctl = FakeLaunchctl::new(true);
        install_collector_service_with(
            service.clone(),
            &root,
            Path::new("/bin/agentobs-v1"),
            &launchctl,
            false,
        )
        .unwrap();
        commit_collector_service_install(&service).unwrap();
        let previous = fs::read(&service.plist).unwrap();
        launchctl.bootstrap_marks_loaded.set(false);
        launchctl
            .kickstart_results
            .borrow_mut()
            .extend([Err("reconnect kickstart failed"), Ok(())]);

        let error = install_collector_service_with(
            service.clone(),
            &root,
            Path::new("/bin/agentobs-v2"),
            &launchctl,
            false,
        )
        .unwrap_err();

        assert!(error.to_string().contains("reconnect kickstart failed"));
        assert_eq!(fs::read(&service.plist).unwrap(), previous);
        assert!(launchctl.loaded.get());
        let transaction = load_launch_agent_transaction(&service).unwrap().unwrap();
        assert_eq!(transaction.phase, LaunchAgentPhase::Owned);
        assert_eq!(transaction.desired_plist.bytes, previous);

        uninstall_collector_service_with(&service, &launchctl).unwrap();
        commit_collector_service_uninstall(&service).unwrap();

        assert_eq!(fs::read(&service.plist).unwrap(), b"inherited plist");
        assert_eq!(
            fs::metadata(&service.plist).unwrap().permissions().mode() & 0o777,
            0o640
        );
        assert!(launchctl.loaded.get());
        assert!(!service.ownership.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn reconnect_with_unchanged_plist_keeps_owned_transaction_idempotent() {
        let root = temporary_root("reconnect-unchanged");
        let service = test_service(&root);
        let launchctl = FakeLaunchctl::new(false);
        install_collector_service_with(
            service.clone(),
            &root,
            Path::new("/bin/agentobs"),
            &launchctl,
            false,
        )
        .unwrap();
        commit_collector_service_install(&service).unwrap();
        let events = launchctl.events.borrow().len();

        install_collector_service_with(
            service.clone(),
            &root,
            Path::new("/bin/agentobs"),
            &launchctl,
            false,
        )
        .unwrap();
        commit_collector_service_install(&service).unwrap();

        assert_eq!(launchctl.events.borrow().len(), events + 1);
        assert_eq!(
            load_launch_agent_transaction(&service)
                .unwrap()
                .unwrap()
                .phase,
            LaunchAgentPhase::Owned
        );
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn in_progress_launch_agent_conflict_preserves_unrelated_plist() {
        let root = temporary_root("in-progress-launch-agent-conflict");
        let service = test_service(&root);
        let launchctl = FakeLaunchctl::new(false);
        install_collector_service_with(
            service.clone(),
            &root,
            Path::new("/bin/agentobs"),
            &launchctl,
            false,
        )
        .unwrap();
        commit_collector_service_install(&service).unwrap();
        let mut transaction = load_launch_agent_transaction(&service).unwrap().unwrap();
        transaction.phase = LaunchAgentPhase::Prepared;
        save_launch_agent_transaction(&service, &transaction).unwrap();
        let unrelated = b"unrelated plist";
        fs::write(&service.plist, unrelated).unwrap();

        let error = uninstall_collector_service_with(&service, &launchctl).unwrap_err();

        assert!(error.to_string().contains("outside the owned transaction"));
        assert_eq!(fs::read(&service.plist).unwrap(), unrelated);
        assert!(service.ownership.exists());
        assert!(launchctl.loaded.get());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn explicit_recovery_forces_owned_launch_agent_to_restart() {
        let root = temporary_root("forced-reconnect-unchanged");
        let service = test_service(&root);
        let launchctl = FakeLaunchctl::new(false);
        install_collector_service_with(
            service.clone(),
            &root,
            Path::new("/bin/agentobs"),
            &launchctl,
            false,
        )
        .unwrap();
        commit_collector_service_install(&service).unwrap();
        launchctl.events.borrow_mut().clear();

        install_collector_service_with(
            service.clone(),
            &root,
            Path::new("/bin/agentobs"),
            &launchctl,
            true,
        )
        .unwrap();

        assert_eq!(
            *launchctl.events.borrow(),
            [
                "is-loaded",
                "is-loaded",
                "bootout",
                "is-loaded",
                "bootstrap",
                "is-loaded",
                "is-loaded"
            ]
        );
        assert_eq!(
            load_launch_agent_transaction(&service)
                .unwrap()
                .unwrap()
                .phase,
            LaunchAgentPhase::Applied
        );
        commit_collector_service_install(&service).unwrap();
        assert_eq!(
            load_launch_agent_transaction(&service)
                .unwrap()
                .unwrap()
                .phase,
            LaunchAgentPhase::Owned
        );
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn status_recovery_converges_crashed_disconnect_to_prior_state() {
        let root = temporary_root("status-crash-recovery");
        let service = test_service(&root);
        let launchctl = FakeLaunchctl::new(false);
        install_collector_service_with(
            service.clone(),
            &root,
            Path::new("/bin/agentobs"),
            &launchctl,
            false,
        )
        .unwrap();
        commit_collector_service_install(&service).unwrap();
        let mut transaction = load_launch_agent_transaction(&service).unwrap().unwrap();
        transaction.rollback_plist = transaction.desired_plist.clone();
        transaction.rollback_loaded = true;
        transaction.desired_plist = transaction.prior_plist.clone();
        transaction.desired_loaded = transaction.prior_loaded;
        transaction.operation = LaunchAgentOperation::Disconnect;
        transaction.phase = LaunchAgentPhase::ServiceStopped;
        save_launch_agent_transaction(&service, &transaction).unwrap();
        launchctl.loaded.set(false);

        let recovered = recover_launch_agent_transaction(
            &service,
            &launchctl,
            LaunchAgentRecovery::Status(ConfigConnectionStatus::Disconnected),
        )
        .unwrap();

        assert!(recovered.is_none());
        assert!(!service.plist.exists());
        assert!(!service.ownership.exists());
        assert!(!launchctl.loaded.get());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn status_commits_applied_install_after_config_connected_crash() {
        let root = temporary_root("status-connect-commit-crash");
        let service = test_service(&root);
        let launchctl = FakeLaunchctl::new(false);
        install_collector_service_with(
            service.clone(),
            &root,
            Path::new("/bin/agentobs"),
            &launchctl,
            false,
        )
        .unwrap();
        assert_eq!(
            load_launch_agent_transaction(&service)
                .unwrap()
                .unwrap()
                .phase,
            LaunchAgentPhase::Applied
        );

        let recovered = recover_launch_agent_transaction(
            &service,
            &launchctl,
            LaunchAgentRecovery::Status(ConfigConnectionStatus::Connected),
        )
        .unwrap()
        .unwrap();

        assert_eq!(recovered.phase, LaunchAgentPhase::Owned);
        assert!(service.plist.exists());
        assert!(launchctl.loaded.get());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn connect_recovers_interrupted_reconnect_before_retrying() {
        let root = temporary_root("connect-crash-recovery");
        let service = test_service(&root);
        let launchctl = FakeLaunchctl::new(false);
        install_collector_service_with(
            service.clone(),
            &root,
            Path::new("/bin/agentobs-v1"),
            &launchctl,
            false,
        )
        .unwrap();
        commit_collector_service_install(&service).unwrap();
        let previous = fs::read(&service.plist).unwrap();
        let mut transaction = load_launch_agent_transaction(&service).unwrap().unwrap();
        transaction.rollback_plist = transaction.desired_plist.clone();
        transaction.rollback_loaded = true;
        transaction.desired_plist.bytes = b"partial replacement".to_vec();
        transaction.operation = LaunchAgentOperation::Reconnect;
        transaction.phase = LaunchAgentPhase::PlistWritten;
        save_launch_agent_transaction(&service, &transaction).unwrap();
        fs::write(&service.plist, b"partial replacement").unwrap();
        launchctl.loaded.set(false);

        install_collector_service_with(
            service.clone(),
            &root,
            Path::new("/bin/agentobs-v1"),
            &launchctl,
            false,
        )
        .unwrap();

        assert_eq!(fs::read(&service.plist).unwrap(), previous);
        assert!(launchctl.loaded.get());
        assert_eq!(
            load_launch_agent_transaction(&service)
                .unwrap()
                .unwrap()
                .phase,
            LaunchAgentPhase::Owned
        );
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn disconnect_after_crash_finishes_pending_restore_idempotently() {
        let root = temporary_root("disconnect-crash-recovery");
        let service = test_service(&root);
        let launchctl = FakeLaunchctl::new(false);
        install_collector_service_with(
            service.clone(),
            &root,
            Path::new("/bin/agentobs"),
            &launchctl,
            false,
        )
        .unwrap();
        commit_collector_service_install(&service).unwrap();
        let mut transaction = load_launch_agent_transaction(&service).unwrap().unwrap();
        transaction.rollback_plist = transaction.desired_plist.clone();
        transaction.rollback_loaded = true;
        transaction.desired_plist = transaction.prior_plist.clone();
        transaction.desired_loaded = false;
        transaction.operation = LaunchAgentOperation::Disconnect;
        transaction.phase = LaunchAgentPhase::Prepared;
        save_launch_agent_transaction(&service, &transaction).unwrap();

        uninstall_collector_service_with(&service, &launchctl).unwrap();
        commit_collector_service_uninstall(&service).unwrap();

        assert!(!service.plist.exists());
        assert!(!service.ownership.exists());
        assert!(!launchctl.loaded.get());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn unowned_disconnect_preserves_inherited_service() {
        let root = temporary_root("unowned-disconnect");
        let service = test_service(&root);
        fs::create_dir_all(service.plist.parent().unwrap()).unwrap();
        fs::write(&service.plist, b"inherited plist").unwrap();
        let launchctl = FakeLaunchctl::new(true);

        uninstall_collector_service_with(&service, &launchctl).unwrap();

        assert_eq!(fs::read(&service.plist).unwrap(), b"inherited plist");
        assert!(launchctl.loaded.get());
        assert!(launchctl.events.borrow().is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(target_os = "macos")]
    fn assert_launch_agent_race_case(
        name: &str,
        operation: LaunchAgentOperation,
        phase: LaunchAgentPhase,
        expected_existed: bool,
        desired_existed: bool,
        external_bytes: &[u8],
        external_mode: u32,
    ) {
        use std::os::unix::fs::PermissionsExt;

        let root = temporary_root(&format!("launch-agent-race-{name}"));
        let service = test_service(&root);
        fs::create_dir_all(service.plist.parent().unwrap()).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        if expected_existed {
            fs::write(&service.plist, b"initial plist").unwrap();
            fs::set_permissions(&service.plist, fs::Permissions::from_mode(0o600)).unwrap();
        }
        let expected = read_launch_agent_file(&service.plist).unwrap();
        let desired = LaunchAgentFileState {
            existed: desired_existed,
            bytes: if desired_existed {
                b"managed plist".to_vec()
            } else {
                Vec::new()
            },
            mode: if desired_existed { 0o644 } else { 0 },
        };
        let mut transaction = LaunchAgentTransaction {
            schema_version: super::LAUNCH_AGENT_OWNERSHIP_VERSION.into(),
            plist_path: service.plist.clone(),
            prior_plist: LaunchAgentFileState {
                existed: false,
                bytes: Vec::new(),
                mode: 0,
            },
            prior_loaded: false,
            rollback_plist: expected.clone(),
            rollback_loaded: false,
            desired_plist: desired,
            desired_loaded: false,
            operation,
            phase,
        };
        let launchctl = FakeLaunchctl::new(false);
        let plist = service.plist.clone();

        let error = apply_launch_agent_desired_with(
            &service,
            &launchctl,
            &mut transaction,
            &expected,
            |boundary| {
                assert_eq!(boundary, LaunchAgentMutationBoundary::ReadyToRevalidate);
                fs::write(&plist, external_bytes)?;
                fs::set_permissions(&plist, fs::Permissions::from_mode(external_mode))?;
                Ok(())
            },
        )
        .unwrap_err();

        assert!(error.to_string().contains("outside the owned transaction"));
        assert_eq!(fs::read(&service.plist).unwrap(), external_bytes, "{name}");
        assert_eq!(
            fs::metadata(&service.plist).unwrap().permissions().mode() & 0o777,
            external_mode,
            "{name}"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn launch_agent_lifecycle_races_fail_closed_before_publish_or_remove() {
        let cases = [
            (
                "connect",
                LaunchAgentOperation::Connect,
                LaunchAgentPhase::Prepared,
                false,
                true,
                b"external create".as_slice(),
                0o640,
            ),
            (
                "reconnect",
                LaunchAgentOperation::Reconnect,
                LaunchAgentPhase::Prepared,
                true,
                true,
                b"external edit".as_slice(),
                0o600,
            ),
            (
                "disconnect",
                LaunchAgentOperation::Disconnect,
                LaunchAgentPhase::Prepared,
                true,
                false,
                b"initial plist".as_slice(),
                0o640,
            ),
            (
                "crash-recovery",
                LaunchAgentOperation::Disconnect,
                LaunchAgentPhase::ServiceStopped,
                true,
                false,
                b"recovery edit".as_slice(),
                0o640,
            ),
        ];

        for (
            name,
            operation,
            phase,
            expected_existed,
            desired_existed,
            external_bytes,
            external_mode,
        ) in cases
        {
            assert_launch_agent_race_case(
                name,
                operation,
                phase,
                expected_existed,
                desired_existed,
                external_bytes,
                external_mode,
            );
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn oversized_launch_agent_plist_and_ownership_are_rejected() {
        use std::os::unix::fs::PermissionsExt;

        let root = temporary_root("launch-agent-size-bounds");
        let service = test_service(&root);
        fs::create_dir_all(service.plist.parent().unwrap()).unwrap();
        fs::write(
            &service.plist,
            vec![b'x'; usize::try_from(MAX_LAUNCH_AGENT_PLIST_BYTES + 1).unwrap()],
        )
        .unwrap();
        let launchctl = FakeLaunchctl::new(false);
        let plist_error = install_collector_service_with(
            service.clone(),
            &root,
            Path::new("/bin/agentobs"),
            &launchctl,
            false,
        )
        .unwrap_err();
        assert!(plist_error.to_string().contains("plist exceeds size bound"));
        assert!(!service.ownership.exists());

        fs::create_dir_all(service.ownership.parent().unwrap()).unwrap();
        fs::write(
            &service.ownership,
            vec![b'x'; usize::try_from(MAX_LAUNCH_AGENT_OWNERSHIP_BYTES + 1).unwrap()],
        )
        .unwrap();
        fs::set_permissions(&service.ownership, fs::Permissions::from_mode(0o600)).unwrap();
        let ownership_error = load_launch_agent_transaction(&service).unwrap_err();
        assert!(
            ownership_error
                .to_string()
                .contains("ownership state exceeds size bound")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn launch_agent_plist_and_ownership_symlinks_are_rejected() {
        use std::os::unix::fs::symlink;

        let root = temporary_root("launch-agent-no-follow");
        let service = test_service(&root);
        fs::create_dir_all(service.plist.parent().unwrap()).unwrap();
        let plist_target = root.join("plist-target");
        fs::write(&plist_target, b"target plist").unwrap();
        symlink(&plist_target, &service.plist).unwrap();
        assert!(
            read_launch_agent_file(&service.plist)
                .unwrap_err()
                .to_string()
                .contains("must be a regular file")
        );

        fs::create_dir_all(service.ownership.parent().unwrap()).unwrap();
        let ownership_target = root.join("ownership-target");
        fs::write(&ownership_target, b"{}").unwrap();
        symlink(&ownership_target, &service.ownership).unwrap();
        assert!(
            load_launch_agent_transaction(&service)
                .unwrap_err()
                .to_string()
                .contains("must be a regular file")
        );
        let _ = fs::remove_dir_all(root);
    }
}
