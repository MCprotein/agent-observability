#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc)]

use agent_observability_adapter_codex::{
    AdapterBatch, AdapterItem, MAX_HANDOFF_BYTES, MAX_PRIVATE_TURN_DETAIL_BYTES,
    OtlpRequestCorrelationState, PrivateCodexTurnDetailV1, parse_otlp_http_json_with_state,
    parse_projected_notify_json, project_notify_json, project_notify_with_private_detail,
};
use agent_observability_application::ReportProjector;
#[cfg(test)]
use agent_observability_application::project_report;
use agent_observability_local_runtime::{
    Admission, InstalledLayout, LocalRuntimeConfigV2, MutationGuard, PressureSample,
    RuntimeControl, Singleton, SingletonError, StorageBudget, install, load,
};
use agent_observability_local_store::{LocalStore, StoreBatchItem};
use agent_observability_static_report::write_private;
use axum::{
    Router,
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, Request, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    serve::Listener,
};
use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, IsCa,
    Issuer, KeyPair, KeyUsagePurpose, SanType,
};
use rustls::{
    ClientConfig, ClientConnection, RootCertStore, ServerConfig, StreamOwned,
    crypto::aws_lc_rs,
    pki_types::{CertificateDer, PrivateKeyDer, ServerName, pem::PemObject},
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener as StdTcpListener, TcpStream},
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    task::{Context, Poll},
    time::{Duration, Instant as StdInstant, SystemTime, UNIX_EPOCH},
};
use time::OffsetDateTime;
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::{TcpListener, TcpStream as TokioTcpStream},
    sync::{Mutex, OwnedSemaphorePermit, Semaphore, mpsc},
    time::Sleep,
};
use tokio_rustls::{TlsAcceptor, server::TlsStream};

