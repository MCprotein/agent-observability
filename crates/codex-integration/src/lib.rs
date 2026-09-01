#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc)]

use agent_observability_codex_config::{
    CodexConfigManager, ConfigError, ConnectionStatus as ConfigConnectionStatus,
};
use agent_observability_local_collector::{
    CollectorError, CollectorSettings, HealthOutcome, check_health, install_settings, load_settings,
};
use agent_observability_local_runtime::{InstalledLayout, install};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    env, fmt, fs,
    path::{Path, PathBuf},
    time::Duration,
};
#[cfg(target_os = "macos")]
use std::{
    fs::OpenOptions,
    io::Write,
    process::{Command, Stdio},
};

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
    let settings = install_settings(&layout.root)?;
    let manager = codex_config_manager(&layout, executable, &settings)?;
    connect_prepared(
        &layout.root,
        executable,
        &settings.endpoint(),
        &manager,
        &SystemLifecycle,
    )
}

fn connect_prepared(
    root: &Path,
    executable: &Path,
    endpoint: &str,
    config: &impl ConfigLifecycle,
    lifecycle: &impl CollectorLifecycle,
) -> Result<CodexIntegrationStatus, IntegrationError> {
    let service = lifecycle.install(root, executable)?;
    if let Err(error) = lifecycle.wait_until_ready(root) {
        return Err(rollback_service(lifecycle, &service, error));
    }
    let config = match config.connect() {
        Ok(status) => status.into(),
        Err(error) => return Err(rollback_service(lifecycle, &service, error)),
    };
    Ok(CodexIntegrationStatus {
        config,
        collector: CollectorStatus::Ready,
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
    let settings = load_settings(&layout.root)?;
    let manager = codex_config_manager(&layout, executable, &settings)?;
    disconnect_prepared(
        &layout.root,
        executable,
        &settings.endpoint(),
        &manager,
        &SystemLifecycle,
    )
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
        Ok(status) => status,
        Err(error) => {
            let reinstall = lifecycle
                .install(root, executable)
                .and_then(|_| lifecycle.wait_until_ready(root));
            return match reinstall {
                Ok(()) => Err(error),
                Err(reinstall) => Err(rollback_error(&error, &reinstall)),
            };
        }
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
    fn wait_until_ready(&self, root: &Path) -> Result<(), IntegrationError>;
    fn uninstall(&self, service: &CollectorService) -> Result<(), IntegrationError>;
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

    fn wait_until_ready(&self, root: &Path) -> Result<(), IntegrationError> {
        wait_for_collector(root)
    }

    fn uninstall(&self, service: &CollectorService) -> Result<(), IntegrationError> {
        uninstall_collector_service(service)
    }
}

fn rollback_service(
    lifecycle: &impl CollectorLifecycle,
    service: &CollectorService,
    error: IntegrationError,
) -> IntegrationError {
    match lifecycle.uninstall(service) {
        Ok(()) => error,
        Err(rollback) => rollback_error(&error, &rollback),
    }
}

fn rollback_error(error: &IntegrationError, rollback: &IntegrationError) -> IntegrationError {
    IntegrationError::Runtime(format!("{error}; rollback failed: {rollback}"))
}

pub fn status(root: &Path, executable: &Path) -> Result<CodexIntegrationStatus, IntegrationError> {
    let layout = install(root).map_err(runtime_error)?;
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
    Ok(CodexIntegrationStatus {
        config: manager.status()?.into(),
        collector: check_health(&layout.root).into(),
        endpoint: Some(settings.endpoint()),
        service: Some(service_label(&layout.root)),
        data_retained: true,
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

impl From<HealthOutcome> for CollectorStatus {
    fn from(status: HealthOutcome) -> Self {
        match status {
            HealthOutcome::Ready => Self::Ready,
            HealthOutcome::Unavailable => Self::Unavailable,
        }
    }
}

fn runtime_error(error: impl fmt::Display) -> IntegrationError {
    IntegrationError::Runtime(error.to_string())
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
    fs::create_dir_all(&codex_home)
        .map_err(|error| IntegrationError::Runtime(format!("cannot create Codex home: {error}")))?;
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

#[derive(Debug)]
struct CollectorService {
    label: String,
    plist: PathBuf,
    target: String,
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
        label,
    })
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
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let parent = service
        .plist
        .parent()
        .ok_or_else(|| IntegrationError::Runtime("LaunchAgent path has no parent".into()))?;
    fs::create_dir_all(parent).map_err(|error| {
        IntegrationError::Runtime(format!("LaunchAgent directory failed: {error}"))
    })?;
    let body = launch_agent_body(&service.label, executable, root);
    let temporary = service
        .plist
        .with_extension(format!("plist.tmp.{}", std::process::id()));
    let _ = fs::remove_file(&temporary);
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o644)
        .open(&temporary)
        .map_err(|error| IntegrationError::Runtime(format!("LaunchAgent write failed: {error}")))?;
    file.write_all(body.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|error| IntegrationError::Runtime(format!("LaunchAgent write failed: {error}")))?;
    fs::rename(&temporary, &service.plist).map_err(|error| {
        IntegrationError::Runtime(format!("LaunchAgent install failed: {error}"))
    })?;
    fs::set_permissions(&service.plist, fs::Permissions::from_mode(0o644)).map_err(|error| {
        IntegrationError::Runtime(format!("LaunchAgent permissions failed: {error}"))
    })?;
    let domain = service
        .target
        .rsplit_once('/')
        .map(|(domain, _)| domain)
        .ok_or_else(|| IntegrationError::Runtime("invalid LaunchAgent target".into()))?;
    let _ = launchctl.bootout(&service.target);
    if let Err(error) = launchctl.bootstrap(domain, &service.plist) {
        return Err(cleanup_failed_install(launchctl, &service, error));
    }
    if let Err(error) = launchctl.kickstart(&service.target) {
        return Err(cleanup_failed_install(launchctl, &service, error));
    }
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
    if !launchctl.bootout(&service.target)? && launchctl.is_loaded(&service.target)? {
        return Err(IntegrationError::Runtime(format!(
            "LaunchAgent stop failed for {}",
            service.target
        )));
    }
    remove_service_plist(service)
}

#[cfg(target_os = "macos")]
fn remove_service_plist(service: &CollectorService) -> Result<(), IntegrationError> {
    match fs::remove_file(&service.plist) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(IntegrationError::Runtime(format!(
            "LaunchAgent removal failed: {error}"
        ))),
    }
}

#[cfg(target_os = "macos")]
fn cleanup_failed_install(
    launchctl: &impl Launchctl,
    service: &CollectorService,
    error: IntegrationError,
) -> IntegrationError {
    let stopped = launchctl.bootout(&service.target).and_then(|stopped| {
        if stopped {
            Ok(true)
        } else {
            launchctl.is_loaded(&service.target).map(|loaded| !loaded)
        }
    });
    match stopped {
        Ok(true) => match remove_service_plist(service) {
            Ok(()) => error,
            Err(rollback) => rollback_error(&error, &rollback),
        },
        Ok(false) => rollback_error(
            &error,
            &IntegrationError::Runtime(format!(
                "LaunchAgent termination unconfirmed for {}; plist retained",
                service.target
            )),
        ),
        Err(rollback) => rollback_error(&error, &rollback),
    }
}

#[cfg(not(target_os = "macos"))]
fn uninstall_collector_service(_service: &CollectorService) -> Result<(), IntegrationError> {
    Err(IntegrationError::Runtime(
        "automatic local collection currently supports macOS".into(),
    ))
}

fn wait_for_collector(root: &Path) -> Result<(), IntegrationError> {
    for _ in 0..40 {
        if check_health(root) == HealthOutcome::Ready {
            return Ok(());
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

fn launch_agent_body(label: &str, executable: &Path, root: &Path) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict>\n<key>Label</key><string>{}</string>\n<key>ProgramArguments</key><array><string>{}</string><string>collector-serve</string><string>{}</string></array>\n<key>RunAtLoad</key><true/>\n<key>KeepAlive</key><true/>\n<key>ProcessType</key><string>Background</string>\n</dict></plist>\n",
        xml_escape(label),
        xml_escape(&executable.display().to_string()),
        xml_escape(&root.display().to_string()),
    )
}

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
        connect_prepared, disconnect_prepared, launch_agent_body, service_label, status,
    };
    #[cfg(target_os = "macos")]
    use super::{Launchctl, install_collector_service_with, uninstall_collector_service_with};
    #[cfg(target_os = "macos")]
    use std::collections::VecDeque;
    use std::{
        cell::{Cell, RefCell},
        fs,
        path::{Path, PathBuf},
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

    struct FakeConfig {
        state: Cell<ConfigConnectionStatus>,
        connect_error: Cell<bool>,
        disconnect_error: Cell<bool>,
        events: RefCell<Vec<&'static str>>,
    }

    impl FakeConfig {
        fn connected() -> Self {
            Self {
                state: Cell::new(ConfigConnectionStatus::Connected),
                connect_error: Cell::new(false),
                disconnect_error: Cell::new(false),
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
            Ok(ConfigConnectionStatus::Disconnected)
        }
    }

    struct FakeLifecycle {
        wait_error: bool,
        uninstall_error: bool,
        install_error: Cell<bool>,
        events: RefCell<Vec<&'static str>>,
    }

    impl FakeLifecycle {
        fn ready() -> Self {
            Self {
                wait_error: false,
                uninstall_error: false,
                install_error: Cell::new(false),
                events: RefCell::new(Vec::new()),
            }
        }

        fn service() -> CollectorService {
            CollectorService {
                label: "test-service".into(),
                plist: PathBuf::from("/tmp/test-service.plist"),
                target: "gui/1/test-service".into(),
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

        fn wait_until_ready(&self, _root: &Path) -> Result<(), IntegrationError> {
            self.events.borrow_mut().push("health");
            if self.wait_error {
                Err(IntegrationError::Runtime("health failed".into()))
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
            ["config-status", "config-disconnect"]
        );
        assert_eq!(config.state.get(), ConfigConnectionStatus::Connected);
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
        bootstrap_error: bool,
        kickstart_error: bool,
        bootout_results: RefCell<VecDeque<Result<bool, &'static str>>>,
        events: RefCell<Vec<&'static str>>,
    }

    #[cfg(target_os = "macos")]
    impl Launchctl for FakeLaunchctl {
        fn bootout(&self, _target: &str) -> Result<bool, IntegrationError> {
            self.events.borrow_mut().push("bootout");
            self.bootout_results
                .borrow_mut()
                .pop_front()
                .unwrap_or(Ok(true))
                .map_err(|message| IntegrationError::Runtime(message.into()))
        }

        fn bootstrap(&self, _domain: &str, _plist: &Path) -> Result<(), IntegrationError> {
            self.events.borrow_mut().push("bootstrap");
            if self.bootstrap_error {
                Err(IntegrationError::Runtime("bootstrap failed".into()))
            } else {
                Ok(())
            }
        }

        fn kickstart(&self, _target: &str) -> Result<(), IntegrationError> {
            self.events.borrow_mut().push("kickstart");
            if self.kickstart_error {
                Err(IntegrationError::Runtime("kickstart failed".into()))
            } else {
                Ok(())
            }
        }
    }

    #[cfg(target_os = "macos")]
    struct AlreadyStoppedLaunchctl;

    #[cfg(target_os = "macos")]
    impl Launchctl for AlreadyStoppedLaunchctl {
        fn bootout(&self, _target: &str) -> Result<bool, IntegrationError> {
            Ok(false)
        }

        fn is_loaded(&self, _target: &str) -> Result<bool, IntegrationError> {
            Ok(false)
        }

        fn bootstrap(&self, _domain: &str, _plist: &Path) -> Result<(), IntegrationError> {
            unreachable!()
        }

        fn kickstart(&self, _target: &str) -> Result<(), IntegrationError> {
            unreachable!()
        }
    }

    #[cfg(target_os = "macos")]
    fn test_service(root: &Path) -> CollectorService {
        CollectorService {
            label: "test-service".into(),
            plist: root.join("LaunchAgents/test-service.plist"),
            target: "gui/1/test-service".into(),
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn bootstrap_failure_removes_plist_and_boots_out_partial_service() {
        let root = temporary_root("bootstrap-failure");
        let service = test_service(&root);
        let plist = service.plist.clone();
        let launchctl = FakeLaunchctl {
            bootstrap_error: true,
            kickstart_error: false,
            bootout_results: RefCell::new(VecDeque::from([Ok(true), Ok(true)])),
            events: RefCell::new(Vec::new()),
        };

        assert!(
            install_collector_service_with(service, &root, Path::new("/bin/agentobs"), &launchctl,)
                .is_err()
        );
        assert_eq!(
            *launchctl.events.borrow(),
            ["bootout", "bootstrap", "bootout"]
        );
        assert!(!plist.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn kickstart_failure_removes_plist_and_boots_out_service() {
        let root = temporary_root("kickstart-failure");
        let service = test_service(&root);
        let plist = service.plist.clone();
        let launchctl = FakeLaunchctl {
            bootstrap_error: false,
            kickstart_error: true,
            bootout_results: RefCell::new(VecDeque::from([Ok(true), Ok(true)])),
            events: RefCell::new(Vec::new()),
        };

        assert!(
            install_collector_service_with(service, &root, Path::new("/bin/agentobs"), &launchctl,)
                .is_err()
        );
        assert_eq!(
            *launchctl.events.borrow(),
            ["bootout", "bootstrap", "kickstart", "bootout"]
        );
        assert!(!plist.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn failed_install_retains_plist_when_bootout_is_unconfirmed() {
        let root = temporary_root("cleanup-bootout-false");
        let service = test_service(&root);
        let plist = service.plist.clone();
        let launchctl = FakeLaunchctl {
            bootstrap_error: true,
            kickstart_error: false,
            bootout_results: RefCell::new(VecDeque::from([Ok(true), Ok(false)])),
            events: RefCell::new(Vec::new()),
        };

        let error =
            install_collector_service_with(service, &root, Path::new("/bin/agentobs"), &launchctl)
                .unwrap_err();

        assert!(error.to_string().contains("termination unconfirmed"));
        assert!(plist.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn failed_install_retains_plist_when_bootout_errors() {
        let root = temporary_root("cleanup-bootout-error");
        let service = test_service(&root);
        let plist = service.plist.clone();
        let launchctl = FakeLaunchctl {
            bootstrap_error: true,
            kickstart_error: false,
            bootout_results: RefCell::new(VecDeque::from([Ok(true), Err("bootout failed")])),
            events: RefCell::new(Vec::new()),
        };

        let error =
            install_collector_service_with(service, &root, Path::new("/bin/agentobs"), &launchctl)
                .unwrap_err();

        assert!(error.to_string().contains("bootout failed"));
        assert!(plist.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn uninstall_retains_plist_when_bootout_is_unconfirmed() {
        let root = temporary_root("uninstall-bootout-false");
        let service = test_service(&root);
        fs::create_dir_all(service.plist.parent().unwrap()).unwrap();
        fs::write(&service.plist, b"plist").unwrap();
        let launchctl = FakeLaunchctl {
            bootstrap_error: false,
            kickstart_error: false,
            bootout_results: RefCell::new(VecDeque::from([Ok(false)])),
            events: RefCell::new(Vec::new()),
        };

        assert!(uninstall_collector_service_with(&service, &launchctl).is_err());
        assert!(service.plist.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn uninstall_retains_plist_when_bootout_errors() {
        let root = temporary_root("uninstall-bootout-error");
        let service = test_service(&root);
        fs::create_dir_all(service.plist.parent().unwrap()).unwrap();
        fs::write(&service.plist, b"plist").unwrap();
        let launchctl = FakeLaunchctl {
            bootstrap_error: false,
            kickstart_error: false,
            bootout_results: RefCell::new(VecDeque::from([Err("bootout failed")])),
            events: RefCell::new(Vec::new()),
        };

        assert!(uninstall_collector_service_with(&service, &launchctl).is_err());
        assert!(service.plist.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn uninstall_recovery_removes_retained_plist_after_confirmed_bootout() {
        let root = temporary_root("uninstall-recovery");
        let service = test_service(&root);
        fs::create_dir_all(service.plist.parent().unwrap()).unwrap();
        fs::write(&service.plist, b"plist").unwrap();
        let launchctl = FakeLaunchctl {
            bootstrap_error: false,
            kickstart_error: false,
            bootout_results: RefCell::new(VecDeque::from([Ok(false), Ok(true)])),
            events: RefCell::new(Vec::new()),
        };

        assert!(uninstall_collector_service_with(&service, &launchctl).is_err());
        assert!(service.plist.exists());
        uninstall_collector_service_with(&service, &launchctl).unwrap();
        assert!(!service.plist.exists());
        assert_eq!(*launchctl.events.borrow(), ["bootout", "bootout"]);
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn uninstall_accepts_independently_confirmed_already_stopped_service() {
        let root = temporary_root("uninstall-already-stopped");
        let service = test_service(&root);
        fs::create_dir_all(service.plist.parent().unwrap()).unwrap();
        fs::write(&service.plist, b"plist").unwrap();

        uninstall_collector_service_with(&service, &AlreadyStoppedLaunchctl).unwrap();

        assert!(!service.plist.exists());
        let _ = fs::remove_dir_all(root);
    }
}
