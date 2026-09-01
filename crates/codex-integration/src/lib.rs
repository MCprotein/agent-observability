#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc)]

use agent_observability_codex_config::{
    CodexConfigManager, ConfigError, ConnectionStatus as ConfigConnectionStatus,
};
use agent_observability_local_collector::{
    CollectorError, CollectorSettings, TOKEN_HEADER, install_settings, load_settings,
};
use agent_observability_local_runtime::{ConfigMutationGuard, InstalledLayout, install};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    env, fmt, fs,
    io::{Read, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream},
    path::{Path, PathBuf},
    time::Duration,
};
#[cfg(target_os = "macos")]
use std::{
    fs::{File, OpenOptions},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};

const MAX_HEALTH_RESPONSE_BYTES: u64 = 4 * 1024;
#[cfg(target_os = "macos")]
const LAUNCH_AGENT_OWNERSHIP_VERSION: &str = "agent-observability.launch-agent-ownership.v1";
#[cfg(target_os = "macos")]
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Serialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionStatus {
    Connected,
    Disconnected,
    Conflict,
}

#[derive(Clone, Copy, Debug, Serialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CollectorStatus {
    Ready,
    Degraded,
    Unavailable,
}

#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct CodexIntegrationStatus {
    pub config: ConnectionStatus,
    pub collector: CollectorStatus,
    pub endpoint: Option<String>,
    pub service: Option<String>,
    pub data_retained: bool,
}

#[derive(Debug)]
pub enum IntegrationError {
    Collector(CollectorError),
    Config(ConfigError),
    Io(std::io::Error),
    Runtime(String),
}

impl fmt::Display for IntegrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Collector(error) => error.fmt(formatter),
            Self::Config(error) => error.fmt(formatter),
            Self::Io(error) => error.fmt(formatter),
            Self::Runtime(message) => formatter.write_str(message),
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
        install_settings(&layout.root)?;
        connect_with_reloaded_settings(
            &layout.root,
            executable,
            &SystemLifecycle,
            || load_settings(&layout.root).map_err(Into::into),
            |settings| codex_config_manager(&layout, executable, settings),
        )
    })
}