pub const REPORT_FILE_NAME: &str = "agent-observability-report.html";
pub const COLLECTOR_SETTINGS_VERSION: &str = "local_collector.v3";
pub const COLLECTOR_TRANSPORT: &str = "private-ca-https-token";
pub const AUTH_HEADER_NAME: &str = "x-agent-observability-token";
const TLS_DIRECTORY: &str = "integrations/codex/tls";
const CA_CERTIFICATE_NAME: &str = "ca-certificate.pem";
const SERVER_CERTIFICATE_NAME: &str = "server-certificate.pem";
const SERVER_PRIVATE_KEY_NAME: &str = "server-private-key.pem";
const LEGACY_CLIENT_CERTIFICATE_NAME: &str = "client-certificate.pem";
const LEGACY_CLIENT_PRIVATE_KEY_NAME: &str = "client-private-key.pem";
const CREDENTIAL_LIFETIME: Duration = Duration::from_hours(8_760);
const SOURCE_GENERATION: &str = "codex-otel-v1";
const MAX_SETTINGS_BYTES: u64 = 16 * 1024;
const MAX_SETTINGS_MIGRATION_BYTES: u64 = 128 * 1024;
const SETTINGS_MIGRATION_VERSION: &str = "local_collector_settings_migration.v1";
const MAX_CREDENTIAL_BYTES: u64 = 64 * 1024;
const MAX_CREDENTIAL_PATH_BYTES: usize = 256;
static SETTINGS_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const REPORT_DIRTY_FILE_NAME: &str = "report-dirty";
const PRIVATE_TURN_DETAIL_DIRECTORY: &str = "private-codex-turn-details";
const MAX_PRIVATE_TURN_DETAIL_FILES: usize = 1_024;
const MAX_PRIVATE_TURN_DETAIL_SCAN_ENTRIES: usize = 4_096;
const REPORT_RETRY_LIMIT: u32 = 4;
const REPORT_RETRY_INITIAL_DELAY: Duration = Duration::from_millis(50);
const REPORT_DEBOUNCE_DELAY: Duration = Duration::from_millis(200);
const REPORT_AUTHORITY_POLL_INTERVAL: Duration = Duration::from_millis(500);
const HEADER_READ_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_LIFETIME: Duration = Duration::from_secs(30);
const MAX_CONNECTIONS: usize = 64;
const HEADER_TERMINATOR: &[u8] = b"\r\n\r\n";

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CollectorSettings {
    pub schema_version: String,
    pub generation: String,
    pub port: u16,
    pub transport: String,
    pub auth_token: String,
    pub credentials: CredentialMetadata,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CredentialMetadata {
    pub ca_certificate: String,
    pub server_certificate: String,
    pub server_private_key: String,
    pub expires_at_unix_ms: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OtlpSubmissionOutcome {
    Accepted,
    Rejected {
        status: u16,
        category: OtlpRejectionCategory,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OtlpRejectionCategory {
    Unauthorized,
    Policy,
    MediaType,
    Invalid,
    Busy,
    Pressure,
    Storage,
    Internal,
    Other,
}

impl OtlpRejectionCategory {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unauthorized => "unauthorized",
            Self::Policy => "policy",
            Self::MediaType => "media-type",
            Self::Invalid => "invalid",
            Self::Busy => "busy",
            Self::Pressure => "pressure",
            Self::Storage => "storage",
            Self::Internal => "internal",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyCollectorSettingsV1 {
    schema_version: String,
    port: u16,
    token: String,
    source_generation: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyCollectorSettingsV2Mtls {
    schema_version: String,
    generation: String,
    port: u16,
    transport: String,
    credentials: LegacyCredentialMetadataV2Mtls,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyCredentialMetadataV2Mtls {
    ca_certificate: String,
    server_certificate: String,
    server_private_key: String,
    client_certificate: String,
    client_private_key: String,
    expires_at_unix_ms: u64,
}

#[derive(Debug, Eq, PartialEq)]
struct PrivateFileSnapshot {
    bytes: Vec<u8>,
    mode: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum SettingsMigrationPhase {
    Pending,
    IntegrationCommitted,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SettingsMigrationV1 {
    schema_version: String,
    phase: SettingsMigrationPhase,
    previous_settings: Vec<u8>,
    previous_mode: u32,
    previous_generation: Option<String>,
    replacement_generation: String,
}

impl CollectorSettings {
    #[must_use]
    pub fn endpoint(&self) -> String {
        format!("https://127.0.0.1:{}/v1/logs", self.port)
    }

    #[must_use]
    pub fn options(&self, root: &Path) -> CollectorOptions {
        CollectorOptions {
            root: root.to_path_buf(),
            port: self.port,
            generation: self.generation.clone(),
            auth_token: self.auth_token.clone(),
            credentials: self.credentials.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct CollectorOptions {
    pub root: PathBuf,
    pub port: u16,
    pub generation: String,
    pub auth_token: String,
    pub credentials: CredentialMetadata,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NotifyOutcome {
    Accepted,
    Rejected,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HealthOutcome {
    Ready,
    Degraded,
    Unavailable,
}

#[derive(Debug)]
pub enum CollectorError {
    Io(std::io::Error),
    RequestIo {
        stage: &'static str,
        source: std::io::Error,
    },
    Runtime(String),
}

/// Creates or loads the private, idempotent local collector settings.
pub fn install_settings(root: &Path) -> Result<CollectorSettings, CollectorError> {
    let layout = install(root).map_err(runtime_error)?;
    recover_settings_migration_before_install(&layout)?;
    let path = settings_path(&layout);
    match fs::symlink_metadata(&path) {
        Ok(_) => {
            let snapshot = read_private_snapshot(&path, MAX_SETTINGS_BYTES)?;
            if let Ok(settings) = parse_owned_settings(&snapshot.bytes) {
                if settings.credentials.expires_at_unix_ms > current_unix_ms()? {
                    validate_owned_credentials(&layout, &settings)?;
                    return Ok(settings);
                }
                validate_owned_credentials(&layout, &settings)?;
                return replace_settings(&layout, Some(&snapshot));
            }
            let legacy_generation =
                validate_legacy_settings_for_migration(&layout, &snapshot.bytes)?;
            begin_settings_migration(&layout, &snapshot, legacy_generation)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => rotate_settings(&layout),
        Err(error) => Err(error.into()),
    }
}

fn validate_legacy_settings_for_migration(
    layout: &InstalledLayout,
    bytes: &[u8],
) -> Result<Option<String>, CollectorError> {
    if let Ok(legacy) = serde_json::from_slice::<LegacyCollectorSettingsV1>(bytes)
        && legacy.schema_version == "local_collector.v1"
        && legacy.port != 0
        && legacy.token.len() == 64
        && legacy.token.bytes().all(|byte| byte.is_ascii_hexdigit())
        && legacy.source_generation == SOURCE_GENERATION
    {
        return Ok(None);
    }
    let legacy: LegacyCollectorSettingsV2Mtls = serde_json::from_slice(bytes)
        .map_err(|_| CollectorError::Runtime("invalid legacy collector settings".into()))?;
    validate_legacy_v2_mtls(layout, &legacy)?;
    Ok(Some(legacy.generation))
}

fn validate_legacy_v2_mtls(
    layout: &InstalledLayout,
    settings: &LegacyCollectorSettingsV2Mtls,
) -> Result<(), CollectorError> {
    if settings.schema_version != "local_collector.v2"
        || settings.port == 0
        || settings.transport != "mtls"
        || settings.generation.len() != 64
        || !settings
            .generation
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || settings.credentials.expires_at_unix_ms == 0
    {
        return Err(CollectorError::Runtime(
            "invalid legacy collector settings".into(),
        ));
    }
    let expected_prefix = format!("{TLS_DIRECTORY}/{}/", settings.generation);
    for (path, name) in [
        (&settings.credentials.ca_certificate, CA_CERTIFICATE_NAME),
        (
            &settings.credentials.server_certificate,
            SERVER_CERTIFICATE_NAME,
        ),
        (
            &settings.credentials.server_private_key,
            SERVER_PRIVATE_KEY_NAME,
        ),
        (
            &settings.credentials.client_certificate,
            LEGACY_CLIENT_CERTIFICATE_NAME,
        ),
        (
            &settings.credentials.client_private_key,
            LEGACY_CLIENT_PRIVATE_KEY_NAME,
        ),
    ] {
        if path.len() > MAX_CREDENTIAL_PATH_BYTES || path != &format!("{expected_prefix}{name}") {
            return Err(CollectorError::Runtime(
                "invalid legacy collector credential path".into(),
            ));
        }
        read_private_bounded(&credential_path(layout, path)?, MAX_CREDENTIAL_BYTES)?;
    }
    let generation_dir = layout
        .runtime
        .join(TLS_DIRECTORY)
        .join(&settings.generation);
    validate_private_directory_tree(&layout.runtime, &generation_dir)?;
    let current_credentials = CredentialMetadata {
        ca_certificate: settings.credentials.ca_certificate.clone(),
        server_certificate: settings.credentials.server_certificate.clone(),
        server_private_key: settings.credentials.server_private_key.clone(),
        expires_at_unix_ms: settings.credentials.expires_at_unix_ms,
    };
    build_server_config(layout, &current_credentials)?;
    build_legacy_client_config(layout, &settings.credentials)?;
    Ok(())
}

fn rotate_settings(layout: &InstalledLayout) -> Result<CollectorSettings, CollectorError> {
    replace_settings(layout, None)
}

fn begin_settings_migration(
    layout: &InstalledLayout,
    previous: &PrivateFileSnapshot,
    previous_generation: Option<String>,
) -> Result<CollectorSettings, CollectorError> {
    let replacement_generation = random_hex_256()?;
    let migration = SettingsMigrationV1 {
        schema_version: SETTINGS_MIGRATION_VERSION.into(),
        phase: SettingsMigrationPhase::Pending,
        previous_settings: previous.bytes.clone(),
        previous_mode: previous.mode,
        previous_generation,
        replacement_generation: replacement_generation.clone(),
    };
    let migration_path = settings_migration_path(layout);
    match fs::symlink_metadata(&migration_path) {
        Ok(_) => {
            return Err(CollectorError::Runtime(
                "collector settings migration is already pending".into(),
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    write_private_json(&migration_path, &migration)?;
    let settings = match generate_settings_for_generation(layout, replacement_generation) {
        Ok(settings) => settings,
        Err(error) => {
            let cleanup = cleanup_credential_generation(layout, &migration.replacement_generation)
                .and_then(|()| remove_private_file(&migration_path));
            return Err(match cleanup {
                Ok(()) => error,
                Err(cleanup) => CollectorError::Runtime(format!(
                    "{error}; collector migration rollback failed: {cleanup}"
                )),
            });
        }
    };
    if let Err(error) = write_private_json_if_unchanged(
        &settings_path(layout),
        &settings,
        previous,
        MAX_SETTINGS_BYTES,
    ) {
        let cleanup = cleanup_credential_generation(layout, &settings.generation)
            .and_then(|()| remove_private_file(&migration_path));
        return Err(match cleanup {
            Ok(()) => error,
            Err(cleanup) => CollectorError::Runtime(format!(
                "{error}; collector migration rollback failed: {cleanup}"
            )),
        });
    }
    Ok(settings)
}

fn replace_settings(
    layout: &InstalledLayout,
    expected: Option<&PrivateFileSnapshot>,
) -> Result<CollectorSettings, CollectorError> {
    let settings = generate_settings(layout)?;
    let write_result = match expected {
        Some(expected) => write_private_json_if_unchanged(
            &settings_path(layout),
            &settings,
            expected,
            MAX_SETTINGS_BYTES,
        ),
        None => write_private_json(&settings_path(layout), &settings),
    };
    if let Err(error) = write_result {
        return Err(
            match cleanup_credential_generation(layout, &settings.generation) {
                Ok(()) => error,
                Err(cleanup) => CollectorError::Runtime(format!(
                    "{error}; collector credential cleanup failed: {cleanup}"
                )),
            },
        );
    }
    Ok(settings)
}

fn generate_settings(layout: &InstalledLayout) -> Result<CollectorSettings, CollectorError> {
    let generation = random_hex_256()?;
    generate_settings_for_generation(layout, generation)
}

fn generate_settings_for_generation(
    layout: &InstalledLayout,
    generation: String,
) -> Result<CollectorSettings, CollectorError> {
    let auth_token = random_hex_256()?;
    let port = available_port()?;
    let credentials = match generate_credentials(layout, &generation) {
        Ok(credentials) => credentials,
        Err(error) => {
            return Err(match cleanup_credential_generation(layout, &generation) {
                Ok(()) => error,
                Err(cleanup) => CollectorError::Runtime(format!(
                    "{error}; collector credential cleanup failed: {cleanup}"
                )),
            });
        }
    };
    Ok(CollectorSettings {
        schema_version: COLLECTOR_SETTINGS_VERSION.into(),
        generation,
        port,
        transport: COLLECTOR_TRANSPORT.into(),
        auth_token,
        credentials,
    })
}

fn random_hex_256() -> Result<String, CollectorError> {
    let mut random = [0_u8; 32];
    getrandom::fill(&mut random)
        .map_err(|error| CollectorError::Runtime(format!("collector entropy failed: {error}")))?;
    Ok(random
        .iter()
        .fold(String::with_capacity(64), |mut value, byte| {
            use std::fmt::Write as _;
            write!(value, "{byte:02x}").expect("writing to String cannot fail");
            value
        }))
}

fn cleanup_credential_generation(
    layout: &InstalledLayout,
    generation: &str,
) -> Result<(), CollectorError> {
    let tls_root = layout.runtime.join(TLS_DIRECTORY);
    match fs::remove_dir_all(tls_root.join(generation)) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    }
    File::open(tls_root)?.sync_all()?;
    Ok(())
}

fn available_port() -> Result<u16, CollectorError> {
    let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    listener
        .local_addr()
        .map(|address| address.port())
        .map_err(Into::into)
}

/// Loads and validates the private local collector settings.
pub fn load_settings(root: &Path) -> Result<CollectorSettings, CollectorError> {
    let layout = install(root).map_err(runtime_error)?;
    let path = settings_path(&layout);
    let snapshot = read_private_snapshot(&path, MAX_SETTINGS_BYTES)?;
    let settings = parse_current_settings(&snapshot.bytes)?;
    validate_owned_credentials(&layout, &settings)?;
    Ok(settings)
}

/// Commits a pending legacy settings migration after collector service and Codex config commit.
pub fn commit_settings_migration(root: &Path) -> Result<(), CollectorError> {
    let layout = install(root).map_err(runtime_error)?;
    let Some(mut migration) = load_settings_migration(&layout)? else {
        return Ok(());
    };
    let current = read_private_snapshot(&settings_path(&layout), MAX_SETTINGS_BYTES)?;
    let settings = parse_owned_settings(&current.bytes)?;
    if settings.generation != migration.replacement_generation {
        return Err(CollectorError::Runtime(
            "collector settings changed before migration commit".into(),
        ));
    }
    if migration.phase == SettingsMigrationPhase::Pending {
        let path = settings_migration_path(&layout);
        let snapshot = read_private_snapshot(&path, MAX_SETTINGS_MIGRATION_BYTES)?;
        migration.phase = SettingsMigrationPhase::IntegrationCommitted;
        write_private_json_if_unchanged(
            &path,
            &migration,
            &snapshot,
            MAX_SETTINGS_MIGRATION_BYTES,
        )?;
    }
    finalize_committed_settings_migration(&layout, &migration)
}

/// Restores exact legacy settings when a collector/config integration transaction fails.
pub fn rollback_settings_migration(root: &Path) -> Result<(), CollectorError> {
    let layout = install(root).map_err(runtime_error)?;
    let Some(migration) = load_settings_migration(&layout)? else {
        return Ok(());
    };
    if migration.phase == SettingsMigrationPhase::IntegrationCommitted {
        return finalize_committed_settings_migration(&layout, &migration);
    }
    let path = settings_path(&layout);
    let current = read_private_snapshot(&path, MAX_SETTINGS_BYTES)?;
    let previous = PrivateFileSnapshot {
        bytes: migration.previous_settings.clone(),
        mode: migration.previous_mode,
    };
    if current != previous {
        let settings = parse_owned_settings(&current.bytes)?;
        if settings.generation != migration.replacement_generation {
            return Err(CollectorError::Runtime(
                "collector settings changed before migration rollback".into(),
            ));
        }
        let validated_generation =
            validate_legacy_settings_for_migration(&layout, &previous.bytes)?;
        if validated_generation != migration.previous_generation {
            return Err(CollectorError::Runtime(
                "collector migration rollback generation mismatch".into(),
            ));
        }
        write_private_bytes_if_unchanged(&path, &previous, &current)?;
    }
    cleanup_credential_generation(&layout, &migration.replacement_generation)?;
    remove_private_file(&settings_migration_path(&layout))
}

/// Reports whether an exact settings migration journal still requires settlement.
pub fn settings_migration_pending(root: &Path) -> Result<bool, CollectorError> {
    let layout = install(root).map_err(runtime_error)?;
    load_settings_migration(&layout).map(|migration| migration.is_some())
}

fn load_settings_migration(
    layout: &InstalledLayout,
) -> Result<Option<SettingsMigrationV1>, CollectorError> {
    let path = settings_migration_path(layout);
    let snapshot = match fs::symlink_metadata(&path) {
        Ok(_) => read_private_snapshot(&path, MAX_SETTINGS_MIGRATION_BYTES)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let migration: SettingsMigrationV1 = serde_json::from_slice(&snapshot.bytes)
        .map_err(|_| CollectorError::Runtime("invalid collector settings migration".into()))?;
    if migration.schema_version != SETTINGS_MIGRATION_VERSION
        || migration.previous_settings.len()
            > usize::try_from(MAX_SETTINGS_BYTES).expect("settings bound fits usize")
        || !valid_generation(&migration.replacement_generation)
        || migration
            .previous_generation
            .as_deref()
            .is_some_and(|generation| !valid_generation(generation))
        || migration.previous_mode & 0o077 != 0
    {
        return Err(CollectorError::Runtime(
            "invalid collector settings migration".into(),
        ));
    }
    Ok(Some(migration))
}

fn recover_settings_migration_before_install(
    layout: &InstalledLayout,
) -> Result<(), CollectorError> {
    let Some(migration) = load_settings_migration(layout)? else {
        return Ok(());
    };
    if migration.phase == SettingsMigrationPhase::IntegrationCommitted {
        return finalize_committed_settings_migration(layout, &migration);
    }
    let current = read_private_snapshot(&settings_path(layout), MAX_SETTINGS_BYTES)?;
    if current.bytes == migration.previous_settings && current.mode == migration.previous_mode {
        cleanup_credential_generation(layout, &migration.replacement_generation)?;
        return remove_private_file(&settings_migration_path(layout));
    }
    let settings = parse_owned_settings(&current.bytes)?;
    if settings.generation != migration.replacement_generation {
        return Err(CollectorError::Runtime(
            "collector settings migration cannot be resumed".into(),
        ));
    }
    Ok(())
}

fn finalize_committed_settings_migration(
    layout: &InstalledLayout,
    migration: &SettingsMigrationV1,
) -> Result<(), CollectorError> {
    let current = read_private_snapshot(&settings_path(layout), MAX_SETTINGS_BYTES)?;
    let settings = parse_owned_settings(&current.bytes)?;
    if settings.generation != migration.replacement_generation {
        return Err(CollectorError::Runtime(
            "committed collector settings migration cannot be finalized".into(),
        ));
    }
    if let Some(previous_generation) = &migration.previous_generation {
        cleanup_credential_generation(layout, previous_generation)?;
    }
    remove_private_file(&settings_migration_path(layout))
}

fn valid_generation(generation: &str) -> bool {
    generation.len() == 64 && generation.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn parse_current_settings(bytes: &[u8]) -> Result<CollectorSettings, CollectorError> {
    let settings = parse_owned_settings(bytes)?;
    if settings.credentials.expires_at_unix_ms <= current_unix_ms()? {
        return Err(CollectorError::Runtime(
            "local collector credentials expired; reconnect to renew".into(),
        ));
    }
    Ok(settings)
}

fn parse_owned_settings(bytes: &[u8]) -> Result<CollectorSettings, CollectorError> {
    let settings: CollectorSettings = serde_json::from_slice(bytes)
        .map_err(|_| CollectorError::Runtime("invalid collector settings".into()))?;
    validate_settings_shape(&settings)?;
    Ok(settings)
}

/// Replaces an occupied persisted loopback port while preserving all other settings.
///
/// The caller must establish that the configured collector is unavailable before invoking this
/// explicit recovery operation. A concurrently changed settings file or a port that has become
/// free is left unchanged.
pub fn recover_occupied_persisted_port(
    root: &Path,
    expected: &CollectorSettings,
) -> Result<CollectorSettings, CollectorError> {
    let layout = install(root).map_err(runtime_error)?;
    let current = load_settings(root)?;
    if current != *expected {
        return Err(CollectorError::Runtime(
            "collector settings changed during port recovery".into(),
        ));
    }

    match StdTcpListener::bind((Ipv4Addr::LOCALHOST, current.port)) {
        Ok(listener) => {
            drop(listener);
            return Ok(current);
        }
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {}
        Err(error) => return Err(error.into()),
    }

    let reservation = StdTcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let mut recovered = current;
    recovered.port = reservation.local_addr()?.port();
    write_private_json(&settings_path(&layout), &recovered)?;
    drop(reservation);
    Ok(recovered)
}

fn settings_path(layout: &InstalledLayout) -> PathBuf {
    layout.runtime.join("collector.json")
}

fn settings_migration_path(layout: &InstalledLayout) -> PathBuf {
    layout.runtime.join("collector-settings-migration.json")
}

fn validate_settings_shape(settings: &CollectorSettings) -> Result<(), CollectorError> {
    if settings.schema_version != COLLECTOR_SETTINGS_VERSION
        || settings.port == 0
        || settings.transport != COLLECTOR_TRANSPORT
        || settings.generation.len() != 64
        || !settings
            .generation
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || settings.auth_token.len() != 64
        || !settings
            .auth_token
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || settings.credentials.expires_at_unix_ms == 0
    {
        return Err(CollectorError::Runtime(
            "invalid local collector settings".into(),
        ));
    }
    let expected_prefix = format!("{TLS_DIRECTORY}/{}/", settings.generation);
    for (path, name) in [
        (&settings.credentials.ca_certificate, CA_CERTIFICATE_NAME),
        (
            &settings.credentials.server_certificate,
            SERVER_CERTIFICATE_NAME,
        ),
        (
            &settings.credentials.server_private_key,
            SERVER_PRIVATE_KEY_NAME,
        ),
    ] {
        if path.len() > MAX_CREDENTIAL_PATH_BYTES || path != &format!("{expected_prefix}{name}") {
            return Err(CollectorError::Runtime(
                "invalid local collector credential path".into(),
            ));
        }
    }
    Ok(())
}

fn validate_owned_credentials(
    layout: &InstalledLayout,
    settings: &CollectorSettings,
) -> Result<(), CollectorError> {
    let generation_dir = layout
        .runtime
        .join(TLS_DIRECTORY)
        .join(&settings.generation);
    validate_private_directory_tree(&layout.runtime, &generation_dir)?;
    for relative in [
        &settings.credentials.ca_certificate,
        &settings.credentials.server_certificate,
        &settings.credentials.server_private_key,
    ] {
        read_private_bounded(&credential_path(layout, relative)?, MAX_CREDENTIAL_BYTES)?;
    }
    build_server_config(layout, &settings.credentials)?;
    build_client_config(layout, &settings.credentials)?;
    Ok(())
}

fn generate_credentials(
    layout: &InstalledLayout,
    generation: &str,
) -> Result<CredentialMetadata, CollectorError> {
    let tls_root = layout.runtime.join(TLS_DIRECTORY);
    ensure_private_directory_tree(&layout.runtime, &tls_root)?;
    let generation_dir = tls_root.join(generation);
    create_private_directory(&generation_dir)?;

    let now = OffsetDateTime::now_utc();
    let not_after = now
        + time::Duration::try_from(CREDENTIAL_LIFETIME)
            .map_err(|_| CollectorError::Runtime("credential lifetime is invalid".into()))?;
    let mut ca_params = CertificateParams::default();
    ca_params.not_before = now - time::Duration::minutes(1);
    ca_params.not_after = not_after;
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![KeyUsagePurpose::KeyCertSign];
    ca_params.distinguished_name = distinguished_name("agent-observability local CA");
    let ca_key = KeyPair::generate().map_err(crypto_error)?;
    let ca_certificate = ca_params.self_signed(&ca_key).map_err(crypto_error)?;
    let issuer = Issuer::from_params(&ca_params, &ca_key);

    let server_key = KeyPair::generate().map_err(crypto_error)?;
    let mut server_params = CertificateParams::default();
    server_params.not_before = ca_params.not_before;
    server_params.not_after = ca_params.not_after;
    server_params.subject_alt_names = vec![SanType::IpAddress(IpAddr::V4(Ipv4Addr::LOCALHOST))];
    server_params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    server_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    server_params.distinguished_name = distinguished_name("agent-observability local server");
    let server_certificate = server_params
        .signed_by(&server_key, &issuer)
        .map_err(crypto_error)?;

    for (name, bytes) in [
        (CA_CERTIFICATE_NAME, ca_certificate.pem().into_bytes()),
        (
            SERVER_CERTIFICATE_NAME,
            server_certificate.pem().into_bytes(),
        ),
        (
            SERVER_PRIVATE_KEY_NAME,
            server_key.serialize_pem().into_bytes(),
        ),
    ] {
        write_private_file(&generation_dir.join(name), &bytes)?;
    }
    File::open(&generation_dir)?.sync_all()?;
    File::open(&tls_root)?.sync_all()?;

    let prefix = format!("{TLS_DIRECTORY}/{generation}");
    Ok(CredentialMetadata {
        ca_certificate: format!("{prefix}/{CA_CERTIFICATE_NAME}"),
        server_certificate: format!("{prefix}/{SERVER_CERTIFICATE_NAME}"),
        server_private_key: format!("{prefix}/{SERVER_PRIVATE_KEY_NAME}"),
        expires_at_unix_ms: u64::try_from(not_after.unix_timestamp())
            .map_err(|_| CollectorError::Runtime("credential expiry is invalid".into()))?
            .saturating_mul(1_000),
    })
}

fn distinguished_name(common_name: &str) -> DistinguishedName {
    let mut name = DistinguishedName::new();
    name.push(DnType::CommonName, common_name);
    name
}

fn crypto_error(error: impl std::fmt::Display) -> CollectorError {
    CollectorError::Runtime(format!("collector credential generation failed: {error}"))
}

fn ensure_private_directory_tree(base: &Path, target: &Path) -> Result<(), CollectorError> {
    let relative = target
        .strip_prefix(base)
        .map_err(|_| CollectorError::Runtime("credential directory escaped runtime".into()))?;
    let mut current = base.to_path_buf();
    for component in relative.components() {
        current.push(component);
        if current.exists() {
            validate_private_directory(&current)?;
        } else {
            create_private_directory(&current)?;
        }
    }
    Ok(())
}

fn create_private_directory(path: &Path) -> Result<(), CollectorError> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)?;
    validate_private_directory(path)
}

fn validate_private_directory(path: &Path) -> Result<(), CollectorError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(CollectorError::Runtime(
            "collector credential directory must be private and regular".into(),
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(CollectorError::Runtime(
                "collector credential directory permissions are too broad".into(),
            ));
        }
    }
    Ok(())
}

fn validate_private_directory_tree(base: &Path, target: &Path) -> Result<(), CollectorError> {
    let relative = target
        .strip_prefix(base)
        .map_err(|_| CollectorError::Runtime("credential directory escaped runtime".into()))?;
    validate_private_directory(base)?;
    let mut current = base.to_path_buf();
    for component in relative.components() {
        let std::path::Component::Normal(component) = component else {
            return Err(CollectorError::Runtime(
                "collector credential directory path is invalid".into(),
            ));
        };
        current.push(component);
        validate_private_directory(&current)?;
    }
    Ok(())
}

fn settings_temporary_path(parent: &Path) -> PathBuf {
    let sequence = SETTINGS_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    parent.join(format!(
        ".collector.json.tmp.{}.{}",
        std::process::id(),
        sequence
    ))
}

fn write_private_json_temporary<T: Serialize>(
    path: &Path,
    value: &T,
) -> Result<File, CollectorError> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options.open(path)?;
    serde_json::to_writer_pretty(&mut file, value)
        .map_err(|_| CollectorError::Runtime("collector settings serialization failed".into()))?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(file)
}

fn write_private_json<T: Serialize>(path: &Path, value: &T) -> Result<(), CollectorError> {
    let parent = path
        .parent()
        .ok_or_else(|| CollectorError::Runtime("collector settings have no parent".into()))?;
    validate_private_directory(parent)?;
    let temporary = settings_temporary_path(parent);
    let result = (|| {
        let file = write_private_json_temporary(&temporary, value)?;
        drop(file);
        fs::rename(&temporary, path)?;
        File::open(parent)?.sync_all()?;
        Ok(())
    })();
    let _ = fs::remove_file(&temporary);
    result
}

fn write_private_json_if_unchanged<T: Serialize>(
    path: &Path,
    value: &T,
    expected: &PrivateFileSnapshot,
    max_bytes: u64,
) -> Result<(), CollectorError> {
    let parent = path
        .parent()
        .ok_or_else(|| CollectorError::Runtime("collector settings have no parent".into()))?;
    validate_private_directory(parent)?;
    let temporary = settings_temporary_path(parent);
    let result = (|| {
        let file = write_private_json_temporary(&temporary, value)?;
        drop(file);
        let current = read_private_snapshot(path, max_bytes)?;
        if current != *expected {
            return Err(CollectorError::Runtime(
                "collector settings changed during credential replacement".into(),
            ));
        }
        fs::rename(&temporary, path)?;
        File::open(parent)?.sync_all()?;
        Ok(())
    })();
    let _ = fs::remove_file(&temporary);
    result
}

fn write_private_bytes_if_unchanged(
    path: &Path,
    replacement: &PrivateFileSnapshot,
    expected: &PrivateFileSnapshot,
) -> Result<(), CollectorError> {
    let parent = path
        .parent()
        .ok_or_else(|| CollectorError::Runtime("collector settings have no parent".into()))?;
    validate_private_directory(parent)?;
    let temporary = settings_temporary_path(parent);
    let result = (|| {
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options
                .mode(replacement.mode & 0o777)
                .custom_flags(libc::O_NOFOLLOW);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(&replacement.bytes)?;
        file.sync_all()?;
        drop(file);
        let current = read_private_snapshot(path, MAX_SETTINGS_BYTES)?;
        if current != *expected {
            return Err(CollectorError::Runtime(
                "collector settings changed during migration rollback".into(),
            ));
        }
        fs::rename(&temporary, path)?;
        File::open(parent)?.sync_all()?;
        Ok(())
    })();
    let _ = fs::remove_file(&temporary);
    result
}

fn remove_private_file(path: &Path) -> Result<(), CollectorError> {
    let parent = path
        .parent()
        .ok_or_else(|| CollectorError::Runtime("collector file has no parent".into()))?;
    match fs::remove_file(path) {
        Ok(()) => File::open(parent)?.sync_all().map_err(Into::into),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), CollectorError> {
    if bytes.len() > usize::try_from(MAX_CREDENTIAL_BYTES).expect("credential bound fits usize") {
        return Err(CollectorError::Runtime(
            "collector credential exceeds size bound".into(),
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| CollectorError::Runtime("collector credential has no parent".into()))?;
    validate_private_directory(parent)?;
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn open_private_read(path: &Path) -> Result<File, CollectorError> {
    if fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(CollectorError::Runtime(
            "collector file must be a private regular file".into(),
        ));
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(CollectorError::Runtime(
            "collector file must be a private regular file".into(),
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(CollectorError::Runtime(
                "collector file permissions are too broad".into(),
            ));
        }
    }
    Ok(file)
}

fn read_private_bounded(path: &Path, maximum: u64) -> Result<Vec<u8>, CollectorError> {
    Ok(read_private_snapshot(path, maximum)?.bytes)
}

fn read_private_snapshot(path: &Path, maximum: u64) -> Result<PrivateFileSnapshot, CollectorError> {
    let file = open_private_read(path)?;
    let metadata = file.metadata()?;
    if metadata.len() > maximum {
        return Err(CollectorError::Runtime(
            "collector file exceeds size bound".into(),
        ));
    }
    #[cfg(unix)]
    let mode = {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode()
    };
    #[cfg(not(unix))]
    let mode = u32::from(metadata.permissions().readonly());
    let mut bytes = Vec::new();
    file.take(maximum.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > maximum {
        return Err(CollectorError::Runtime(
            "collector file exceeds size bound".into(),
        ));
    }
    Ok(PrivateFileSnapshot { bytes, mode })
}

fn credential_path(layout: &InstalledLayout, relative: &str) -> Result<PathBuf, CollectorError> {
    if relative.len() > MAX_CREDENTIAL_PATH_BYTES {
        return Err(CollectorError::Runtime(
            "collector credential path exceeds size bound".into(),
        ));
    }
    let relative = Path::new(relative);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(CollectorError::Runtime(
            "collector credential path is invalid".into(),
        ));
    }
    Ok(layout.runtime.join(relative))
}

impl std::fmt::Display for CollectorError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "local collector I/O error: {error}"),
            Self::RequestIo { stage, source } => {
                write!(formatter, "local collector {stage} I/O error: {source}")
            }
            Self::Runtime(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for CollectorError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) | Self::RequestIo { source: error, .. } => Some(error),
            Self::Runtime(_) => None,
        }
    }
}

impl From<std::io::Error> for CollectorError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

fn request_io(stage: &'static str, error: std::io::Error) -> CollectorError {
    let source = if error.kind() == std::io::ErrorKind::WouldBlock {
        std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "collector request deadline expired",
        )
    } else {
        error
    };
    CollectorError::RequestIo { stage, source }
}

fn load_certificates(path: &Path) -> Result<Vec<CertificateDer<'static>>, CollectorError> {
    let bytes = read_private_bounded(path, MAX_CREDENTIAL_BYTES)?;
    let certificates = CertificateDer::pem_slice_iter(&bytes)
        .collect::<Result<Vec<_>, _>>()
        .map_err(crypto_error)?;
    if certificates.is_empty() {
        return Err(CollectorError::Runtime(
            "collector certificate file is empty".into(),
        ));
    }
    Ok(certificates)
}

fn load_private_key(path: &Path) -> Result<PrivateKeyDer<'static>, CollectorError> {
    let bytes = read_private_bounded(path, MAX_CREDENTIAL_BYTES)?;
    PrivateKeyDer::from_pem_slice(&bytes).map_err(crypto_error)
}

fn root_store(certificate: CertificateDer<'static>) -> Result<RootCertStore, CollectorError> {
    let mut roots = RootCertStore::empty();
    roots.add(certificate).map_err(crypto_error)?;
    Ok(roots)
}

fn build_server_config(
    layout: &InstalledLayout,
    credentials: &CredentialMetadata,
) -> Result<Arc<ServerConfig>, CollectorError> {
    let provider = Arc::new(aws_lc_rs::default_provider());
    let certificates =
        load_certificates(&credential_path(layout, &credentials.server_certificate)?)?;
    let key = load_private_key(&credential_path(layout, &credentials.server_private_key)?)?;
    let config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(crypto_error)?
        .with_no_client_auth()
        .with_single_cert(certificates, key)
        .map_err(crypto_error)?;
    Ok(Arc::new(config))
}

fn build_client_config(
    layout: &InstalledLayout,
    credentials: &CredentialMetadata,
) -> Result<Arc<ClientConfig>, CollectorError> {
    let ca = load_certificates(&credential_path(layout, &credentials.ca_certificate)?)?
        .into_iter()
        .next()
        .ok_or_else(|| CollectorError::Runtime("collector CA certificate is empty".into()))?;
    let config = ClientConfig::builder_with_provider(Arc::new(aws_lc_rs::default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(crypto_error)?
        .with_root_certificates(root_store(ca)?)
        .with_no_client_auth();
    Ok(Arc::new(config))
}

fn build_legacy_client_config(
    layout: &InstalledLayout,
    credentials: &LegacyCredentialMetadataV2Mtls,
) -> Result<Arc<ClientConfig>, CollectorError> {
    let ca = load_certificates(&credential_path(layout, &credentials.ca_certificate)?)?
        .into_iter()
        .next()
        .ok_or_else(|| {
            CollectorError::Runtime("legacy collector CA certificate is empty".into())
        })?;
    let certificates =
        load_certificates(&credential_path(layout, &credentials.client_certificate)?)?;
    let key = load_private_key(&credential_path(layout, &credentials.client_private_key)?)?;
    let config = ClientConfig::builder_with_provider(Arc::new(aws_lc_rs::default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(crypto_error)?
        .with_root_certificates(root_store(ca)?)
        .with_client_auth_cert(certificates, key)
        .map_err(crypto_error)?;
    Ok(Arc::new(config))
}

#[derive(Debug)]
struct CollectorState {
    layout: InstalledLayout,
    store: LocalStore,
    source_generation: String,
    last_cursor: Option<String>,
    request_correlation: OtlpRequestCorrelationState,
    accepted_requests: u64,
    rejected_requests: u64,
    suppressed_requests: u64,
    last_ingest_unix_ms: Option<u64>,
    report_dirty: bool,
    report_degraded: bool,
    report_refresh_failures: u32,
    report_failure: Option<ReportFailure>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ReportFailure {
    Task,
    Install,
    OpenStore,
    Clock,
    RenderGuard,
    Snapshot,
    Projection,
    Publish,
    Acknowledge,
    Status,
}

#[derive(Clone, Debug)]
struct AppState {
    collector: Arc<Mutex<CollectorState>>,
    auth_token: Arc<str>,
    report_refresh_scheduled: Arc<AtomicBool>,
    report_refresh_requested: Arc<AtomicU64>,
    #[cfg(test)]
    report_refresh_attempts: Arc<AtomicU64>,
}

#[derive(Debug, Serialize)]
struct Health {
    status: &'static str,
    accepted_requests: u64,
    rejected_requests: u64,
    suppressed_requests: u64,
    last_ingest_unix_ms: Option<u64>,
    report_dirty: bool,
    report_refresh_failures: u32,
    report_failure: Option<ReportFailure>,
}

#[derive(Debug, Deserialize)]
struct HealthProbe {
    status: String,
    report_dirty: bool,
}

/// Runs the authenticated OTLP/HTTP receiver until the process is terminated.
pub async fn serve(options: CollectorOptions) -> Result<(), CollectorError> {
    validate_options(&options)?;
    let layout = install(&options.root).map_err(runtime_error)?;
    validate_owned_credentials(
        &layout,
        &CollectorSettings {
            schema_version: COLLECTOR_SETTINGS_VERSION.into(),
            generation: options.generation.clone(),
            port: options.port,
            transport: COLLECTOR_TRANSPORT.into(),
            auth_token: options.auth_token.clone(),
            credentials: options.credentials.clone(),
        },
    )?;
    let tls_config = build_server_config(&layout, &options.credentials)?;
    let singleton = Singleton::acquire(&layout.runtime.join("collector")).map_err(runtime_error)?;
    let mutation = try_collector_mutation(&layout.runtime)?;
    let config = load(&layout.config).map_err(runtime_error)?;
    let store = open_store(&mutation, &layout, &config)?;
    let report_status = store.report_status().map_err(runtime_error)?;
    let report_missing = !layout.logs.join(REPORT_FILE_NAME).is_file();
    let report_wakeup = reconcile_report_state(&layout, report_status.pending() || report_missing);
    let report_dirty = report_status.pending() || report_missing;
    let source_generation = SOURCE_GENERATION.to_owned();
    let last_cursor = store
        .cursor("codex", &source_generation)
        .map_err(runtime_error)?;
    let now = current_unix_ms()?;
    let request_correlation = store
        .codex_request_correlation_state(&source_generation)
        .map_err(runtime_error)?
        .map(|snapshot| OtlpRequestCorrelationState::from_persisted_json(&snapshot, now))
        .transpose()
        .map_err(runtime_error)?
        .unwrap_or_default();
    drop(mutation);
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), options.port);
    let initial_bind = TcpListener::bind(address).await;
    let listener = bind_persisted_port(initial_bind)?;
    let collector = Arc::new(Mutex::new(CollectorState {
        layout,
        store,
        source_generation,
        last_cursor,
        request_correlation,
        accepted_requests: 0,
        rejected_requests: 0,
        suppressed_requests: 0,
        last_ingest_unix_ms: None,
        report_dirty,
        report_degraded: report_dirty,
        report_refresh_failures: 0,
        report_failure: None,
    }));
    let state = AppState {
        collector,
        auth_token: Arc::from(options.auth_token),
        report_refresh_scheduled: Arc::new(AtomicBool::new(false)),
        report_refresh_requested: Arc::new(AtomicU64::new(0)),
        #[cfg(test)]
        report_refresh_attempts: Arc::new(AtomicU64::new(0)),
    };
    let app = router(state.clone());
    if report_wakeup {
        schedule_report_refresh(&state);
    }
    let report_watcher = tokio::spawn(watch_report_authority(
        state.clone(),
        report_status.generation,
        REPORT_AUTHORITY_POLL_INTERVAL,
    ));
    let result = serve_transport(
        listener,
        tls_config,
        app,
        HEADER_READ_TIMEOUT,
        REQUEST_LIFETIME,
        MAX_CONNECTIONS,
    )
    .await;
    report_watcher.abort();
    let _ = report_watcher.await;
    drop(singleton);
    result
}

fn bind_persisted_port(
    bind_result: std::io::Result<TcpListener>,
) -> Result<TcpListener, CollectorError> {
    bind_result.map_err(Into::into)
}

fn router(state: AppState) -> Router {
    let ingest = Router::new()
        .route("/v1/logs", post(ingest_logs))
        .route("/v1/notify", post(ingest_notify))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            ingest_preflight,
        ));
    Router::new()
        .route("/health", get(health))
        .merge(ingest)
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            authenticate_request,
        ))
        .layer(DefaultBodyLimit::max(
            usize::try_from(MAX_HANDOFF_BYTES).expect("handoff bound fits usize"),
        ))
        .with_state(state)
}

async fn authenticate_request(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if !token_matches(request.headers(), &state.auth_token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    next.run(request).await
}

fn token_matches(headers: &HeaderMap, expected: &str) -> bool {
    let mut values = headers.get_all(AUTH_HEADER_NAME).iter();
    let Some(actual) = values.next().map(axum::http::HeaderValue::as_bytes) else {
        return false;
    };
    if values.next().is_some() {
        return false;
    }
    let expected = expected.as_bytes();
    if actual.len() != expected.len() {
        return false;
    }
    actual
        .iter()
        .zip(expected)
        .fold(0_u8, |difference, (actual, expected)| {
            difference | (actual ^ expected)
        })
        == 0
}

async fn serve_transport(
    listener: TcpListener,
    tls_config: Arc<ServerConfig>,
    app: Router,
    header_read_timeout: Duration,
    request_lifetime: Duration,
    max_connections: usize,
) -> Result<(), CollectorError> {
    let app = protect_request_lifetime(app, request_lifetime);
    let listener =
        TransportListener::new(listener, tls_config, header_read_timeout, max_connections);
    axum::serve(listener, app).await.map_err(CollectorError::Io)
}

fn protect_request_lifetime(app: Router, request_lifetime: Duration) -> Router {
    app.layer(middleware::from_fn(
        move |request: Request<Body>, next: Next| async move {
            match tokio::time::timeout(request_lifetime, next.run(request)).await {
                Ok(response) => response,
                Err(_) => StatusCode::REQUEST_TIMEOUT.into_response(),
            }
        },
    ))
}

struct TransportListener {
    listener: TcpListener,
    acceptor: TlsAcceptor,
    handshake_timeout: Duration,
    header_read_timeout: Duration,
    connection_slots: Arc<Semaphore>,
    completed_handshakes:
        mpsc::Receiver<(TlsStream<TokioTcpStream>, OwnedSemaphorePermit, SocketAddr)>,
    completed_handshake_sender:
        mpsc::Sender<(TlsStream<TokioTcpStream>, OwnedSemaphorePermit, SocketAddr)>,
}

impl TransportListener {
    fn new(
        listener: TcpListener,
        tls_config: Arc<ServerConfig>,
        header_read_timeout: Duration,
        max_connections: usize,
    ) -> Self {
        assert!(max_connections > 0, "collector must admit a connection");
        let (completed_handshake_sender, completed_handshakes) = mpsc::channel(max_connections);
        Self {
            listener,
            acceptor: TlsAcceptor::from(tls_config),
            handshake_timeout: header_read_timeout,
            header_read_timeout,
            connection_slots: Arc::new(Semaphore::new(max_connections)),
            completed_handshakes,
            completed_handshake_sender,
        }
    }
}

impl Listener for TransportListener {
    type Io = ProtectedIo;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            tokio::select! {
                biased;
                Some((stream, permit, address)) = self.completed_handshakes.recv() => {
                    return (
                        ProtectedIo::new(stream, permit, self.header_read_timeout),
                        address,
                    );
                }
                (stream, address) = Listener::accept(&mut self.listener) => {
                    let Ok(permit) = Arc::clone(&self.connection_slots).try_acquire_owned() else {
                        drop(stream);
                        continue;
                    };
                    let acceptor = self.acceptor.clone();
                    let handshake_timeout = self.handshake_timeout;
                    let completed = self.completed_handshake_sender.clone();
                    tokio::spawn(async move {
                        let handshake = tokio::time::timeout(
                            handshake_timeout,
                            acceptor.accept(stream),
                        )
                        .await;
                        if let Ok(Ok(stream)) = handshake {
                            let _ = completed.send((stream, permit, address)).await;
                        }
                    });
                }
            }
        }
    }

    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        self.listener.local_addr()
    }
}

#[derive(Debug)]
struct ProtectedIo {
    stream: TlsStream<TokioTcpStream>,
    _permit: OwnedSemaphorePermit,
    header_read_timeout: Duration,
    header_deadline: Pin<Box<Sleep>>,
    header_match: usize,
    reading_headers: bool,
}

impl ProtectedIo {
    fn new(
        stream: TlsStream<TokioTcpStream>,
        permit: OwnedSemaphorePermit,
        header_read_timeout: Duration,
    ) -> Self {
        Self {
            stream,
            _permit: permit,
            header_read_timeout,
            header_deadline: Box::pin(tokio::time::sleep(header_read_timeout)),
            header_match: 0,
            reading_headers: true,
        }
    }

    fn observe_read(&mut self, bytes: &[u8]) {
        if !self.reading_headers {
            return;
        }
        for &byte in bytes {
            if byte == HEADER_TERMINATOR[self.header_match] {
                self.header_match += 1;
                if self.header_match == HEADER_TERMINATOR.len() {
                    self.reading_headers = false;
                    self.header_match = 0;
                    break;
                }
            } else {
                self.header_match = usize::from(byte == HEADER_TERMINATOR[0]);
            }
        }
    }

    fn rearm_header_deadline(&mut self) {
        if !self.reading_headers {
            self.reading_headers = true;
            self.header_match = 0;
            self.header_deadline
                .as_mut()
                .reset(tokio::time::Instant::now() + self.header_read_timeout);
        }
    }
}

impl AsyncRead for ProtectedIo {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if self.reading_headers && self.header_deadline.as_mut().poll(context).is_ready() {
            return Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "collector request headers timed out",
            )));
        }
        let filled_before = buffer.filled().len();
        match Pin::new(&mut self.stream).poll_read(context, buffer) {
            Poll::Ready(Ok(())) => {
                self.observe_read(&buffer.filled()[filled_before..]);
                Poll::Ready(Ok(()))
            }
            result => result,
        }
    }
}

impl AsyncWrite for ProtectedIo {
    fn poll_write(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let this = self.get_mut();
        this.rearm_header_deadline();
        Pin::new(&mut this.stream).poll_write(context, buffer)
    }

    fn poll_flush(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().stream).poll_flush(context)
    }

    fn poll_shutdown(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().stream).poll_shutdown(context)
    }
}

async fn ingest_preflight(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let mut collector = state.collector.lock().await;
    if !is_json(request.headers()) {
        collector.rejected_requests = collector.rejected_requests.saturating_add(1);
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    }
    drop(collector);
    next.run(request).await
}

async fn ingest_notify(State(state): State<AppState>, body: Bytes) -> impl IntoResponse {
    let mut collector = state.collector.lock().await;
    let (outcome, committed) = match ingest_notify_locked(&mut collector, &body) {
        Ok(IngestOutcome::Committed) => {
            collector.accepted_requests = collector.accepted_requests.saturating_add(1);
            (StatusCode::OK.into_response(), true)
        }
        Ok(IngestOutcome::Disabled) => {
            collector.suppressed_requests = collector.suppressed_requests.saturating_add(1);
            (StatusCode::OK.into_response(), false)
        }
        Err(error) => {
            collector.rejected_requests = collector.rejected_requests.saturating_add(1);
            (error.into_response(), false)
        }
    };
    drop(collector);
    if committed {
        schedule_report_refresh(&state);
    }
    outcome
}

async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let mut collector = state.collector.lock().await;
    let report_pending = if let Ok(status) = collector.store.report_status() {
        status.pending()
    } else {
        collector.report_failure = Some(ReportFailure::Status);
        true
    };
    collector.report_dirty = report_pending;
    axum::Json(Health {
        status: if collector.report_degraded || report_pending {
            "degraded"
        } else {
            "ready"
        },
        accepted_requests: collector.accepted_requests,
        rejected_requests: collector.rejected_requests,
        suppressed_requests: collector.suppressed_requests,
        last_ingest_unix_ms: collector.last_ingest_unix_ms,
        report_dirty: collector.report_dirty,
        report_refresh_failures: collector.report_refresh_failures,
        report_failure: collector.report_failure,
    })
    .into_response()
}

async fn ingest_logs(State(state): State<AppState>, body: Bytes) -> impl IntoResponse {
    let mut collector = state.collector.lock().await;
    let (outcome, committed) = match ingest_locked(&mut collector, &body) {
        Ok(IngestOutcome::Committed) => {
            collector.accepted_requests = collector.accepted_requests.saturating_add(1);
            (StatusCode::OK.into_response(), true)
        }
        Ok(IngestOutcome::Disabled) => {
            collector.suppressed_requests = collector.suppressed_requests.saturating_add(1);
            (StatusCode::OK.into_response(), false)
        }
        Err(error) => {
            collector.rejected_requests = collector.rejected_requests.saturating_add(1);
            (error.into_response(), false)
        }
    };
    drop(collector);
    if committed {
        schedule_report_refresh(&state);
    }
    outcome
}

fn is_json(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum IngestOutcome {
    Committed,
    Disabled,
}

#[derive(Debug)]
enum IngestError {
    Invalid(CollectorError),
    Busy,
    Policy,
    Pressure,
    Storage,
}

impl IngestError {
    const fn status(&self) -> StatusCode {
        match self {
            Self::Invalid(CollectorError::Io(_) | CollectorError::RequestIo { .. }) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
            Self::Invalid(CollectorError::Runtime(_)) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Busy | Self::Pressure => StatusCode::SERVICE_UNAVAILABLE,
            Self::Policy => StatusCode::PAYLOAD_TOO_LARGE,
            Self::Storage => StatusCode::INSUFFICIENT_STORAGE,
        }
    }

    fn into_response(self) -> Response {
        if matches!(self, Self::Busy) {
            return (StatusCode::SERVICE_UNAVAILABLE, "busy").into_response();
        }
        self.status().into_response()
    }
}

impl From<CollectorError> for IngestError {
    fn from(error: CollectorError) -> Self {
        Self::Invalid(error)
    }
}

fn ingest_locked(state: &mut CollectorState, body: &[u8]) -> Result<IngestOutcome, IngestError> {
    let _mutation = try_ingest_mutation(&state.layout.runtime)?;
    let Some(config) = admit_request(state, body.len())? else {
        return Ok(IngestOutcome::Disabled);
    };
    let now = current_unix_ms()?;
    let next_cursor = state
        .last_cursor
        .as_deref()
        .map_or(Ok(1), |cursor| {
            cursor
                .parse::<u64>()
                .map_err(|_| CollectorError::Runtime("invalid durable Codex cursor".into()))
        })?
        .checked_add(u64::from(state.last_cursor.is_some()))
        .ok_or_else(|| CollectorError::Runtime("Codex cursor overflow".into()))?;
    let mut request_correlation = state.request_correlation.clone();
    let (batch, last_cursor) = parse_otlp_http_json_with_state(
        body,
        &state.source_generation,
        state.last_cursor.as_deref(),
        next_cursor,
        now,
        &mut request_correlation,
    )
    .map_err(runtime_error)?;
    enforce_batch_policy(&batch, &config)?;
    let persisted_correlation = request_correlation
        .to_persisted_json()
        .map_err(runtime_error)?;
    commit_batch(
        state,
        &batch,
        last_cursor,
        now,
        Some(&persisted_correlation),
    )?;
    state.request_correlation = request_correlation;
    Ok(IngestOutcome::Committed)
}

fn ingest_notify_locked(
    state: &mut CollectorState,
    body: &[u8],
) -> Result<IngestOutcome, IngestError> {
    let _mutation = try_ingest_mutation(&state.layout.runtime)?;
    let Some(config) = admit_request(state, body.len())? else {
        return Ok(IngestOutcome::Disabled);
    };
    let now = current_unix_ms()?;
    let cursor = next_cursor(state)?;
    let batch = parse_projected_notify_json(
        body,
        &state.source_generation,
        state.last_cursor.as_deref(),
        cursor,
        now,
    )
    .map_err(runtime_error)?;
    enforce_batch_policy(&batch, &config)?;
    commit_batch(state, &batch, Some(cursor.to_string()), now, None)?;
    Ok(IngestOutcome::Committed)
}

fn try_ingest_mutation(runtime: &Path) -> Result<MutationGuard, IngestError> {
    MutationGuard::try_acquire(runtime).map_err(|error| match error {
        SingletonError::AlreadyRunning => IngestError::Busy,
        error => IngestError::Invalid(runtime_error(error)),
    })
}

fn try_collector_mutation(runtime: &Path) -> Result<MutationGuard, CollectorError> {
    MutationGuard::try_acquire(runtime).map_err(|error| match error {
        SingletonError::AlreadyRunning => CollectorError::Runtime("runtime mutation busy".into()),
        error => runtime_error(error),
    })
}

fn admit_request(
    state: &CollectorState,
    body_bytes: usize,
) -> Result<Option<LocalRuntimeConfigV2>, IngestError> {
    let config = load(&state.layout.config).map_err(runtime_error)?;
    if !config.enabled {
        return Ok(None);
    }
    if body_bytes > usize::try_from(config.collection.max_batch_bytes).unwrap_or(usize::MAX) {
        return Err(IngestError::Policy);
    }
    let mut control = RuntimeControl::new(&config).map_err(runtime_error)?;
    let allocated =
        StorageBudget::allocated_tree_bytes(&state.layout.root).map_err(runtime_error)?;
    let schedule = control.evaluate(
        0,
        PressureSample {
            resource_percent: 0,
            disk_percent: control.storage_percent(allocated),
            queue_percent: 0,
        },
    );
    if schedule.flush_paused {
        return Err(IngestError::Pressure);
    }
    let store_directory = state.layout.state.join("store");
    let existing_store = if store_directory.exists() {
        StorageBudget::allocated_tree_bytes(&store_directory).map_err(runtime_error)?
    } else {
        0
    };
    let reservation = existing_store
        .checked_add(u64::from(config.collection.max_batch_bytes))
        .ok_or(IngestError::Storage)?;
    if control
        .admit(&state.layout.root, reservation)
        .map_err(runtime_error)?
        == Admission::Denied
    {
        return Err(IngestError::Storage);
    }
    Ok(Some(config))
}

fn enforce_batch_policy(
    batch: &AdapterBatch,
    config: &LocalRuntimeConfigV2,
) -> Result<(), IngestError> {
    if batch.items.len() > usize::from(config.collection.max_batch_records) {
        return Err(IngestError::Policy);
    }
    Ok(())
}

fn next_cursor(state: &CollectorState) -> Result<u64, CollectorError> {
    state.last_cursor.as_deref().map_or(Ok(1), |cursor| {
        cursor
            .parse::<u64>()
            .map_err(|_| CollectorError::Runtime("invalid durable Codex cursor".into()))?
            .checked_add(1)
            .ok_or_else(|| CollectorError::Runtime("Codex cursor overflow".into()))
    })
}

fn commit_batch(
    state: &mut CollectorState,
    batch: &AdapterBatch,
    last_cursor: Option<String>,
    now: u64,
    persisted_correlation: Option<&str>,
) -> Result<(), CollectorError> {
    let items = batch
        .items
        .iter()
        .map(|item| match item {
            AdapterItem::Observation(observation) => {
                StoreBatchItem::Observation(observation.as_ref())
            }
            AdapterItem::Disposition(diagnostic) => StoreBatchItem::Disposition {
                checkpoint: &diagnostic.checkpoint,
                disposition: diagnostic.disposition,
                code: diagnostic.code,
                canonical_payload_hash: diagnostic.payload_hash.as_deref(),
            },
        })
        .collect::<Vec<_>>();
    let _ = mark_report_dirty(&state.layout);
    let result = match persisted_correlation {
        Some(snapshot) => state
            .store
            .ingest_codex_batch_with_correlation_state_deferred_projection(
                &items,
                &state.source_generation,
                snapshot,
            ),
        None => state.store.ingest_ordered_batch_deferred_projection(&items),
    };
    match result {
        Ok(_) => {}
        Err(error) => {
            state.report_dirty = state
                .store
                .report_status()
                .map_err(runtime_error)?
                .pending();
            if !state.report_dirty {
                let _ = clear_report_dirty(&state.layout);
            }
            return Err(runtime_error(error));
        }
    }
    state.last_cursor = last_cursor;
    state.last_ingest_unix_ms = Some(now);
    state.report_dirty = state
        .store
        .report_status()
        .map_err(runtime_error)?
        .pending();
    if !state.report_dirty {
        let _ = clear_report_dirty(&state.layout);
    }
    Ok(())
}

fn schedule_report_refresh(state: &AppState) {
    schedule_report_refresh_with_timing(
        state,
        ReportRefreshTiming {
            debounce: REPORT_DEBOUNCE_DELAY,
            retry_initial: REPORT_RETRY_INITIAL_DELAY,
        },
    );
}

async fn watch_report_authority(state: AppState, mut observed_generation: u64, interval: Duration) {
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    ticker.tick().await;
    loop {
        ticker.tick().await;
        let mut collector = state.collector.lock().await;
        let Ok(status) = collector.store.report_status() else {
            collector.report_dirty = true;
            collector.report_degraded = true;
            collector.report_failure = Some(ReportFailure::Status);
            continue;
        };
        collector.report_dirty = status.pending();
        let changed = status.generation != observed_generation;
        observed_generation = status.generation;
        if !status.pending() {
            collector.report_degraded = false;
            collector.report_refresh_failures = 0;
            collector.report_failure = None;
            let _ = clear_report_dirty(&collector.layout);
            continue;
        }
        if !changed {
            continue;
        }
        let _ = mark_report_dirty(&collector.layout);
        drop(collector);
        schedule_report_refresh(&state);
    }
}

#[derive(Clone, Copy, Debug)]
struct ReportRefreshTiming {
    debounce: Duration,
    retry_initial: Duration,
}

fn schedule_report_refresh_with_timing(state: &AppState, timing: ReportRefreshTiming) {
    state
        .report_refresh_requested
        .fetch_add(1, Ordering::Release);
    if state
        .report_refresh_scheduled
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    let state = state.clone();
    tokio::spawn(async move {
        let mut failure_attempts = 0;
        let mut retry_delay = timing.retry_initial;
        loop {
            if failure_attempts == 0 {
                await_report_debounce(&state, timing).await;
            } else {
                tokio::time::sleep(retry_delay).await;
            }
            let attempt_epoch = state.report_refresh_requested.load(Ordering::Acquire);
            let root = {
                let collector = state.collector.lock().await;
                collector.layout.root.clone()
            };
            #[cfg(test)]
            state
                .report_refresh_attempts
                .fetch_add(1, Ordering::Release);
            let (refresh, mut failure) =
                match tokio::task::spawn_blocking(move || refresh_report_from_root(&root)).await {
                    Ok(Ok(refreshed)) => (Some(refreshed), None),
                    Ok(Err(report_failure)) => (None, Some(report_failure)),
                    Err(_) => (None, Some(ReportFailure::Task)),
                };
            let mut collector = state.collector.lock().await;
            let pending = if let Ok(status) = collector.store.report_status() {
                status.pending()
            } else {
                failure = Some(ReportFailure::Status);
                true
            };
            collector.report_dirty = pending;
            let completed = refresh.is_some() && !pending;
            if completed {
                collector.report_degraded = false;
                collector.report_refresh_failures = 0;
                collector.report_failure = None;
                state
                    .report_refresh_scheduled
                    .store(false, Ordering::Release);
                let requested_after_clear = state.report_refresh_requested.load(Ordering::Acquire);
                let pending_after_clear = collector
                    .store
                    .report_status()
                    .map_or(true, agent_observability_local_store::ReportStatus::pending);
                collector.report_dirty = pending_after_clear;
                if !pending_after_clear {
                    let _ = clear_report_dirty(&collector.layout);
                }
                let retry_latest = requested_after_clear != attempt_epoch || pending_after_clear;
                drop(collector);
                if retry_latest {
                    schedule_report_refresh_with_timing(&state, timing);
                }
                return;
            }
            if refresh.is_some() {
                collector.report_refresh_failures = 0;
                collector.report_failure = failure;
                failure_attempts = 0;
                retry_delay = timing.retry_initial;
            } else {
                collector.report_refresh_failures =
                    collector.report_refresh_failures.saturating_add(1);
                collector.report_failure = failure;
                failure_attempts += 1;
            }
            if failure_attempts == REPORT_RETRY_LIMIT {
                collector.report_degraded = true;
                state
                    .report_refresh_scheduled
                    .store(false, Ordering::Release);
                let retry_latest =
                    state.report_refresh_requested.load(Ordering::Acquire) != attempt_epoch;
                drop(collector);
                if retry_latest {
                    schedule_report_refresh_with_timing(&state, timing);
                }
                return;
            }
            drop(collector);
            if refresh.is_none() {
                retry_delay = retry_delay.saturating_mul(2);
            }
        }
    });
}

async fn await_report_debounce(state: &AppState, timing: ReportRefreshTiming) {
    let mut observed = state.report_refresh_requested.load(Ordering::Acquire);
    loop {
        tokio::time::sleep(timing.debounce).await;
        let latest = state.report_refresh_requested.load(Ordering::Acquire);
        if latest == observed {
            return;
        }
        observed = latest;
    }
}

fn report_dirty_path(layout: &InstalledLayout) -> PathBuf {
    layout.runtime.join(REPORT_DIRTY_FILE_NAME)
}

fn reconcile_report_state(layout: &InstalledLayout, durable_pending_or_missing: bool) -> bool {
    let marker_exists = report_dirty_path(layout).exists();
    if durable_pending_or_missing || marker_exists {
        let _ = mark_report_dirty(layout);
        return true;
    }
    false
}

fn mark_report_dirty(layout: &InstalledLayout) -> Result<(), CollectorError> {
    let path = report_dirty_path(layout);
    match fs::symlink_metadata(&path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(CollectorError::Runtime(
                    "report dirty marker must be a private regular file".into(),
                ));
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if metadata.permissions().mode() & 0o077 != 0 {
                    return Err(CollectorError::Runtime(
                        "report dirty marker permissions are too broad".into(),
                    ));
                }
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let mut options = OpenOptions::new();
            options.create_new(true).write(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&path)?;
            file.write_all(b"dirty\n")?;
            file.sync_all()?;
            File::open(&layout.runtime)?.sync_all()?;
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}

fn clear_report_dirty(layout: &InstalledLayout) -> Result<(), CollectorError> {
    match fs::remove_file(report_dirty_path(layout)) {
        Ok(()) => File::open(&layout.runtime)?.sync_all().map_err(Into::into),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn refresh_report_from_root(root: &Path) -> Result<bool, ReportFailure> {
    let layout = install(root).map_err(|_| ReportFailure::Install)?;
    let store = LocalStore::open_current(layout.state.join("store"))
        .map_err(|_| ReportFailure::OpenStore)?;
    let now_unix_ms = current_unix_ms().map_err(|_| ReportFailure::Clock)?;
    refresh_report(&layout, &store, now_unix_ms)
}

/// Projects and sends a raw notify argument with bounded foreground deadlines.
/// Projection happens before any settings or network I/O.
#[must_use]
pub fn submit_notify(root: &Path, payload: &[u8]) -> NotifyOutcome {
    let Ok(projected) = project_notify_json(payload) else {
        return NotifyOutcome::Rejected;
    };
    let Ok(body) = serde_json::to_vec(&projected) else {
        return NotifyOutcome::Rejected;
    };
    if let Ok(config) = load(&root.join("config.json"))
        && config.enabled
        && config.capture_private_codex_turn_details
        && let Ok(layout) = install(root).map_err(runtime_error)
        && let Ok((_, private_detail)) = project_notify_with_private_detail(payload)
    {
        let _ = persist_private_turn_detail(&layout, &private_detail, &config);
    }
    match authenticated_request(
        root,
        "POST",
        "/v1/notify",
        Some(&body),
        Duration::from_millis(50),
        Duration::from_millis(250),
    ) {
        Ok(response) if response.status == 200 => NotifyOutcome::Accepted,
        Ok(_) => NotifyOutcome::Rejected,
        Err(_) => NotifyOutcome::Unavailable,
    }
}

/// Reads one opt-in private Codex turn detail by the same hashed turn ID emitted in reports.
pub fn read_private_turn_detail(root: &Path, turn_id: &str) -> Result<Vec<u8>, CollectorError> {
    let config = load(&root.join("config.json")).map_err(runtime_error)?;
    if !config.enabled || !config.capture_private_codex_turn_details {
        return Err(CollectorError::Runtime(
            "private turn detail is unavailable".into(),
        ));
    }
    let directory = root.join("state").join(PRIVATE_TURN_DETAIL_DIRECTORY);
    validate_private_directory_tree(&root.join("state"), &directory)?;
    let path = private_turn_detail_path(&directory, turn_id)?;
    let max_detail_bytes = u64::try_from(MAX_PRIVATE_TURN_DETAIL_BYTES)
        .map_err(|_| CollectorError::Runtime("private detail size bound overflow".into()))?;
    let bytes = read_private_bounded(&path, max_detail_bytes)?;
    let detail = PrivateCodexTurnDetailV1::from_json(&bytes).map_err(runtime_error)?;
    if detail.turn_id() != turn_id {
        return Err(CollectorError::Runtime(
            "private turn detail identity mismatch".into(),
        ));
    }
    Ok(bytes)
}

fn persist_private_turn_detail(
    layout: &InstalledLayout,
    detail: &PrivateCodexTurnDetailV1,
    config: &LocalRuntimeConfigV2,
) -> Result<(), CollectorError> {
    let directory = layout.state.join(PRIVATE_TURN_DETAIL_DIRECTORY);
    ensure_private_directory_tree(&layout.state, &directory)?;
    let path = private_turn_detail_path(&directory, detail.turn_id())?;
    match fs::symlink_metadata(&path) {
        Ok(_) => {
            let _ = open_private_read(&path)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let bytes = detail.to_json().map_err(runtime_error)?;
    prune_private_turn_details(&directory, &path, config, SystemTime::now())?;
    let reservation = u64::try_from(MAX_PRIVATE_TURN_DETAIL_BYTES)
        .map_err(|_| CollectorError::Runtime("private detail size bound overflow".into()))?;
    let control = RuntimeControl::new(config).map_err(runtime_error)?;
    if control
        .admit(&layout.root, reservation)
        .map_err(runtime_error)?
        == Admission::Denied
    {
        return Err(CollectorError::Runtime(
            "private turn detail exceeds local storage budget".into(),
        ));
    }
    let temporary = settings_temporary_path(&directory);
    let result = (|| {
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, &path)?;
        File::open(&directory)?.sync_all()?;
        Ok(())
    })();
    let _ = fs::remove_file(&temporary);
    result
}

fn prune_private_turn_details(
    directory: &Path,
    replacement: &Path,
    config: &LocalRuntimeConfigV2,
    now: SystemTime,
) -> Result<(), CollectorError> {
    prune_private_turn_details_with_limit(
        directory,
        replacement,
        config,
        now,
        MAX_PRIVATE_TURN_DETAIL_FILES,
    )
}

fn prune_private_turn_details_with_limit(
    directory: &Path,
    replacement: &Path,
    config: &LocalRuntimeConfigV2,
    now: SystemTime,
    max_files: usize,
) -> Result<(), CollectorError> {
    if max_files == 0 || max_files > MAX_PRIVATE_TURN_DETAIL_FILES {
        return Err(CollectorError::Runtime(
            "private turn detail file bound is invalid".into(),
        ));
    }
    let max_age = Duration::from_secs(
        u64::from(config.retention.max_record_age_days)
            .checked_mul(24 * 60 * 60)
            .ok_or_else(|| CollectorError::Runtime("private detail retention overflow".into()))?,
    );
    let max_detail_bytes = u64::try_from(MAX_PRIVATE_TURN_DETAIL_BYTES)
        .map_err(|_| CollectorError::Runtime("private detail size bound overflow".into()))?;
    let mut retained = Vec::new();
    let mut expired = Vec::new();
    for (index, entry) in fs::read_dir(directory)?.enumerate() {
        if index >= MAX_PRIVATE_TURN_DETAIL_SCAN_ENTRIES {
            return Err(CollectorError::Runtime(
                "private turn detail directory exceeds scan bound".into(),
            ));
        }
        let entry = entry?;
        let path = entry.path();
        if path == replacement {
            continue;
        }
        if path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with(".collector.json.tmp."))
        {
            let file = open_private_read(&path)?;
            let metadata = file.metadata()?;
            if metadata.len() > max_detail_bytes {
                return Err(CollectorError::Runtime(
                    "private turn detail temporary exceeds size bound".into(),
                ));
            }
            if now
                .duration_since(metadata.modified()?)
                .is_ok_and(|age| age > Duration::from_mins(5))
            {
                expired.push(path);
            }
            continue;
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                CollectorError::Runtime("private turn detail entry is invalid".into())
            })?;
        let digest = name.strip_suffix(".json").ok_or_else(|| {
            CollectorError::Runtime("private turn detail entry is invalid".into())
        })?;
        let _ = private_turn_detail_path(directory, &format!("id:sha256:{digest}"))?;
        let file = open_private_read(&path)?;
        let metadata = file.metadata()?;
        if metadata.len() > max_detail_bytes {
            return Err(CollectorError::Runtime(
                "private turn detail exceeds size bound".into(),
            ));
        }
        let modified = metadata.modified()?;
        if now.duration_since(modified).is_ok_and(|age| age > max_age) {
            expired.push(path);
        } else {
            retained.push((modified, path));
        }
    }
    retained.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    for (_, path) in retained.into_iter().skip(max_files - 1) {
        expired.push(path);
    }
    if expired.is_empty() {
        return Ok(());
    }
    for path in expired {
        fs::remove_file(path)?;
    }
    File::open(directory)?.sync_all()?;
    Ok(())
}

fn private_turn_detail_path(directory: &Path, turn_id: &str) -> Result<PathBuf, CollectorError> {
    let Some(digest) = turn_id.strip_prefix("id:sha256:") else {
        return Err(CollectorError::Runtime(
            "private turn detail identifier is invalid".into(),
        ));
    };
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(CollectorError::Runtime(
            "private turn detail identifier is invalid".into(),
        ));
    }
    Ok(directory.join(format!("{digest}.json")))
}

/// Performs a bounded authenticated health probe against the local collector.
#[must_use]
pub fn check_health(root: &Path) -> HealthOutcome {
    match authenticated_request(
        root,
        "GET",
        "/health",
        None,
        Duration::from_millis(50),
        Duration::from_millis(100),
    ) {
        Ok(response) if response.status == 200 => {
            match serde_json::from_slice::<HealthProbe>(&response.body) {
                Ok(HealthProbe {
                    status,
                    report_dirty: false,
                }) if status == "ready" => HealthOutcome::Ready,
                Ok(HealthProbe {
                    status,
                    report_dirty: true,
                }) if status == "degraded" => HealthOutcome::Degraded,
                _ => HealthOutcome::Unavailable,
            }
        }
        _ => HealthOutcome::Unavailable,
    }
}

/// Sends bounded OTLP JSON through the same authenticated direct-loopback client.
pub fn submit_otlp_json(root: &Path, payload: &[u8]) -> Result<bool, CollectorError> {
    submit_otlp_json_outcome(root, payload)
        .map(|outcome| matches!(outcome, OtlpSubmissionOutcome::Accepted))
}

/// Sends bounded OTLP JSON and returns a content-free rejection classification.
pub fn submit_otlp_json_outcome(
    root: &Path,
    payload: &[u8],
) -> Result<OtlpSubmissionOutcome, CollectorError> {
    if payload.len() > usize::try_from(MAX_HANDOFF_BYTES).unwrap_or(usize::MAX) {
        return Ok(OtlpSubmissionOutcome::Rejected {
            status: StatusCode::PAYLOAD_TOO_LARGE.as_u16(),
            category: OtlpRejectionCategory::Policy,
        });
    }
    let response = authenticated_request(
        root,
        "POST",
        "/v1/logs",
        Some(payload),
        Duration::from_millis(250),
        Duration::from_secs(1),
    )?;
    if response.status == StatusCode::OK.as_u16() {
        return Ok(OtlpSubmissionOutcome::Accepted);
    }
    let category = classify_otlp_rejection(response.status, &response.body);
    Ok(OtlpSubmissionOutcome::Rejected {
        status: response.status,
        category,
    })
}

fn classify_otlp_rejection(status: u16, body: &[u8]) -> OtlpRejectionCategory {
    match status {
        401 => OtlpRejectionCategory::Unauthorized,
        413 => OtlpRejectionCategory::Policy,
        415 => OtlpRejectionCategory::MediaType,
        422 => OtlpRejectionCategory::Invalid,
        503 if body == b"busy" => OtlpRejectionCategory::Busy,
        503 => OtlpRejectionCategory::Pressure,
        507 => OtlpRejectionCategory::Storage,
        500 => OtlpRejectionCategory::Internal,
        _ => OtlpRejectionCategory::Other,
    }
}

struct AuthenticatedResponse {
    status: u16,
    body: Vec<u8>,
}

struct DeadlineStream {
    stream: TcpStream,
    deadline: StdInstant,
}

impl DeadlineStream {
    fn new(stream: TcpStream, deadline: StdInstant) -> Self {
        Self { stream, deadline }
    }

    fn remaining(&self) -> std::io::Result<Duration> {
        let remaining = self.deadline.saturating_duration_since(StdInstant::now());
        if remaining.is_zero() {
            Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "collector foreground request timed out",
            ))
        } else {
            Ok(remaining)
        }
    }
}

impl Read for DeadlineStream {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.stream.set_read_timeout(Some(self.remaining()?))?;
        self.stream.read(buffer)
    }
}

impl Write for DeadlineStream {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.stream.set_write_timeout(Some(self.remaining()?))?;
        self.stream.write(buffer)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.stream.set_write_timeout(Some(self.remaining()?))?;
        self.stream.flush()
    }
}

fn authenticated_request(
    root: &Path,
    method: &str,
    path: &str,
    body: Option<&[u8]>,
    connect_timeout: Duration,
    io_timeout: Duration,
) -> Result<AuthenticatedResponse, CollectorError> {
    let deadline = StdInstant::now() + connect_timeout + io_timeout;
    let settings = load_settings(root)?;
    let layout = install(root).map_err(runtime_error)?;
    let config = build_client_config(&layout, &settings.credentials)?;
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), settings.port);
    let remaining = deadline.saturating_duration_since(StdInstant::now());
    if remaining.is_zero() {
        return Err(CollectorError::Io(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "collector foreground request timed out",
        )));
    }
    let stream = TcpStream::connect_timeout(&address, connect_timeout.min(remaining))
        .map_err(|error| request_io("connect", error))?;
    let server_name = ServerName::try_from("127.0.0.1").map_err(crypto_error)?;
    let connection = ClientConnection::new(config, server_name).map_err(crypto_error)?;
    let mut tls = StreamOwned::new(connection, DeadlineStream::new(stream, deadline));
    while tls.conn.is_handshaking() {
        tls.conn
            .complete_io(&mut tls.sock)
            .map_err(|error| request_io("tls-handshake", error))?;
    }
    let body = body.unwrap_or_default();
    let content_headers = if body.is_empty() {
        String::new()
    } else {
        format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            body.len()
        )
    };
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n{AUTH_HEADER_NAME}: {}\r\n{content_headers}Connection: close\r\n\r\n",
        settings.auth_token,
    );
    tls.write_all(request.as_bytes())
        .map_err(|error| request_io("request-write", error))?;
    if !body.is_empty() {
        tls.write_all(body)
            .map_err(|error| request_io("request-write", error))?;
    }
    tls.flush()
        .map_err(|error| request_io("request-write", error))?;
    read_bounded_http_response(&mut tls).map_err(|error| match error {
        CollectorError::Io(error) => request_io("response-read", error),
        error => error,
    })
}

fn read_bounded_http_response(
    stream: &mut impl Read,
) -> Result<AuthenticatedResponse, CollectorError> {
    const MAX_RESPONSE_BYTES: usize = 4 * 1024;
    let mut response = Vec::with_capacity(512);
    let mut chunk = [0_u8; 512];
    loop {
        let bytes = stream.read(&mut chunk)?;
        if bytes == 0 {
            break;
        }
        if response.len().saturating_add(bytes) > MAX_RESPONSE_BYTES {
            return Err(CollectorError::Runtime(
                "collector response is oversized".into(),
            ));
        }
        response.extend_from_slice(&chunk[..bytes]);
        if let Some(parsed) = parse_complete_http_response(&response)? {
            return Ok(parsed);
        }
    }
    Err(CollectorError::Runtime(
        "collector response is incomplete".into(),
    ))
}

fn parse_complete_http_response(
    response: &[u8],
) -> Result<Option<AuthenticatedResponse>, CollectorError> {
    let Some(header_end) = response.windows(4).position(|window| window == b"\r\n\r\n") else {
        return Ok(None);
    };
    let headers = std::str::from_utf8(&response[..header_end])
        .map_err(|_| CollectorError::Runtime("collector response headers are not UTF-8".into()))?;
    let mut lines = headers.split("\r\n");
    let status = lines
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|status| status.parse().ok())
        .ok_or_else(|| CollectorError::Runtime("collector response is invalid".into()))?;
    let mut content_length = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            return Err(CollectorError::Runtime(
                "collector response header is invalid".into(),
            ));
        };
        if name.eq_ignore_ascii_case("transfer-encoding") {
            return Err(CollectorError::Runtime(
                "collector response transfer encoding is unsupported".into(),
            ));
        }
        if name.eq_ignore_ascii_case("content-length") {
            let length = value.trim().parse::<usize>().map_err(|_| {
                CollectorError::Runtime("collector response content length is invalid".into())
            })?;
            if content_length.replace(length).is_some() {
                return Err(CollectorError::Runtime(
                    "collector response has duplicate content length".into(),
                ));
            }
        }
    }
    let content_length = content_length.ok_or_else(|| {
        CollectorError::Runtime("collector response has no content length".into())
    })?;
    let body_start = header_end + 4;
    let expected = body_start.saturating_add(content_length);
    if response.len() < expected {
        return Ok(None);
    }
    if response.len() != expected {
        return Err(CollectorError::Runtime(
            "collector response has trailing bytes".into(),
        ));
    }
    Ok(Some(AuthenticatedResponse {
        status,
        body: response[body_start..].to_vec(),
    }))
}