fn connect_with_reloaded_settings<C: ConfigLifecycle>(
    root: &Path,
    executable: &Path,
    lifecycle: &impl CollectorLifecycle,
    load_after_ready: impl FnOnce() -> Result<CollectorSettings, IntegrationError>,
    config_from_settings: impl FnOnce(&CollectorSettings) -> Result<C, IntegrationError>,
) -> Result<CodexIntegrationStatus, IntegrationError> {
    let service = lifecycle.install(root, executable)?;
    let collector = lifecycle
        .wait_until_ready(root)
        .map_err(|error| rollback_install(lifecycle, &service, error))?;
    let settings =
        load_after_ready().map_err(|error| rollback_install(lifecycle, &service, error))?;
    let manager = config_from_settings(&settings)
        .map_err(|error| rollback_install(lifecycle, &service, error))?;
    let config = manager
        .connect()
        .map(Into::into)
        .map_err(|error| rollback_install(lifecycle, &service, error))?;
    lifecycle.commit_install(&service)?;
    Ok(CodexIntegrationStatus {
        config,
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
    let config = match config.connect() {
        Ok(status) => status.into(),
        Err(error) => return Err(rollback_install(lifecycle, &service, error)),
    };
    lifecycle.commit_install(&service)?;
    Ok(CodexIntegrationStatus {
        config,
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
        let settings = load_settings(&layout.root)?;
        let manager = codex_config_manager(&layout, executable, &settings)?;
        disconnect_prepared(
            &layout.root,
            executable,
            &settings.endpoint(),
            &manager,
            &SystemLifecycle,
        )
    })
}

fn disconnect_prepared(
    root: &Path,
    executable: &Path,
    endpoint: &str,
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
        Err(error) => match config.status() {
            Ok(ConfigConnectionStatus::Connected) => {
                let reinstall = lifecycle
                    .rollback_uninstall(&service, root, executable)
                    .and_then(|()| lifecycle.wait_until_ready(root).map(|_| ()));
                return match reinstall {
                    Ok(()) => Err(error),
                    Err(reinstall) => Err(rollback_error(&error, &reinstall)),
                };
            }
            Ok(ConfigConnectionStatus::Disconnected | ConfigConnectionStatus::Conflict) => {
                lifecycle
                    .commit_uninstall(&service)
                    .map_err(|commit| rollback_error(&error, &commit))?;
                return Err(error);
            }
            Err(status_error) => return Err(rollback_error(&error, &status_error)),
        },
    };
    Ok(CodexIntegrationStatus {
        config: status.into(),
        collector: CollectorStatus::Unavailable,
        endpoint: Some(endpoint.into()),
        service: Some(service.label),
        data_retained: true,
    })
}

trait ConfigLifecycle {
    fn status(&self) -> Result<ConfigConnectionStatus, IntegrationError>;
    fn connect(&self) -> Result<ConfigConnectionStatus, IntegrationError>;
    fn disconnect(&self) -> Result<ConfigConnectionStatus, IntegrationError>;
}

impl ConfigLifecycle for CodexConfigManager {
    fn status(&self) -> Result<ConfigConnectionStatus, IntegrationError> {
        self.status().map_err(Into::into)
    }

    fn connect(&self) -> Result<ConfigConnectionStatus, IntegrationError> {
        self.connect().map_err(Into::into)
    }

    fn disconnect(&self) -> Result<ConfigConnectionStatus, IntegrationError> {
        self.disconnect().map_err(Into::into)
    }
}

trait CollectorLifecycle {
    fn install(&self, root: &Path, executable: &Path)
    -> Result<CollectorService, IntegrationError>;
    fn service(&self, root: &Path) -> Result<CollectorService, IntegrationError>;
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

fn rollback_install(
    lifecycle: &impl CollectorLifecycle,
    service: &CollectorService,
    error: IntegrationError,
) -> IntegrationError {
    match lifecycle.rollback_install(service) {
        Ok(()) => error,
        Err(rollback) => rollback_error(&error, &rollback),
    }
}

fn rollback_error(error: &IntegrationError, rollback: &IntegrationError) -> IntegrationError {
    IntegrationError::Runtime(format!("{error}; rollback failed: {rollback}"))
}

pub fn status(root: &Path, executable: &Path) -> Result<CodexIntegrationStatus, IntegrationError> {
    let layout = install(root).map_err(runtime_error)?;
    with_lifecycle_lock(&layout, || {
        let settings = match load_settings(&layout.root) {
            Ok(settings) => settings,
            Err(CollectorError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(CodexIntegrationStatus {
                    config: ConnectionStatus::Disconnected,
                    collector: CollectorStatus::Unavailable,
                    endpoint: None,
                    service: None,
                    data_retained: true,
                });
            }
            Err(error) => return Err(error.into()),
        };
        let manager = codex_config_manager(&layout, executable, &settings)?;
        let config = manager.status()?;
        reconcile_collector_service(&layout.root, config)?;
        Ok(CodexIntegrationStatus {
            config: config.into(),
            collector: probe_health(&settings),
            endpoint: Some(settings.endpoint()),
            service: Some(service_label(&layout.root)),
            data_retained: true,
        })
    })
}

impl From<ConfigConnectionStatus> for ConnectionStatus {
    fn from(status: ConfigConnectionStatus) -> Self {
        match status {
            ConfigConnectionStatus::Connected => Self::Connected,
            ConfigConnectionStatus::Disconnected => Self::Disconnected,
            ConfigConnectionStatus::Conflict => Self::Conflict,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum HealthStatus {
    Ready,
    Degraded,
}

#[derive(Debug, Deserialize)]
struct HealthResponse {
    status: HealthStatus,
    report_dirty: bool,
}

fn probe_health(settings: &CollectorSettings) -> CollectorStatus {
    let request = format!(
        "GET /health HTTP/1.1\r\nHost: 127.0.0.1\r\n{TOKEN_HEADER}: {}\r\nConnection: close\r\n\r\n",
        settings.token,
    );
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), settings.port);
    let Ok(mut stream) = TcpStream::connect_timeout(&address, Duration::from_millis(50)) else {
        return CollectorStatus::Unavailable;
    };
    let timeout = Some(Duration::from_millis(100));
    if stream.set_write_timeout(timeout).is_err()
        || stream.set_read_timeout(timeout).is_err()
        || stream.write_all(request.as_bytes()).is_err()
        || stream.flush().is_err()
    {
        return CollectorStatus::Unavailable;
    }

    let mut response = Vec::new();
    if stream
        .take(MAX_HEALTH_RESPONSE_BYTES + 1)
        .read_to_end(&mut response)
        .is_err()
        || response.len() as u64 > MAX_HEALTH_RESPONSE_BYTES
    {
        return CollectorStatus::Unavailable;
    }
    parse_health_response(&response)
}

fn parse_health_response(response: &[u8]) -> CollectorStatus {
    let Some(header_end) = response.windows(4).position(|window| window == b"\r\n\r\n") else {
        return CollectorStatus::Unavailable;
    };
    let (headers, body_with_separator) = response.split_at(header_end);
    let body = &body_with_separator[4..];
    let Ok(headers) = std::str::from_utf8(headers) else {
        return CollectorStatus::Unavailable;
    };
    let mut lines = headers.split("\r\n");
    if !lines
        .next()
        .is_some_and(|status| status.starts_with("HTTP/1.1 200 "))
    {
        return CollectorStatus::Unavailable;
    }
    let mut content_length = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            return CollectorStatus::Unavailable;
        };
        if name.eq_ignore_ascii_case("transfer-encoding") {
            return CollectorStatus::Unavailable;
        }
        if name.eq_ignore_ascii_case("content-length") {
            let Ok(length) = value.trim().parse::<usize>() else {
                return CollectorStatus::Unavailable;
            };
            if content_length.replace(length).is_some() {
                return CollectorStatus::Unavailable;
            }
        }
    }
    if content_length != Some(body.len()) {
        return CollectorStatus::Unavailable;
    }
    let Ok(health) = serde_json::from_slice::<HealthResponse>(body) else {
        return CollectorStatus::Unavailable;
    };
    match (health.status, health.report_dirty) {
        (HealthStatus::Ready, _) => CollectorStatus::Ready,
        (HealthStatus::Degraded, true) => CollectorStatus::Degraded,
        (HealthStatus::Degraded, false) => CollectorStatus::Unavailable,
    }
}

fn runtime_error(error: impl fmt::Display) -> IntegrationError {
    IntegrationError::Runtime(error.to_string())
}

fn acquire_lifecycle_lock(
    layout: &InstalledLayout,
) -> Result<ConfigMutationGuard, IntegrationError> {
    ConfigMutationGuard::acquire(layout).map_err(|error| {
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
) -> Result<CodexConfigManager, IntegrationError> {
    let codex_home = env::var_os("CODEX_HOME")
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
        )?;
    ensure_codex_home(&codex_home)?;
    CodexConfigManager::new(
        codex_home.join("config.toml"),
        layout.runtime.join("integrations/codex"),
        executable,
        &layout.root,
        settings.port,
        &settings.token,
    )
    .map_err(Into::into)
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
    install_collector_service_with(service, root, executable, &SystemLaunchctl)
}

#[cfg(target_os = "macos")]
fn install_collector_service_with(
    service: CollectorService,
    root: &Path,
    executable: &Path,
    launchctl: &impl Launchctl,
) -> Result<CollectorService, IntegrationError> {
    let owned =
        recover_launch_agent_transaction(&service, launchctl, LaunchAgentRecovery::Connect)?;
    let desired = LaunchAgentFileState {
        existed: true,
        bytes: launch_agent_body(&service.label, executable, root).into_bytes(),
        mode: 0o644,
    };
    if owned
        .as_ref()
        .is_some_and(|transaction| transaction.desired_plist == desired)
    {
        return Ok(service);
    }

    let mut transaction = if let Some(owned) = owned {
        LaunchAgentTransaction {
            rollback_plist: owned.desired_plist.clone(),
            rollback_loaded: owned.desired_loaded,
            desired_plist: desired,
            desired_loaded: true,
            operation: LaunchAgentOperation::Reconnect,
            phase: LaunchAgentPhase::Prepared,
            ..owned
        }
    } else {
        let prior_plist = read_launch_agent_file(&service.plist)?;
        let prior_loaded = launchctl.is_loaded(&service.target)?;
        if prior_loaded && !prior_plist.existed {
            return Err(IntegrationError::Runtime(format!(
                "loaded LaunchAgent {} has no restorable plist",
                service.target
            )));
        }
        LaunchAgentTransaction {
            schema_version: LAUNCH_AGENT_OWNERSHIP_VERSION.into(),
            plist_path: service.plist.clone(),
            prior_plist: prior_plist.clone(),
            prior_loaded,
            rollback_plist: prior_plist,
            rollback_loaded: prior_loaded,
            desired_plist: desired,
            desired_loaded: true,
            operation: LaunchAgentOperation::Connect,
            phase: LaunchAgentPhase::Prepared,
        }
    };
    save_launch_agent_transaction(&service, &transaction)?;
    if let Err(error) = apply_launch_agent_desired(&service, launchctl, &mut transaction) {
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
    transaction.rollback_plist = transaction.desired_plist.clone();
    transaction.rollback_loaded = transaction.desired_loaded;
    transaction.desired_plist = transaction.prior_plist.clone();
    transaction.desired_loaded = transaction.prior_loaded;
    transaction.operation = LaunchAgentOperation::Disconnect;
    transaction.phase = LaunchAgentPhase::Prepared;
    save_launch_agent_transaction(service, &transaction)?;
    apply_launch_agent_desired(service, launchctl, &mut transaction)?;
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

#[cfg(not(target_os = "macos"))]
fn reconcile_collector_service(
    _root: &Path,
    _config: ConfigConnectionStatus,
) -> Result<(), IntegrationError> {
    Ok(())
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
    transaction.desired_plist = transaction.rollback_plist.clone();
    transaction.desired_loaded = transaction.rollback_loaded;
    transaction.phase = LaunchAgentPhase::Prepared;
    save_launch_agent_transaction(service, &transaction)?;
    apply_launch_agent_desired(service, &SystemLaunchctl, &mut transaction)?;
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
    if transaction.phase == LaunchAgentPhase::Owned {
        let current = read_launch_agent_file(&service.plist)?;
        if current != transaction.desired_plist {
            return Err(IntegrationError::Runtime(
                "LaunchAgent plist changed outside the owned transaction".into(),
            ));
        }
        if launchctl.is_loaded(&service.target)? != transaction.desired_loaded {
            transaction.operation = LaunchAgentOperation::Reconnect;
            transaction.rollback_plist = transaction.desired_plist.clone();
            transaction.rollback_loaded = transaction.desired_loaded;
            transaction.phase = LaunchAgentPhase::Prepared;
            save_launch_agent_transaction(service, &transaction)?;
            apply_launch_agent_desired(service, launchctl, &mut transaction)?;
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
                apply_launch_agent_desired(service, launchctl, &mut transaction)?;
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
                apply_launch_agent_desired(service, launchctl, &mut transaction)?;
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
                apply_launch_agent_desired(service, launchctl, &mut transaction)?;
                transaction.operation = LaunchAgentOperation::Connect;
                transaction.phase = LaunchAgentPhase::Owned;
                save_launch_agent_transaction(service, &transaction)?;
                Ok(Some(transaction))
            }
            LaunchAgentRecovery::Disconnect
            | LaunchAgentRecovery::Status(ConfigConnectionStatus::Disconnected) => {
                apply_launch_agent_desired(service, launchctl, &mut transaction)?;
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
fn rollback_launch_agent_operation(
    service: &CollectorService,
    launchctl: &impl Launchctl,
    error: IntegrationError,
) -> IntegrationError {
    match recover_launch_agent_transaction(service, launchctl, LaunchAgentRecovery::Connect) {
        Ok(_) => error,
        Err(rollback) => rollback_error(&error, &rollback),
    }
}

#[cfg(target_os = "macos")]
fn apply_launch_agent_desired(
    service: &CollectorService,
    launchctl: &impl Launchctl,
    transaction: &mut LaunchAgentTransaction,
) -> Result<(), IntegrationError> {
    stop_launch_agent(launchctl, service)?;
    transaction.phase = LaunchAgentPhase::ServiceStopped;
    save_launch_agent_transaction(service, transaction)?;

    replace_launch_agent_file(&service.plist, &transaction.desired_plist)?;
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
        launchctl.kickstart(&service.target)?;
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
    if !launchctl.is_loaded(&service.target)? {
        return Ok(());
    }
    let bootout = launchctl.bootout(&service.target);
    match launchctl.is_loaded(&service.target) {
        Ok(false) => Ok(()),
        Ok(true) => match bootout {
            Ok(_) => Err(IntegrationError::Runtime(format!(
                "LaunchAgent stop failed for {}",
                service.target
            ))),
            Err(error) => Err(error),
        },
        Err(status_error) => match bootout {
            Ok(_) => Err(status_error),
            Err(error) => Err(rollback_error(&error, &status_error)),
        },
    }
}

#[cfg(target_os = "macos")]
fn read_launch_agent_file(path: &Path) -> Result<LaunchAgentFileState, IntegrationError> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(LaunchAgentFileState {
                existed: false,
                bytes: Vec::new(),
                mode: 0,
            });
        }
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(IntegrationError::Runtime(
            "LaunchAgent plist must be a regular file".into(),
        ));
    }
    Ok(LaunchAgentFileState {
        existed: true,
        bytes: fs::read(path)?,
        mode: metadata.permissions().mode() & 0o777,
    })
}

#[cfg(target_os = "macos")]
fn replace_launch_agent_file(
    path: &Path,
    desired: &LaunchAgentFileState,
) -> Result<(), IntegrationError> {
    let parent = path
        .parent()
        .ok_or_else(|| IntegrationError::Runtime("LaunchAgent path has no parent".into()))?;
    fs::create_dir_all(parent).map_err(|error| {
        IntegrationError::Runtime(format!("LaunchAgent directory failed: {error}"))
    })?;
    if desired.existed {
        atomic_write(path, &desired.bytes, desired.mode, "LaunchAgent")
    } else {
        match fs::remove_file(path) {
            Ok(()) => sync_launch_agent_parent(path, "removal"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(IntegrationError::Runtime(format!(
                "LaunchAgent removal failed: {error}"
            ))),
        }
    }
}

#[cfg(target_os = "macos")]
fn load_launch_agent_transaction(
    service: &CollectorService,
) -> Result<Option<LaunchAgentTransaction>, IntegrationError> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = match fs::symlink_metadata(&service.ownership) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err(IntegrationError::Runtime(
            "LaunchAgent ownership state is not a private regular file".into(),
        ));
    }
    let transaction: LaunchAgentTransaction =
        serde_json::from_slice(&fs::read(&service.ownership)?).map_err(|error| {
            IntegrationError::Runtime(format!("invalid LaunchAgent ownership state: {error}"))
        })?;
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
    let parent = service
        .ownership
        .parent()
        .ok_or_else(|| IntegrationError::Runtime("LaunchAgent state path has no parent".into()))?;
    let integrations = parent.parent().ok_or_else(|| {
        IntegrationError::Runtime("LaunchAgent state parent has no runtime directory".into())
    })?;
    let runtime = integrations.parent().ok_or_else(|| {
        IntegrationError::Runtime("LaunchAgent state path has no runtime root".into())
    })?;
    let root = runtime.parent().ok_or_else(|| {
        IntegrationError::Runtime("LaunchAgent state path has no installed root".into())
    })?;
    ensure_private_runtime_directory(root)?;
    ensure_private_runtime_directory(runtime)?;
    ensure_private_runtime_directory(integrations)?;
    ensure_private_runtime_directory(parent)?;
    let mut bytes = serde_json::to_vec(transaction).map_err(|error| {
        IntegrationError::Runtime(format!("LaunchAgent state encode failed: {error}"))
    })?;
    bytes.push(b'\n');
    atomic_write(
        &service.ownership,
        &bytes,
        0o600,
        "LaunchAgent ownership state",
    )
}

#[cfg(target_os = "macos")]
fn ensure_private_runtime_directory(path: &Path) -> Result<(), IntegrationError> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

    let mut builder = fs::DirBuilder::new();
    builder.mode(0o700);
    match builder.create(path) {
        Ok(()) => {
            let parent = path.parent().ok_or_else(|| {
                IntegrationError::Runtime("private runtime directory has no parent".into())
            })?;
            File::open(parent)?.sync_all()?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err(IntegrationError::Runtime(
            "LaunchAgent state directory must be private and real".into(),
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn remove_launch_agent_transaction(service: &CollectorService) -> Result<(), IntegrationError> {
    match fs::remove_file(&service.ownership) {
        Ok(()) => sync_launch_agent_parent(&service.ownership, "state removal"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
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

#[cfg(not(target_os = "macos"))]
fn uninstall_collector_service(_service: &CollectorService) -> Result<(), IntegrationError> {
    Err(IntegrationError::Runtime(
        "automatic local collection currently supports macOS".into(),
    ))
}

fn wait_for_collector(root: &Path) -> Result<CollectorStatus, IntegrationError> {
    for _ in 0..40 {
        if let Ok(settings) = load_settings(root) {
            let status = probe_health(&settings);
            if matches!(status, CollectorStatus::Ready | CollectorStatus::Degraded) {
                return Ok(status);
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(IntegrationError::Runtime(
        "local collector did not become ready within 2 seconds".into(),
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
            Command::new("launchctl").args(["kickstart", "-k", target]),
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
        ConfigConnectionStatus, ConfigLifecycle, ConnectionStatus, IntegrationError,
        connect_prepared, connect_with_reloaded_settings, disconnect_prepared, ensure_codex_home,
        launch_agent_body, parse_health_response, probe_health, service_label, status,
        with_lifecycle_lock,
    };
    #[cfg(target_os = "macos")]
    use super::{
        LaunchAgentOperation, LaunchAgentPhase, LaunchAgentRecovery, Launchctl,
        commit_collector_service_install, commit_collector_service_uninstall,
        install_collector_service_with, load_launch_agent_transaction,
        recover_launch_agent_transaction, save_launch_agent_transaction,
        uninstall_collector_service_with,
    };
    use agent_observability_local_collector::{
        COLLECTOR_SETTINGS_VERSION, CollectorSettings, TOKEN_HEADER,
    };
    use agent_observability_local_runtime::{ConfigServiceError, LocalConfigService};
    #[cfg(target_os = "macos")]
    use std::collections::VecDeque;
    use std::{
        cell::{Cell, RefCell},
        fs,
        io::{Read as _, Write as _},
        net::TcpListener,
        path::{Path, PathBuf},
        sync::{Arc, Barrier, Mutex},
        thread,
        time::{SystemTime, UNIX_EPOCH},
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

    struct FakeConfig {
        state: Cell<ConfigConnectionStatus>,
        connect_error: Cell<bool>,
        disconnect_error: Cell<bool>,
        disconnect_error_after_restore: Cell<bool>,
        events: RefCell<Vec<&'static str>>,
    }

    impl FakeConfig {
        fn connected() -> Self {
            Self {
                state: Cell::new(ConfigConnectionStatus::Connected),
                connect_error: Cell::new(false),
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

        fn connect(&self) -> Result<ConfigConnectionStatus, IntegrationError> {
            self.events.borrow_mut().push("config-connect");
            if self.connect_error.get() {
                return Err(IntegrationError::Runtime("config connect failed".into()));
            }
            self.state.set(ConfigConnectionStatus::Connected);
            Ok(ConfigConnectionStatus::Connected)
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
        wait_error: bool,
        uninstall_error: bool,
        install_error: Cell<bool>,
        collector_status: CollectorStatus,
        events: RefCell<Vec<&'static str>>,
    }

    impl FakeLifecycle {
        fn ready() -> Self {
            Self {
                wait_error: false,
                uninstall_error: false,
                install_error: Cell::new(false),
                collector_status: CollectorStatus::Ready,
                events: RefCell::new(Vec::new()),
            }
        }

        fn service() -> CollectorService {
            CollectorService {
                label: "test-service".into(),
                plist: PathBuf::from("/tmp/test-service.plist"),
                target: "gui/1/test-service".into(),
                ownership: PathBuf::from("/tmp/launch-agent-ownership-v1.json"),
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
            if self.install_error.get() {
                return Err(IntegrationError::Runtime("install failed".into()));
            }
            Ok(Self::service())
        }

        fn service(&self, _root: &Path) -> Result<CollectorService, IntegrationError> {
            self.events.borrow_mut().push("service");
            Ok(Self::service())
        }

        fn wait_until_ready(&self, _root: &Path) -> Result<CollectorStatus, IntegrationError> {
            self.events.borrow_mut().push("health");
            if self.wait_error {
                Err(IntegrationError::Runtime("health failed".into()))
            } else {
                Ok(self.collector_status)
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
            config: ConnectionStatus::Conflict,
            collector: CollectorStatus::Unavailable,
            endpoint: Some("http://127.0.0.1:43181/v1/logs".into()),
            service: Some("io.agent-observability.collector.example".into()),
            data_retained: true,
        };
        let value = serde_json::to_value(status).unwrap();
        assert_eq!(value["config"], "conflict");
        assert_eq!(value["collector"], "unavailable");
        assert_eq!(value["data_retained"], true);
    }

    #[test]
    fn degraded_status_serializes_without_becoming_ready() {
        let status = CodexIntegrationStatus {
            config: ConnectionStatus::Connected,
            collector: CollectorStatus::Degraded,
            endpoint: None,
            service: None,
            data_retained: true,
        };

        let value = serde_json::to_value(status).unwrap();

        assert_eq!(value["collector"], "degraded");
    }

    #[test]
    fn health_parser_distinguishes_ready_degraded_and_invalid_payloads() {
        assert_eq!(
            parse_health_response(&health_response(
                r#"{"status":"ready","report_dirty":false}"#
            )),
            CollectorStatus::Ready
        );
        assert_eq!(
            parse_health_response(&health_response(
                r#"{"status":"degraded","report_dirty":true}"#,
            )),
            CollectorStatus::Degraded
        );
        assert_eq!(
            parse_health_response(&health_response(
                r#"{"status":"degraded","report_dirty":false}"#,
            )),
            CollectorStatus::Unavailable
        );
        assert_eq!(
            parse_health_response(b"HTTP/1.1 200 OK\r\nContent-Length: 999\r\n\r\n{}"),
            CollectorStatus::Unavailable
        );
        assert_eq!(
            parse_health_response(b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n"),
            CollectorStatus::Unavailable
        );
    }

    #[test]
    fn health_probe_is_authenticated_and_bounded() {
        let (ready, ready_server) = serve_health_once(health_response(
            r#"{"status":"ready","report_dirty":false,"accepted_requests":0}"#,
        ));
        assert_eq!(probe_health(&ready), CollectorStatus::Ready);
        ready_server.join().unwrap();

        let (oversized, oversized_server) = serve_health_once(vec![b'x'; 4 * 1024 + 1]);
        assert_eq!(probe_health(&oversized), CollectorStatus::Unavailable);
        oversized_server.join().unwrap();
    }

    #[test]
    fn root_lifecycle_lock_rejects_concurrent_interleaving_through_rollback() {
        let root = temporary_root("lifecycle-lock");
        let layout = agent_observability_local_runtime::install(&root).unwrap();
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let events = Arc::new(Mutex::new(Vec::new()));
        let worker_layout = layout.clone();
        let worker_entered = Arc::clone(&entered);
        let worker_release = Arc::clone(&release);
        let worker_events = Arc::clone(&events);
        let worker = thread::spawn(move || {
            with_lifecycle_lock(&worker_layout, || {
                worker_events
                    .lock()
                    .unwrap()
                    .extend(["service", "settings", "health", "config"]);
                worker_entered.wait();
                worker_release.wait();
                worker_events.lock().unwrap().push("rollback");
                Ok(())
            })
            .unwrap();
        });

        entered.wait();
        let config = LocalConfigService::new(&layout);
        let versioned = config.read().unwrap();
        assert!(matches!(
            config.save(&versioned.revision, &versioned.config),
            Err(ConfigServiceError::Busy)
        ));
        let contender_events = Arc::clone(&events);
        let error = with_lifecycle_lock(&layout, || {
            contender_events.lock().unwrap().push("interleaved");
            Ok(())
        })
        .unwrap_err();
        assert!(error.to_string().contains("lifecycle is busy"));
        release.wait();
        worker.join().unwrap();
        assert_eq!(
            *events.lock().unwrap(),
            ["service", "settings", "health", "config", "rollback"]
        );
        let _ = fs::remove_dir_all(root);
    }

    fn health_response(body: &str) -> Vec<u8> {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .into_bytes()
    }

    fn serve_health_once(response: Vec<u8>) -> (CollectorSettings, thread::JoinHandle<()>) {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 1024];
            let bytes = stream.read(&mut request).unwrap();
            let request = std::str::from_utf8(&request[..bytes]).unwrap();
            assert!(request.contains(&format!("{TOKEN_HEADER}: test-token\r\n")));
            stream.write_all(&response).unwrap();
        });
        (
            CollectorSettings {
                schema_version: COLLECTOR_SETTINGS_VERSION.into(),
                port,
                token: "test-token".into(),
                source_generation: "test-generation".into(),
            },
            server,
        )
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
            "http://127.0.0.1:43181/v1/logs",
            &config,
            &lifecycle,
        )
        .unwrap();

        assert_eq!(status.config, ConnectionStatus::Connected);
        assert_eq!(status.collector, CollectorStatus::Degraded);
        assert_eq!(*lifecycle.events.borrow(), ["install", "health"]);
        assert_eq!(*config.events.borrow(), ["config-connect"]);
    }

    #[test]
    fn connect_builds_config_from_settings_reloaded_after_collector_ready() {
        let lifecycle = FakeLifecycle::ready();
        let manager_port = Cell::new(0);
        let status = connect_with_reloaded_settings(
            Path::new("/runtime"),
            Path::new("/bin/agentobs"),
            &lifecycle,
            || {
                assert_eq!(*lifecycle.events.borrow(), ["install", "health"]);
                Ok(CollectorSettings {
                    schema_version: COLLECTOR_SETTINGS_VERSION.into(),
                    port: 49_321,
                    token: "rotated-token".into(),
                    source_generation: "rotated-generation".into(),
                })
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
            Some("http://127.0.0.1:49321/v1/logs")
        );
        assert_eq!(status.config, ConnectionStatus::Connected);
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
                "http://127.0.0.1:43181/v1/logs",
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
                "http://127.0.0.1:43181/v1/logs",
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
                "http://127.0.0.1:43181/v1/logs",
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
                "http://127.0.0.1:43181/v1/logs",
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
                "http://127.0.0.1:43181/v1/logs",
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
                "http://127.0.0.1:43181/v1/logs",
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
            "http://127.0.0.1:43181/v1/logs",
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
                "http://127.0.0.1:43181/v1/logs",
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
            "http://127.0.0.1:43181/v1/logs",
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
        loaded: Cell<bool>,
        bootstrap_results: RefCell<VecDeque<Result<(), &'static str>>>,
        kickstart_results: RefCell<VecDeque<Result<(), &'static str>>>,
        bootout_results: RefCell<VecDeque<Result<bool, &'static str>>>,
        events: RefCell<Vec<&'static str>>,
    }

    #[cfg(target_os = "macos")]
    impl FakeLaunchctl {
        fn new(loaded: bool) -> Self {
            Self {
                loaded: Cell::new(loaded),
                bootstrap_results: RefCell::new(VecDeque::new()),
                kickstart_results: RefCell::new(VecDeque::new()),
                bootout_results: RefCell::new(VecDeque::new()),
                events: RefCell::new(Vec::new()),
            }
        }
    }

    #[cfg(target_os = "macos")]
    impl Launchctl for FakeLaunchctl {
        fn bootout(&self, _target: &str) -> Result<bool, IntegrationError> {
            self.events.borrow_mut().push("bootout");
            let result = self
                .bootout_results
                .borrow_mut()
                .pop_front()
                .unwrap_or(Ok(true));
            if matches!(result, Ok(true)) {
                self.loaded.set(false);
            }
            result.map_err(|message| IntegrationError::Runtime(message.into()))
        }

        fn is_loaded(&self, _target: &str) -> Result<bool, IntegrationError> {
            self.events.borrow_mut().push("is-loaded");
            Ok(self.loaded.get())
        }

        fn bootstrap(&self, _domain: &str, _plist: &Path) -> Result<(), IntegrationError> {
            self.events.borrow_mut().push("bootstrap");
            let result = self
                .bootstrap_results
                .borrow_mut()
                .pop_front()
                .unwrap_or(Ok(()));
            if result.is_ok() {
                self.loaded.set(true);
            }
            result.map_err(|message| IntegrationError::Runtime(message.into()))
        }

        fn kickstart(&self, _target: &str) -> Result<(), IntegrationError> {
            self.events.borrow_mut().push("kickstart");
            self.kickstart_results
                .borrow_mut()
                .pop_front()
                .unwrap_or(Ok(()))
                .map_err(|message| IntegrationError::Runtime(message.into()))
        }
    }

    #[cfg(target_os = "macos")]
    fn test_service(root: &Path) -> CollectorService {
        CollectorService {
            label: "test-service".into(),
            plist: root.join("LaunchAgents/test-service.plist"),
            target: "gui/1/test-service".into(),
            ownership: root.join("runtime/integrations/codex/launch-agent-ownership-v1.json"),
        }
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
        launchctl
            .kickstart_results
            .borrow_mut()
            .push_back(Err("kickstart failed"));

        let error = install_collector_service_with(
            service.clone(),
            &root,
            Path::new("/bin/agentobs"),
            &launchctl,
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
    fn reconnect_failure_restores_previous_owned_service() {
        let root = temporary_root("reconnect-failure");
        let service = test_service(&root);
        let launchctl = FakeLaunchctl::new(false);
        install_collector_service_with(
            service.clone(),
            &root,
            Path::new("/bin/agentobs-v1"),
            &launchctl,
        )
        .unwrap();
        commit_collector_service_install(&service).unwrap();
        let previous = fs::read(&service.plist).unwrap();
        launchctl
            .kickstart_results
            .borrow_mut()
            .extend([Err("reconnect kickstart failed"), Ok(())]);

        let error = install_collector_service_with(
            service.clone(),
            &root,
            Path::new("/bin/agentobs-v2"),
            &launchctl,
        )
        .unwrap_err();

        assert!(error.to_string().contains("reconnect kickstart failed"));
        assert_eq!(fs::read(&service.plist).unwrap(), previous);
        assert!(launchctl.loaded.get());
        let transaction = load_launch_agent_transaction(&service).unwrap().unwrap();
        assert_eq!(transaction.phase, LaunchAgentPhase::Owned);
        assert_eq!(transaction.desired_plist.bytes, previous);
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
        )
        .unwrap();
        commit_collector_service_install(&service).unwrap();
        let events = launchctl.events.borrow().len();

        install_collector_service_with(
            service.clone(),
            &root,
            Path::new("/bin/agentobs"),
            &launchctl,
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
    fn status_recovery_converges_crashed_disconnect_to_prior_state() {
        let root = temporary_root("status-crash-recovery");
        let service = test_service(&root);
        let launchctl = FakeLaunchctl::new(false);
        install_collector_service_with(
            service.clone(),
            &root,
            Path::new("/bin/agentobs"),
            &launchctl,
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
}