fn refresh_report(
    layout: &InstalledLayout,
    store: &LocalStore,
    now_unix_ms: u64,
) -> Result<bool, ReportFailure> {
    let _render_guard = store
        .acquire_report_render_guard()
        .map_err(|_| ReportFailure::RenderGuard)?;
    let capacity = usize::try_from(store.record_count().map_err(|_| ReportFailure::Snapshot)?)
        .map_err(|_| ReportFailure::Snapshot)?;
    let mut projector = ReportProjector::new(capacity, None);
    let mut projection_failure = false;
    let visit = store
        .visit_report_snapshot(|index, record| {
            if !projection_failure && projector.push_owned(index, record).is_err() {
                projection_failure = true;
            }
        })
        .map_err(|_| ReportFailure::Snapshot)?;
    if projection_failure {
        return Err(ReportFailure::Projection);
    }
    let report = projector
        .finish(
            timestamp_from_unix_ms(now_unix_ms).map_err(|_| ReportFailure::Projection)?,
            "Agent Observability Report",
        )
        .map_err(|_| ReportFailure::Projection)?;
    write_private(&layout.logs.join(REPORT_FILE_NAME), &report)
        .map_err(|_| ReportFailure::Publish)?;
    store
        .acknowledge_report_generation(visit.generation)
        .map_err(|_| ReportFailure::Acknowledge)
}

fn open_store(
    _mutation: &MutationGuard,
    layout: &InstalledLayout,
    config: &LocalRuntimeConfigV2,
) -> Result<LocalStore, CollectorError> {
    let control = RuntimeControl::new(config).map_err(runtime_error)?;
    let headroom = control
        .migration_headroom(&layout.root)
        .map_err(runtime_error)?;
    LocalStore::open_with_migration_headroom_deferred_projection(
        layout.state.join("store"),
        headroom,
    )
    .map_err(runtime_error)
}

fn validate_options(options: &CollectorOptions) -> Result<(), CollectorError> {
    if options.port == 0
        || options.generation.len() != 64
        || !options
            .generation
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || options.auth_token.len() != 64
        || !options
            .auth_token
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || !options.root.is_absolute()
        || options.credentials.expires_at_unix_ms <= current_unix_ms()?
    {
        return Err(CollectorError::Runtime(
            "invalid local collector options".into(),
        ));
    }
    Ok(())
}

fn current_unix_ms() -> Result<u64, CollectorError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| CollectorError::Runtime("system clock is before Unix epoch".into()))?;
    u64::try_from(duration.as_millis())
        .map_err(|_| CollectorError::Runtime("system clock is out of range".into()))
}

fn timestamp_from_unix_ms(unix_ms: u64) -> Result<String, CollectorError> {
    let seconds = i64::try_from(unix_ms / 1_000)
        .map_err(|_| CollectorError::Runtime("system clock is out of range".into()))?;
    let days = seconds / 86_400;
    let seconds_in_day = seconds % 86_400;
    let (year, month, day) = civil_date_from_days(days);
    let hour = seconds_in_day / 3_600;
    let minute = (seconds_in_day % 3_600) / 60;
    let second = seconds_in_day % 60;
    Ok(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{:03}Z",
        unix_ms % 1_000
    ))
}

fn civil_date_from_days(days_since_epoch: i64) -> (i64, i64, i64) {
    let days = days_since_epoch + 719_468;
    let era = days / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month, day)
}

fn runtime_error(error: impl std::fmt::Display) -> CollectorError {
    CollectorError::Runtime(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        AUTH_HEADER_NAME, AppState, CollectorState, IngestError, IngestOutcome, NotifyOutcome,
        OtlpRejectionCategory, OtlpRequestCorrelationState, OtlpSubmissionOutcome,
        REPORT_FILE_NAME, ReportFailure, admit_request, authenticated_request, build_client_config,
        build_server_config, classify_otlp_rejection, enforce_batch_policy, ingest_locked,
        ingest_notify_locked, install_settings, is_json, load_settings, open_store,
        parse_complete_http_response, persist_private_turn_detail, private_turn_detail_path,
        project_report, prune_private_turn_details_with_limit, read_private_snapshot,
        read_private_turn_detail, reconcile_report_state, recover_occupied_persisted_port,
        refresh_report_from_root, report_dirty_path, router, schedule_report_refresh,
        settings_path, submit_notify, submit_otlp_json_outcome, timestamp_from_unix_ms,
        token_matches, watch_report_authority, write_private, write_private_json,
        write_private_json_if_unchanged,
    };
    use agent_observability_adapter_codex::{
        MAX_HANDOFF_BYTES, parse_otlp_http_json, project_notify_with_private_detail,
    };
    use agent_observability_local_runtime::{
        ConfigMutationGuard, MutationGuard, StorageBudget, install, load, save,
    };
    use axum::{
        extract::State,
        http::{HeaderMap, HeaderValue, StatusCode, header},
        response::IntoResponse,
    };
    use std::{
        fs::{self, OpenOptions},
        io::{Read, Write},
        net::{Ipv4Addr, TcpListener, TcpStream},
        path::{Path, PathBuf},
        sync::{
            Arc,
            atomic::{AtomicBool, AtomicU64, Ordering},
        },
        thread,
        time::{Duration, Instant},
    };
    use tokio::sync::Mutex;

    #[test]
    fn report_failure_health_codes_are_bounded_stage_names() {
        for (failure, expected) in [
            (ReportFailure::Task, "\"task\""),
            (ReportFailure::Install, "\"install\""),
            (ReportFailure::OpenStore, "\"open_store\""),
            (ReportFailure::Clock, "\"clock\""),
            (ReportFailure::RenderGuard, "\"render_guard\""),
            (ReportFailure::Snapshot, "\"snapshot\""),
            (ReportFailure::Projection, "\"projection\""),
            (ReportFailure::Publish, "\"publish\""),
            (ReportFailure::Acknowledge, "\"acknowledge\""),
            (ReportFailure::Status, "\"status\""),
        ] {
            assert_eq!(serde_json::to_string(&failure).unwrap(), expected);
        }
    }

    fn test_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "agent-observability-collector-{name}-{}",
            std::process::id()
        ))
    }

    #[test]
    fn authentication_requires_exactly_one_matching_header() {
        let expected = "a".repeat(64);
        let matching = HeaderValue::from_str(&expected).unwrap();
        let wrong = HeaderValue::from_static("wrong");

        let mut headers = HeaderMap::new();
        assert!(!token_matches(&headers, &expected));
        headers.append(super::AUTH_HEADER_NAME, matching.clone());
        assert!(token_matches(&headers, &expected));
        headers.append(super::AUTH_HEADER_NAME, wrong.clone());
        assert!(!token_matches(&headers, &expected));

        let mut reversed = HeaderMap::new();
        reversed.append(super::AUTH_HEADER_NAME, wrong);
        reversed.append(super::AUTH_HEADER_NAME, matching.clone());
        assert!(!token_matches(&reversed, &expected));

        let mut duplicated = HeaderMap::new();
        duplicated.append(super::AUTH_HEADER_NAME, matching.clone());
        duplicated.append(super::AUTH_HEADER_NAME, matching);
        assert!(!token_matches(&duplicated, &expected));
    }

    #[test]
    fn otlp_submission_rejections_are_content_free_and_exactly_classified() {
        assert_eq!(
            classify_otlp_rejection(503, b"busy"),
            OtlpRejectionCategory::Busy
        );
        assert_eq!(
            classify_otlp_rejection(503, b"anything else"),
            OtlpRejectionCategory::Pressure
        );
        assert_eq!(
            classify_otlp_rejection(422, b"RAW_RESPONSE_SECRET"),
            OtlpRejectionCategory::Invalid
        );
        assert_eq!(
            classify_otlp_rejection(599, b"RAW_RESPONSE_SECRET"),
            OtlpRejectionCategory::Other
        );

        let root = test_root("oversized-submission-outcome");
        let oversized = vec![0_u8; usize::try_from(MAX_HANDOFF_BYTES).unwrap() + 1];
        assert_eq!(
            submit_otlp_json_outcome(&root, &oversized).unwrap(),
            OtlpSubmissionOutcome::Rejected {
                status: StatusCode::PAYLOAD_TOO_LARGE.as_u16(),
                category: OtlpRejectionCategory::Policy,
            }
        );
    }

    fn collector_state(root: &Path) -> CollectorState {
        let layout = install(root).unwrap();
        let config = load(&layout.config).unwrap();
        let store = open_store_for_test(&layout, &config);
        let source_generation = "codex-test".to_owned();
        let last_cursor = store.cursor("codex", &source_generation).unwrap();
        let request_correlation = store
            .codex_request_correlation_state(&source_generation)
            .unwrap()
            .map(|snapshot| {
                OtlpRequestCorrelationState::from_persisted_json(
                    &snapshot,
                    super::current_unix_ms().unwrap(),
                )
                .unwrap()
            })
            .unwrap_or_default();
        CollectorState {
            layout,
            store,
            source_generation,
            last_cursor,
            request_correlation,
            accepted_requests: 0,
            rejected_requests: 0,
            suppressed_requests: 0,
            last_ingest_unix_ms: None,
            report_dirty: false,
            report_degraded: false,
            report_refresh_failures: 0,
            report_failure: None,
        }
    }

    fn open_store_for_test(
        layout: &agent_observability_local_runtime::InstalledLayout,
        config: &agent_observability_local_runtime::LocalRuntimeConfigV2,
    ) -> agent_observability_local_store::LocalStore {
        let mutation = MutationGuard::acquire(&layout.runtime).unwrap();
        open_store(&mutation, layout, config).unwrap()
    }

    fn app_state(root: &Path) -> AppState {
        let auth_token = install_settings(root).unwrap().auth_token;
        AppState {
            collector: Arc::new(Mutex::new(collector_state(root))),
            auth_token: Arc::from(auth_token),
            report_refresh_scheduled: Arc::new(AtomicBool::new(false)),
            report_refresh_requested: Arc::new(AtomicU64::new(0)),
            report_refresh_attempts: Arc::new(AtomicU64::new(0)),
        }
    }

    fn padded_json(body: &[u8], bytes: usize) -> Vec<u8> {
        assert!(body.len() <= bytes);
        let mut padded = Vec::with_capacity(bytes);
        padded.extend_from_slice(body);
        padded.resize(bytes, b' ');
        padded
    }

    fn inflate_allocated_accounting(root: &Path) {
        let source = root.join("allocated-budget-fixture");
        fs::write(&source, vec![0_u8; 1024 * 1024]).unwrap();
        let reduced = StorageBudget::calculate(256 * 1024 * 1024, false).unwrap();
        for index in 0..300 {
            let allocated = StorageBudget::allocated_tree_bytes(root).unwrap();
            if allocated + 512 * 1024 > reduced.writable_limit() {
                return;
            }
            fs::hard_link(&source, root.join(format!("allocated-budget-link-{index}"))).unwrap();
        }
        panic!("failed to inflate storage accounting above the reduced budget");
    }

    #[test]
    fn authenticated_response_parser_requires_one_bounded_content_length_body() {
        let complete = b"HTTP/1.1 200 OK\r\ncontent-length: 5\r\n\r\nready";
        assert!(
            parse_complete_http_response(&complete[..complete.len() - 1])
                .unwrap()
                .is_none()
        );
        let parsed = parse_complete_http_response(complete).unwrap().unwrap();
        assert_eq!(parsed.status, 200);
        assert_eq!(parsed.body, b"ready");

        for invalid in [
            b"HTTP/1.1 200 OK\r\n\r\n".as_slice(),
            b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\ncontent-length: 0\r\n\r\n".as_slice(),
            b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n".as_slice(),
            b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\ntrailing".as_slice(),
        ] {
            assert!(parse_complete_http_response(invalid).is_err());
        }
    }

    #[test]
    fn collector_ingest_returns_busy_without_waiting_for_the_shared_runtime_mutation_guard() {
        let root = test_root("shared-mutation-guard");
        let _ = fs::remove_dir_all(&root);
        let mut state = collector_state(&root);
        let guard = MutationGuard::acquire(&state.layout.runtime).unwrap();
        let result = ingest_notify_locked(&mut state, &projected_notify("thread-1", "turn-1"));
        assert!(matches!(result, Err(IngestError::Busy)));
        assert_eq!(state.store.record_count().unwrap(), 0);
        drop(guard);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn busy_ingest_response_is_visible_as_service_unavailable() {
        let response = IngestError::Busy.into_response();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let body = runtime
            .block_on(axum::body::to_bytes(response.into_body(), 16))
            .unwrap();
        assert_eq!(&body[..], b"busy");
    }

    #[test]
    fn current_schema_report_refresh_is_independent_of_config_mutation() {
        let root = test_root("open-rebuild-mutation");
        let _ = fs::remove_dir_all(&root);
        let state = collector_state(&root);
        let layout = state.layout.clone();
        drop(state);
        let guard = ConfigMutationGuard::acquire(&layout).unwrap();

        assert!(refresh_report_from_root(&root).unwrap());

        drop(guard);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn concurrent_disable_blocks_automatic_ingest_before_admission() {
        let root = test_root("concurrent-disable");
        let _ = fs::remove_dir_all(&root);
        let mut state = collector_state(&root);
        let layout = state.layout.clone();
        let guard = ConfigMutationGuard::acquire(&layout).unwrap();
        let busy = ingest_notify_locked(&mut state, &projected_notify("thread-1", "turn-1"));
        assert!(matches!(busy, Err(IngestError::Busy)));

        let mut config = load(&layout.config).unwrap();
        config.enabled = false;
        save(&guard, &config).unwrap();
        drop(guard);

        let retry = ingest_notify_locked(&mut state, &projected_notify("thread-1", "turn-1"));
        assert_eq!(retry.unwrap(), IngestOutcome::Disabled);
        assert_eq!(state.store.record_count().unwrap(), 0);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn concurrent_budget_reduction_blocks_automatic_ingest_before_commit() {
        let root = test_root("concurrent-budget");
        let _ = fs::remove_dir_all(&root);
        let mut state = collector_state(&root);
        let layout = state.layout.clone();
        inflate_allocated_accounting(&root);
        let guard = ConfigMutationGuard::acquire(&layout).unwrap();
        let busy = ingest_notify_locked(&mut state, &projected_notify("thread-1", "turn-1"));
        assert!(matches!(busy, Err(IngestError::Busy)));

        let mut config = load(&layout.config).unwrap();
        config.collection.local_storage_budget_bytes = 256 * 1024 * 1024;
        save(&guard, &config).unwrap();
        drop(guard);

        let retry = ingest_notify_locked(&mut state, &projected_notify("thread-1", "turn-1"));
        assert!(matches!(retry, Err(IngestError::Storage)));
        assert_eq!(state.store.record_count().unwrap(), 0);
        let _ = fs::remove_dir_all(root);
    }

    fn projected_notify(thread: &str, turn: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "schema_version": "codex_projected_notify.v1",
            "event_name": "agent-turn-complete",
            "thread_id": thread,
            "turn_id": turn,
            "project_name": "RAW_PATH_SECRET",
        }))
        .unwrap()
    }

    fn raw_notify(thread: &str, turn: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "type": "agent-turn-complete",
            "thread-id": thread,
            "turn-id": turn,
            "cwd": "/RAW_PATH_SECRET",
            "input-messages": ["RAW_INPUT_SECRET"],
            "last-assistant-message": "RAW_OUTPUT_SECRET",
        }))
        .unwrap()
    }

    fn set_private_turn_details(root: &Path, enabled: bool) {
        let layout = install(root).unwrap();
        let guard = ConfigMutationGuard::acquire(&layout).unwrap();
        let mut config = load(&layout.config).unwrap();
        config.capture_private_codex_turn_details = enabled;
        save(&guard, &config).unwrap();
    }

    #[cfg(unix)]
    fn write_private_test_file(path: &Path, bytes: &[u8]) {
        use std::os::unix::fs::OpenOptionsExt;

        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(path)
            .unwrap();
        file.write_all(bytes).unwrap();
        file.sync_all().unwrap();
    }

    fn test_tls_configs(root: &Path) -> (Arc<rustls::ServerConfig>, Arc<rustls::ClientConfig>) {
        let settings = install_settings(root).unwrap();
        let layout = install(root).unwrap();
        (
            build_server_config(&layout, &settings.credentials).unwrap(),
            build_client_config(&layout, &settings.credentials).unwrap(),
        )
    }

    fn tls_stream(
        port: u16,
        config: Arc<rustls::ClientConfig>,
    ) -> rustls::StreamOwned<rustls::ClientConnection, TcpStream> {
        let stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        let timeout = Some(Duration::from_secs(2));
        stream.set_write_timeout(timeout).unwrap();
        stream.set_read_timeout(timeout).unwrap();
        let connection = rustls::ClientConnection::new(
            config,
            rustls::pki_types::ServerName::try_from("127.0.0.1").unwrap(),
        )
        .unwrap();
        rustls::StreamOwned::new(connection, stream)
    }

    fn attempt_tls_http(
        port: u16,
        config: Arc<rustls::ClientConfig>,
        server_name: &str,
        token: Option<&str>,
    ) -> std::io::Result<Vec<u8>> {
        let stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port))?;
        let timeout = Some(Duration::from_secs(1));
        stream.set_write_timeout(timeout)?;
        stream.set_read_timeout(timeout)?;
        let connection = rustls::ClientConnection::new(
            config,
            rustls::pki_types::ServerName::try_from(server_name.to_owned())
                .map_err(std::io::Error::other)?,
        )
        .map_err(std::io::Error::other)?;
        let mut tls = rustls::StreamOwned::new(connection, stream);
        let auth = token.map_or_else(String::new, |token| {
            format!("{AUTH_HEADER_NAME}: {token}\r\n")
        });
        tls.write_all(
            format!("GET /health HTTP/1.1\r\nHost: 127.0.0.1\r\n{auth}Connection: close\r\n\r\n")
                .as_bytes(),
        )?;
        let mut response = Vec::new();
        tls.read_to_end(&mut response)?;
        Ok(response)
    }

    async fn post(
        port: u16,
        config: Arc<rustls::ClientConfig>,
        token: String,
        content_type: Option<&str>,
        body: Vec<u8>,
    ) -> StatusCode {
        let content_type = content_type.map(str::to_owned);
        tokio::task::spawn_blocking(move || {
            use std::fmt::Write as _;

            let mut stream = tls_stream(port, config);
            let mut request = format!(
                "POST /v1/logs HTTP/1.1\r\nHost: 127.0.0.1\r\n{AUTH_HEADER_NAME}: {token}\r\nContent-Length: {}\r\nConnection: close\r\n",
                body.len()
            );
            if let Some(content_type) = content_type {
                write!(request, "Content-Type: {content_type}\r\n").unwrap();
            }
            request.push_str("\r\n");
            stream.write_all(request.as_bytes()).unwrap();
            stream.write_all(&body).unwrap();
            let mut response = Vec::new();
            stream.read_to_end(&mut response).unwrap();
            let status = String::from_utf8_lossy(&response)
                .split_whitespace()
                .nth(1)
                .unwrap_or_else(|| panic!("missing HTTP status in {response:?}"))
                .parse::<u16>()
                .unwrap();
            StatusCode::from_u16(status).unwrap()
        })
            .await
            .unwrap()
    }

    async fn wait_for_available_permits(slots: &Arc<tokio::sync::Semaphore>, expected: usize) {
        tokio::time::timeout(Duration::from_secs(1), async {
            while slots.available_permits() != expected {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap_or_else(|_| {
            panic!(
                "expected {expected} available permits, found {}",
                slots.available_permits()
            )
        });
    }

    fn response_status(response: &[u8]) -> StatusCode {
        let status = String::from_utf8_lossy(response)
            .split_whitespace()
            .nth(1)
            .unwrap_or_else(|| panic!("missing HTTP status in {response:?}"))
            .parse::<u16>()
            .unwrap();
        StatusCode::from_u16(status).unwrap()
    }

    fn fast_report_timing() -> super::ReportRefreshTiming {
        super::ReportRefreshTiming {
            debounce: Duration::from_millis(20),
            retry_initial: Duration::from_millis(10),
        }
    }

    fn configure_port(root: &Path, port: u16) {
        let mut settings = install_settings(root).unwrap();
        settings.port = port;
        let layout = install(root).unwrap();
        write_private_json(&settings_path(&layout), &settings).unwrap();
    }

    fn otlp_start_records(count: usize) -> Vec<u8> {
        let records = (0..count)
            .map(|index| {
                format!(
                    r#"{{"attributes":[{{"key":"event.name","value":{{"stringValue":"codex.conversation_starts"}}}},{{"key":"conversation.id","value":{{"stringValue":"conversation-{index}"}}}}]}}"#
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        format!(r#"{{"resourceLogs":[{{"scopeLogs":[{{"logRecords":[{records}]}}]}}]}}"#)
            .into_bytes()
    }

    fn otlp_disposition_records(count: usize) -> Vec<u8> {
        let records = std::iter::repeat_n(
            r#"{"attributes":[{"key":"event.name","value":{"stringValue":"codex.user_prompt"}}]}"#,
            count,
        )
        .collect::<Vec<_>>()
        .join(",");
        format!(r#"{{"resourceLogs":[{{"scopeLogs":[{{"logRecords":[{records}]}}]}}]}}"#)
            .into_bytes()
    }

    fn assert_tree_excludes(root: &Path, secrets: &[&[u8]]) {
        let mut pending = vec![root.to_path_buf()];
        while let Some(path) = pending.pop() {
            for entry in fs::read_dir(path).unwrap() {
                let entry = entry.unwrap();
                let file_type = entry.file_type().unwrap();
                if file_type.is_dir() {
                    pending.push(entry.path());
                } else if file_type.is_file() {
                    let body = fs::read(entry.path()).unwrap();
                    for secret in secrets {
                        assert!(
                            !body.windows(secret.len()).any(|window| window == *secret),
                            "secret persisted in {}",
                            entry.path().display()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn report_timestamp_is_content_free_and_stable() {
        assert_eq!(
            timestamp_from_unix_ms(946_684_800_123).unwrap(),
            "2000-01-01T00:00:00.123Z"
        );
    }

    #[test]
    fn receiver_accepts_only_json_media_types() {
        let mut headers = HeaderMap::new();
        assert!(!is_json(&headers));
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/plain"));
        assert!(!is_json(&headers));
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json; charset=utf-8"),
        );
        assert!(is_json(&headers));
    }

    #[test]
    fn transport_closes_partial_headers_at_the_read_deadline() {
        let root = test_root("partial-header-timeout");
        let _ = fs::remove_dir_all(&root);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let (server_config, client_config) = test_tls_configs(&root);
            let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
                .await
                .unwrap();
            let port = listener.local_addr().unwrap().port();
            let transport = super::TransportListener::new(
                listener,
                server_config,
                Duration::from_millis(50),
                1,
            );
            let slots = Arc::clone(&transport.connection_slots);
            let app =
                super::protect_request_lifetime(router(app_state(&root)), Duration::from_secs(1));
            let server = tokio::spawn(async move { axum::serve(transport, app).await });
            let mut stream = tokio::task::spawn_blocking(move || {
                let mut stream = tls_stream(port, client_config);
                stream
                    .write_all(b"GET /health HTTP/1.1\r\nHost: 127.0.0.1")
                    .unwrap();
                stream
            })
            .await
            .unwrap();
            wait_for_available_permits(&slots, 0).await;

            let result = tokio::task::spawn_blocking(move || {
                stream
                    .sock
                    .set_read_timeout(Some(Duration::from_secs(1)))
                    .unwrap();
                let mut byte = [0_u8; 1];
                stream.read(&mut byte)
            })
            .await
            .unwrap();
            assert!(
                result.as_ref().is_ok_and(|bytes| *bytes == 0)
                    || result.as_ref().is_err_and(|error| matches!(
                        error.kind(),
                        std::io::ErrorKind::ConnectionReset
                            | std::io::ErrorKind::ConnectionAborted
                            | std::io::ErrorKind::UnexpectedEof
                    )),
                "partial header connection remained open: {result:?}"
            );
            wait_for_available_permits(&slots, 1).await;
            server.abort();
        });
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn transport_bounds_an_incomplete_tls_handshake() {
        let root = test_root("partial-tls-handshake");
        let _ = fs::remove_dir_all(&root);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        runtime.block_on(async {
            let (server_config, _) = test_tls_configs(&root);
            let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
                .await
                .unwrap();
            let port = listener.local_addr().unwrap().port();
            let transport = super::TransportListener::new(
                listener,
                server_config,
                Duration::from_millis(50),
                1,
            );
            let slots = Arc::clone(&transport.connection_slots);
            let app = router(app_state(&root));
            let server = tokio::spawn(async move { axum::serve(transport, app).await });
            let started = Instant::now();
            let result = tokio::task::spawn_blocking(move || {
                let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(1)))
                    .unwrap();
                stream.write_all(&[0x16, 0x03, 0x03]).unwrap();
                let mut byte = [0_u8; 1];
                stream.read(&mut byte)
            })
            .await
            .unwrap();
            let elapsed = started.elapsed();
            assert!(
                result.as_ref().is_ok_and(|bytes| *bytes == 0)
                    || result.as_ref().is_err_and(|error| matches!(
                        error.kind(),
                        std::io::ErrorKind::ConnectionReset
                            | std::io::ErrorKind::ConnectionAborted
                            | std::io::ErrorKind::UnexpectedEof
                    )),
                "incomplete TLS handshake remained open: {result:?}"
            );
            assert!(elapsed < Duration::from_millis(500), "elapsed={elapsed:?}");
            wait_for_available_permits(&slots, 1).await;
            server.abort();
        });
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn authenticated_request_labels_a_stalled_tls_handshake() {
        let root = test_root("authenticated-request-tls-stage");
        let _ = fs::remove_dir_all(&root);
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        configure_port(&root, listener.local_addr().unwrap().port());
        let server = thread::spawn(move || {
            let (_stream, _) = listener.accept().unwrap();
            thread::sleep(Duration::from_millis(250));
        });

        let Err(error) = authenticated_request(
            &root,
            "GET",
            "/health",
            None,
            Duration::from_millis(50),
            Duration::from_millis(50),
        ) else {
            panic!("stalled TLS handshake unexpectedly completed");
        };

        assert!(matches!(
            error,
            super::CollectorError::RequestIo {
                stage: "tls-handshake",
                source,
            } if source.kind() == std::io::ErrorKind::TimedOut
                || source.kind() == std::io::ErrorKind::WouldBlock
        ));
        server.join().unwrap();
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn stalled_tls_handshake_does_not_delay_an_authenticated_client() {
        let root = test_root("concurrent-tls-handshakes");
        let _ = fs::remove_dir_all(&root);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        runtime.block_on(async {
            let (server_config, client_config) = test_tls_configs(&root);
            let auth_token = install_settings(&root).unwrap().auth_token;
            let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
                .await
                .unwrap();
            let port = listener.local_addr().unwrap().port();
            let transport = super::TransportListener::new(
                listener,
                server_config,
                Duration::from_millis(600),
                2,
            );
            let slots = Arc::clone(&transport.connection_slots);
            let app = router(app_state(&root));
            let server = tokio::spawn(async move { axum::serve(transport, app).await });

            let mut stalled = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
            stalled.write_all(&[0x16, 0x03, 0x03]).unwrap();
            wait_for_available_permits(&slots, 1).await;
            let started = Instant::now();
            let response = tokio::task::spawn_blocking(move || {
                attempt_tls_http(port, client_config, "127.0.0.1", Some(&auth_token)).unwrap()
            })
            .await
            .unwrap();
            let elapsed = started.elapsed();

            assert_eq!(response_status(&response), StatusCode::OK);
            assert!(
                elapsed < Duration::from_millis(300),
                "authenticated client waited for stalled handshake: {elapsed:?}"
            );
            drop(stalled);
            server.abort();
        });
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn private_ca_https_rejects_wrong_server_trust_and_unauthenticated_requests() {
        let root = test_root("mtls-rejections");
        let rogue = test_root("mtls-rejections-rogue");
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&rogue);
        let settings = install_settings(&root).unwrap();
        let layout = install(&root).unwrap();
        let rogue_settings = install_settings(&rogue).unwrap();
        let rogue_layout = install(&rogue).unwrap();
        let wrong_ca = build_client_config(&rogue_layout, &rogue_settings.credentials).unwrap();
        let valid = build_client_config(&layout, &settings.credentials).unwrap();
        let server_config = build_server_config(&layout, &settings.credentials).unwrap();
        let state = app_state(&root);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        runtime.block_on(async {
            let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
                .await
                .unwrap();
            let port = listener.local_addr().unwrap().port();
            let transport =
                super::TransportListener::new(listener, server_config, Duration::from_secs(1), 8);
            let app = router(state.clone());
            let server = tokio::spawn(async move { axum::serve(transport, app).await });

            for (config, server_name) in
                [(wrong_ca, "127.0.0.1"), (Arc::clone(&valid), "localhost")]
            {
                let result = tokio::task::spawn_blocking(move || {
                    attempt_tls_http(port, config, server_name, None)
                })
                .await
                .unwrap();
                assert!(result.is_err(), "invalid TLS peer reached HTTP: {result:?}");
            }

            let unauthenticated = Arc::clone(&valid);
            let response = tokio::task::spawn_blocking(move || {
                attempt_tls_http(port, unauthenticated, "127.0.0.1", None).unwrap()
            })
            .await
            .unwrap();
            assert_eq!(response_status(&response), StatusCode::UNAUTHORIZED);

            let collector = state.collector.lock().await;
            assert_eq!(collector.accepted_requests, 0);
            assert_eq!(collector.rejected_requests, 0);
            drop(collector);
            let response = tokio::task::spawn_blocking(move || {
                attempt_tls_http(port, valid, "127.0.0.1", Some(&settings.auth_token)).unwrap()
            })
            .await
            .unwrap();
            assert_eq!(response_status(&response), StatusCode::OK);
            server.abort();
        });
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(rogue);
    }

    #[test]
    fn token_authentication_does_not_require_a_client_identity() {
        let root = test_root("token-without-client-identity");
        let _ = fs::remove_dir_all(&root);
        let settings = install_settings(&root).unwrap();
        let layout = install(&root).unwrap();
        let no_identity = build_client_config(&layout, &settings.credentials).unwrap();
        let server_config = build_server_config(&layout, &settings.credentials).unwrap();
        let state = app_state(&root);
        let auth_token = settings.auth_token;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        runtime.block_on(async {
            let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
                .await
                .unwrap();
            let port = listener.local_addr().unwrap().port();
            let transport =
                super::TransportListener::new(listener, server_config, Duration::from_secs(1), 4);
            let app = router(state.clone());
            let server = tokio::spawn(async move { axum::serve(transport, app).await });

            let response = tokio::task::spawn_blocking(move || {
                attempt_tls_http(port, no_identity, "127.0.0.1", Some(&auth_token)).unwrap()
            })
            .await
            .unwrap();
            assert_eq!(response_status(&response), StatusCode::OK);
            assert_eq!(state.collector.lock().await.accepted_requests, 0);
            server.abort();
        });
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn transport_times_out_an_incomplete_request_body() {
        let root = test_root("request-lifetime-timeout");
        let _ = fs::remove_dir_all(&root);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let auth_token = install_settings(&root).unwrap().auth_token;

        runtime.block_on(async {
            let (server_config, client_config) = test_tls_configs(&root);
            let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
                .await
                .unwrap();
            let port = listener.local_addr().unwrap().port();
            let transport = super::TransportListener::new(
                listener,
                server_config,
                Duration::from_secs(1),
                1,
            );
            let app = super::protect_request_lifetime(
                router(app_state(&root)),
                Duration::from_millis(50),
            );
            let server = tokio::spawn(async move { axum::serve(transport, app).await });
            let response = tokio::task::spawn_blocking(move || {
                let mut stream = tls_stream(port, client_config);
                stream.write_all(
                    format!(
                        "POST /v1/logs HTTP/1.1\r\nHost: 127.0.0.1\r\n{AUTH_HEADER_NAME}: {auth_token}\r\nContent-Type: application/json\r\nContent-Length: 10\r\nConnection: close\r\n\r\n{{",
                    )
                    .as_bytes(),
                ).unwrap();
                let mut response = Vec::new();
                stream.read_to_end(&mut response).unwrap();
                response
            })
            .await
            .unwrap();
            assert_eq!(response_status(&response), StatusCode::REQUEST_TIMEOUT);
            server.abort();
        });
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ingest_preflight_rejects_before_buffering_large_partial_bodies() {
        let root = test_root("ingest-preflight-partial-body");
        let _ = fs::remove_dir_all(&root);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        runtime.block_on(async {
            let (server_config, client_config) = test_tls_configs(&root);
            let auth_token = install_settings(&root).unwrap().auth_token;
            let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
                .await
                .unwrap();
            let port = listener.local_addr().unwrap().port();
            let transport = super::TransportListener::new(
                listener,
                server_config,
                Duration::from_secs(1),
                2,
            );
            let state = app_state(&root);
            let app =
                super::protect_request_lifetime(router(state.clone()), Duration::from_secs(1));
            let server = tokio::spawn(async move { axum::serve(transport, app).await });
            let content_length = MAX_HANDOFF_BYTES + 1;

            let invalid_media = tokio::task::spawn_blocking(move || {
                let mut stream = tls_stream(port, client_config);
                let request = format!(
                    "POST /v1/logs HTTP/1.1\r\nHost: 127.0.0.1\r\n{AUTH_HEADER_NAME}: {auth_token}\r\nContent-Type: text/plain\r\nContent-Length: {content_length}\r\nConnection: close\r\n\r\n{{"
                );
                stream.write_all(request.as_bytes()).unwrap();
                let mut response = Vec::new();
                stream.read_to_end(&mut response).unwrap();
                response
            })
            .await
            .unwrap();
            assert_eq!(
                response_status(&invalid_media),
                StatusCode::UNSUPPORTED_MEDIA_TYPE
            );
            assert_eq!(state.collector.lock().await.rejected_requests, 1);
            server.abort();
        });
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn transport_admits_only_the_configured_connection_count() {
        let root = test_root("connection-saturation");
        let _ = fs::remove_dir_all(&root);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        runtime.block_on(async {
            let (server_config, client_config) = test_tls_configs(&root);
            let auth_token = install_settings(&root).unwrap().auth_token;
            let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
                .await
                .unwrap();
            let port = listener.local_addr().unwrap().port();
            let transport =
                super::TransportListener::new(listener, server_config, Duration::from_secs(2), 1);
            let slots = Arc::clone(&transport.connection_slots);
            let app =
                super::protect_request_lifetime(router(app_state(&root)), Duration::from_secs(1));
            let server = tokio::spawn(async move { axum::serve(transport, app).await });
            let first_client_config = Arc::clone(&client_config);
            let first = tokio::task::spawn_blocking(move || {
                let mut stream = tls_stream(port, first_client_config);
                stream
                    .write_all(b"GET /health HTTP/1.1\r\nHost: 127.0.0.1")
                    .unwrap();
                stream
            })
            .await
            .unwrap();
            wait_for_available_permits(&slots, 0).await;

            let saturated_read = tokio::task::spawn_blocking(move || {
                let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(1)))
                    .unwrap();
                let request =
                    "GET /health HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
                stream.write_all(request.as_bytes()).unwrap();
                let mut byte = [0_u8; 1];
                stream.read(&mut byte)
            })
            .await
            .unwrap();
            assert!(
                saturated_read.as_ref().is_ok_and(|bytes| *bytes == 0)
                    || saturated_read.as_ref().is_err_and(|error| matches!(
                        error.kind(),
                        std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
                    )),
                "saturated connection remained admitted: {saturated_read:?}"
            );

            drop(first);
            wait_for_available_permits(&slots, 1).await;
            let response = tokio::task::spawn_blocking(move || {
                let mut stream = tls_stream(port, client_config);
                let request = format!(
                    "GET /health HTTP/1.1\r\nHost: 127.0.0.1\r\n{AUTH_HEADER_NAME}: {auth_token}\r\nConnection: close\r\n\r\n"
                );
                stream.write_all(request.as_bytes()).unwrap();
                let mut response = Vec::new();
                stream.read_to_end(&mut response).unwrap();
                response
            })
            .await
            .unwrap();
            assert_eq!(response_status(&response), StatusCode::OK);
            server.abort();
        });
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn https_receiver_enforces_auth_media_type_and_transport_body_bound() {
        let root = test_root("http-contract");
        let _ = fs::remove_dir_all(&root);
        let layout = install(&root).unwrap();
        let guard = ConfigMutationGuard::acquire(&layout).unwrap();
        let mut config = load(&layout.config).unwrap();
        config.collection.max_batch_bytes = 2 * 1024 * 1024;
        save(&guard, &config).unwrap();
        drop(guard);
        let app = router(app_state(&root));
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        runtime.block_on(async {
            let (server_config, client_config) = test_tls_configs(&root);
            let auth_token = install_settings(&root).unwrap().auth_token;
            let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
                .await
                .unwrap();
            let port = listener.local_addr().unwrap().port();
            let transport =
                super::TransportListener::new(listener, server_config, Duration::from_secs(1), 4);
            let server = tokio::spawn(async move {
                axum::serve(transport, app).await.unwrap();
            });
            assert_eq!(
                post(
                    port,
                    Arc::clone(&client_config),
                    auth_token.clone(),
                    None,
                    b"{}".to_vec(),
                )
                .await,
                StatusCode::UNSUPPORTED_MEDIA_TYPE
            );
            assert_eq!(
                post(
                    port,
                    Arc::clone(&client_config),
                    auth_token.clone(),
                    Some("text/plain"),
                    b"{}".to_vec(),
                )
                .await,
                StatusCode::UNSUPPORTED_MEDIA_TYPE
            );
            assert_eq!(
                post(
                    port,
                    Arc::clone(&client_config),
                    auth_token.clone(),
                    Some("application/json"),
                    b"{".to_vec(),
                )
                .await,
                StatusCode::UNPROCESSABLE_ENTITY
            );

            let exact = padded_json(
                br#"{"resourceLogs":[]}"#,
                usize::try_from(MAX_HANDOFF_BYTES).unwrap(),
            );
            assert_eq!(
                post(
                    port,
                    Arc::clone(&client_config),
                    auth_token.clone(),
                    Some("application/json; charset=utf-8"),
                    exact,
                )
                .await,
                StatusCode::OK
            );
            assert_eq!(
                post(
                    port,
                    client_config,
                    auth_token,
                    Some("application/json"),
                    vec![b' '; usize::try_from(MAX_HANDOFF_BYTES + 1).unwrap()],
                )
                .await,
                StatusCode::PAYLOAD_TOO_LARGE
            );
            server.abort();
        });
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn config_admission_and_record_count_accept_exact_bounds_only() {
        let root = test_root("admission-bounds");
        let _ = fs::remove_dir_all(&root);
        let state = collector_state(&root);
        let config = load(&state.layout.config).unwrap();
        let exact_bytes = usize::try_from(config.collection.max_batch_bytes).unwrap();
        assert!(admit_request(&state, exact_bytes).unwrap().is_some());
        assert!(matches!(
            admit_request(&state, exact_bytes + 1),
            Err(IngestError::Policy)
        ));

        let exact = parse_otlp_http_json(&otlp_start_records(500), "codex-test", None, 1, 0)
            .unwrap()
            .0;
        let over = parse_otlp_http_json(&otlp_start_records(501), "codex-test", None, 1, 0)
            .unwrap()
            .0;
        let mut record_config = config;
        record_config.collection.max_batch_records = 500;
        assert!(enforce_batch_policy(&exact, &record_config).is_ok());
        assert!(matches!(
            enforce_batch_policy(&over, &record_config),
            Err(IngestError::Policy)
        ));

        let exact_dispositions =
            parse_otlp_http_json(&otlp_disposition_records(500), "codex-test", None, 1, 0)
                .unwrap()
                .0;
        let over_dispositions =
            parse_otlp_http_json(&otlp_disposition_records(501), "codex-test", None, 1, 0)
                .unwrap()
                .0;
        assert!(enforce_batch_policy(&exact_dispositions, &record_config).is_ok());
        assert!(matches!(
            enforce_batch_policy(&over_dispositions, &record_config),
            Err(IngestError::Policy)
        ));

        let mut mixed =
            parse_otlp_http_json(&otlp_disposition_records(499), "codex-test", None, 1, 0)
                .unwrap()
                .0;
        mixed.items.push(exact.items.into_iter().next().unwrap());
        assert!(enforce_batch_policy(&mixed, &record_config).is_ok());
        mixed.items.push(over.items.into_iter().next().unwrap());
        assert!(matches!(
            enforce_batch_policy(&mixed, &record_config),
            Err(IngestError::Policy)
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn notify_projects_before_io_and_uses_only_authenticated_https() {
        let missing = test_root("notify-missing");
        let _ = fs::remove_dir_all(&missing);
        assert_eq!(submit_notify(&missing, b"{}"), NotifyOutcome::Rejected);

        let refused = test_root("notify-refused");
        let _ = fs::remove_dir_all(&refused);
        let refused_port = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        configure_port(&refused, refused_port);
        assert_eq!(
            submit_notify(&refused, &raw_notify("thread-1", "turn-1")),
            NotifyOutcome::Unavailable
        );

        let oversized = vec![b'x'; 64 * 1024 + 1];
        assert_eq!(submit_notify(&refused, &oversized), NotifyOutcome::Rejected);

        let rogue = test_root("notify-rogue-plaintext");
        let _ = fs::remove_dir_all(&rogue);
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        configure_port(&rogue, port);
        let (captured_tx, captured_rx) = std::sync::mpsc::channel();
        let rogue_server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            let mut captured = [0_u8; 4096];
            let bytes = stream.read(&mut captured).unwrap_or_default();
            captured_tx.send(captured[..bytes].to_vec()).unwrap();
        });
        assert_eq!(
            submit_notify(&rogue, &raw_notify("thread-rogue", "turn-rogue")),
            NotifyOutcome::Unavailable
        );
        rogue_server.join().unwrap();
        let captured = captured_rx.recv().unwrap();
        for forbidden in [
            b"HTTP/1.1".as_slice(),
            b"RAW_PATH_SECRET".as_slice(),
            b"RAW_INPUT_SECRET".as_slice(),
            b"RAW_OUTPUT_SECRET".as_slice(),
            b"x-agent-observability-token".as_slice(),
        ] {
            assert!(
                !captured
                    .windows(forbidden.len())
                    .any(|part| part == forbidden)
            );
        }

        let accepted = test_root("notify-accepted");
        let _ = fs::remove_dir_all(&accepted);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let (server_config, _) = test_tls_configs(&accepted);
            let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
                .await
                .unwrap();
            configure_port(&accepted, listener.local_addr().unwrap().port());
            let transport =
                super::TransportListener::new(listener, server_config, Duration::from_secs(1), 2);
            let app = router(app_state(&accepted));
            let server = tokio::spawn(async move { axum::serve(transport, app).await });
            tokio::task::yield_now().await;
            let root = accepted.clone();
            let outcome = tokio::task::spawn_blocking(move || {
                submit_notify(&root, &raw_notify("thread-ok", "turn-ok"))
            })
            .await
            .unwrap();
            assert_eq!(outcome, NotifyOutcome::Accepted);
            server.abort();
        });

        for root in [refused, rogue, accepted] {
            let _ = fs::remove_dir_all(root);
        }
    }

    #[cfg(unix)]
    #[test]
    fn private_turn_details_are_default_off_bounded_private_and_idempotent() {
        use std::os::unix::fs::PermissionsExt;

        let root = test_root("private-turn-detail");
        let _ = fs::remove_dir_all(&root);
        let layout = install(&root).unwrap();
        let first = raw_notify("thread-private", "turn-private");
        let (_, first_detail) = project_notify_with_private_detail(&first).unwrap();
        let turn_id = first_detail.turn_id().to_owned();
        assert!(read_private_turn_detail(&root, &turn_id).is_err());

        set_private_turn_details(&root, true);
        let config = load(&layout.config).unwrap();
        persist_private_turn_detail(&layout, &first_detail, &config).unwrap();

        let directory = layout.state.join(super::PRIVATE_TURN_DETAIL_DIRECTORY);
        let path = private_turn_detail_path(&directory, &turn_id).unwrap();
        assert_eq!(
            fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let first_json: serde_json::Value =
            serde_json::from_slice(&read_private_turn_detail(&root, &turn_id).unwrap()).unwrap();
        assert_eq!(first_json["turnId"], turn_id);
        assert_eq!(first_json["cwd"], "/RAW_PATH_SECRET");
        assert_eq!(first_json["inputMessages"][0], "RAW_INPUT_SECRET");
        assert_eq!(first_json["lastAssistantMessage"], "RAW_OUTPUT_SECRET");

        let replacement = serde_json::to_vec(&serde_json::json!({
            "type": "agent-turn-complete",
            "thread-id": "thread-private",
            "turn-id": "turn-private",
            "cwd": "/second/project",
            "input-messages": ["SECOND_INPUT_SECRET"],
            "last-assistant-message": null,
        }))
        .unwrap();
        let (_, replacement_detail) = project_notify_with_private_detail(&replacement).unwrap();
        persist_private_turn_detail(&layout, &replacement_detail, &config).unwrap();
        let replaced: serde_json::Value =
            serde_json::from_slice(&read_private_turn_detail(&root, &turn_id).unwrap()).unwrap();
        assert_eq!(replaced["cwd"], "/second/project");
        assert_eq!(replaced["inputMessages"][0], "SECOND_INPUT_SECRET");
        assert!(replaced["lastAssistantMessage"].is_null());

        set_private_turn_details(&root, false);
        assert!(read_private_turn_detail(&root, &turn_id).is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn private_turn_detail_pruning_keeps_the_replacement_inside_the_file_bound() {
        let root = test_root("private-turn-detail-prune");
        let _ = fs::remove_dir_all(&root);
        let layout = install(&root).unwrap();
        set_private_turn_details(&root, true);
        let config = load(&layout.config).unwrap();
        let directory = layout.state.join(super::PRIVATE_TURN_DETAIL_DIRECTORY);
        let mut paths = Vec::new();

        for index in 0..3 {
            let payload = raw_notify("thread-private", &format!("turn-private-{index}"));
            let (_, detail) = project_notify_with_private_detail(&payload).unwrap();
            persist_private_turn_detail(&layout, &detail, &config).unwrap();
            paths.push(private_turn_detail_path(&directory, detail.turn_id()).unwrap());
        }

        prune_private_turn_details_with_limit(
            &directory,
            &paths[0],
            &config,
            std::time::SystemTime::now(),
            2,
        )
        .unwrap();

        let retained = fs::read_dir(&directory)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|value| value == "json")
            })
            .count();
        assert_eq!(retained, 2);
        assert!(paths[0].is_file());

        prune_private_turn_details_with_limit(
            &directory,
            &paths[0],
            &config,
            std::time::SystemTime::now() + Duration::from_hours(31 * 24),
            2,
        )
        .unwrap();
        let retained_after_age_prune = fs::read_dir(&directory)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|value| value == "json")
            })
            .count();
        assert_eq!(retained_after_age_prune, 1);
        assert!(paths[0].is_file());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn private_turn_detail_rejects_traversal_symlinks_and_oversize() {
        use std::os::unix::fs::symlink;

        let root = test_root("private-turn-detail-boundaries");
        let _ = fs::remove_dir_all(&root);
        let layout = install(&root).unwrap();
        set_private_turn_details(&root, true);
        let config = load(&layout.config).unwrap();
        assert!(read_private_turn_detail(&root, "../../config.json").is_err());

        let payload = raw_notify("thread-private", "turn-private");
        let (_, detail) = project_notify_with_private_detail(&payload).unwrap();
        let directory = layout.state.join(super::PRIVATE_TURN_DETAIL_DIRECTORY);
        super::ensure_private_directory_tree(&layout.state, &directory).unwrap();
        let path = private_turn_detail_path(&directory, detail.turn_id()).unwrap();
        let target = layout.state.join("symlink-target");
        write_private_test_file(&target, b"target");
        symlink(&target, &path).unwrap();
        assert!(persist_private_turn_detail(&layout, &detail, &config).is_err());

        let oversized_message = "x".repeat(super::MAX_PRIVATE_TURN_DETAIL_BYTES);
        let oversized = serde_json::to_vec(&serde_json::json!({
            "type": "agent-turn-complete",
            "thread-id": "thread-private",
            "turn-id": "turn-private",
            "cwd": "/project",
            "input-messages": [oversized_message],
            "last-assistant-message": null,
        }))
        .unwrap();
        assert!(project_notify_with_private_detail(&oversized).is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn authenticated_notify_endpoint_rejects_raw_codex_payload() {
        let root = test_root("notify-endpoint-projected-only");
        let _ = fs::remove_dir_all(&root);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let (server_config, _) = test_tls_configs(&root);
            let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
                .await
                .unwrap();
            configure_port(&root, listener.local_addr().unwrap().port());
            let transport =
                super::TransportListener::new(listener, server_config, Duration::from_secs(1), 2);
            let state = app_state(&root);
            let app = router(state.clone());
            let server = tokio::spawn(async move { axum::serve(transport, app).await });

            let request_root = root.clone();
            let response = tokio::task::spawn_blocking(move || {
                authenticated_request(
                    &request_root,
                    "POST",
                    "/v1/notify",
                    Some(&raw_notify("RAW_THREAD_SECRET", "RAW_TURN_SECRET")),
                    Duration::from_millis(250),
                    Duration::from_secs(1),
                )
                .unwrap()
            })
            .await
            .unwrap();
            assert_eq!(response.status, StatusCode::UNPROCESSABLE_ENTITY.as_u16());
            assert_eq!(
                state.collector.lock().await.store.record_count().unwrap(),
                0
            );
            server.abort();
            let _ = server.await;
        });
        assert_tree_excludes(
            &root,
            &[
                b"RAW_THREAD_SECRET",
                b"RAW_TURN_SECRET",
                b"RAW_PATH_SECRET",
                b"RAW_INPUT_SECRET",
                b"RAW_OUTPUT_SECRET",
            ],
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn foreground_deadline_is_absolute_against_slow_drip_responses() {
        let root = test_root("foreground-absolute-deadline");
        let _ = fs::remove_dir_all(&root);
        let settings = install_settings(&root).unwrap();
        let layout = install(&root).unwrap();
        let server_config = build_server_config(&layout, &settings.credentials).unwrap();
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        configure_port(&root, port);
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            let connection = rustls::ServerConnection::new(server_config).unwrap();
            let mut tls = rustls::StreamOwned::new(connection, stream);
            let mut request = [0_u8; 4096];
            let _ = tls.read(&mut request);
            for byte in b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}" {
                if tls.write_all(&[*byte]).is_err() || tls.flush().is_err() {
                    break;
                }
                thread::sleep(Duration::from_millis(25));
            }
        });

        let started = Instant::now();
        let result = super::authenticated_request(
            &root,
            "GET",
            "/health",
            None,
            Duration::from_millis(50),
            Duration::from_millis(120),
        );
        let elapsed = started.elapsed();
        let Err(error) = result else {
            panic!("slow-drip response escaped deadline");
        };
        assert!(matches!(
            &error,
            super::CollectorError::RequestIo { stage: "response-read", source }
                if source.kind() == std::io::ErrorKind::TimedOut
        ));
        assert!(!error.to_string().contains("os error 35"));
        assert!(
            elapsed < Duration::from_millis(400),
            "foreground request exceeded absolute deadline: {elapsed:?}"
        );
        server.join().unwrap();
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn collector_settings_are_private_and_idempotent() {
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!(
            "agent-observability-collector-settings-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let first = install_settings(&root).unwrap();
        let second = install_settings(&root).unwrap();
        assert_eq!(first, second);
        assert_eq!(load_settings(&root).unwrap(), first);
        assert_eq!(first.generation.len(), 64);
        assert_eq!(first.transport, super::COLLECTOR_TRANSPORT);
        let path = root.join("runtime/collector.json");
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let generation = root
            .join("runtime")
            .join(super::TLS_DIRECTORY)
            .join(&first.generation);
        assert_eq!(
            fs::metadata(&generation).unwrap().permissions().mode() & 0o777,
            0o700
        );
        let mut names = fs::read_dir(&generation)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect::<Vec<_>>();
        names.sort();
        assert_eq!(
            names,
            [
                super::CA_CERTIFICATE_NAME,
                super::SERVER_CERTIFICATE_NAME,
                super::SERVER_PRIVATE_KEY_NAME,
            ]
        );
        for name in &names {
            assert_eq!(
                fs::metadata(generation.join(name))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        let ca = fs::read_to_string(generation.join(super::CA_CERTIFICATE_NAME)).unwrap();
        assert!(ca.contains("BEGIN CERTIFICATE"));
        assert!(!ca.contains("PRIVATE KEY"));
        let remaining = first.credentials.expires_at_unix_ms - super::current_unix_ms().unwrap();
        let minimum = u64::try_from(Duration::from_hours(8_736).as_millis()).unwrap();
        let maximum = u64::try_from(Duration::from_hours(8_784).as_millis()).unwrap();
        assert!(remaining > minimum);
        assert!(remaining <= maximum);
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn settings_and_credentials_require_private_bounded_regular_files_and_directories() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let oversized_settings = test_root("oversized-settings");
        let _ = fs::remove_dir_all(&oversized_settings);
        let _ = install_settings(&oversized_settings).unwrap();
        let oversized_settings_path = settings_path(&install(&oversized_settings).unwrap());
        write_private_test_file(
            &oversized_settings_path,
            &vec![b'x'; usize::try_from(super::MAX_SETTINGS_BYTES + 1).unwrap()],
        );
        assert!(load_settings(&oversized_settings).is_err());

        let oversized_credential = test_root("oversized-credential");
        let _ = fs::remove_dir_all(&oversized_credential);
        let settings = install_settings(&oversized_credential).unwrap();
        let layout = install(&oversized_credential).unwrap();
        write_private_test_file(
            &layout
                .runtime
                .join(&settings.credentials.server_private_key),
            &vec![b'x'; usize::try_from(super::MAX_CREDENTIAL_BYTES + 1).unwrap()],
        );
        assert!(load_settings(&oversized_credential).is_err());

        let broad_directory = test_root("broad-credential-directory");
        let _ = fs::remove_dir_all(&broad_directory);
        let settings = install_settings(&broad_directory).unwrap();
        let layout = install(&broad_directory).unwrap();
        let generation = layout
            .runtime
            .join(super::TLS_DIRECTORY)
            .join(&settings.generation);
        fs::set_permissions(&generation, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(load_settings(&broad_directory).is_err());

        let symlinked_credential = test_root("symlinked-credential");
        let _ = fs::remove_dir_all(&symlinked_credential);
        let settings = install_settings(&symlinked_credential).unwrap();
        let layout = install(&symlinked_credential).unwrap();
        let server_key = layout
            .runtime
            .join(&settings.credentials.server_private_key);
        fs::remove_file(&server_key).unwrap();
        symlink(
            layout.runtime.join(&settings.credentials.ca_certificate),
            &server_key,
        )
        .unwrap();
        assert!(load_settings(&symlinked_credential).is_err());

        let broad_settings = test_root("broad-settings");
        let _ = fs::remove_dir_all(&broad_settings);
        let _ = install_settings(&broad_settings).unwrap();
        let path = settings_path(&install(&broad_settings).unwrap());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(load_settings(&broad_settings).is_err());

        for root in [
            oversized_settings,
            oversized_credential,
            broad_directory,
            symlinked_credential,
            broad_settings,
        ] {
            let _ = fs::remove_dir_all(root);
        }
    }

    #[cfg(unix)]
    fn write_legacy_v2_mtls_settings(root: &Path) -> String {
        let current = install_settings(root).unwrap();
        let layout = install(root).unwrap();
        let generation_dir = layout
            .runtime
            .join(super::TLS_DIRECTORY)
            .join(&current.generation);
        let client_certificate = generation_dir.join(super::LEGACY_CLIENT_CERTIFICATE_NAME);
        let client_private_key = generation_dir.join(super::LEGACY_CLIENT_PRIVATE_KEY_NAME);
        write_private_test_file(
            &client_certificate,
            &fs::read(layout.runtime.join(&current.credentials.server_certificate)).unwrap(),
        );
        write_private_test_file(
            &client_private_key,
            &fs::read(layout.runtime.join(&current.credentials.server_private_key)).unwrap(),
        );
        let prefix = format!("{}/{}/", super::TLS_DIRECTORY, current.generation);
        let legacy = serde_json::json!({
            "schema_version": "local_collector.v2",
            "generation": current.generation,
            "port": current.port,
            "transport": "mtls",
            "credentials": {
                "ca_certificate": format!("{prefix}{}", super::CA_CERTIFICATE_NAME),
                "server_certificate": format!("{prefix}{}", super::SERVER_CERTIFICATE_NAME),
                "server_private_key": format!("{prefix}{}", super::SERVER_PRIVATE_KEY_NAME),
                "client_certificate": format!("{prefix}{}", super::LEGACY_CLIENT_CERTIFICATE_NAME),
                "client_private_key": format!("{prefix}{}", super::LEGACY_CLIENT_PRIVATE_KEY_NAME),
                "expires_at_unix_ms": current.credentials.expires_at_unix_ms,
            }
        });
        write_private_test_file(
            &settings_path(&layout),
            &serde_json::to_vec(&legacy).unwrap(),
        );
        legacy["generation"].as_str().unwrap().to_owned()
    }

    #[cfg(unix)]
    #[test]
    fn install_migrates_exact_v1_and_v2_mtls_and_renews_only_current_expired_v3() {
        let legacy_root = test_root("legacy-migration");
        let _ = fs::remove_dir_all(&legacy_root);
        let legacy_layout = install(&legacy_root).unwrap();
        write_private_test_file(
            &settings_path(&legacy_layout),
            br#"{"schema_version":"local_collector.v1","port":4318,"token":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","source_generation":"codex-otel-v1"}"#,
        );
        let migrated = install_settings(&legacy_root).unwrap();
        assert_eq!(migrated.schema_version, super::COLLECTOR_SETTINGS_VERSION);
        assert_eq!(migrated.transport, super::COLLECTOR_TRANSPORT);
        assert_ne!(migrated.generation, super::SOURCE_GENERATION);

        let legacy_v2_root = test_root("legacy-v2-mtls-migration");
        let _ = fs::remove_dir_all(&legacy_v2_root);
        let legacy_v2_generation = write_legacy_v2_mtls_settings(&legacy_v2_root);
        let migrated_v2 = install_settings(&legacy_v2_root).unwrap();
        assert_eq!(
            migrated_v2.schema_version,
            super::COLLECTOR_SETTINGS_VERSION
        );
        assert_eq!(migrated_v2.transport, super::COLLECTOR_TRANSPORT);
        assert_eq!(migrated_v2.auth_token.len(), 64);
        assert_ne!(migrated_v2.generation, legacy_v2_generation);
        assert!(
            legacy_v2_root
                .join("runtime")
                .join(super::TLS_DIRECTORY)
                .join(&legacy_v2_generation)
                .exists()
        );
        assert!(super::settings_migration_path(&install(&legacy_v2_root).unwrap()).exists());
        super::commit_settings_migration(&legacy_v2_root).unwrap();
        assert!(
            !legacy_v2_root
                .join("runtime")
                .join(super::TLS_DIRECTORY)
                .join(legacy_v2_generation)
                .exists()
        );
        assert_eq!(load_settings(&legacy_v2_root).unwrap(), migrated_v2);

        let expired_root = test_root("expired-renewal");
        let _ = fs::remove_dir_all(&expired_root);
        let mut expired = install_settings(&expired_root).unwrap();
        let expired_generation = expired.generation.clone();
        expired.credentials.expires_at_unix_ms = 1;
        let expired_layout = install(&expired_root).unwrap();
        write_private_json(&settings_path(&expired_layout), &expired).unwrap();
        assert!(load_settings(&expired_root).is_err());
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        assert!(
            runtime
                .block_on(super::serve(expired.options(&expired_root)))
                .is_err()
        );
        let renewed = install_settings(&expired_root).unwrap();
        assert_ne!(renewed.generation, expired_generation);
        assert_eq!(renewed.schema_version, super::COLLECTOR_SETTINGS_VERSION);
        assert!(renewed.credentials.expires_at_unix_ms > super::current_unix_ms().unwrap());
        assert_eq!(super::SOURCE_GENERATION, "codex-otel-v1");

        for (name, bytes) in [
            ("corrupt", b"{".as_slice()),
            (
                "partial-v1",
                br#"{"schema_version":"local_collector.v1","port":4318}"#,
            ),
            (
                "unknown-v4",
                br#"{"schema_version":"local_collector.v4","port":4318,"token":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","source_generation":"codex-otel-v1"}"#,
            ),
        ] {
            let root = test_root(name);
            let _ = fs::remove_dir_all(&root);
            let layout = install(&root).unwrap();
            write_private_test_file(&settings_path(&layout), bytes);
            assert!(install_settings(&root).is_err(), "accepted {name}");
            let _ = fs::remove_dir_all(root);
        }

        let _ = fs::remove_dir_all(legacy_root);
        let _ = fs::remove_dir_all(legacy_v2_root);
        let _ = fs::remove_dir_all(expired_root);
    }

    #[cfg(unix)]
    #[test]
    fn failed_v2_migration_restores_exact_settings_and_credentials() {
        let root = test_root("legacy-v2-mtls-rollback");
        let _ = fs::remove_dir_all(&root);
        let previous_generation = write_legacy_v2_mtls_settings(&root);
        let layout = install(&root).unwrap();
        let previous =
            super::read_private_snapshot(&super::settings_path(&layout), super::MAX_SETTINGS_BYTES)
                .unwrap();
        let replacement = install_settings(&root).unwrap();

        super::rollback_settings_migration(&root).unwrap();

        assert_eq!(
            super::read_private_snapshot(
                &super::settings_path(&layout),
                super::MAX_SETTINGS_BYTES,
            )
            .unwrap(),
            previous
        );
        assert!(
            layout
                .runtime
                .join(super::TLS_DIRECTORY)
                .join(previous_generation)
                .exists()
        );
        assert!(
            !layout
                .runtime
                .join(super::TLS_DIRECTORY)
                .join(replacement.generation)
                .exists()
        );
        assert!(!super::settings_migration_path(&layout).exists());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn interrupted_migration_before_settings_publish_recovers_and_retries() {
        let root = test_root("legacy-v2-mtls-prepublish-crash");
        let _ = fs::remove_dir_all(&root);
        let previous_generation = write_legacy_v2_mtls_settings(&root);
        let layout = install(&root).unwrap();
        let previous =
            super::read_private_snapshot(&super::settings_path(&layout), super::MAX_SETTINGS_BYTES)
                .unwrap();
        let abandoned = super::generate_settings(&layout).unwrap();
        let migration = super::SettingsMigrationV1 {
            schema_version: super::SETTINGS_MIGRATION_VERSION.into(),
            phase: super::SettingsMigrationPhase::Pending,
            previous_settings: previous.bytes.clone(),
            previous_mode: previous.mode,
            previous_generation: Some(previous_generation.clone()),
            replacement_generation: abandoned.generation.clone(),
        };
        super::write_private_json(&super::settings_migration_path(&layout), &migration).unwrap();

        let resumed = install_settings(&root).unwrap();

        assert_ne!(resumed.generation, abandoned.generation);
        assert!(super::settings_migration_path(&layout).exists());
        assert!(
            layout
                .runtime
                .join(super::TLS_DIRECTORY)
                .join(previous_generation)
                .exists()
        );
        assert!(
            !layout
                .runtime
                .join(super::TLS_DIRECTORY)
                .join(abandoned.generation)
                .exists()
        );
        super::rollback_settings_migration(&root).unwrap();
        assert_eq!(
            super::read_private_snapshot(
                &super::settings_path(&layout),
                super::MAX_SETTINGS_BYTES,
            )
            .unwrap(),
            previous
        );
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn committed_migration_resumes_cleanup_without_rolling_back() {
        let root = test_root("legacy-v2-mtls-published-crash");
        let _ = fs::remove_dir_all(&root);
        let previous_generation = write_legacy_v2_mtls_settings(&root);
        let replacement = install_settings(&root).unwrap();

        let layout = install(&root).unwrap();
        let path = super::settings_migration_path(&layout);
        let snapshot =
            super::read_private_snapshot(&path, super::MAX_SETTINGS_MIGRATION_BYTES).unwrap();
        let mut migration = super::load_settings_migration(&layout).unwrap().unwrap();
        migration.phase = super::SettingsMigrationPhase::IntegrationCommitted;
        super::write_private_json_if_unchanged(
            &path,
            &migration,
            &snapshot,
            super::MAX_SETTINGS_MIGRATION_BYTES,
        )
        .unwrap();

        assert_eq!(install_settings(&root).unwrap(), replacement);
        assert_eq!(load_settings(&root).unwrap(), replacement);
        assert!(!path.exists());
        assert!(
            !layout
                .runtime
                .join(super::TLS_DIRECTORY)
                .join(&previous_generation)
                .exists()
        );

        let previous_generation = write_legacy_v2_mtls_settings(&root);
        let replacement = install_settings(&root).unwrap();
        let snapshot =
            super::read_private_snapshot(&path, super::MAX_SETTINGS_MIGRATION_BYTES).unwrap();
        let mut migration = super::load_settings_migration(&layout).unwrap().unwrap();
        migration.phase = super::SettingsMigrationPhase::IntegrationCommitted;
        super::write_private_json_if_unchanged(
            &path,
            &migration,
            &snapshot,
            super::MAX_SETTINGS_MIGRATION_BYTES,
        )
        .unwrap();
        super::cleanup_credential_generation(&layout, &previous_generation).unwrap();
        super::rollback_settings_migration(&root).unwrap();

        assert_eq!(load_settings(&root).unwrap(), replacement);
        assert!(!path.exists());
        assert!(
            !layout
                .runtime
                .join(super::TLS_DIRECTORY)
                .join(previous_generation)
                .exists()
        );
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn partial_rollback_cleanup_resumes_from_restored_settings() {
        let root = test_root("legacy-v2-mtls-partial-rollback");
        let _ = fs::remove_dir_all(&root);
        write_legacy_v2_mtls_settings(&root);
        let layout = install(&root).unwrap();
        let previous =
            super::read_private_snapshot(&super::settings_path(&layout), super::MAX_SETTINGS_BYTES)
                .unwrap();
        let replacement = install_settings(&root).unwrap();
        let current =
            super::read_private_snapshot(&super::settings_path(&layout), super::MAX_SETTINGS_BYTES)
                .unwrap();
        super::write_private_bytes_if_unchanged(
            &super::settings_path(&layout),
            &previous,
            &current,
        )
        .unwrap();

        super::rollback_settings_migration(&root).unwrap();

        assert!(!super::settings_migration_path(&layout).exists());
        assert!(
            !layout
                .runtime
                .join(super::TLS_DIRECTORY)
                .join(replacement.generation)
                .exists()
        );
        assert_eq!(
            super::read_private_snapshot(
                &super::settings_path(&layout),
                super::MAX_SETTINGS_BYTES,
            )
            .unwrap(),
            previous
        );
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn settings_replacement_detects_exact_content_or_mode_conflicts_and_cleans_temporary_files() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let root = test_root("settings-conflict");
        let _ = fs::remove_dir_all(&root);
        let original = install_settings(&root).unwrap();
        let layout = install(&root).unwrap();
        let path = settings_path(&layout);
        let expected = read_private_snapshot(&path, super::MAX_SETTINGS_BYTES).unwrap();
        let mut concurrent = original.clone();
        concurrent.port = concurrent.port.saturating_add(1).max(1);
        write_private_json(&path, &concurrent).unwrap();
        let mut replacement = original.clone();
        replacement.port = replacement.port.saturating_add(2).max(1);
        assert!(
            write_private_json_if_unchanged(
                &path,
                &replacement,
                &expected,
                super::MAX_SETTINGS_BYTES,
            )
            .is_err()
        );
        assert_eq!(load_settings(&root).unwrap(), concurrent);

        let expected = read_private_snapshot(&path, super::MAX_SETTINGS_BYTES).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();
        assert!(
            write_private_json_if_unchanged(
                &path,
                &replacement,
                &expected,
                super::MAX_SETTINGS_BYTES,
            )
            .is_err()
        );
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(load_settings(&root).unwrap(), concurrent);
        assert!(fs::read_dir(&layout.runtime).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".collector.json.tmp.")
        }));

        let symlink_root = test_root("settings-symlink");
        let _ = fs::remove_dir_all(&symlink_root);
        let layout = install(&symlink_root).unwrap();
        let target = layout.runtime.join("missing-target.json");
        symlink(&target, settings_path(&layout)).unwrap();
        assert!(install_settings(&symlink_root).is_err());

        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(symlink_root);
    }

    #[test]
    fn explicit_port_recovery_preserves_settings_scalars() {
        let root = test_root("explicit-port-recovery");
        let _ = fs::remove_dir_all(&root);
        let original = install_settings(&root).unwrap();
        let occupied = TcpListener::bind((Ipv4Addr::LOCALHOST, original.port)).unwrap();

        let recovered = recover_occupied_persisted_port(&root, &original).unwrap();

        assert_ne!(recovered.port, original.port);
        assert_eq!(recovered.schema_version, original.schema_version);
        assert_eq!(recovered.generation, original.generation);
        assert_eq!(recovered.credentials, original.credentials);
        assert_eq!(load_settings(&root).unwrap(), recovered);
        drop(occupied);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn explicit_port_recovery_does_not_rotate_a_free_port() {
        let root = test_root("free-port-no-recovery");
        let _ = fs::remove_dir_all(&root);
        let original = install_settings(&root).unwrap();
        let path = settings_path(&install(&root).unwrap());
        let original_bytes = fs::read(&path).unwrap();

        assert_eq!(
            recover_occupied_persisted_port(&root, &original).unwrap(),
            original
        );
        assert_eq!(fs::read(path).unwrap(), original_bytes);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn explicit_port_recovery_failure_preserves_settings() {
        use std::os::unix::fs::PermissionsExt;

        let root = test_root("port-recovery-failure");
        let _ = fs::remove_dir_all(&root);
        let original = install_settings(&root).unwrap();
        let layout = install(&root).unwrap();
        let path = settings_path(&layout);
        let original_bytes = fs::read(&path).unwrap();
        let occupied = TcpListener::bind((Ipv4Addr::LOCALHOST, original.port)).unwrap();
        fs::set_permissions(&layout.runtime, fs::Permissions::from_mode(0o500)).unwrap();

        assert!(recover_occupied_persisted_port(&root, &original).is_err());
        assert_eq!(fs::read(&path).unwrap(), original_bytes);
        assert_eq!(load_settings(&root).unwrap(), original);

        fs::set_permissions(&layout.runtime, fs::Permissions::from_mode(0o700)).unwrap();
        drop(occupied);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn occupied_persisted_port_preserves_settings_and_returns_address_in_use() {
        let root = test_root("occupied-persisted-port");
        let _ = fs::remove_dir_all(&root);
        let original = install_settings(&root).unwrap();
        let settings_path = settings_path(&install(&root).unwrap());
        let original_bytes = fs::read(&settings_path).unwrap();
        let occupied = TcpListener::bind((Ipv4Addr::LOCALHOST, original.port)).unwrap();
        let options = original.options(&root);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        let error = runtime.block_on(super::serve(options)).unwrap_err();
        assert!(matches!(
            error,
            super::CollectorError::Io(ref error)
                if error.kind() == std::io::ErrorKind::AddrInUse
        ));
        assert_eq!(fs::read(&settings_path).unwrap(), original_bytes);
        drop(occupied);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn non_address_in_use_bind_failure_does_not_rotate_settings() {
        let root = test_root("bind-failure-no-rotation");
        let _ = fs::remove_dir_all(&root);
        let settings = install_settings(&root).unwrap();

        let error = super::bind_persisted_port(Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "injected bind failure",
        )))
        .unwrap_err();
        assert!(matches!(
            error,
            super::CollectorError::Io(ref error)
                if error.kind() == std::io::ErrorKind::PermissionDenied
        ));
        assert_eq!(load_settings(&root).unwrap(), settings);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn otlp_batch_commits_and_refreshes_private_report() {
        let root = std::env::temp_dir().join(format!(
            "agent-observability-collector-ingest-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let layout = install(&root).unwrap();
        let config = load(&layout.config).unwrap();
        let store = open_store_for_test(&layout, &config);
        let mut state = CollectorState {
            layout: layout.clone(),
            store,
            source_generation: "codex-test".into(),
            last_cursor: None,
            request_correlation: OtlpRequestCorrelationState::default(),
            accepted_requests: 0,
            rejected_requests: 0,
            suppressed_requests: 0,
            last_ingest_unix_ms: None,
            report_dirty: false,
            report_degraded: false,
            report_refresh_failures: 0,
            report_failure: None,
        };
        let body = br#"{"resourceLogs":[{"scopeLogs":[{"logRecords":[
          {"timeUnixNano":"1787875200000000000","attributes":[
            {"key":"event.name","value":{"stringValue":"codex.conversation_starts"}},
            {"key":"conversation.id","value":{"stringValue":"conversation-1"}},
            {"key":"model","value":{"stringValue":"gpt-5.6-sol"}}
          ]},
          {"timeUnixNano":"1787875200100000000","attributes":[
            {"key":"event.name","value":{"stringValue":"codex.sse_event"}},
            {"key":"conversation.id","value":{"stringValue":"conversation-1"}},
            {"key":"event.kind","value":{"stringValue":"response.completed"}},
            {"key":"model","value":{"stringValue":"gpt-5.6-sol"}},
            {"key":"input_token_count","value":{"stringValue":"100"}},
            {"key":"output_token_count","value":{"stringValue":"25"}},
            {"key":"tool_token_count","value":{"stringValue":"125"}}
          ]}
        ]}]}]}"#;

        ingest_locked(&mut state, body).unwrap();
        ingest_notify_locked(&mut state, &projected_notify("conversation-1", "turn-1")).unwrap();
        refresh_report_from_root(&layout.root).unwrap();

        assert_eq!(state.last_cursor.as_deref(), Some("3"));
        assert_eq!(state.store.counts().unwrap().0, 2);
        let report = layout.logs.join(REPORT_FILE_NAME);
        assert!(report.is_file());
        let html = fs::read_to_string(report).unwrap();
        assert!(html.contains("Agent Observability Report"));
        assert!(!html.contains("conversation-1"));
        assert!(!html.contains("SECRET_PROMPT"));
        assert!(!html.contains("SECRET_OUTPUT"));
        assert!(!html.contains("SECRET_PATH"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn mixed_otlp_batch_commits_content_disposition_in_source_order_idempotently() {
        let root = test_root("mixed-otlp-batch");
        let _ = fs::remove_dir_all(&root);
        let mut state = collector_state(&root);
        let body = br#"{"resourceLogs":[{"scopeLogs":[{"logRecords":[
          {"attributes":[
            {"key":"event.name","value":{"stringValue":"codex.conversation_starts"}},
            {"key":"conversation.id","value":{"stringValue":"conversation-1"}}
          ]},
          {"attributes":[
            {"key":"event.name","value":{"stringValue":"codex.user_prompt"}},
            {"key":"conversation.id","value":{"stringValue":"conversation-1"}},
            {"key":"body","value":{"stringValue":"SECRET_PROMPT"}}
          ]},
          {"attributes":[
            {"key":"event.name","value":{"stringValue":"codex.sse_event"}},
            {"key":"conversation.id","value":{"stringValue":"conversation-1"}},
            {"key":"event.kind","value":{"stringValue":"response.completed"}}
          ]}
        ]}]}]}"#;

        let (batch, cursor) = parse_otlp_http_json(body, "codex-test", None, 1, 0).unwrap();
        super::commit_batch(&mut state, &batch, cursor.clone(), 1, None).unwrap();
        assert_eq!(state.store.observation_count().unwrap(), 1);
        assert_eq!(state.store.disposition_count().unwrap(), 2);
        assert_eq!(state.last_cursor.as_deref(), Some("3"));
        assert!(report_dirty_path(&state.layout).is_file());

        super::commit_batch(&mut state, &batch, cursor, 2, None).unwrap();
        assert_eq!(state.store.observation_count().unwrap(), 1);
        assert_eq!(state.store.disposition_count().unwrap(), 2);
        assert_eq!(state.last_cursor.as_deref(), Some("3"));

        let (replayed, cursor) = parse_otlp_http_json(body, "codex-test", Some("3"), 4, 3).unwrap();
        super::commit_batch(&mut state, &replayed, cursor, 3, None).unwrap();
        assert_eq!(state.store.observation_count().unwrap(), 2);
        assert_eq!(state.store.disposition_count().unwrap(), 4);
        assert_eq!(state.last_cursor.as_deref(), Some("6"));
        assert_tree_excludes(&root, &[b"SECRET_PROMPT"]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn collector_correlates_model_request_across_otlp_exports() {
        let root = test_root("split-otlp-correlation");
        let _ = fs::remove_dir_all(&root);
        let mut state = collector_state(&root);
        let api_request = br#"{"resourceLogs":[{"scopeLogs":[{"logRecords":[
          {"attributes":[
            {"key":"event.name","value":{"stringValue":"codex.api_request"}},
            {"key":"conversation.id","value":{"stringValue":"conversation-1"}},
            {"key":"model","value":{"stringValue":"gpt-test"}},
            {"key":"auth.request_id","value":{"stringValue":"request-1"}}
          ]}
        ]}]}]}"#;
        let completed = br#"{"resourceLogs":[{"scopeLogs":[{"logRecords":[
          {"attributes":[
            {"key":"event.name","value":{"stringValue":"codex.sse_event"}},
            {"key":"conversation.id","value":{"stringValue":"conversation-1"}},
            {"key":"model","value":{"stringValue":"gpt-test"}},
            {"key":"event.kind","value":{"stringValue":"response.completed"}}
          ]}
        ]}]}]}"#;

        ingest_locked(&mut state, api_request).unwrap();
        assert_eq!(state.request_correlation.pending_len(), 1);
        ingest_locked(&mut state, completed).unwrap();

        assert_eq!(state.request_correlation.pending_len(), 0);
        assert_eq!(state.last_cursor.as_deref(), Some("2"));
        assert_eq!(state.store.observation_count().unwrap(), 2);
        assert_eq!(state.store.disposition_count().unwrap(), 0);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn collector_correlates_websocket_request_across_restart() {
        let root = test_root("split-websocket-correlation");
        let _ = fs::remove_dir_all(&root);
        let request = br#"{"resourceLogs":[{"scopeLogs":[{"logRecords":[
          {"attributes":[
            {"key":"event.name","value":{"stringValue":"codex.api_request"}},
            {"key":"duration_ms","value":{"intValue":"5"}},
            {"key":"success","value":{"boolValue":true}}
          ]},
          {"timeUnixNano":"100000001","attributes":[
            {"key":"event.name","value":{"stringValue":"codex.websocket_request"}},
            {"key":"conversation.id","value":{"stringValue":"conversation-1"}},
            {"key":"model","value":{"stringValue":"gpt-test"}},
            {"key":"duration_ms","value":{"intValue":"12"}},
            {"key":"success","value":{"boolValue":true}}
          ]}
        ]}]}]}"#;
        let completed = br#"{"resourceLogs":[{"scopeLogs":[{"logRecords":[
          {"attributes":[
            {"key":"event.name","value":{"stringValue":"codex.sse_event"}},
            {"key":"conversation.id","value":{"stringValue":"conversation-1"}},
            {"key":"model","value":{"stringValue":"gpt-test"}},
            {"key":"event.kind","value":{"stringValue":"response.completed"}},
            {"key":"input_token_count","value":{"intValue":"100"}},
            {"key":"output_token_count","value":{"intValue":"25"}}
          ]}
        ]}]}]}"#;

        let mut state = collector_state(&root);
        ingest_locked(&mut state, request).unwrap();
        assert_eq!(state.request_correlation.pending_len(), 1);
        assert_eq!(state.store.observation_count().unwrap(), 1);
        assert_eq!(state.store.disposition_count().unwrap(), 1);
        drop(state);

        let mut restarted = collector_state(&root);
        assert_eq!(restarted.request_correlation.pending_len(), 1);
        ingest_locked(&mut restarted, request).unwrap();
        assert_eq!(restarted.request_correlation.pending_len(), 1);
        assert_eq!(restarted.store.observation_count().unwrap(), 1);
        assert_eq!(restarted.store.disposition_count().unwrap(), 3);
        ingest_locked(&mut restarted, completed).unwrap();

        assert_eq!(restarted.request_correlation.pending_len(), 0);
        assert_eq!(restarted.last_cursor.as_deref(), Some("5"));
        assert_eq!(restarted.store.observation_count().unwrap(), 2);
        assert_eq!(restarted.store.disposition_count().unwrap(), 3);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn collector_restart_preserves_success_fifo_across_failed_retry_and_next_request() {
        let root = test_root("restart-correlation-fifo");
        let _ = fs::remove_dir_all(&root);
        let failed = br#"{"resourceLogs":[{"scopeLogs":[{"logRecords":[{"attributes":[
          {"key":"event.name","value":{"stringValue":"codex.api_request"}},
          {"key":"conversation.id","value":{"stringValue":"PRIVATE_CONVERSATION"}},
          {"key":"model","value":{"stringValue":"gpt-test"}},
          {"key":"http.response.status_code","value":{"intValue":"500"}},
          {"key":"user.email","value":{"stringValue":"PRIVATE_EMAIL@example.com"}}
        ]}]}]}]}"#;
        let successful = br#"{"resourceLogs":[{"scopeLogs":[{"logRecords":[{"attributes":[
          {"key":"event.name","value":{"stringValue":"codex.api_request"}},
          {"key":"conversation.id","value":{"stringValue":"PRIVATE_CONVERSATION"}},
          {"key":"model","value":{"stringValue":"gpt-test"}},
          {"key":"http.response.status_code","value":{"intValue":"200"}}
        ]}]}]}]}"#;
        let completed = br#"{"resourceLogs":[{"scopeLogs":[{"logRecords":[{"attributes":[
          {"key":"event.name","value":{"stringValue":"codex.sse_event"}},
          {"key":"conversation.id","value":{"stringValue":"PRIVATE_CONVERSATION"}},
          {"key":"model","value":{"stringValue":"gpt-test"}},
          {"key":"event.kind","value":{"stringValue":"response.completed"}}
        ]}]}]}]}"#;

        let mut state = collector_state(&root);
        ingest_locked(&mut state, failed).unwrap();
        ingest_locked(&mut state, successful).unwrap();
        assert_eq!(state.request_correlation.pending_len(), 1);
        assert_eq!(state.store.observation_count().unwrap(), 2);
        drop(state);

        let mut restarted = collector_state(&root);
        assert_eq!(restarted.last_cursor.as_deref(), Some("2"));
        assert_eq!(restarted.request_correlation.pending_len(), 1);
        ingest_locked(&mut restarted, completed).unwrap();
        assert_eq!(restarted.request_correlation.pending_len(), 0);
        assert_eq!(restarted.store.observation_count().unwrap(), 3);
        let mut request_counts = std::collections::BTreeMap::new();
        for request_id in restarted
            .store
            .current_records()
            .unwrap()
            .into_iter()
            .filter_map(|record| {
                serde_json::to_value(record.attributes.request_id)
                    .ok()?
                    .as_str()
                    .map(str::to_owned)
            })
        {
            *request_counts.entry(request_id).or_insert(0_u8) += 1;
        }
        assert_eq!(request_counts.len(), 2);
        assert!(request_counts.values().any(|count| *count == 2));
        assert_eq!(restarted.store.disposition_count().unwrap(), 0);

        ingest_locked(&mut restarted, successful).unwrap();
        assert_eq!(restarted.request_correlation.pending_len(), 1);
        assert_eq!(restarted.last_cursor.as_deref(), Some("4"));
        assert_eq!(restarted.store.observation_count().unwrap(), 4);
        let request_ids = restarted
            .store
            .current_records()
            .unwrap()
            .into_iter()
            .filter_map(|record| {
                serde_json::to_value(record.attributes.request_id)
                    .ok()?
                    .as_str()
                    .map(str::to_owned)
            })
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(request_ids.len(), 3);
        assert_tree_excludes(
            &root,
            &[b"PRIVATE_CONVERSATION", b"PRIVATE_EMAIL@example.com"],
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn report_wakeup_marker_failure_does_not_block_durable_ingest() {
        let root = test_root("report-marker-optional");
        let _ = fs::remove_dir_all(&root);
        let mut state = collector_state(&root);
        fs::create_dir(report_dirty_path(&state.layout)).unwrap();

        ingest_notify_locked(&mut state, &projected_notify("thread-1", "turn-1")).unwrap();

        assert_eq!(state.store.record_count().unwrap(), 1);
        assert!(state.store.report_status().unwrap().pending());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn report_refresh_coalesces_a_burst_into_one_rebuild() {
        let root = test_root("report-refresh-coalescing");
        let _ = fs::remove_dir_all(&root);
        let state = app_state(&root);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        runtime.block_on(async {
            {
                let mut collector = state.collector.lock().await;
                ingest_notify_locked(&mut collector, &projected_notify("thread-1", "turn-1"))
                    .unwrap();
            }
            for _ in 0..20 {
                super::schedule_report_refresh_with_timing(&state, fast_report_timing());
            }
            tokio::time::timeout(Duration::from_secs(1), async {
                while state.report_refresh_scheduled.load(Ordering::Acquire) {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();

            assert_eq!(state.report_refresh_attempts.load(Ordering::Acquire), 1);
            assert!(
                !state
                    .collector
                    .lock()
                    .await
                    .store
                    .report_status()
                    .unwrap()
                    .pending()
            );
        });
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn report_refresh_waits_for_quiet_during_continuous_ingest() {
        let root = test_root("report-refresh-continuous");
        let _ = fs::remove_dir_all(&root);
        let state = app_state(&root);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        runtime.block_on(async {
            for event in 0..10 {
                {
                    let mut collector = state.collector.lock().await;
                    ingest_notify_locked(
                        &mut collector,
                        &projected_notify(&format!("thread-{event}"), "turn-1"),
                    )
                    .unwrap();
                }
                super::schedule_report_refresh_with_timing(&state, fast_report_timing());
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            assert_eq!(state.report_refresh_attempts.load(Ordering::Acquire), 0);
            tokio::time::timeout(Duration::from_secs(1), async {
                while state.report_refresh_scheduled.load(Ordering::Acquire) {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            assert_eq!(state.report_refresh_attempts.load(Ordering::Acquire), 1);
        });
        let html = fs::read_to_string(root.join("logs").join(REPORT_FILE_NAME)).unwrap();
        assert!(html.contains(r#""generatedSpans":10"#));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn request_would_block_is_reported_as_a_staged_timeout() {
        let error = super::request_io(
            "response-read",
            std::io::Error::from(std::io::ErrorKind::WouldBlock),
        );
        assert!(matches!(
            &error,
            super::CollectorError::RequestIo { stage: "response-read", source }
                if source.kind() == std::io::ErrorKind::TimedOut
        ));
        assert!(!error.to_string().contains("os error 35"));
    }

    #[test]
    fn report_refresh_does_not_lose_an_inflight_wakeup() {
        let root = test_root("report-refresh-lost-wakeup");
        let _ = fs::remove_dir_all(&root);
        let state = app_state(&root);
        let render_guard = {
            let collector = state.collector.blocking_lock();
            let config = load(&collector.layout.config).unwrap();
            let store = open_store_for_test(&collector.layout, &config);
            drop(collector);
            store.acquire_report_render_guard().unwrap()
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        runtime.block_on(async {
            {
                let mut collector = state.collector.lock().await;
                ingest_notify_locked(&mut collector, &projected_notify("thread-1", "turn-1"))
                    .unwrap();
            }
            super::schedule_report_refresh_with_timing(&state, fast_report_timing());
            tokio::time::timeout(Duration::from_secs(1), async {
                while state.report_refresh_attempts.load(Ordering::Acquire) == 0 {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();

            {
                let mut collector = state.collector.lock().await;
                ingest_notify_locked(&mut collector, &projected_notify("thread-2", "turn-2"))
                    .unwrap();
            }
            super::schedule_report_refresh_with_timing(&state, fast_report_timing());
            drop(render_guard);

            tokio::time::timeout(Duration::from_secs(1), async {
                while state.report_refresh_scheduled.load(Ordering::Acquire) {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            let collector = state.collector.lock().await;
            assert!(!collector.store.report_status().unwrap().pending());
            assert_eq!(collector.store.record_count().unwrap(), 2);
        });

        let html = fs::read_to_string(root.join("logs").join(REPORT_FILE_NAME)).unwrap();
        assert!(html.contains(r#""generatedSpans":2"#));
        assert_eq!(state.report_refresh_attempts.load(Ordering::Acquire), 2);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn report_refresh_retries_transient_failure_and_converges_to_latest_generation() {
        let root = test_root("report-retry");
        let _ = fs::remove_dir_all(&root);
        let state = app_state(&root);
        let report = root.join("logs").join(REPORT_FILE_NAME);
        fs::create_dir(&report).unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        runtime.block_on(async {
            {
                let mut collector = state.collector.lock().await;
                ingest_notify_locked(&mut collector, &projected_notify("thread-1", "turn-1"))
                    .unwrap();
            }
            schedule_report_refresh(&state);
            tokio::time::sleep(Duration::from_millis(120)).await;
            assert!(state.report_refresh_scheduled.load(Ordering::Acquire));

            {
                let mut collector = state.collector.lock().await;
                ingest_notify_locked(&mut collector, &projected_notify("thread-2", "turn-2"))
                    .unwrap();
            }
            schedule_report_refresh(&state);
            fs::remove_dir(&report).unwrap();

            for _ in 0..100 {
                if report.is_file() && !state.report_refresh_scheduled.load(Ordering::Acquire) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            assert!(report.is_file());
            assert!(!state.report_refresh_scheduled.load(Ordering::Acquire));
        });

        let html = fs::read_to_string(&report).unwrap();
        assert!(html.contains(r#""generatedSpans":2"#));
        assert!(!report_dirty_path(&state.collector.blocking_lock().layout).exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn report_authority_watcher_converges_an_external_store_commit() {
        let root = test_root("report-external-store-commit");
        let _ = fs::remove_dir_all(&root);
        let state = app_state(&root);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        runtime.block_on(async {
            let baseline = state
                .collector
                .lock()
                .await
                .store
                .report_status()
                .unwrap()
                .generation;
            let watcher = tokio::spawn(watch_report_authority(
                state.clone(),
                baseline,
                Duration::from_millis(10),
            ));
            let mut external = collector_state(&root);
            ingest_notify_locked(
                &mut external,
                &projected_notify("external-thread", "external-turn"),
            )
            .unwrap();

            tokio::time::timeout(Duration::from_secs(2), async {
                loop {
                    if state.report_refresh_attempts.load(Ordering::Acquire) > 0
                        && !state.report_refresh_scheduled.load(Ordering::Acquire)
                        && root.join("logs").join(REPORT_FILE_NAME).is_file()
                    {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            watcher.abort();
            let _ = watcher.await;
            assert!(
                !state
                    .collector
                    .lock()
                    .await
                    .store
                    .report_status()
                    .unwrap()
                    .pending()
            );
        });
        let html = fs::read_to_string(root.join("logs").join(REPORT_FILE_NAME)).unwrap();
        assert!(html.contains(r#""generatedSpans":1"#));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn concurrent_ingest_during_render_cannot_acknowledge_a_stale_report() {
        let root = test_root("report-concurrent-ingest");
        let _ = fs::remove_dir_all(&root);
        let mut collector = collector_state(&root);
        ingest_notify_locked(&mut collector, &projected_notify("thread-1", "turn-1")).unwrap();

        let config = load(&collector.layout.config).unwrap();
        let renderer = open_store_for_test(&collector.layout, &config);
        let report_path = collector.layout.logs.join(REPORT_FILE_NAME);
        let snapshot_ready = Arc::new(std::sync::Barrier::new(2));
        let ingest_finished = Arc::new(std::sync::Barrier::new(2));
        let render_handle = {
            let snapshot_ready = Arc::clone(&snapshot_ready);
            let ingest_finished = Arc::clone(&ingest_finished);
            let report_path = report_path.clone();
            thread::spawn(move || {
                let _render_guard = renderer.acquire_report_render_guard().unwrap();
                let snapshot = renderer.report_snapshot().unwrap();
                snapshot_ready.wait();
                ingest_finished.wait();
                let report = project_report(
                    &snapshot.records,
                    "2026-09-02T00:00:00.000Z",
                    "Agent Observability Report",
                    None,
                )
                .unwrap();
                write_private(&report_path, &report).unwrap();
                renderer
                    .acknowledge_report_generation(snapshot.generation)
                    .unwrap()
            })
        };

        snapshot_ready.wait();
        ingest_notify_locked(&mut collector, &projected_notify("thread-2", "turn-2")).unwrap();
        ingest_finished.wait();
        assert!(!render_handle.join().unwrap());
        assert!(collector.store.report_status().unwrap().pending());
        assert!(
            fs::read_to_string(&report_path)
                .unwrap()
                .contains(r#""generatedSpans":1"#)
        );

        assert!(refresh_report_from_root(&root).unwrap());
        assert!(!collector.store.report_status().unwrap().pending());
        assert!(
            fs::read_to_string(&report_path)
                .unwrap()
                .contains(r#""generatedSpans":2"#)
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn report_refresh_does_not_require_the_runtime_mutation_guard() {
        let root = test_root("report-render-mutation-lock-boundary");
        let _ = fs::remove_dir_all(&root);
        let mut collector = collector_state(&root);
        ingest_notify_locked(&mut collector, &projected_notify("thread-1", "turn-1")).unwrap();

        let config = load(&collector.layout.config).unwrap();
        let blocker = open_store_for_test(&collector.layout, &config);
        let render_guard = blocker.acquire_report_render_guard().unwrap();
        let mutation = MutationGuard::acquire(&collector.layout.runtime).unwrap();
        let projection = collector.layout.state.join("store/observations.jsonl");
        assert!(!projection.exists());
        let refresh_root = root.clone();
        let refresh = thread::spawn(move || refresh_report_from_root(&refresh_root));

        thread::sleep(Duration::from_millis(50));
        assert!(!projection.exists());
        drop(render_guard);
        assert!(refresh.join().unwrap().unwrap());
        drop(mutation);
        assert!(!projection.exists());
        assert_eq!(collector.store.record_count().unwrap(), 1);
        assert!(!collector.store.report_status().unwrap().pending());
        assert!(
            fs::read_to_string(collector.layout.logs.join(REPORT_FILE_NAME))
                .unwrap()
                .contains(r#""generatedSpans":1"#)
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn report_refresh_preserves_exact_counts_at_visitor_batch_boundaries() {
        for count in [128_usize, 129, 257] {
            let root = test_root(&format!("report-visitor-boundary-{count}"));
            let _ = fs::remove_dir_all(&root);
            let mut collector = collector_state(&root);
            for index in 0..count {
                ingest_notify_locked(
                    &mut collector,
                    &projected_notify(&format!("thread-{index}"), &format!("turn-{index}")),
                )
                .unwrap();
            }

            assert!(refresh_report_from_root(&root).unwrap());
            assert!(!collector.store.report_status().unwrap().pending());
            let html = fs::read_to_string(collector.layout.logs.join(REPORT_FILE_NAME)).unwrap();
            assert!(
                html.contains(&format!(r#""generatedSpans":{count}"#)),
                "report did not preserve the {count}-record boundary"
            );
            let _ = fs::remove_dir_all(root);
        }
    }

    #[test]
    fn startup_reconciles_durable_dirty_marker_after_ingest_crash_window() {
        let root = test_root("report-startup-reconcile");
        let _ = fs::remove_dir_all(&root);
        {
            let mut crashed = collector_state(&root);
            ingest_notify_locked(&mut crashed, &projected_notify("thread-1", "turn-1")).unwrap();
            assert!(report_dirty_path(&crashed.layout).is_file());
        }

        let mut restarted = collector_state(&root);
        assert!(reconcile_report_state(&restarted.layout, true));
        restarted.report_dirty = true;
        restarted.report_degraded = true;
        let state = AppState {
            collector: Arc::new(Mutex::new(restarted)),
            auth_token: Arc::from("a".repeat(64)),
            report_refresh_scheduled: Arc::new(AtomicBool::new(false)),
            report_refresh_requested: Arc::new(AtomicU64::new(0)),
            report_refresh_attempts: Arc::new(AtomicU64::new(0)),
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            schedule_report_refresh(&state);
            for _ in 0..100 {
                if !state.report_refresh_scheduled.load(Ordering::Acquire) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            let collector = state.collector.lock().await;
            assert!(!collector.report_dirty);
            assert!(!collector.report_degraded);
        });
        assert!(root.join("logs").join(REPORT_FILE_NAME).is_file());
        assert!(!report_dirty_path(&install(&root).unwrap()).exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn external_report_ack_recovers_health_after_retry_exhaustion() {
        let root = test_root("report-persistent-failure");
        let _ = fs::remove_dir_all(&root);
        let state = app_state(&root);
        let report = root.join("logs").join(REPORT_FILE_NAME);
        fs::create_dir(&report).unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            {
                let mut collector = state.collector.lock().await;
                ingest_notify_locked(&mut collector, &projected_notify("thread-1", "turn-1"))
                    .unwrap();
            }
            schedule_report_refresh(&state);
            for _ in 0..100 {
                if !state.report_refresh_scheduled.load(Ordering::Acquire) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            assert!(!state.report_refresh_scheduled.load(Ordering::Acquire));
            let collector = state.collector.lock().await;
            assert!(collector.report_dirty);
            assert!(collector.report_degraded);
            assert_eq!(collector.report_refresh_failures, super::REPORT_RETRY_LIMIT);
            drop(collector);

            let response = super::health(State(state.clone())).await.into_response();
            let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
                .await
                .unwrap();
            let health: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(health["status"], "degraded");
            assert_eq!(health["report_dirty"], true);
            assert_eq!(health["report_refresh_failures"], super::REPORT_RETRY_LIMIT);

            let generation = state
                .collector
                .lock()
                .await
                .store
                .report_status()
                .unwrap()
                .generation;
            let watcher = tokio::spawn(watch_report_authority(
                state.clone(),
                generation,
                Duration::from_millis(10),
            ));
            fs::remove_dir(&report).unwrap();
            assert!(refresh_report_from_root(&root).unwrap());
            tokio::time::timeout(Duration::from_secs(1), async {
                loop {
                    let collector = state.collector.lock().await;
                    if !collector.report_degraded && collector.report_refresh_failures == 0 {
                        break;
                    }
                    drop(collector);
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            watcher.abort();
            let _ = watcher.await;

            let response = super::health(State(state.clone())).await.into_response();
            let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
                .await
                .unwrap();
            let health: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(health["status"], "ready");
            assert_eq!(health["report_dirty"], false);
            assert_eq!(health["report_refresh_failures"], 0);
        });
        assert!(!report_dirty_path(&install(&root).unwrap()).exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn full_runtime_tree_excludes_raw_otlp_and_notify_content() {
        let root = test_root("privacy-tree");
        let _ = fs::remove_dir_all(&root);
        let settings = install_settings(&root).unwrap();
        let mut state = collector_state(&root);
        let otlp = br#"{"resourceLogs":[{"scopeLogs":[{"logRecords":[
          {"attributes":[
            {"key":"event.name","value":{"stringValue":"codex.conversation_starts"}},
            {"key":"conversation.id","value":{"stringValue":"RAW_CONVERSATION_SECRET"}},
            {"key":"body","value":{"stringValue":"RAW_EVENT_BODY_SECRET"}},
            {"key":"account.email","value":{"stringValue":"RAW_EMAIL_SECRET@example.test"}}
          ]}
        ]}]}]}"#;
        ingest_locked(&mut state, otlp).unwrap();
        drop(state);

        let layout = install(&root).unwrap();
        let server_config = build_server_config(&layout, &settings.credentials).unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
                .await
                .unwrap();
            configure_port(&root, listener.local_addr().unwrap().port());
            let transport =
                super::TransportListener::new(listener, server_config, Duration::from_secs(1), 2);
            let app = router(app_state(&root));
            let server = tokio::spawn(async move { axum::serve(transport, app).await });
            let health_root = root.clone();
            tokio::time::timeout(Duration::from_secs(2), async move {
                loop {
                    let probe_root = health_root.clone();
                    let health =
                        tokio::task::spawn_blocking(move || super::check_health(&probe_root))
                            .await
                            .unwrap();
                    if matches!(
                        health,
                        super::HealthOutcome::Ready | super::HealthOutcome::Degraded
                    ) {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            let notify_root = root.clone();
            let outcome = tokio::task::spawn_blocking(move || {
                submit_notify(
                    &notify_root,
                    &raw_notify("RAW_THREAD_SECRET", "RAW_TURN_SECRET"),
                )
            })
            .await
            .unwrap();
            assert_eq!(outcome, NotifyOutcome::Accepted);
            server.abort();
            let _ = server.await;
        });
        refresh_report_from_root(&root).unwrap();

        assert_tree_excludes(
            &root,
            &[
                b"RAW_CONVERSATION_SECRET",
                b"RAW_EVENT_BODY_SECRET",
                b"RAW_EMAIL_SECRET@example.test",
                b"RAW_THREAD_SECRET",
                b"RAW_TURN_SECRET",
                b"RAW_PATH_SECRET",
                b"RAW_INPUT_SECRET",
                b"RAW_OUTPUT_SECRET",
            ],
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn automatic_receiver_applies_collection_disable_without_restart() {
        let root = std::env::temp_dir().join(format!(
            "agent-observability-collector-disabled-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let layout = install(&root).unwrap();
        let initial = load(&layout.config).unwrap();
        let store = open_store_for_test(&layout, &initial);
        let mut state = CollectorState {
            layout: layout.clone(),
            store,
            source_generation: "codex-test".into(),
            last_cursor: None,
            request_correlation: OtlpRequestCorrelationState::default(),
            accepted_requests: 0,
            rejected_requests: 0,
            suppressed_requests: 0,
            last_ingest_unix_ms: None,
            report_dirty: false,
            report_degraded: false,
            report_refresh_failures: 0,
            report_failure: None,
        };
        let guard = ConfigMutationGuard::acquire(&layout).unwrap();
        let mut disabled = initial;
        disabled.enabled = false;
        save(&guard, &disabled).unwrap();
        drop(guard);

        let outcome =
            ingest_notify_locked(&mut state, &projected_notify("thread", "turn")).unwrap();

        assert_eq!(outcome, IngestOutcome::Disabled);
        assert_eq!(state.store.counts().unwrap().0, 0);
        assert_eq!(state.last_cursor, None);
        let _ = fs::remove_dir_all(root);
    }
}
