#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc)]

mod report_coherence;
pub mod storage_ownership;

use report_coherence::{ReportMutationScope, ReportWritePermits, report_coherence_failure};

use agent_observability_adapter_codex::{
    AdapterBatch, AdapterItem, MAX_HANDOFF_BYTES, MAX_PRIVATE_TURN_DETAIL_BYTES,
    OtlpRequestCorrelationState, PrivateCodexTurnDetailV1, ProjectedNotifyV2,
    parse_otlp_http_json_with_state, parse_projected_notify_json, project_notify_json,
    project_notify_with_private_detail,
};
#[cfg(test)]
use agent_observability_application::project_report;
use agent_observability_contracts::{CollectorDegradationReasonV1, LOCAL_COLLECTOR_HEALTH_VERSION};
use agent_observability_local_runtime::{
    Admission, ControlError, InstalledLayout, LocalRuntimeConfigV3, MutationGuard, PressureSample,
    REPORT_RESERVATION_METADATA_ALLOWANCE, ReservationError, RuntimeControl, Singleton,
    SingletonError, StorageBudget, inspect, install, load,
};
use agent_observability_local_store::{
    LocalStore, MISSING_RATE_FINGERPRINT, ReportViewBuildError, ReportViewCatalogError,
    StoreBatchItem, current_report_view, current_report_view_needs_kernel_upgrade,
    publish_report_view, recover_report_view_catalog, recover_report_view_catalog_before_migration,
};
#[cfg(test)]
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
    thread,
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
const MAX_AUTOMATIC_REPORT_VIEW_BYTES: u64 = agent_observability_local_store::MAX_REPORT_VIEW_BYTES;
// The bounded catalog replacement and authority ACK journal are separate from the
// staging builder's own database/journal allowance. Include both in shared admission.
const REPORT_VIEW_PUBLICATION_RESERVE_BYTES: u64 =
    64 * 1024 + agent_observability_local_store::MAX_REPORT_ACKNOWLEDGEMENT_BYTES;
const PRIVATE_TURN_DETAIL_DIRECTORY: &str = "private-codex-turn-details";
const PRIVATE_TURN_DETAIL_STATUS_DIRECTORY: &str = "private-codex-turn-detail-statuses";
const PRIVATE_TURN_DETAIL_STATUS_VERSION: &str = "private_codex_turn_detail_status.v1";
const MAX_PRIVATE_TURN_DETAIL_FILES: usize = 1_024;
const MAX_PRIVATE_TURN_DETAIL_STATUS_BYTES: u64 = 1_024;
const MAX_PRIVATE_TURN_DETAIL_SCAN_ENTRIES: usize = 4_096;
const PRIVATE_NOTIFY_ENVELOPE_VERSION: &str = "private_codex_notify_envelope.v1";
const PRIVATE_TURN_DETAIL_RECEIPT_VERSION: &str = "private_codex_turn_detail_receipt.v1";
const PRIVATE_TURN_DETAIL_LOCK_RETRIES: usize = 4;
const PRIVATE_TURN_DETAIL_LOCK_RETRY_DELAY: Duration = Duration::from_millis(10);
const PRIVATE_NOTIFY_FOREGROUND_DEADLINE: Duration = Duration::from_millis(250);
const PRIVATE_NOTIFY_CONNECT_TIMEOUT: Duration = Duration::from_millis(50);
const REPORT_RETRY_LIMIT: u32 = 4;
const REPORT_RETRY_INITIAL_DELAY: Duration = Duration::from_millis(50);
const REPORT_DEBOUNCE_DELAY: Duration = Duration::from_millis(200);
const REPORT_CONTENTION_QUIET_LIMIT: Duration = Duration::from_secs(30);
const REPORT_AUTHORITY_POLL_INTERVAL: Duration = Duration::from_millis(500);
const LIFECYCLE_POLICY_POLL_INTERVAL: Duration = Duration::from_secs(30);
const LIFECYCLE_QUIET_PERIOD_MS: u64 = 30_000;
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

#[derive(Debug, Eq, PartialEq)]
pub enum PrivateTurnDetailLookup {
    Available(Vec<u8>),
    NotCollected,
    Failed(&'static str),
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PrivateTurnDetailCaptureStatusV1 {
    schema_version: String,
    turn_id: String,
    state: String,
    code: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PrivateNotifyEnvelopeV1 {
    schema_version: String,
    projected: ProjectedNotifyV2,
    private_detail: serde_json::Value,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum PrivateTurnDetailReceiptState {
    Available,
    Failed,
    Busy,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PrivateTurnDetailReceiptV1 {
    schema_version: String,
    state: PrivateTurnDetailReceiptState,
    code: String,
}

impl PrivateTurnDetailReceiptV1 {
    fn is_available(&self, http_status: u16) -> bool {
        http_status == 200
            && self.schema_version == PRIVATE_TURN_DETAIL_RECEIPT_VERSION
            && self.state == PrivateTurnDetailReceiptState::Available
            && self.code == "ok"
    }
}

#[derive(Debug)]
pub enum CollectorError {
    LifecycleStoragePressure,
    DashboardStorageCapacity,
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
        && valid_legacy_v1_metadata(&legacy)
    {
        return Ok(None);
    }
    let legacy: LegacyCollectorSettingsV2Mtls = serde_json::from_slice(bytes)
        .map_err(|_| CollectorError::Runtime("invalid legacy collector settings".into()))?;
    validate_legacy_v2_mtls(layout, &legacy)?;
    Ok(Some(legacy.generation))
}

fn valid_legacy_v1_metadata(settings: &LegacyCollectorSettingsV1) -> bool {
    settings.schema_version == "local_collector.v1"
        && settings.port != 0
        && settings.token.len() == 64
        && settings.token.bytes().all(|byte| byte.is_ascii_hexdigit())
        && settings.source_generation == SOURCE_GENERATION
}

fn validate_legacy_v2_metadata(
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
    }
    Ok(())
}

fn validate_legacy_v2_mtls(
    layout: &InstalledLayout,
    settings: &LegacyCollectorSettingsV2Mtls,
) -> Result<(), CollectorError> {
    validate_legacy_v2_metadata(settings)?;
    for path in [
        &settings.credentials.ca_certificate,
        &settings.credentials.server_certificate,
        &settings.credentials.server_private_key,
        &settings.credentials.client_certificate,
        &settings.credentials.client_private_key,
    ] {
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
    load_settings_from_layout(&layout)
}

fn load_settings_from_layout(
    layout: &InstalledLayout,
) -> Result<CollectorSettings, CollectorError> {
    let path = settings_path(layout);
    let snapshot = read_private_snapshot(&path, MAX_SETTINGS_BYTES)?;
    let settings = parse_current_settings(&snapshot.bytes)?;
    validate_owned_credentials(layout, &settings)?;
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
            Self::LifecycleStoragePressure => {
                formatter.write_str("lifecycle temporary storage headroom unavailable")
            }
            Self::DashboardStorageCapacity => {
                formatter.write_str("dashboard snapshot storage headroom unavailable")
            }
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
            Self::Runtime(_) | Self::LifecycleStoragePressure | Self::DashboardStorageCapacity => {
                None
            }
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
    RenderGuard,
    Snapshot,
    // Preserve the v1 public stage code; distinguish transient contention internally without
    // leaking database diagnostics or treating it as a persistent snapshot corruption failure.
    #[serde(rename = "snapshot")]
    SnapshotChanged,
    Projection,
    Publish,
    #[serde(rename = "publish")]
    Capacity,
    Acknowledge,
    Status,
}

#[derive(Clone, Debug)]
struct AppState {
    collector: Arc<Mutex<CollectorState>>,
    auth_token: Arc<str>,
    private_detail_failures: Arc<AtomicU64>,
    lifecycle_failures: Arc<AtomicU64>,
    lifecycle_storage_pressure: Arc<AtomicBool>,
    report_refresh_scheduled: Arc<AtomicBool>,
    report_refresh_requested: Arc<AtomicU64>,
    report_contention_quiet_ms: Arc<AtomicU64>,
    #[cfg(test)]
    report_refresh_attempts: Arc<AtomicU64>,
    #[cfg(test)]
    report_snapshot_test: Arc<ReportSnapshotTest>,
}

#[cfg(test)]
#[derive(Debug, Default)]
struct ReportSnapshotTest {
    delay_ms: AtomicU64,
    started: AtomicBool,
    fail_after_reservation: AtomicBool,
    cleanup_attempts: AtomicU64,
}

#[derive(Debug, Serialize)]
struct Health {
    schema_version: &'static str,
    degradation_reasons: Vec<CollectorDegradationReasonV1>,
    status: &'static str,
    accepted_requests: u64,
    rejected_requests: u64,
    suppressed_requests: u64,
    last_ingest_unix_ms: Option<u64>,
    report_dirty: bool,
    report_refresh_failures: u32,
    report_failure: Option<ReportFailure>,
    private_detail_failures: u64,
    lifecycle_failures: u64,
    expired_trace_dispositions: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct HealthProbe {
    status: String,
    report_dirty: bool,
}

#[derive(Debug, Deserialize)]
struct HealthReasonProjection {
    schema_version: String,
    degradation_reasons: Vec<CollectorDegradationReasonV1>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HealthDetails {
    pub outcome: HealthOutcome,
    pub degradation_reasons: Vec<CollectorDegradationReasonV1>,
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
    recover_report_reservation_for_startup(&layout, &config, &mutation)?;
    maintain_private_turn_details_locked(&layout, &config, SystemTime::now())?;
    let store = open_store(&mutation, &layout, &config)?;
    recover_report_view_catalog_for_startup(&store)?;
    let report_status = store.report_status().map_err(runtime_error)?;
    let report_missing = automatic_report_view_missing(&store)?;
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
    let private_detail_failures = Arc::new(AtomicU64::new(0));
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
        private_detail_failures,
        lifecycle_failures: Arc::new(AtomicU64::new(0)),
        lifecycle_storage_pressure: Arc::new(AtomicBool::new(false)),
        report_refresh_scheduled: Arc::new(AtomicBool::new(false)),
        report_refresh_requested: Arc::new(AtomicU64::new(0)),
        report_contention_quiet_ms: Arc::new(AtomicU64::new(0)),
        #[cfg(test)]
        report_refresh_attempts: Arc::new(AtomicU64::new(0)),
        #[cfg(test)]
        report_snapshot_test: Arc::default(),
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
    let lifecycle_watcher = tokio::spawn(watch_storage_lifecycle(state.clone()));
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
    lifecycle_watcher.abort();
    let _ = lifecycle_watcher.await;
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
        .route(
            "/v1/notify/private-detail",
            post(ingest_notify_with_private_detail),
        )
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

async fn ingest_notify_with_private_detail(State(state): State<AppState>, body: Bytes) -> Response {
    let Some((projected, private_detail)) = parse_private_notify_envelope(&body) else {
        return private_turn_detail_receipt_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            PrivateTurnDetailReceiptState::Failed,
            "invalid_request",
        );
    };

    let mut collector = state.collector.lock().await;
    match ingest_notify_locked_with_config(&mut collector, &projected) {
        Ok((IngestOutcome::Committed, Some(config))) => {
            collector.accepted_requests = collector.accepted_requests.saturating_add(1);
            let layout = collector.layout.clone();
            drop(collector);
            schedule_report_refresh(&state);
            if config.capture_private_codex_turn_details {
                let writer_state = state.clone();
                if let Ok(response) = tokio::task::spawn_blocking(move || {
                    persist_private_turn_detail_request(
                        &writer_state,
                        &layout,
                        &config,
                        &private_detail,
                    )
                })
                .await
                {
                    response
                } else {
                    state.private_detail_failures.fetch_add(1, Ordering::AcqRel);
                    private_turn_detail_receipt_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        PrivateTurnDetailReceiptState::Failed,
                        "writer_task",
                    )
                }
            } else {
                private_turn_detail_receipt_response(
                    StatusCode::CONFLICT,
                    PrivateTurnDetailReceiptState::Failed,
                    "capture_disabled",
                )
            }
        }
        Ok((IngestOutcome::Disabled, None)) => {
            collector.suppressed_requests = collector.suppressed_requests.saturating_add(1);
            private_turn_detail_receipt_response(
                StatusCode::CONFLICT,
                PrivateTurnDetailReceiptState::Failed,
                "capture_disabled",
            )
        }
        Ok(_) => private_turn_detail_receipt_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            PrivateTurnDetailReceiptState::Failed,
            "invalid_state",
        ),
        Err(error) => {
            collector.rejected_requests = collector.rejected_requests.saturating_add(1);
            private_turn_detail_receipt_response(
                error.status(),
                if matches!(error, IngestError::Busy) {
                    PrivateTurnDetailReceiptState::Busy
                } else {
                    PrivateTurnDetailReceiptState::Failed
                },
                match error {
                    IngestError::Busy => "runtime_busy",
                    IngestError::Pressure => "pressure",
                    IngestError::Storage => "storage_budget",
                    IngestError::Policy => "policy",
                    IngestError::Invalid(_) => "invalid_request",
                },
            )
        }
    }
}

fn persist_private_turn_detail_request(
    state: &AppState,
    layout: &InstalledLayout,
    config: &LocalRuntimeConfigV3,
    private_detail: &PrivateCodexTurnDetailV1,
) -> Response {
    #[cfg(test)]
    let lock_started = StdInstant::now();
    let mutation = match acquire_private_turn_detail_mutation(&layout.runtime) {
        Ok(Some(mutation)) => mutation,
        Ok(None) => {
            state.private_detail_failures.fetch_add(1, Ordering::AcqRel);
            return private_turn_detail_receipt_response(
                StatusCode::SERVICE_UNAVAILABLE,
                PrivateTurnDetailReceiptState::Busy,
                "busy",
            );
        }
        Err(_) => {
            state.private_detail_failures.fetch_add(1, Ordering::AcqRel);
            return private_turn_detail_receipt_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                PrivateTurnDetailReceiptState::Failed,
                "runtime_error",
            );
        }
    };
    #[cfg(test)]
    eprintln!(
        "private_capture_stage=lock elapsed={:?}",
        lock_started.elapsed()
    );
    let result = capture_private_turn_detail_locked(layout, private_detail, config);
    drop(mutation);
    match result {
        Ok((PrivateTurnDetailReceiptState::Available, code)) => {
            private_turn_detail_receipt_response(
                StatusCode::OK,
                PrivateTurnDetailReceiptState::Available,
                code,
            )
        }
        Ok((receipt_state, code)) => {
            state.private_detail_failures.fetch_add(1, Ordering::AcqRel);
            private_turn_detail_receipt_response(
                if code == "storage_budget" {
                    StatusCode::INSUFFICIENT_STORAGE
                } else {
                    StatusCode::SERVICE_UNAVAILABLE
                },
                receipt_state,
                code,
            )
        }
        Err(_) => {
            state.private_detail_failures.fetch_add(1, Ordering::AcqRel);
            private_turn_detail_receipt_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                PrivateTurnDetailReceiptState::Failed,
                "status_publication",
            )
        }
    }
}

fn parse_private_notify_envelope(body: &[u8]) -> Option<(Vec<u8>, PrivateCodexTurnDetailV1)> {
    let envelope = serde_json::from_slice::<PrivateNotifyEnvelopeV1>(body).ok()?;
    if envelope.schema_version != PRIVATE_NOTIFY_ENVELOPE_VERSION {
        return None;
    }
    let private_detail = serde_json::to_vec(&envelope.private_detail)
        .ok()
        .and_then(|bytes| PrivateCodexTurnDetailV1::from_json(&bytes).ok())?;
    if !private_detail.matches_projected_turn(&envelope.projected) {
        return None;
    }
    let projected = serde_json::to_vec(&envelope.projected).ok()?;
    Some((projected, private_detail))
}

fn private_turn_detail_receipt_response(
    status: StatusCode,
    state: PrivateTurnDetailReceiptState,
    code: &str,
) -> Response {
    (
        status,
        axum::Json(PrivateTurnDetailReceiptV1 {
            schema_version: PRIVATE_TURN_DETAIL_RECEIPT_VERSION.into(),
            state,
            code: code.into(),
        }),
    )
        .into_response()
}

fn lifecycle_degradation_reasons(
    failures: u64,
    storage_pressure: bool,
    expired_dispositions: Option<u64>,
) -> Vec<CollectorDegradationReasonV1> {
    let mut reasons = Vec::with_capacity(2);
    if storage_pressure {
        reasons.push(CollectorDegradationReasonV1::StoragePressure);
    } else if failures > 0 {
        reasons.push(CollectorDegradationReasonV1::LifecycleFailure);
    }
    if expired_dispositions.is_some_and(|count| count > 0) {
        reasons.push(CollectorDegradationReasonV1::ExpiredTrace);
    }
    reasons
}

async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let mut collector = state.collector.lock().await;
    let private_detail_failures = state.private_detail_failures.load(Ordering::Acquire);
    let lifecycle_failures = state.lifecycle_failures.load(Ordering::Acquire);
    let expired_trace_dispositions = collector.store.expired_trace_disposition_count().ok();
    let degradation_reasons = lifecycle_degradation_reasons(
        lifecycle_failures,
        state.lifecycle_storage_pressure.load(Ordering::Acquire),
        expired_trace_dispositions,
    );
    let lifecycle_degraded = !degradation_reasons.is_empty();
    let report_pending = if let Ok(status) = collector.store.report_status() {
        status.pending()
    } else {
        collector.report_failure = Some(ReportFailure::Status);
        true
    };
    collector.report_dirty = report_pending;
    axum::Json(Health {
        schema_version: LOCAL_COLLECTOR_HEALTH_VERSION,
        degradation_reasons,
        status: if collector.report_degraded
            || report_pending
            || lifecycle_failures > 0
            || lifecycle_degraded
            || expired_trace_dispositions.is_none_or(|count| count > 0)
        {
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
        private_detail_failures,
        lifecycle_failures,
        expired_trace_dispositions,
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
            Self::Storage
            | Self::Invalid(
                CollectorError::LifecycleStoragePressure | CollectorError::DashboardStorageCapacity,
            ) => StatusCode::INSUFFICIENT_STORAGE,
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
    ingest_notify_locked_with_config(state, body).map(|(outcome, _)| outcome)
}

fn ingest_notify_locked_with_config(
    state: &mut CollectorState,
    body: &[u8],
) -> Result<(IngestOutcome, Option<LocalRuntimeConfigV3>), IngestError> {
    let _mutation = try_ingest_mutation(&state.layout.runtime)?;
    let Some(config) = admit_request(state, body.len())? else {
        return Ok((IngestOutcome::Disabled, None));
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
    Ok((IngestOutcome::Committed, Some(config)))
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
) -> Result<Option<LocalRuntimeConfigV3>, IngestError> {
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
    let diagnostic = control
        .collector_admission_diagnostic(
            &state.layout.root,
            u64::from(config.collection.max_batch_bytes),
        )
        .map_err(|error| match error {
            ControlError::CollectorAdmissionOverflow => IngestError::Storage,
            error => IngestError::Invalid(runtime_error(error)),
        })?;
    if diagnostic.admission == Admission::Denied {
        return Err(IngestError::Storage);
    }
    Ok(Some(config))
}

fn enforce_batch_policy(
    batch: &AdapterBatch,
    config: &LocalRuntimeConfigV3,
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

/// Executes one bounded, opt-in lifecycle pass without starting a collector.
/// A busy writer is not waited on; the next scheduled pass can retry.
pub fn maintain_storage_lifecycle(root: &Path) -> Result<String, CollectorError> {
    let layout = inspect(root).map_err(runtime_error)?;
    let _mutation = match MutationGuard::try_acquire(&layout.runtime) {
        Ok(guard) => guard,
        Err(SingletonError::AlreadyRunning) => return Ok("lifecycle=busy".into()),
        Err(error) => return Err(runtime_error(error)),
    };
    let config = load(&layout.config).map_err(runtime_error)?;
    if !config.lifecycle.enabled {
        return Ok("lifecycle=disabled".into());
    }
    // Raw expiry needs no database copy and must still run when tier migration lacks disk space.
    maintain_private_turn_details_locked(&layout, &config, SystemTime::now())?;
    let store = LocalStore::open_current(layout.state.join("store")).map_err(runtime_error)?;
    let policy = &config.lifecycle;
    let request = agent_observability_local_store::LifecycleRequest {
        now_unix_ms: current_unix_ms()?,
        hot_days: policy.hot_days,
        warm_days: policy.warm_days,
        delete_after_days: policy.delete_after_days,
        max_traces_per_pass: policy.max_traces_per_pass,
        max_archive_records: config.retention.max_archive_records,
        max_archive_bytes: config.retention.max_archive_bytes,
    };
    let preflight = store.lifecycle_preflight(request).map_err(runtime_error)?;
    if !preflight.has_candidates && !preflight.has_backfill_pending {
        return Ok("lifecycle=idle".into());
    }
    let headroom = RuntimeControl::new(&config)
        .map_err(runtime_error)?
        .migration_headroom(&layout.root)
        .map_err(runtime_error)?;
    if headroom < preflight.required_temporary_bytes {
        return Err(CollectorError::LifecycleStoragePressure);
    }
    let Some(render_guard) = store
        .try_acquire_report_render_guard()
        .map_err(runtime_error)?
    else {
        return Ok("lifecycle=busy".into());
    };
    store.invalidate_report().map_err(runtime_error)?;
    agent_observability_static_report::write_refresh_pending(&layout.logs.join(REPORT_FILE_NAME))
        .map_err(runtime_error)?;
    mark_report_dirty(&layout)?;
    let result = store
        .maintain_lifecycle_guarded(request, render_guard)
        .map_err(runtime_error)?;
    let status = if result.blocked > 0 {
        "blocked"
    } else {
        "completed"
    };
    Ok(format!(
        "lifecycle={status}\nreport_refresh=pending\n{result:?}"
    ))
}

fn lifecycle_idle(last_ingest: Option<u64>, now: u64) -> bool {
    last_ingest.is_none_or(|last| now.saturating_sub(last) >= LIFECYCLE_QUIET_PERIOD_MS)
}

fn record_lifecycle_pass(state: &AppState, output: &str) {
    if output == "lifecycle=busy" {
        return;
    }
    state
        .lifecycle_storage_pressure
        .store(false, Ordering::Release);
    state.lifecycle_failures.store(
        u64::from(output.starts_with("lifecycle=blocked\n")),
        Ordering::Release,
    );
    if output.contains("report_refresh=pending") {
        schedule_report_refresh(state);
    }
}

async fn watch_storage_lifecycle(state: AppState) {
    let mut ticker = tokio::time::interval(LIFECYCLE_POLICY_POLL_INTERVAL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last_pass: Option<StdInstant> = None;
    loop {
        ticker.tick().await;
        let Ok(collector) = state.collector.try_lock() else {
            continue;
        };
        let Ok(now) = current_unix_ms() else {
            continue;
        };
        if !lifecycle_idle(collector.last_ingest_unix_ms, now) {
            continue;
        }
        let root = collector.layout.root.clone();
        drop(collector);
        // Config I/O and maintenance never occupy a Tokio network executor thread.
        let elapsed = last_pass.map(|last| last.elapsed());
        let outcome = tokio::task::spawn_blocking(move || {
            let layout = inspect(&root).map_err(runtime_error)?;
            let config = load(&layout.config).map_err(runtime_error)?;
            if !config.lifecycle.enabled {
                return Ok(Some("lifecycle=disabled".to_owned()));
            }
            let cadence =
                Duration::from_secs(u64::from(config.lifecycle.maintenance_interval_seconds));
            if elapsed.is_some_and(|elapsed| elapsed < cadence) {
                return Ok(None);
            }
            maintain_storage_lifecycle(&root).map(Some)
        })
        .await;
        match outcome {
            Ok(Ok(Some(output))) => {
                if output != "lifecycle=busy" {
                    last_pass = Some(StdInstant::now());
                }
                record_lifecycle_pass(&state, &output);
            }
            Ok(Ok(None)) => {}
            outcome => {
                state.lifecycle_storage_pressure.store(
                    matches!(outcome, Ok(Err(CollectorError::LifecycleStoragePressure))),
                    Ordering::Release,
                );
                last_pass = Some(StdInstant::now());
                state.lifecycle_failures.fetch_add(1, Ordering::AcqRel);
                // A failed pass may already have hidden the previous HTML safely.
                // Rebuild from committed storage rather than leaving that placeholder stale.
                schedule_report_refresh(&state);
            }
        }
    }
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
        let mut quiet_period = timing.debounce.max(Duration::from_millis(
            state.report_contention_quiet_ms.load(Ordering::Acquire),
        ));
        loop {
            if failure_attempts == 0 {
                await_report_debounce(&state, quiet_period).await;
            } else {
                tokio::time::sleep(retry_delay).await;
            }
            let attempt_epoch = state.report_refresh_requested.load(Ordering::Acquire);
            let (refresh, mut failure, attempt_duration) = run_report_refresh_attempt(&state).await;
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
            if failure == Some(ReportFailure::SnapshotChanged)
                || (refresh.is_some() && failure.is_none())
            {
                // A concurrent commit invalidated this read/publication. Preserve the same
                // scheduled task and its learned quiet period across new wakeups: restarting
                // the ordinary short retry cycle would repeatedly scan a growing store.
                quiet_period = contention_quiet_period(quiet_period, attempt_duration);
                state.report_contention_quiet_ms.store(
                    u64::try_from(quiet_period.as_millis()).unwrap_or(u64::MAX),
                    Ordering::Release,
                );
                failure_attempts = 0;
                retry_delay = timing.retry_initial;
                collector.report_degraded = true;
                if collector.report_failure.is_none() {
                    collector.report_failure = Some(ReportFailure::SnapshotChanged);
                }
                drop(collector);
                continue;
            }
            collector.report_refresh_failures = collector.report_refresh_failures.saturating_add(1);
            collector.report_failure = failure;
            failure_attempts += 1;
            if failure_attempts == REPORT_RETRY_LIMIT {
                collector.report_degraded = true;
                let layout = collector.layout.clone();
                drop(collector);
                // Retain scheduler ownership across bounded cleanup-only retries. A busy
                // mutation lock must not silently discard recovery after the last build.
                if let Err(failure) =
                    cleanup_report_reservation_with_retry(&state, &layout, timing).await
                {
                    state.collector.lock().await.report_failure = Some(failure);
                }
                state
                    .report_refresh_scheduled
                    .store(false, Ordering::Release);
                let retry_latest =
                    state.report_refresh_requested.load(Ordering::Acquire) != attempt_epoch;
                if retry_latest {
                    schedule_report_refresh_with_timing(&state, timing);
                }
                return;
            }
            drop(collector);
            retry_delay = retry_delay.saturating_mul(2);
        }
    });
}

async fn run_report_refresh_attempt(
    state: &AppState,
) -> (Option<bool>, Option<ReportFailure>, Duration) {
    let root = state.collector.lock().await.layout.root.clone();
    #[cfg(test)]
    state
        .report_refresh_attempts
        .fetch_add(1, Ordering::Release);
    #[cfg(test)]
    let snapshot_test = Arc::clone(&state.report_snapshot_test);
    let result = tokio::task::spawn_blocking(move || {
        let started = StdInstant::now();
        #[cfg(not(test))]
        {
            (refresh_report_from_root(&root), started.elapsed())
        }
        #[cfg(test)]
        {
            (
                refresh_report_from_root_observing(&root, |index| {
                    if index == 0 {
                        snapshot_test.started.store(true, Ordering::Release);
                        assert!(
                            !snapshot_test.fail_after_reservation.load(Ordering::Acquire),
                            "injected post-reservation projection failure"
                        );
                        std::thread::sleep(Duration::from_millis(
                            snapshot_test.delay_ms.load(Ordering::Acquire),
                        ));
                    }
                }),
                started.elapsed(),
            )
        }
    })
    .await;
    let (published, failure, attempt_duration) = match result {
        Ok((Ok(published), attempt_duration)) => (Some(published), None, attempt_duration),
        Ok((Err(failure), attempt_duration)) => (None, Some(failure), attempt_duration),
        Err(_) => (None, Some(ReportFailure::Task), Duration::ZERO),
    };
    (published, failure, attempt_duration)
}

fn contention_quiet_period(previous: Duration, attempt: Duration) -> Duration {
    previous
        .saturating_mul(2)
        .max(attempt.saturating_mul(4))
        .min(REPORT_CONTENTION_QUIET_LIMIT)
}

async fn await_report_debounce(state: &AppState, quiet_period: Duration) {
    let mut observed = state.report_refresh_requested.load(Ordering::Acquire);
    loop {
        tokio::time::sleep(quiet_period).await;
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
    refresh_report_from_root_observing(root, |_| {})
}

/// Builds and publishes the bounded immutable dashboard snapshot for a prepared local store.
///
/// The caller remains responsible for initializing or migrating the store under the CLI/runtime
/// mutation guard. Concurrent publication and source-generation drift are retryable and return
/// `Ok(false)`; storage-capacity rejection and other durable failures remain explicit errors.
pub fn refresh_dashboard_snapshot(root: &Path) -> Result<bool, CollectorError> {
    match refresh_report_from_root(root) {
        Ok(published) => Ok(published),
        Err(ReportFailure::RenderGuard | ReportFailure::SnapshotChanged) => Ok(false),
        Err(ReportFailure::Capacity) => Err(CollectorError::DashboardStorageCapacity),
        Err(error) => Err(CollectorError::Runtime(format!(
            "dashboard snapshot refresh failed at {}",
            report_failure_stage(error)
        ))),
    }
}

const fn report_failure_stage(error: ReportFailure) -> &'static str {
    match error {
        ReportFailure::Task => "task",
        ReportFailure::Install => "install",
        ReportFailure::OpenStore => "open_store",
        ReportFailure::RenderGuard => "render_guard",
        ReportFailure::Snapshot | ReportFailure::SnapshotChanged => "snapshot",
        ReportFailure::Projection => "projection",
        ReportFailure::Publish | ReportFailure::Capacity => "publish",
        ReportFailure::Acknowledge => "acknowledge",
        ReportFailure::Status => "status",
    }
}

fn refresh_report_from_root_observing(
    root: &Path,
    on_record: impl FnMut(usize),
) -> Result<bool, ReportFailure> {
    let layout = install(root).map_err(|_| ReportFailure::Install)?;
    let barrier =
        agent_observability_local_runtime::storage_coherence::StorageBarrier::open_if_initialized(
            &layout.root,
        )
        .map_err(report_coherence_failure)?;
    let permit = barrier
        .as_ref()
        .map(agent_observability_local_runtime::storage_coherence::StorageBarrier::try_begin_write)
        .transpose()
        .map_err(report_coherence_failure)?;
    let store = LocalStore::open_current(layout.state.join("store"))
        .map_err(|_| ReportFailure::OpenStore)?;
    if let Some(permit) = permit.as_ref() {
        permit.revalidate().map_err(report_coherence_failure)?;
    }
    drop(permit);
    refresh_report_observing(&layout, &store, on_record)
}

/// Projects and sends a raw notify argument with bounded foreground deadlines.
/// Projection happens before any settings or network I/O.
#[must_use]
pub fn submit_notify(root: &Path, payload: &[u8]) -> NotifyOutcome {
    let deadline = StdInstant::now() + PRIVATE_NOTIFY_FOREGROUND_DEADLINE;
    submit_notify_until(root, payload, deadline)
}

// Keep the production deadline fixed at entry while allowing functional transport
// tests to exercise this same path independently of host scheduling latency.
fn submit_notify_until(root: &Path, payload: &[u8], deadline: StdInstant) -> NotifyOutcome {
    let Ok(projected) = project_notify_json(payload) else {
        return NotifyOutcome::Rejected;
    };
    let Ok(body) = serde_json::to_vec(&projected) else {
        return NotifyOutcome::Rejected;
    };
    let Ok(private_body) = capture_private_turn_detail_if_enabled(root, payload) else {
        return NotifyOutcome::Unavailable;
    };
    let (path, body) = private_body
        .as_deref()
        .map_or(("/v1/notify", body.as_slice()), |private_body| {
            ("/v1/notify/private-detail", private_body)
        });
    match authenticated_request_until(
        root,
        "POST",
        path,
        Some(body),
        PRIVATE_NOTIFY_CONNECT_TIMEOUT,
        deadline,
    ) {
        Ok(response) if path == "/v1/notify" && response.status == 200 => NotifyOutcome::Accepted,
        Ok(response) if path == "/v1/notify/private-detail" => {
            match serde_json::from_slice::<PrivateTurnDetailReceiptV1>(&response.body) {
                Ok(receipt) if receipt.is_available(response.status) => NotifyOutcome::Accepted,
                Ok(_) | Err(_) => NotifyOutcome::Unavailable,
            }
        }
        Ok(_) => NotifyOutcome::Rejected,
        Err(_) => NotifyOutcome::Unavailable,
    }
}

fn capture_private_turn_detail_if_enabled(
    root: &Path,
    payload: &[u8],
) -> Result<Option<Vec<u8>>, CollectorError> {
    let config = load(&root.join("config.json")).map_err(runtime_error)?;
    if !config.enabled || !config.capture_private_codex_turn_details {
        return Ok(None);
    }
    let (projected, private_detail) =
        project_notify_with_private_detail(payload).map_err(runtime_error)?;
    let private_detail = private_detail.to_json().map_err(runtime_error)?;
    let envelope = serde_json::json!({
        "schema_version": PRIVATE_NOTIFY_ENVELOPE_VERSION,
        "projected": serde_json::to_value(projected).map_err(runtime_error)?,
        "private_detail": serde_json::from_slice::<serde_json::Value>(&private_detail)
            .map_err(runtime_error)?,
    });
    serde_json::to_vec(&envelope)
        .map(Some)
        .map_err(runtime_error)
}

fn acquire_private_turn_detail_mutation(
    runtime: &Path,
) -> Result<Option<MutationGuard>, CollectorError> {
    for attempt in 0..PRIVATE_TURN_DETAIL_LOCK_RETRIES {
        match MutationGuard::try_acquire(runtime) {
            Ok(mutation) => return Ok(Some(mutation)),
            Err(SingletonError::AlreadyRunning) => {
                if attempt + 1 < PRIVATE_TURN_DETAIL_LOCK_RETRIES {
                    thread::sleep(PRIVATE_TURN_DETAIL_LOCK_RETRY_DELAY);
                }
            }
            Err(error) => return Err(runtime_error(error)),
        }
    }
    Ok(None)
}

/// Reads one opt-in private Codex turn detail by the same hashed turn ID emitted in reports.
pub fn read_private_turn_detail(root: &Path, turn_id: &str) -> Result<Vec<u8>, CollectorError> {
    match lookup_private_turn_detail(root, turn_id) {
        PrivateTurnDetailLookup::Available(bytes) => Ok(bytes),
        PrivateTurnDetailLookup::NotCollected => Err(CollectorError::Runtime(
            "private turn detail was not collected".into(),
        )),
        PrivateTurnDetailLookup::Failed(code) => Err(CollectorError::Runtime(format!(
            "private turn detail unavailable: {code}"
        ))),
    }
}

#[must_use]
pub fn lookup_private_turn_detail(root: &Path, turn_id: &str) -> PrivateTurnDetailLookup {
    match lookup_private_turn_detail_inner(root, turn_id) {
        Ok(Some(bytes)) => PrivateTurnDetailLookup::Available(bytes),
        Ok(None) => PrivateTurnDetailLookup::NotCollected,
        Err(code) => PrivateTurnDetailLookup::Failed(code),
    }
}

fn lookup_private_turn_detail_inner(
    root: &Path,
    turn_id: &str,
) -> Result<Option<Vec<u8>>, &'static str> {
    let config = load(&root.join("config.json")).map_err(|_| "config_unavailable")?;
    if !config.enabled || !config.capture_private_codex_turn_details {
        return Ok(None);
    }
    let directory = root.join("state").join(PRIVATE_TURN_DETAIL_DIRECTORY);
    match fs::symlink_metadata(&directory) {
        Ok(_) => validate_private_directory_tree(&root.join("state"), &directory)
            .map_err(|_| "storage_unavailable")?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return match read_private_turn_detail_status(root, turn_id) {
                Ok(Some("ok")) => Err("artifact_missing"),
                Ok(Some(code)) | Err(code) => Err(code),
                Ok(None) => Ok(None),
            };
        }
        Err(_) => return Err("storage_unavailable"),
    }
    let path = private_turn_detail_path(&directory, turn_id).map_err(|_| "invalid_turn_id")?;
    let max_detail_bytes =
        u64::try_from(MAX_PRIVATE_TURN_DETAIL_BYTES).map_err(|_| "size_bound")?;
    let bytes = match read_private_bounded(&path, max_detail_bytes) {
        Ok(bytes) => bytes,
        Err(CollectorError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            return match read_private_turn_detail_status(root, turn_id) {
                Ok(Some("ok")) => Err("artifact_missing"),
                Ok(Some(code)) | Err(code) => Err(code),
                Ok(None) => Ok(None),
            };
        }
        Err(_) => return Err("storage_unavailable"),
    };
    let detail = PrivateCodexTurnDetailV1::from_json(&bytes).map_err(|_| "artifact_invalid")?;
    if detail.turn_id() != turn_id {
        return Err("artifact_invalid");
    }
    match read_private_turn_detail_status(root, turn_id) {
        Ok(Some("ok")) => {}
        Ok(Some(code)) | Err(code) => return Err(code),
        Ok(None) => return Err("status_missing"),
    }
    Ok(Some(bytes))
}

#[cfg(test)]
fn persist_private_turn_detail(
    layout: &InstalledLayout,
    detail: &PrivateCodexTurnDetailV1,
    config: &LocalRuntimeConfigV3,
) -> Result<(), CollectorError> {
    let _mutation = MutationGuard::try_acquire(&layout.runtime).map_err(runtime_error)?;
    persist_private_turn_detail_locked(layout, detail, config, 0)
}

#[cfg(test)]
fn capture_private_turn_detail(
    layout: &InstalledLayout,
    detail: &PrivateCodexTurnDetailV1,
    config: &LocalRuntimeConfigV3,
) -> Result<(), CollectorError> {
    let _mutation = MutationGuard::try_acquire(&layout.runtime).map_err(runtime_error)?;
    capture_private_turn_detail_locked(layout, detail, config).map(|_| ())
}

fn capture_private_turn_detail_locked(
    layout: &InstalledLayout,
    detail: &PrivateCodexTurnDetailV1,
    config: &LocalRuntimeConfigV3,
) -> Result<(PrivateTurnDetailReceiptState, &'static str), CollectorError> {
    let result = persist_private_turn_detail_locked(layout, detail, config, 0);
    let code = result
        .as_ref()
        .map_or_else(|error| private_turn_detail_error_code(error), |()| "ok");
    write_private_turn_detail_status_locked(
        layout,
        detail.turn_id(),
        if result.is_ok() {
            "available"
        } else {
            "failed"
        },
        code,
        config,
    )?;
    Ok((
        if result.is_ok() {
            PrivateTurnDetailReceiptState::Available
        } else {
            PrivateTurnDetailReceiptState::Failed
        },
        code,
    ))
}

fn persist_private_turn_detail_locked(
    layout: &InstalledLayout,
    detail: &PrivateCodexTurnDetailV1,
    config: &LocalRuntimeConfigV3,
    additional_reservation: u64,
) -> Result<(), CollectorError> {
    let directory = layout.state.join(PRIVATE_TURN_DETAIL_DIRECTORY);
    ensure_private_directory_tree(&layout.state, &directory)?;
    let path = private_turn_detail_path(&directory, detail.turn_id())?;
    let bytes = detail.to_json().map_err(runtime_error)?;
    match fs::symlink_metadata(&path) {
        Ok(_) => {
            let existing = read_private_bounded(
                &path,
                u64::try_from(MAX_PRIVATE_TURN_DETAIL_BYTES).map_err(|_| {
                    CollectorError::Runtime("private detail size bound overflow".into())
                })?,
            )?;
            if existing == bytes {
                return Ok(());
            }
            return Err(CollectorError::Runtime(
                "private turn detail conflict".into(),
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    // Test-only, content-free timings keep a failed latency gate attributable
    // without changing admission, durability, retention, or the time limit.
    #[cfg(test)]
    let prune_started = StdInstant::now();
    prune_private_turn_details(&directory, Some(&path), config, SystemTime::now())?;
    #[cfg(test)]
    eprintln!(
        "private_capture_stage=detail_prune elapsed={:?}",
        prune_started.elapsed()
    );
    let reservation = u64::try_from(MAX_PRIVATE_TURN_DETAIL_BYTES)
        .map_err(|_| CollectorError::Runtime("private detail size bound overflow".into()))?
        .checked_add(additional_reservation)
        .ok_or_else(|| CollectorError::Runtime("private detail size bound overflow".into()))?;
    #[cfg(test)]
    let admission_started = StdInstant::now();
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
    #[cfg(test)]
    eprintln!(
        "private_capture_stage=admission elapsed={:?}",
        admission_started.elapsed()
    );
    #[cfg(test)]
    let publication_started = StdInstant::now();
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
    #[cfg(test)]
    eprintln!(
        "private_capture_stage=detail_publication elapsed={:?}",
        publication_started.elapsed()
    );
    result
}

fn prune_private_turn_details(
    directory: &Path,
    replacement: Option<&Path>,
    config: &LocalRuntimeConfigV3,
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
    replacement: Option<&Path>,
    config: &LocalRuntimeConfigV3,
    now: SystemTime,
    max_files: usize,
) -> Result<(), CollectorError> {
    let max_detail_bytes = u64::try_from(MAX_PRIVATE_TURN_DETAIL_BYTES)
        .map_err(|_| CollectorError::Runtime("private detail size bound overflow".into()))?;
    prune_private_turn_files_with_limit(
        directory,
        replacement,
        config,
        now,
        max_files,
        max_detail_bytes,
    )
}

fn prune_private_turn_files_with_limit(
    directory: &Path,
    replacement: Option<&Path>,
    config: &LocalRuntimeConfigV3,
    now: SystemTime,
    max_files: usize,
    max_file_bytes: u64,
) -> Result<(), CollectorError> {
    if max_files == 0 || max_files > MAX_PRIVATE_TURN_DETAIL_FILES {
        return Err(CollectorError::Runtime(
            "private turn detail file bound is invalid".into(),
        ));
    }
    let max_age = Duration::from_secs(
        u64::from(if config.lifecycle.enabled {
            config.lifecycle.private_raw_days
        } else {
            config.retention.max_record_age_days
        })
        .checked_mul(24 * 60 * 60)
        .ok_or_else(|| CollectorError::Runtime("private detail retention overflow".into()))?,
    );
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
        if replacement.is_some_and(|replacement| path == replacement) {
            continue;
        }
        if path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with(".collector.json.tmp."))
        {
            let file = open_private_read(&path)?;
            let metadata = file.metadata()?;
            if metadata.len() > max_file_bytes {
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
        if metadata.len() > max_file_bytes {
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
    let retained_limit = max_files.saturating_sub(usize::from(replacement.is_some()));
    for (_, path) in retained.into_iter().skip(retained_limit) {
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

/// Applies the configured age/count policy to local-only raw Codex detail files.
///
/// This is invoked at collector startup and by explicit retention apply, so expiry does not depend
/// on a subsequent private capture.
pub fn maintain_private_turn_details(root: &Path) -> Result<(), CollectorError> {
    let layout = install(root).map_err(runtime_error)?;
    let config = load(&layout.config).map_err(runtime_error)?;
    let _mutation = MutationGuard::acquire(&layout.runtime).map_err(runtime_error)?;
    maintain_private_turn_details_locked(&layout, &config, SystemTime::now())
}

fn maintain_private_turn_details_locked(
    layout: &InstalledLayout,
    config: &LocalRuntimeConfigV3,
    now: SystemTime,
) -> Result<(), CollectorError> {
    let directory = layout.state.join(PRIVATE_TURN_DETAIL_DIRECTORY);
    maintain_private_turn_directory_locked(layout, &directory, config, now, None)?;
    let status_directory = private_turn_detail_status_directory(layout);
    maintain_private_turn_directory_locked(
        layout,
        &status_directory,
        config,
        now,
        Some(MAX_PRIVATE_TURN_DETAIL_STATUS_BYTES),
    )?;
    reconcile_private_turn_detail_pairs_locked(layout)
}

fn reconcile_private_turn_detail_pairs_locked(
    layout: &InstalledLayout,
) -> Result<(), CollectorError> {
    let directory = layout.state.join(PRIVATE_TURN_DETAIL_DIRECTORY);
    match fs::symlink_metadata(&directory) {
        Ok(_) => validate_private_directory_tree(&layout.state, &directory)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    }
    let mut removed = false;
    for (index, entry) in fs::read_dir(&directory)?.enumerate() {
        if index >= MAX_PRIVATE_TURN_DETAIL_SCAN_ENTRIES {
            return Err(CollectorError::Runtime(
                "private turn detail directory exceeds scan bound".into(),
            ));
        }
        let entry = entry?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            return Err(CollectorError::Runtime(
                "private turn detail entry is invalid".into(),
            ));
        };
        if name.starts_with(".collector.json.tmp.") {
            continue;
        }
        let digest = name.strip_suffix(".json").ok_or_else(|| {
            CollectorError::Runtime("private turn detail entry is invalid".into())
        })?;
        let turn_id = format!("id:sha256:{digest}");
        let expected = private_turn_detail_path(&directory, &turn_id)?;
        if expected != path {
            return Err(CollectorError::Runtime(
                "private turn detail entry is invalid".into(),
            ));
        }
        let _ = open_private_read(&path)?;
        if read_private_turn_detail_status(&layout.root, &turn_id) != Ok(Some("ok")) {
            fs::remove_file(path)?;
            removed = true;
        }
    }
    if removed {
        File::open(directory)?.sync_all()?;
    }
    Ok(())
}

fn maintain_private_turn_directory_locked(
    layout: &InstalledLayout,
    directory: &Path,
    config: &LocalRuntimeConfigV3,
    now: SystemTime,
    max_file_bytes: Option<u64>,
) -> Result<(), CollectorError> {
    match fs::symlink_metadata(directory) {
        Ok(_) => validate_private_directory_tree(&layout.state, directory)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    }
    if let Some(max_file_bytes) = max_file_bytes {
        prune_private_turn_files_with_limit(
            directory,
            None,
            config,
            now,
            MAX_PRIVATE_TURN_DETAIL_FILES,
            max_file_bytes,
        )
    } else {
        prune_private_turn_details(directory, None, config, now)
    }
}

fn private_turn_detail_error_code(error: &CollectorError) -> &'static str {
    match error {
        CollectorError::LifecycleStoragePressure | CollectorError::DashboardStorageCapacity => {
            "storage_budget"
        }
        CollectorError::Runtime(message) if message.contains("conflict") => "conflict",
        CollectorError::Runtime(message) if message.contains("storage budget") => "storage_budget",
        CollectorError::Runtime(message) if message.contains("already running") => "busy",
        CollectorError::Runtime(message) if message.contains("size") => "size_bound",
        CollectorError::Runtime(_) => "runtime_error",
        CollectorError::Io(_) | CollectorError::RequestIo { .. } => "io_error",
    }
}

fn private_turn_detail_status_directory(layout: &InstalledLayout) -> PathBuf {
    layout.state.join(PRIVATE_TURN_DETAIL_STATUS_DIRECTORY)
}

fn private_turn_detail_status_path(
    directory: &Path,
    turn_id: &str,
) -> Result<PathBuf, CollectorError> {
    private_turn_detail_path(directory, turn_id)
}

fn write_private_turn_detail_status_locked(
    layout: &InstalledLayout,
    turn_id: &str,
    state: &str,
    code: &str,
    config: &LocalRuntimeConfigV3,
) -> Result<(), CollectorError> {
    let directory = private_turn_detail_status_directory(layout);
    ensure_private_directory_tree(&layout.state, &directory)?;
    let path = private_turn_detail_status_path(&directory, turn_id)?;
    match fs::symlink_metadata(&path) {
        Ok(_) => {
            let _ = open_private_read(&path)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    #[cfg(test)]
    let prune_started = StdInstant::now();
    prune_private_turn_files_with_limit(
        &directory,
        Some(&path),
        config,
        SystemTime::now(),
        MAX_PRIVATE_TURN_DETAIL_FILES,
        MAX_PRIVATE_TURN_DETAIL_STATUS_BYTES,
    )?;
    #[cfg(test)]
    eprintln!(
        "private_capture_stage=status_prune elapsed={:?}",
        prune_started.elapsed()
    );
    let status = PrivateTurnDetailCaptureStatusV1 {
        schema_version: PRIVATE_TURN_DETAIL_STATUS_VERSION.into(),
        turn_id: turn_id.into(),
        state: state.into(),
        code: code.into(),
    };
    #[cfg(test)]
    let publication_started = StdInstant::now();
    let result = write_private_json(&path, &status);
    #[cfg(test)]
    eprintln!(
        "private_capture_stage=status_publication elapsed={:?}",
        publication_started.elapsed()
    );
    result
}

fn read_private_turn_detail_status(
    root: &Path,
    turn_id: &str,
) -> Result<Option<&'static str>, &'static str> {
    let state = root.join("state");
    let directory = state.join(PRIVATE_TURN_DETAIL_STATUS_DIRECTORY);
    match fs::symlink_metadata(&directory) {
        Ok(_) => validate_private_directory_tree(&state, &directory)
            .map_err(|_| "status_storage_unavailable")?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("status_storage_unavailable"),
    }
    let path =
        private_turn_detail_status_path(&directory, turn_id).map_err(|_| "invalid_turn_id")?;
    let bytes = match read_private_bounded(&path, MAX_PRIVATE_TURN_DETAIL_STATUS_BYTES) {
        Ok(bytes) => bytes,
        Err(CollectorError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(None);
        }
        Err(_) => return Err("status_storage_unavailable"),
    };
    let status: PrivateTurnDetailCaptureStatusV1 =
        serde_json::from_slice(&bytes).map_err(|_| "status_artifact_invalid")?;
    if status.schema_version != PRIVATE_TURN_DETAIL_STATUS_VERSION
        || status.turn_id != turn_id
        || status.state
            != if status.code == "ok" {
                "available"
            } else {
                "failed"
            }
    {
        return Err("status_artifact_invalid");
    }
    let code = match status.code.as_str() {
        "ok" => "ok",
        "storage_budget" => "storage_budget",
        "busy" => "busy",
        "size_bound" => "size_bound",
        "runtime_error" => "runtime_error",
        "io_error" => "io_error",
        "config_unavailable" => "config_unavailable",
        "capture_disabled" => "capture_disabled",
        "conflict" => "conflict",
        _ => return Err("status_artifact_invalid"),
    };
    Ok(Some(code))
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
    check_health_details(root).outcome
}

/// Preserves allowlisted degradation reasons from a versioned collector health response.
#[must_use]
pub fn check_health_details(root: &Path) -> HealthDetails {
    match authenticated_request(
        root,
        "GET",
        "/health",
        None,
        Duration::from_millis(50),
        Duration::from_millis(100),
    ) {
        Ok(response) if response.status == 200 => classify_health_details(&response.body),
        _ => HealthDetails {
            outcome: HealthOutcome::Unavailable,
            degradation_reasons: Vec::new(),
        },
    }
}

fn classify_health_details(body: &[u8]) -> HealthDetails {
    let outcome = classify_health_probe(body);
    let degradation_reasons = if outcome == HealthOutcome::Degraded {
        serde_json::from_slice::<HealthReasonProjection>(body)
            .ok()
            .filter(|projection| projection.schema_version == LOCAL_COLLECTOR_HEALTH_VERSION)
            .filter(|projection| projection.degradation_reasons.len() <= 3)
            .map_or_else(Vec::new, |projection| projection.degradation_reasons)
    } else {
        Vec::new()
    };
    HealthDetails {
        outcome,
        degradation_reasons,
    }
}

fn classify_health_probe(body: &[u8]) -> HealthOutcome {
    match serde_json::from_slice::<HealthProbe>(body) {
        Ok(HealthProbe {
            status,
            report_dirty: false,
        }) if status == "ready" => HealthOutcome::Ready,
        Ok(HealthProbe { status, .. }) if status == "degraded" => HealthOutcome::Degraded,
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
    authenticated_request_until(root, method, path, body, connect_timeout, deadline)
}

fn authenticated_request_until(
    root: &Path,
    method: &str,
    path: &str,
    body: Option<&[u8]>,
    connect_timeout: Duration,
    deadline: StdInstant,
) -> Result<AuthenticatedResponse, CollectorError> {
    let layout = inspect(root).map_err(runtime_error)?;
    let settings = load_settings_from_layout(&layout)?;
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

fn refresh_report_observing(
    layout: &InstalledLayout,
    store: &LocalStore,
    on_record: impl FnMut(usize),
) -> Result<bool, ReportFailure> {
    let mutation = try_report_mutation(layout)?;
    let barrier =
        agent_observability_local_runtime::storage_coherence::StorageBarrier::open_if_initialized(
            &layout.root,
        )
        .map_err(report_coherence_failure)?;
    let scope = ReportMutationScope::acquire(mutation, barrier.as_ref())?;
    let config = load(&layout.config).map_err(|_| ReportFailure::Publish)?;
    let control = RuntimeControl::new(&config).map_err(report_control_failure)?;
    if let Some(stale) = control
        .claim_stale_report_reservation(&layout.root, scope.mutation())
        .map_err(report_control_failure)?
    {
        // Keep both locks until cleanup succeeds. Failed cleanup preserves the stale promise.
        recover_report_view_catalog(store).map_err(report_catalog_failure)?;
        stale
            .release(&layout.root, scope.mutation())
            .map_err(|_| ReportFailure::Publish)?;
    }
    let admitted_bytes = automatic_report_view_admitted_bytes(layout, &config)?;
    let mut reservation = control
        .reserve_report_build(
            &layout.root,
            scope.mutation(),
            admitted_bytes + REPORT_VIEW_PUBLICATION_RESERVE_BYTES,
        )
        .map_err(report_control_failure)?;
    let staging = build_automatic_report_view_staging(
        store,
        admitted_bytes,
        barrier.as_ref(),
        |path, file| {
            reservation
                .bind_staging(&layout.root, scope.mutation(), path, file)
                .map_err(|_| ReportViewBuildError::InvalidStagingState)?;
            // Release only after the created descriptor is durably bound; projection must
            // not occupy the ingest mutation lock. The publication guard remains held.
            scope
                .revalidate()
                .map_err(|_| ReportViewBuildError::InvalidStagingState)?;
            drop(scope);
            Ok(())
        },
        on_record,
    )?;
    // Never wait while holding the publication guard. Other writers can own mutation and
    // attempt publication in the opposite order; contention is retryable, not a deadlock.
    let mutation = try_report_mutation(layout)?;
    let scope = ReportMutationScope::acquire(mutation, barrier.as_ref())?;
    // Settings may change during projection. Publication must obey the latest budget, not
    // the admission-time copy, even though the original reservation remains conservative.
    let config = load(&layout.config).map_err(|_| ReportFailure::Publish)?;
    let control = RuntimeControl::new(&config).map_err(report_control_failure)?;
    let remaining = control
        .reservation_finalization_headroom(&layout.root, scope.mutation(), &reservation)
        .map_err(report_control_failure)?;
    if remaining < REPORT_VIEW_PUBLICATION_RESERVE_BYTES {
        return Err(ReportFailure::Capacity);
    }
    reservation
        .validate_staging(
            &layout.root,
            scope.mutation(),
            staging.path(),
            staging.identity_file(),
        )
        .map_err(|_| ReportFailure::Publish)?;
    let publication = publish_report_view(store, staging).map_err(report_catalog_failure)?;
    if publication.cleanup_pending() {
        return Err(ReportFailure::Publish);
    }
    let acknowledged = acknowledge_report_reservation(
        layout,
        store,
        publication.current().generation(),
        scope.mutation(),
        reservation,
    )?;
    scope.revalidate()?;
    Ok(acknowledged)
}

fn acknowledge_report_reservation(
    layout: &InstalledLayout,
    store: &LocalStore,
    generation: u64,
    mutation: &MutationGuard,
    reservation: agent_observability_local_runtime::WriteReservation,
) -> Result<bool, ReportFailure> {
    let acknowledged = store
        .acknowledge_report_generation(generation)
        .map_err(|_| ReportFailure::Acknowledge)?;
    if !acknowledged {
        // Do not discharge an incomplete finalization. Guarded recovery will reconcile the
        // published catalog before a later attempt clears this now-stale reservation.
        return Err(ReportFailure::SnapshotChanged);
    }
    reservation
        .release(&layout.root, mutation)
        .map_err(|_| ReportFailure::Publish)?;
    Ok(acknowledged)
}

fn try_report_mutation(layout: &InstalledLayout) -> Result<MutationGuard, ReportFailure> {
    MutationGuard::try_acquire(&layout.runtime).map_err(|error| match error {
        SingletonError::AlreadyRunning => ReportFailure::RenderGuard,
        _ => ReportFailure::Publish,
    })
}

#[allow(clippy::needless_pass_by_value)] // Direct Result::map_err adapter consumes the source error.
fn report_control_failure(error: ControlError) -> ReportFailure {
    match error {
        ControlError::Reservation(ReservationError::Busy) => ReportFailure::RenderGuard,
        ControlError::Reservation(ReservationError::Capacity) => ReportFailure::Capacity,
        _ => ReportFailure::Publish,
    }
}

fn recover_report_reservation_for_startup(
    layout: &InstalledLayout,
    config: &LocalRuntimeConfigV3,
    mutation: &MutationGuard,
) -> Result<(), CollectorError> {
    let control = RuntimeControl::new(config).map_err(runtime_error)?;
    if let Some(stale) = control
        .claim_stale_report_reservation(&layout.root, mutation)
        .map_err(runtime_error)?
    {
        // A reservation is created only for an already current store. Recovery must not
        // require migration headroom that is still conservatively reserved by the stale owner.
        recover_report_view_catalog_before_migration(layout.state.join("store"))
            .map_err(runtime_error)?;
        stale
            .release(&layout.root, mutation)
            .map_err(runtime_error)?;
    }
    Ok(())
}

async fn cleanup_report_reservation_with_retry(
    state: &AppState,
    layout: &InstalledLayout,
    timing: ReportRefreshTiming,
) -> Result<(), ReportFailure> {
    let mut delay = timing.retry_initial;
    for attempt in 0..REPORT_RETRY_LIMIT {
        #[cfg(test)]
        state
            .report_snapshot_test
            .cleanup_attempts
            .fetch_add(1, Ordering::Release);
        #[cfg(not(test))]
        let _ = state;
        let layout = layout.clone();
        let result = tokio::task::spawn_blocking(move || cleanup_report_reservation(&layout))
            .await
            .map_err(|_| ReportFailure::Task)?;
        match result {
            Err(ReportFailure::RenderGuard) if attempt + 1 < REPORT_RETRY_LIMIT => {
                tokio::time::sleep(delay).await;
                delay = delay.saturating_mul(2);
            }
            result => return result,
        }
    }
    unreachable!("cleanup retry limit is nonzero")
}

fn cleanup_report_reservation(layout: &InstalledLayout) -> Result<(), ReportFailure> {
    let mutation = try_report_mutation(layout)?;
    let barrier =
        agent_observability_local_runtime::storage_coherence::StorageBarrier::open_if_initialized(
            &layout.root,
        )
        .map_err(report_coherence_failure)?;
    let scope = ReportMutationScope::acquire(mutation, barrier.as_ref())?;
    let config = load(&layout.config).map_err(|_| ReportFailure::Publish)?;
    let control = RuntimeControl::new(&config).map_err(|_| ReportFailure::Publish)?;
    if let Some(stale) = control
        .claim_stale_report_reservation(&layout.root, scope.mutation())
        .map_err(report_control_failure)?
    {
        recover_report_view_catalog_before_migration(layout.state.join("store"))
            .map_err(report_catalog_failure)?;
        stale
            .release(&layout.root, scope.mutation())
            .map_err(|error| report_control_failure(ControlError::Reservation(error)))?;
    }
    scope.revalidate()
}

fn automatic_report_view_missing(store: &LocalStore) -> Result<bool, CollectorError> {
    match current_report_view(store) {
        Ok(view) => match current_report_view_needs_kernel_upgrade(store) {
            Ok(needs_upgrade) => Ok(view.is_none() || needs_upgrade),
            Err(ReportViewCatalogError::Busy) => Ok(true),
            Err(error) => Err(runtime_error(error)),
        },
        // A staging build or destructive pass already owns publication. Start normally and let
        // the existing adaptive refresh scheduler converge after that bounded operation ends.
        Err(ReportViewCatalogError::Busy) => Ok(true),
        Err(error) => Err(runtime_error(error)),
    }
}

fn recover_report_view_catalog_for_startup(store: &LocalStore) -> Result<(), CollectorError> {
    recover_report_view_catalog(store).map_err(runtime_error)
}

fn automatic_report_view_admitted_bytes(
    layout: &InstalledLayout,
    config: &LocalRuntimeConfigV3,
) -> Result<u64, ReportFailure> {
    // Runtime accounting covers the entire managed tree, including current/retired/staging
    // sidecars. The builder keeps its SQLite journal reserve inside this per-generation amount.
    RuntimeControl::new(config)
        .map_err(|_| ReportFailure::Publish)?
        .writable_headroom(&layout.root)
        .map_err(|_| ReportFailure::Publish)
        .map(|headroom| {
            headroom
                .saturating_sub(REPORT_VIEW_PUBLICATION_RESERVE_BYTES)
                .saturating_sub(REPORT_RESERVATION_METADATA_ALLOWANCE)
                .min(MAX_AUTOMATIC_REPORT_VIEW_BYTES)
        })
}

fn build_automatic_report_view_staging(
    store: &LocalStore,
    admitted_bytes: u64,
    barrier: Option<&agent_observability_local_runtime::storage_coherence::StorageBarrier>,
    before_write: impl FnOnce(&Path, &File) -> Result<(), ReportViewBuildError>,
    on_record: impl FnMut(usize),
) -> Result<agent_observability_local_store::ReportViewStaging, ReportFailure> {
    #[cfg(not(test))]
    let on_record = {
        let _ = on_record;
        |_| {}
    };
    if let Some(barrier) = barrier {
        let mut permits = ReportWritePermits { barrier };
        return agent_observability_local_store::build_report_view_staging_bound_coordinated(
            store,
            MISSING_RATE_FINGERPRINT,
            admitted_bytes,
            None,
            before_write,
            &mut permits,
            on_record,
        )
        .map_err(|error| report_view_build_failure(&error));
    }
    agent_observability_local_store::build_report_view_staging_bound(
        store,
        MISSING_RATE_FINGERPRINT,
        admitted_bytes,
        None,
        before_write,
        on_record,
    )
    .map_err(|error| report_view_build_failure(&error))
}

fn report_view_build_failure(error: &ReportViewBuildError) -> ReportFailure {
    match error {
        ReportViewBuildError::Busy => ReportFailure::RenderGuard,
        ReportViewBuildError::SnapshotChanged
        | ReportViewBuildError::Store(
            agent_observability_local_store::StoreError::ReportSnapshotChanged,
        ) => ReportFailure::SnapshotChanged,
        ReportViewBuildError::Store(
            agent_observability_local_store::StoreError::ReportSnapshotRecordTooLarge { .. },
        ) => ReportFailure::Capacity,
        ReportViewBuildError::Store(_) => ReportFailure::Snapshot,
        ReportViewBuildError::Projection(_) | ReportViewBuildError::Json(_) => {
            ReportFailure::Projection
        }
        ReportViewBuildError::Sqlite(_)
        | ReportViewBuildError::Io(_)
        | ReportViewBuildError::InvalidRateFingerprint
        | ReportViewBuildError::CoordinationDenied
        | ReportViewBuildError::InvalidStagingState => ReportFailure::Publish,
        ReportViewBuildError::InvalidByteBudget | ReportViewBuildError::CapacityExceeded => {
            ReportFailure::Capacity
        }
    }
}

fn report_catalog_failure(error: ReportViewCatalogError) -> ReportFailure {
    match error {
        ReportViewCatalogError::Busy => ReportFailure::RenderGuard,
        ReportViewCatalogError::SnapshotChanged => ReportFailure::SnapshotChanged,
        ReportViewCatalogError::Store(_) => ReportFailure::Snapshot,
        ReportViewCatalogError::Build(error) => report_view_build_failure(&error),
        ReportViewCatalogError::Sqlite(_)
        | ReportViewCatalogError::Json(_)
        | ReportViewCatalogError::Io(_)
        | ReportViewCatalogError::SourceMismatch
        | ReportViewCatalogError::SnapshotExpired
        | ReportViewCatalogError::RefreshPending
        | ReportViewCatalogError::InvalidCatalog
        | ReportViewCatalogError::CatalogCapacityExceeded
        | ReportViewCatalogError::InvalidVisibilityAdvance => ReportFailure::Publish,
    }
}

fn open_store(
    _mutation: &MutationGuard,
    layout: &InstalledLayout,
    config: &LocalRuntimeConfigV3,
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

#[cfg(test)]
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

#[cfg(test)]
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
    #[test]
    fn lifecycle_health_degradation_does_not_require_a_dirty_report() {
        assert_eq!(
            super::classify_health_probe(
                br#"{"status":"degraded","report_dirty":false,"lifecycle_failures":1}"#
            ),
            super::HealthOutcome::Degraded
        );
        assert_eq!(
            super::classify_health_probe(br#"{"status":"ready","report_dirty":true}"#),
            super::HealthOutcome::Unavailable
        );
        assert_eq!(
            super::classify_health_probe(br#"{"status":"unknown","report_dirty":false}"#),
            super::HealthOutcome::Unavailable
        );
    }

    #[test]
    fn lifecycle_quiet_period_protects_recent_and_future_ingest() {
        assert!(super::lifecycle_idle(None, 0));
        assert!(!super::lifecycle_idle(Some(1_000), 30_999));
        assert!(super::lifecycle_idle(Some(1_000), 31_000));
        assert!(!super::lifecycle_idle(Some(32_000), 31_000));
    }

    #[test]
    fn lifecycle_disabled_does_not_require_or_create_store() {
        let root = std::env::temp_dir().join(format!(
            "agentobs-lifecycle-disabled-{}-{}",
            std::process::id(),
            super::current_unix_ms().unwrap()
        ));
        let layout = agent_observability_local_runtime::install(&root).unwrap();
        assert_eq!(
            super::maintain_storage_lifecycle(&root).unwrap(),
            "lifecycle=disabled"
        );
        assert!(!layout.state.join("store").exists());
        let mutation_guard =
            agent_observability_local_runtime::MutationGuard::acquire(&layout.runtime).unwrap();
        assert_eq!(
            super::maintain_storage_lifecycle(&root).unwrap(),
            "lifecycle=busy"
        );
        drop(mutation_guard);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn lifecycle_idle_preserves_existing_report() {
        let root = test_root("lifecycle-idle-report");
        let state = collector_state(&root);
        let layout = state.layout.clone();
        drop(state);
        let guard = ConfigMutationGuard::acquire(&layout).unwrap();
        let mut config = load(&layout.config).unwrap();
        config.lifecycle.enabled = true;
        save(&guard, &config).unwrap();
        drop(guard);
        let report_path = layout.logs.join(REPORT_FILE_NAME);
        let report = project_report(
            &[],
            "2026-09-07T00:00:00.000Z",
            "Agent Observability Report",
            None,
        )
        .unwrap();
        write_private(&report_path, &report).unwrap();
        let before = fs::read(&report_path).unwrap();
        assert_eq!(
            super::maintain_storage_lifecycle(&root).unwrap(),
            "lifecycle=idle"
        );
        assert_eq!(fs::read(&report_path).unwrap(), before);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn versioned_health_reasons_are_preserved_without_guessing_legacy_causes() {
        use agent_observability_contracts::CollectorDegradationReasonV1 as Reason;
        let body = serde_json::to_vec(&serde_json::json!({
            "schema_version": super::LOCAL_COLLECTOR_HEALTH_VERSION,
            "status": "degraded", "report_dirty": false,
            "degradation_reasons": ["storage_pressure", "expired_trace"]
        }))
        .unwrap();
        let details = super::classify_health_details(&body);
        assert_eq!(details.outcome, super::HealthOutcome::Degraded);
        assert_eq!(
            details.degradation_reasons,
            vec![Reason::StoragePressure, Reason::ExpiredTrace]
        );
        for body in [
            br#"{"status":"degraded","report_dirty":false,"lifecycle_failures":1}"#.as_slice(),
            br#"{"schema_version":"local_collector_health.v1","status":"degraded","report_dirty":false,"degradation_reasons":["future_reason"]}"#,
            br#"{"schema_version":"future.v2","status":"degraded","report_dirty":false,"degradation_reasons":["storage_pressure"]}"#,
        ] {
            let details = super::classify_health_details(body);
            assert_eq!(details.outcome, super::HealthOutcome::Degraded);
            assert!(details.degradation_reasons.is_empty());
        }
        assert_eq!(
            super::lifecycle_degradation_reasons(1, false, Some(0)),
            vec![Reason::LifecycleFailure]
        );
        assert_eq!(
            super::lifecycle_degradation_reasons(1, true, Some(1)),
            vec![Reason::StoragePressure, Reason::ExpiredTrace]
        );
        assert!(super::lifecycle_degradation_reasons(0, false, Some(0)).is_empty());
    }

    #[test]
    fn lifecycle_pass_health_tracks_last_pass_and_preserves_busy_state() {
        let root = test_root("lifecycle-pass-health");
        let state = app_state(&root);
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                super::record_lifecycle_pass(&state, "lifecycle=blocked\nreport_refresh=pending");
                assert_eq!(state.lifecycle_failures.load(Ordering::Acquire), 1);
                assert!(state.report_refresh_scheduled.load(Ordering::Acquire));
                super::record_lifecycle_pass(&state, "lifecycle=busy");
                assert_eq!(state.lifecycle_failures.load(Ordering::Acquire), 1);
                state
                    .lifecycle_storage_pressure
                    .store(true, Ordering::Release);
                super::record_lifecycle_pass(&state, "lifecycle=busy");
                assert!(state.lifecycle_storage_pressure.load(Ordering::Acquire));
                for output in [
                    "lifecycle=completed",
                    "lifecycle=idle",
                    "lifecycle=disabled",
                ] {
                    state.lifecycle_failures.store(1, Ordering::Release);
                    super::record_lifecycle_pass(&state, output);
                    assert_eq!(state.lifecycle_failures.load(Ordering::Acquire), 0);
                    assert!(!state.lifecycle_storage_pressure.load(Ordering::Acquire));
                }
            });
        drop(state);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn lifecycle_oversized_trace_reports_blocked_without_deleting_it() {
        let root = test_root("lifecycle-blocked");
        let mut state = collector_state(&root);
        let layout = state.layout.clone();
        let body = br#"{"resourceLogs":[{"scopeLogs":[{"logRecords":[
          {"timeUnixNano":"1000000000000000000","attributes":[
            {"key":"event.name","value":{"stringValue":"codex.conversation_starts"}},
            {"key":"conversation.id","value":{"stringValue":"blocked-trace"}}]},
          {"timeUnixNano":"1000000000000000000","attributes":[
            {"key":"event.name","value":{"stringValue":"codex.api_request"}},
            {"key":"conversation.id","value":{"stringValue":"blocked-trace"}},
            {"key":"auth.request_id","value":{"stringValue":"blocked-request"}}]}
        ]}]}]}"#;
        ingest_locked(&mut state, body).unwrap();
        let count = state.store.record_count().unwrap();
        assert!(count > 1);
        let guard = ConfigMutationGuard::acquire(&layout).unwrap();
        let mut config = load(&layout.config).unwrap();
        config.lifecycle.enabled = true;
        config.retention.max_archive_records = 1;
        config.collection.local_storage_budget_bytes = 256 * 1024 * 1024;
        config.retention.max_archive_bytes = 256 * 1024 * 1024;
        save(&guard, &config).unwrap();
        drop(guard);
        assert!(matches!(
            super::maintain_storage_lifecycle(&root),
            Err(super::CollectorError::LifecycleStoragePressure)
        ));
        assert_eq!(state.store.record_count().unwrap(), count);
        let guard = ConfigMutationGuard::acquire(&layout).unwrap();
        config.retention.max_archive_bytes = 65_536;
        save(&guard, &config).unwrap();
        drop(guard);
        let output = super::maintain_storage_lifecycle(&root).unwrap();
        assert!(output.starts_with("lifecycle=blocked\n"), "{output}");
        assert_eq!(state.store.record_count().unwrap(), count);
        drop(state);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn lifecycle_publication_lock_protects_report_and_expiry() {
        let root = test_root("lifecycle-publication-lock");
        let mut state = collector_state(&root);
        let layout = state.layout.clone();
        let body = br#"{"resourceLogs":[{"scopeLogs":[{"logRecords":[{"timeUnixNano":"1000000000000000000","attributes":[{"key":"event.name","value":{"stringValue":"codex.conversation_starts"}},{"key":"conversation.id","value":{"stringValue":"old-lifecycle-trace"}}]}]}]}]}"#;
        ingest_locked(&mut state, body).unwrap();
        assert_eq!(state.store.record_count().unwrap(), 1);
        refresh_report_from_root(&root).unwrap();
        let snapshot = state.store.report_snapshot().unwrap();
        let report = project_report(
            &snapshot.records,
            "2026-09-07T00:00:00.000Z",
            "Agent Observability Report",
            None,
        )
        .unwrap();
        write_private(&layout.logs.join(REPORT_FILE_NAME), &report).unwrap();
        let config_guard = ConfigMutationGuard::acquire(&layout).unwrap();
        let mut config = load(&layout.config).unwrap();
        config.lifecycle.enabled = true;
        save(&config_guard, &config).unwrap();
        drop(config_guard);
        let render_guard = state.store.acquire_report_render_guard().unwrap();
        let path = layout.logs.join(REPORT_FILE_NAME);
        let before = fs::read(&path).unwrap();
        assert_eq!(
            super::maintain_storage_lifecycle(&root).unwrap(),
            "lifecycle=busy"
        );
        assert_eq!(fs::read(&path).unwrap(), before);
        assert_eq!(state.store.record_count().unwrap(), 1);
        drop(render_guard);
        let result = super::maintain_storage_lifecycle(&root).unwrap();
        assert!(result.contains("lifecycle=completed"));
        assert_eq!(state.store.record_count().unwrap(), 0);
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .contains("리포트 갱신 대기")
        );
        assert!(state.store.report_status().unwrap().pending());
        refresh_report_from_root(&root).unwrap();
        assert!(!state.store.report_status().unwrap().pending());
        assert_published_report_view(&root, 0);
        ingest_notify_locked(
            &mut state,
            &projected_notify("old-lifecycle-trace", "new-after-expiry"),
        )
        .unwrap();
        assert_eq!(state.store.expired_trace_disposition_count().unwrap(), 1);
        let health_state = app_state(&root);
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let response = super::health(State(health_state)).await.into_response();
                let body = axum::body::to_bytes(response.into_body(), 4096)
                    .await
                    .unwrap();
                let health: serde_json::Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(health["expired_trace_dispositions"], 1);
                assert_eq!(health["status"], "degraded");
                assert_eq!(
                    health["schema_version"],
                    super::LOCAL_COLLECTOR_HEALTH_VERSION
                );
                assert_eq!(
                    health["degradation_reasons"],
                    serde_json::json!(["expired_trace"])
                );
                let schema: serde_json::Value = serde_json::from_str(
                    agent_observability_contracts::LOCAL_COLLECTOR_HEALTH_SCHEMA,
                )
                .unwrap();
                assert_eq!(
                    health.as_object().unwrap().keys().collect::<Vec<_>>(),
                    schema["properties"]
                        .as_object()
                        .unwrap()
                        .keys()
                        .collect::<Vec<_>>(),
                );
            });
        drop(state);
        fs::remove_dir_all(root).unwrap();
    }

    use super::{
        AUTH_HEADER_NAME, AppState, CollectorState, IngestError, IngestOutcome, LocalStore,
        MISSING_RATE_FINGERPRINT, NotifyOutcome, OtlpRejectionCategory,
        OtlpRequestCorrelationState, OtlpSubmissionOutcome, PrivateTurnDetailLookup,
        REPORT_FILE_NAME, ReportFailure, admit_request, authenticated_request, build_client_config,
        build_server_config, capture_private_turn_detail, capture_private_turn_detail_if_enabled,
        capture_private_turn_detail_locked, classify_otlp_rejection, current_report_view,
        enforce_batch_policy, ensure_private_directory_tree, ingest_locked, ingest_notify_locked,
        ingest_notify_with_private_detail, install_settings, is_json, load_settings,
        lookup_private_turn_detail, maintain_private_turn_details_locked, open_store,
        parse_complete_http_response, persist_private_turn_detail,
        persist_private_turn_detail_locked, persist_private_turn_detail_request,
        private_turn_detail_error_code, private_turn_detail_path,
        private_turn_detail_status_directory, private_turn_detail_status_path, project_report,
        prune_private_turn_details_with_limit, read_private_snapshot, read_private_turn_detail,
        read_private_turn_detail_status, reconcile_report_state, recover_occupied_persisted_port,
        recover_report_view_catalog_for_startup, refresh_dashboard_snapshot,
        refresh_report_from_root, report_dirty_path, router, schedule_report_refresh,
        settings_path, submit_notify, submit_otlp_json_outcome, timestamp_from_unix_ms,
        token_matches, watch_report_authority, write_private, write_private_json,
        write_private_json_if_unchanged, write_private_turn_detail_status_locked,
    };
    use agent_observability_adapter_codex::{
        MAX_HANDOFF_BYTES, parse_otlp_http_json, project_notify_with_private_detail,
    };
    use agent_observability_local_runtime::{
        Admission, ConfigMutationGuard, MutationGuard, RuntimeControl, StorageBudget, install,
        load, save,
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
        time::{Duration, Instant, SystemTime},
    };
    use tokio::sync::Mutex;

    #[test]
    fn report_failure_health_codes_are_bounded_stage_names() {
        for (failure, expected) in [
            (ReportFailure::Task, "\"task\""),
            (ReportFailure::Install, "\"install\""),
            (ReportFailure::OpenStore, "\"open_store\""),
            (ReportFailure::RenderGuard, "\"render_guard\""),
            (ReportFailure::Snapshot, "\"snapshot\""),
            (ReportFailure::SnapshotChanged, "\"snapshot\""),
            (ReportFailure::Projection, "\"projection\""),
            (ReportFailure::Publish, "\"publish\""),
            (ReportFailure::Capacity, "\"publish\""),
            (ReportFailure::Acknowledge, "\"acknowledge\""),
            (ReportFailure::Status, "\"status\""),
        ] {
            assert_eq!(serde_json::to_string(&failure).unwrap(), expected);
        }
    }

    include!("rotation_diagnostic.rs");

    fn test_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "agent-observability-collector-{name}-{}",
            std::process::id()
        ))
    }

    fn assert_published_report_view(root: &Path, expected_records: usize) {
        let store = LocalStore::open_current(root.join("state/store")).unwrap();
        let snapshot = current_report_view(&store).unwrap().unwrap();
        assert_eq!(snapshot.records(), expected_records);
        assert_eq!(snapshot.rate_fingerprint(), MISSING_RATE_FINGERPRINT);
        assert_eq!(
            snapshot.generation(),
            store.report_status().unwrap().acknowledged_generation
        );
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
        config: &agent_observability_local_runtime::LocalRuntimeConfigV3,
    ) -> agent_observability_local_store::LocalStore {
        let mutation = MutationGuard::acquire(&layout.runtime).unwrap();
        open_store(&mutation, layout, config).unwrap()
    }

    fn app_state(root: &Path) -> AppState {
        let auth_token = install_settings(root).unwrap().auth_token;
        let private_detail_failures = Arc::new(AtomicU64::new(0));
        AppState {
            collector: Arc::new(Mutex::new(collector_state(root))),
            auth_token: Arc::from(auth_token),
            private_detail_failures,
            lifecycle_failures: Arc::new(AtomicU64::new(0)),
            lifecycle_storage_pressure: Arc::new(AtomicBool::new(false)),
            report_refresh_scheduled: Arc::new(AtomicBool::new(false)),
            report_refresh_requested: Arc::new(AtomicU64::new(0)),
            report_contention_quiet_ms: Arc::new(AtomicU64::new(0)),
            report_refresh_attempts: Arc::new(AtomicU64::new(0)),
            report_snapshot_test: Arc::default(),
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
    fn report_refresh_retries_after_config_mutation_without_waiting() {
        let root = test_root("open-rebuild-mutation");
        let _ = fs::remove_dir_all(&root);
        let state = collector_state(&root);
        let layout = state.layout.clone();
        drop(state);
        let guard = ConfigMutationGuard::acquire(&layout).unwrap();

        assert!(!refresh_dashboard_snapshot(&root).unwrap());
        drop(guard);
        assert!(refresh_report_from_root(&root).unwrap());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn snapshot_admission_preserves_writable_headroom_and_publication_reserve() {
        let root = test_root("snapshot-writable-admission");
        let _ = fs::remove_dir_all(&root);
        let state = collector_state(&root);
        let mut config = load(&state.layout.config).unwrap();
        assert_eq!(
            config.collection.local_storage_budget_bytes,
            1024 * 1024 * 1024
        );
        assert_eq!(super::MAX_AUTOMATIC_REPORT_VIEW_BYTES, 256 * 1024 * 1024);
        config.collection.local_storage_budget_bytes = 256 * 1024 * 1024;
        let budget =
            StorageBudget::calculate(config.collection.local_storage_budget_bytes, false).unwrap();
        let allocated = StorageBudget::allocated_tree_bytes(&root).unwrap();
        let admitted = super::automatic_report_view_admitted_bytes(&state.layout, &config).unwrap();
        assert!(admitted < super::MAX_AUTOMATIC_REPORT_VIEW_BYTES);
        assert!(allocated + admitted + 64 * 1024 <= budget.writable_limit());
        drop(state);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn automatic_report_refresh_publishes_a_readable_empty_index() {
        let root = test_root("empty-index-publication");
        let _ = fs::remove_dir_all(&root);
        let state = collector_state(&root);
        let config = load(&state.layout.config).unwrap();
        let admitted = super::automatic_report_view_admitted_bytes(&state.layout, &config).unwrap();
        assert!(admitted > 0);
        assert!(admitted <= super::MAX_AUTOMATIC_REPORT_VIEW_BYTES);
        drop(state);

        assert!(super::refresh_dashboard_snapshot(&root).unwrap());
        assert_published_report_view(&root, 0);
        assert!(!root.join("logs").join(REPORT_FILE_NAME).exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn collector_startup_recovers_interrupted_report_view_files_before_catalog_read() {
        let root = test_root("startup-report-view-recovery");
        let _ = fs::remove_dir_all(&root);
        let state = collector_state(&root);
        assert!(refresh_dashboard_snapshot(&root).unwrap());
        let published = current_report_view(&state.store).unwrap().unwrap();
        let orphan = root
            .join("state/store/report-views.v1")
            .join(".report-view.sqlite3.staging.interrupted");
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut interrupted = options.open(&orphan).unwrap();
        interrupted.write_all(b"interrupted").unwrap();
        interrupted.sync_all().unwrap();
        drop(interrupted);

        recover_report_view_catalog_for_startup(&state.store).unwrap();

        assert!(!orphan.exists());
        assert_eq!(current_report_view(&state.store).unwrap(), Some(published));
        drop(state);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn standalone_dashboard_refresh_reports_busy_as_retryable() {
        let root = test_root("standalone-dashboard-busy");
        let _ = fs::remove_dir_all(&root);
        let state = collector_state(&root);
        let guard = state.store.acquire_report_render_guard().unwrap();

        assert!(!super::refresh_dashboard_snapshot(&root).unwrap());
        assert!(current_report_view(&state.store).is_err());

        drop(guard);
        drop(state);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn standalone_dashboard_capacity_mapping_remains_typed() {
        assert_eq!(
            super::report_view_build_failure(
                &agent_observability_local_store::ReportViewBuildError::InvalidByteBudget
            ),
            ReportFailure::Capacity
        );
        assert_eq!(
            super::CollectorError::DashboardStorageCapacity.to_string(),
            "dashboard snapshot storage headroom unavailable"
        );
    }

    #[test]
    fn standalone_dashboard_refresh_maps_oversized_authority_record_to_capacity() {
        let root = test_root("standalone-dashboard-oversized-authority-record");
        let _ = fs::remove_dir_all(&root);
        let mut state = collector_state(&root);
        let body = br#"{"resourceLogs":[{"scopeLogs":[{"logRecords":[{
          "timeUnixNano":"1787875200000000000",
          "attributes":[
            {"key":"event.name","value":{"stringValue":"codex.conversation_starts"}},
            {"key":"conversation.id","value":{"stringValue":"conversation-capacity"}},
            {"key":"model","value":{"stringValue":"gpt-5.6-sol"}}
          ]
        }]}]}]}"#;
        let (batch, _) = parse_otlp_http_json(body, "codex-test", None, 1, 0).unwrap();
        let mut observation = match batch.items.into_iter().next().unwrap() {
            agent_observability_adapter_codex::AdapterItem::Observation(observation) => observation,
            agent_observability_adapter_codex::AdapterItem::Disposition(_) => {
                panic!("conversation start must produce an authority observation")
            }
        };
        observation.event = agent_observability_contracts::ObservationEvent::Session {
            model: Some(format!("gpt-5.6-sol-{}", "x".repeat(2 * 1024 * 1024))),
            project: Some("agent-observability".to_owned()),
        };
        state.store.ingest(&observation).unwrap();
        let record = state.store.current_records().unwrap().pop().unwrap();
        assert!(serde_json::to_vec(&record).unwrap().len() > 2 * 1024 * 1024);
        drop(state);

        assert!(matches!(
            refresh_dashboard_snapshot(&root),
            Err(super::CollectorError::DashboardStorageCapacity)
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn standalone_dashboard_refresh_keeps_ordinary_failures_as_errors() {
        let root = test_root("standalone-dashboard-ordinary-error");
        let _ = fs::remove_dir_all(&root);
        let layout = install(&root).unwrap();
        assert!(!layout.state.join("store").exists());

        assert!(matches!(
            refresh_dashboard_snapshot(&root),
            Err(super::CollectorError::Runtime(message))
                if message == "dashboard snapshot refresh failed at open_store"
        ));
        assert!(!layout.state.join("store").exists());
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

    #[test]
    fn collector_admission_denies_full_store_reservation_when_batch_alone_fits() {
        let root = test_root("collector-full-store-admission");
        let _ = fs::remove_dir_all(&root);
        let state = collector_state(&root);
        fs::write(
            state.layout.state.join("store/admission-fixture"),
            vec![0_u8; 2 * 1024 * 1024],
        )
        .unwrap();
        inflate_allocated_accounting(&root);
        let guard = ConfigMutationGuard::acquire(&state.layout).unwrap();
        let mut config = load(&state.layout.config).unwrap();
        config.collection.local_storage_budget_bytes = 256 * 1024 * 1024;
        save(&guard, &config).unwrap();
        drop(guard);
        let control = RuntimeControl::new(&config).unwrap();
        let max_batch_bytes = u64::from(config.collection.max_batch_bytes);
        for index in (0..300).rev() {
            if matches!(
                control.admit(&root, max_batch_bytes).unwrap(),
                Admission::Allowed { .. }
            ) {
                break;
            }
            let link = root.join(format!("allocated-budget-link-{index}"));
            if link.exists() {
                fs::remove_file(link).unwrap();
            }
        }

        assert!(matches!(
            control.admit(&root, max_batch_bytes).unwrap(),
            Admission::Allowed { .. }
        ));
        let diagnostic = control
            .collector_admission_diagnostic(&root, max_batch_bytes)
            .unwrap();
        assert!(diagnostic.existing_store_allocated_bytes > max_batch_bytes);
        assert_eq!(diagnostic.admission, Admission::Denied);
        assert!(diagnostic.deficit_bytes > 0);
        assert!(matches!(
            admit_request(&state, usize::try_from(max_batch_bytes).unwrap()),
            Err(IngestError::Storage)
        ));
        drop(state);
        fs::remove_dir_all(root).unwrap();
    }

    fn projected_notify(thread: &str, turn: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "schema_version": "codex_projected_notify.v2",
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

    fn report_refresh_diagnostics(state: &AppState) -> String {
        format!(
            "attempts={} scheduled={} requested={} quiet_ms={} state={:?}",
            state.report_refresh_attempts.load(Ordering::Acquire),
            state.report_refresh_scheduled.load(Ordering::Acquire),
            state.report_refresh_requested.load(Ordering::Acquire),
            state.report_contention_quiet_ms.load(Ordering::Acquire),
            state.collector.try_lock().ok().map(|collector| (
                collector.report_dirty,
                collector.report_degraded,
                collector.report_refresh_failures,
                collector.report_failure,
                collector
                    .store
                    .report_status()
                    .ok()
                    .map(agent_observability_local_store::ReportStatus::pending),
            )),
        )
    }

    async fn wait_for_report_refresh_completion(state: &AppState) {
        // After the last wakeup, debounce can finish its current quiet interval and
        // repeat it once. This is a finite hang guard, not a publication latency SLO.
        let guard = super::REPORT_CONTENTION_QUIET_LIMIT * 2 + Duration::from_secs(5);
        let completed = tokio::time::timeout(guard, async {
            loop {
                let collector = state.collector.lock().await;
                let published = collector.store.report_status().unwrap();
                if !state.report_refresh_scheduled.load(Ordering::Acquire) && !published.pending() {
                    break;
                }
                drop(collector);
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await;
        assert!(
            completed.is_ok(),
            "refresh did not converge: {}",
            report_refresh_diagnostics(state)
        );
    }

    #[test]
    fn report_refresh_contention_quiet_obeys_duration_and_ceiling_without_wall_clock() {
        for (previous, attempt, expected) in [
            (20, 120, 480),
            (480, 1, 960),
            (20, 900, 3600),
            (20, 8000, 30_000),
            (30_000, 1, 30_000),
        ] {
            assert_eq!(
                super::contention_quiet_period(
                    Duration::from_millis(previous),
                    Duration::from_millis(attempt)
                ),
                Duration::from_millis(expected),
            );
        }
        assert_eq!(
            super::contention_quiet_period(Duration::MAX, Duration::MAX),
            super::REPORT_CONTENTION_QUIET_LIMIT,
        );
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
        assert_eq!(
            submit_notify(&missing, &raw_notify("thread-missing", "turn-missing")),
            NotifyOutcome::Unavailable
        );
        assert!(!missing.exists());

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
                // This is an authenticated functional-success check, not a claim
                // that a loaded debug CI host completes durable ingest in 250ms.
                super::submit_notify_until(
                    &root,
                    &raw_notify("thread-ok", "turn-ok"),
                    super::StdInstant::now() + Duration::from_secs(5),
                )
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

    #[test]
    fn notify_expired_deadline_never_connects_or_changes_the_foreground_budget() {
        assert_eq!(
            super::PRIVATE_NOTIFY_FOREGROUND_DEADLINE,
            Duration::from_millis(250)
        );
        let root = test_root("notify-expired-deadline");
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        configure_port(&root, listener.local_addr().unwrap().port());
        let expired = super::StdInstant::now();
        assert_eq!(
            super::submit_notify_until(&root, &raw_notify("expired", "expired"), expired),
            NotifyOutcome::Unavailable
        );
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        assert_eq!(
            super::submit_notify_until(&root, b"{}", expired),
            NotifyOutcome::Rejected
        );
        let _ = fs::remove_dir_all(root);
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
        capture_private_turn_detail(&layout, &first_detail, &config).unwrap();

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
        assert!(persist_private_turn_detail(&layout, &replacement_detail, &config).is_err());
        persist_private_turn_detail(&layout, &first_detail, &config).unwrap();
        let preserved: serde_json::Value =
            serde_json::from_slice(&read_private_turn_detail(&root, &turn_id).unwrap()).unwrap();
        assert_eq!(preserved["cwd"], "/RAW_PATH_SECRET");
        assert_eq!(preserved["inputMessages"][0], "RAW_INPUT_SECRET");
        assert_eq!(preserved["lastAssistantMessage"], "RAW_OUTPUT_SECRET");

        set_private_turn_details(&root, false);
        assert!(read_private_turn_detail(&root, &turn_id).is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn private_turn_detail_conflicting_retry_preserves_first_artifact_and_is_explicit() {
        let root = test_root("private-turn-detail-conflict");
        let _ = fs::remove_dir_all(&root);
        let layout = install(&root).unwrap();
        set_private_turn_details(&root, true);
        let config = load(&layout.config).unwrap();
        let (_, first) =
            project_notify_with_private_detail(&raw_notify("thread-conflict", "turn-conflict"))
                .unwrap();
        let replacement = serde_json::to_vec(&serde_json::json!({
            "type": "agent-turn-complete",
            "thread-id": "thread-conflict",
            "turn-id": "turn-conflict",
            "cwd": "/different/project",
            "input-messages": ["DIFFERENT_PRIVATE_INPUT"],
            "last-assistant-message": null,
        }))
        .unwrap();
        let (_, conflicting) = project_notify_with_private_detail(&replacement).unwrap();
        let mutation = MutationGuard::acquire(&layout.runtime).unwrap();
        assert_eq!(
            capture_private_turn_detail_locked(&layout, &first, &config).unwrap(),
            (super::PrivateTurnDetailReceiptState::Available, "ok")
        );
        assert_eq!(
            capture_private_turn_detail_locked(&layout, &first, &config).unwrap(),
            (super::PrivateTurnDetailReceiptState::Available, "ok")
        );
        assert_eq!(
            capture_private_turn_detail_locked(&layout, &conflicting, &config).unwrap(),
            (super::PrivateTurnDetailReceiptState::Failed, "conflict")
        );
        assert_eq!(
            lookup_private_turn_detail(&root, first.turn_id()),
            PrivateTurnDetailLookup::Failed("conflict")
        );
        let path = private_turn_detail_path(
            &layout.state.join(super::PRIVATE_TURN_DETAIL_DIRECTORY),
            first.turn_id(),
        )
        .unwrap();
        let preserved = fs::read_to_string(path).unwrap();
        assert!(preserved.contains("RAW_INPUT_SECRET"));
        assert!(!preserved.contains("DIFFERENT_PRIVATE_INPUT"));
        drop(mutation);
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn private_turn_detail_status_failure_refuses_success_acknowledgement() {
        let root = test_root("private-turn-detail-status-publication-failure");
        let _ = fs::remove_dir_all(&root);
        let layout = install(&root).unwrap();
        set_private_turn_details(&root, true);
        let status_directory = private_turn_detail_status_directory(&layout);
        write_private_test_file(&status_directory, b"not-a-directory");

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let state = app_state(&root);
            let body = capture_private_turn_detail_if_enabled(
                &root,
                &raw_notify("thread-status-failure", "turn-status-failure"),
            )
            .unwrap()
            .unwrap();
            let response =
                ingest_notify_with_private_detail(State(state.clone()), body.into()).await;
            assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
            let body = axum::body::to_bytes(response.into_body(), 4 * 1024)
                .await
                .unwrap();
            let receipt: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(receipt["state"], "failed");
            assert_eq!(receipt["code"], "status_publication");
            assert_eq!(state.collector.lock().await.accepted_requests, 1);
            let (_, detail) = project_notify_with_private_detail(&raw_notify(
                "thread-status-failure",
                "turn-status-failure",
            ))
            .unwrap();
            assert_eq!(
                lookup_private_turn_detail(&root, detail.turn_id()),
                PrivateTurnDetailLookup::Failed("status_storage_unavailable")
            );
            assert_eq!(state.private_detail_failures.load(Ordering::Acquire), 1);
        });
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn private_turn_detail_foreground_handoff_does_not_write_without_a_collector() {
        let root = test_root("private-turn-detail-no-collector-write");
        let _ = fs::remove_dir_all(&root);
        let layout = install(&root).unwrap();
        set_private_turn_details(&root, true);

        assert_eq!(
            submit_notify(
                &root,
                &raw_notify("thread-no-collector", "turn-no-collector")
            ),
            NotifyOutcome::Unavailable
        );
        assert!(
            !layout
                .state
                .join(super::PRIVATE_TURN_DETAIL_DIRECTORY)
                .exists()
        );
        assert!(!private_turn_detail_status_directory(&layout).exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn private_detail_envelope_rejects_cross_turn_correlation() {
        let root = test_root("private-detail-cross-turn");
        let _ = fs::remove_dir_all(&root);
        set_private_turn_details(&root, true);
        let state = app_state(&root);
        let (projected, _) =
            project_notify_with_private_detail(&raw_notify("thread-correlation", "turn-canonical"))
                .unwrap();
        let (_, other_detail) = project_notify_with_private_detail(&raw_notify(
            "thread-correlation",
            "turn-private-other",
        ))
        .unwrap();
        let body = serde_json::to_vec(&serde_json::json!({
            "schema_version": super::PRIVATE_NOTIFY_ENVELOPE_VERSION,
            "projected": serde_json::to_value(projected).unwrap(),
            "private_detail": serde_json::from_slice::<serde_json::Value>(
                &other_detail.to_json().unwrap(),
            )
            .unwrap(),
        }))
        .unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let response =
                ingest_notify_with_private_detail(State(state.clone()), body.into()).await;
            assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
            assert_eq!(state.collector.lock().await.accepted_requests, 0);
        });
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn private_turn_detail_endpoint_keeps_raw_content_out_of_canonical_artifacts() {
        let root = test_root("private-turn-detail-private-only");
        let _ = fs::remove_dir_all(&root);
        let layout = install(&root).unwrap();
        set_private_turn_details(&root, true);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            // Privacy is independent of host scheduling inside the foreground 250ms budget.
            // Dedicated transport/deadline tests retain that production timing contract.
            let envelope = capture_private_turn_detail_if_enabled(
                &root,
                &raw_notify("thread-private-only", "turn-private-only"),
            )
            .unwrap()
            .unwrap();
            let response =
                ingest_notify_with_private_detail(State(app_state(&root)), envelope.into()).await;
            assert_eq!(response.status(), StatusCode::OK);
            let (_, detail) = project_notify_with_private_detail(&raw_notify(
                "thread-private-only",
                "turn-private-only",
            ))
            .unwrap();
            let stored = tokio::time::timeout(Duration::from_secs(1), async {
                loop {
                    if let Ok(stored) = read_private_turn_detail(&root, detail.turn_id()) {
                        break stored;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            assert!(
                stored
                    .windows(b"RAW_PATH_SECRET".len())
                    .any(|part| part == b"RAW_PATH_SECRET")
            );
            assert!(
                stored
                    .windows(b"RAW_INPUT_SECRET".len())
                    .any(|part| part == b"RAW_INPUT_SECRET")
            );
            assert!(
                stored
                    .windows(b"RAW_OUTPUT_SECRET".len())
                    .any(|part| part == b"RAW_OUTPUT_SECRET")
            );
        });
        drop(runtime);
        refresh_report_from_root(&root).unwrap();
        fs::remove_dir_all(layout.state.join(super::PRIVATE_TURN_DETAIL_DIRECTORY)).unwrap();
        fs::remove_dir_all(private_turn_detail_status_directory(&layout)).unwrap();
        assert_tree_excludes(
            &root,
            &[
                b"RAW_PATH_SECRET",
                b"RAW_INPUT_SECRET",
                b"RAW_OUTPUT_SECRET",
            ],
        );
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
            Some(&paths[0]),
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
            Some(&paths[0]),
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
    fn private_turn_detail_maintenance_expires_without_a_later_capture_at_exact_boundary() {
        let root = test_root("private-turn-detail-maintenance");
        let _ = fs::remove_dir_all(&root);
        let layout = install(&root).unwrap();
        set_private_turn_details(&root, true);
        let config = load(&layout.config).unwrap();
        let (_, detail) =
            project_notify_with_private_detail(&raw_notify("thread-expire", "turn-expire"))
                .unwrap();
        capture_private_turn_detail(&layout, &detail, &config).unwrap();
        let directory = layout.state.join(super::PRIVATE_TURN_DETAIL_DIRECTORY);
        let path = private_turn_detail_path(&directory, detail.turn_id()).unwrap();
        let modified = fs::metadata(&path).unwrap().modified().unwrap();
        let max_age =
            Duration::from_secs(u64::from(config.retention.max_record_age_days) * 24 * 60 * 60);

        maintain_private_turn_details_locked(&layout, &config, modified + max_age).unwrap();
        assert!(path.is_file(), "the exact retention boundary is inclusive");

        maintain_private_turn_details_locked(
            &layout,
            &config,
            modified + max_age + Duration::from_nanos(1),
        )
        .unwrap();
        assert!(
            !path.exists(),
            "startup/retention maintenance expires detail without another capture"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn lifecycle_raw_retention_is_independent_and_opt_in() {
        let root = test_root("lifecycle-raw-retention");
        let layout = install(&root).unwrap();
        set_private_turn_details(&root, true);
        let mut config = load(&layout.config).unwrap();
        config.lifecycle.private_raw_days = 1;
        let (_, detail) = project_notify_with_private_detail(&raw_notify(
            "thread-lifecycle-raw",
            "turn-lifecycle-raw",
        ))
        .unwrap();
        capture_private_turn_detail(&layout, &detail, &config).unwrap();
        let path = private_turn_detail_path(
            &layout.state.join(super::PRIVATE_TURN_DETAIL_DIRECTORY),
            detail.turn_id(),
        )
        .unwrap();
        let modified = fs::metadata(&path).unwrap().modified().unwrap();
        let now = modified + Duration::from_hours(25);
        maintain_private_turn_details_locked(&layout, &config, now).unwrap();
        assert!(
            path.is_file(),
            "disabled lifecycle preserves legacy raw retention"
        );
        config.lifecycle.enabled = true;
        maintain_private_turn_details_locked(&layout, &config, now).unwrap();
        assert!(
            !path.exists(),
            "enabled lifecycle uses the independent raw age"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn lifecycle_raw_expiry_runs_even_when_store_is_unavailable() {
        let root = test_root("lifecycle-raw-without-store");
        let layout = install(&root).unwrap();
        set_private_turn_details(&root, true);
        let guard = ConfigMutationGuard::acquire(&layout).unwrap();
        let mut config = load(&layout.config).unwrap();
        config.lifecycle.enabled = true;
        config.lifecycle.private_raw_days = 1;
        save(&guard, &config).unwrap();
        drop(guard);
        let (_, detail) =
            project_notify_with_private_detail(&raw_notify("raw-no-db", "turn-no-db")).unwrap();
        capture_private_turn_detail(&layout, &detail, &config).unwrap();
        let path = private_turn_detail_path(
            &layout.state.join(super::PRIVATE_TURN_DETAIL_DIRECTORY),
            detail.turn_id(),
        )
        .unwrap();
        let old = std::time::SystemTime::now() - Duration::from_hours(25);
        fs::File::open(&path)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(old))
            .unwrap();
        assert!(super::maintain_storage_lifecycle(&root).is_err());
        assert!(
            !path.exists(),
            "raw expiry does not depend on a usable canonical store"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn private_turn_detail_capture_lock_serializes_accounting_and_publication() {
        let root = test_root("private-turn-detail-concurrent-lock");
        let _ = fs::remove_dir_all(&root);
        let layout = install(&root).unwrap();
        set_private_turn_details(&root, true);
        let config = load(&layout.config).unwrap();
        let (_, detail) =
            project_notify_with_private_detail(&raw_notify("thread-lock", "turn-lock")).unwrap();
        let guard = MutationGuard::acquire(&layout.runtime).unwrap();
        let error = capture_private_turn_detail(&layout, &detail, &config).unwrap_err();
        assert_eq!(private_turn_detail_error_code(&error), "busy");
        assert_eq!(
            read_private_turn_detail_status(&root, detail.turn_id()),
            Ok(None)
        );
        drop(guard);
        capture_private_turn_detail(&layout, &detail, &config).unwrap();
        assert!(read_private_turn_detail(&root, detail.turn_id()).is_ok());
        assert_eq!(
            read_private_turn_detail_status(&root, detail.turn_id()),
            Ok(Some("ok"))
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn private_turn_detail_without_terminal_status_is_not_exposed() {
        let root = test_root("private-turn-detail-missing-status");
        let _ = fs::remove_dir_all(&root);
        let layout = install(&root).unwrap();
        set_private_turn_details(&root, true);
        let config = load(&layout.config).unwrap();
        let (_, detail) = project_notify_with_private_detail(&raw_notify(
            "thread-missing-status",
            "turn-missing-status",
        ))
        .unwrap();
        let mutation = MutationGuard::acquire(&layout.runtime).unwrap();
        persist_private_turn_detail_locked(&layout, &detail, &config, 0).unwrap();
        drop(mutation);

        assert_eq!(
            lookup_private_turn_detail(&root, detail.turn_id()),
            PrivateTurnDetailLookup::Failed("status_missing")
        );
        let detail_path = private_turn_detail_path(
            &layout.state.join(super::PRIVATE_TURN_DETAIL_DIRECTORY),
            detail.turn_id(),
        )
        .unwrap();
        assert!(detail_path.is_file());

        maintain_private_turn_details_locked(&layout, &config, SystemTime::now()).unwrap();
        assert!(!detail_path.exists());
        assert_eq!(
            lookup_private_turn_detail(&root, detail.turn_id()),
            PrivateTurnDetailLookup::NotCollected
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn private_turn_detail_receipt_requires_consistent_success_tuple() {
        for http_status in [200, 202, 400, 503] {
            for state in [
                super::PrivateTurnDetailReceiptState::Available,
                super::PrivateTurnDetailReceiptState::Failed,
                super::PrivateTurnDetailReceiptState::Busy,
            ] {
                for code in ["ok", "busy", "conflict", "future_code"] {
                    let receipt = super::PrivateTurnDetailReceiptV1 {
                        schema_version: super::PRIVATE_TURN_DETAIL_RECEIPT_VERSION.into(),
                        state,
                        code: code.into(),
                    };
                    assert_eq!(
                        receipt.is_available(http_status),
                        http_status == 200
                            && state == super::PrivateTurnDetailReceiptState::Available
                            && code == "ok"
                    );
                }
            }
        }
    }

    #[test]
    fn private_turn_detail_contradictory_status_is_not_exposed_or_retained() {
        let root = test_root("private-turn-detail-contradictory-status");
        let layout = install(&root).unwrap();
        set_private_turn_details(&root, true);
        let config = load(&layout.config).unwrap();
        let (_, detail) =
            project_notify_with_private_detail(&raw_notify("thread-tuple", "turn-tuple")).unwrap();
        for (state, code) in [("failed", "ok"), ("available", "busy"), ("future", "ok")] {
            capture_private_turn_detail(&layout, &detail, &config).unwrap();
            super::write_private_turn_detail_status_locked(
                &layout,
                detail.turn_id(),
                state,
                code,
                &config,
            )
            .unwrap();
            assert_eq!(
                lookup_private_turn_detail(&root, detail.turn_id()),
                PrivateTurnDetailLookup::Failed("status_artifact_invalid")
            );
            maintain_private_turn_details_locked(&layout, &config, SystemTime::now()).unwrap();
            assert!(
                !private_turn_detail_path(
                    &layout.state.join(super::PRIVATE_TURN_DETAIL_DIRECTORY),
                    detail.turn_id()
                )
                .unwrap()
                .exists()
            );
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn private_turn_detail_success_status_without_artifact_is_explicit() {
        let root = test_root("private-turn-detail-missing-artifact");
        let _ = fs::remove_dir_all(&root);
        let layout = install(&root).unwrap();
        set_private_turn_details(&root, true);
        let config = load(&layout.config).unwrap();
        let (_, detail) = project_notify_with_private_detail(&raw_notify(
            "thread-missing-artifact",
            "turn-missing-artifact",
        ))
        .unwrap();
        capture_private_turn_detail(&layout, &detail, &config).unwrap();
        let detail_path = private_turn_detail_path(
            &layout.state.join(super::PRIVATE_TURN_DETAIL_DIRECTORY),
            detail.turn_id(),
        )
        .unwrap();
        fs::remove_file(detail_path).unwrap();

        assert_eq!(
            lookup_private_turn_detail(&root, detail.turn_id()),
            PrivateTurnDetailLookup::Failed("artifact_missing")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn private_turn_detail_foreground_path_is_bounded_at_status_capacity_and_lock_contention() {
        use std::os::unix::fs::OpenOptionsExt;

        let root = test_root("private-turn-detail-foreground-bound");
        let _ = fs::remove_dir_all(&root);
        let layout = install(&root).unwrap();
        set_private_turn_details(&root, true);
        let config = load(&layout.config).unwrap();
        let status_directory = private_turn_detail_status_directory(&layout);
        super::ensure_private_directory_tree(&layout.state, &status_directory).unwrap();
        for index in 0..super::MAX_PRIVATE_TURN_DETAIL_FILES {
            let path = status_directory.join(format!("{index:064x}.json"));
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .open(path)
                .unwrap();
            file.write_all(b"{}").unwrap();
        }

        let state = app_state(&root);
        let (_, detail) =
            project_notify_with_private_detail(&raw_notify("thread-capacity", "turn-capacity"))
                .unwrap();
        let started = Instant::now();
        let response = persist_private_turn_detail_request(&state, &layout, &config, &detail);
        let elapsed = started.elapsed();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            elapsed < Duration::from_millis(250),
            "max-cardinality private capture exceeded foreground deadline: {elapsed:?}"
        );
        assert!(matches!(
            lookup_private_turn_detail(&root, detail.turn_id()),
            PrivateTurnDetailLookup::Available(_)
        ));

        let (_, contended_detail) =
            project_notify_with_private_detail(&raw_notify("thread-contended", "turn-contended"))
                .unwrap();
        let guard = MutationGuard::acquire(&layout.runtime).unwrap();
        let started = Instant::now();
        let response =
            persist_private_turn_detail_request(&state, &layout, &config, &contended_detail);
        let elapsed = started.elapsed();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(
            elapsed < super::PRIVATE_NOTIFY_FOREGROUND_DEADLINE,
            "contended private capture exceeded bounded retries: {elapsed:?}"
        );
        drop(guard);

        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn private_turn_detail_lookup_distinguishes_not_collected_capture_failure_and_corruption() {
        let root = test_root("private-turn-detail-health");
        let _ = fs::remove_dir_all(&root);
        let layout = install(&root).unwrap();
        set_private_turn_details(&root, true);
        let (_, detail) =
            project_notify_with_private_detail(&raw_notify("thread-health", "turn-health"))
                .unwrap();
        let (_, other_detail) =
            project_notify_with_private_detail(&raw_notify("thread-health", "turn-other")).unwrap();
        assert_eq!(
            lookup_private_turn_detail(&root, detail.turn_id()),
            PrivateTurnDetailLookup::NotCollected
        );

        let config = load(&layout.config).unwrap();
        let guard = MutationGuard::acquire(&layout.runtime).unwrap();
        write_private_turn_detail_status_locked(
            &layout,
            detail.turn_id(),
            "failed",
            "storage_budget",
            &config,
        )
        .unwrap();
        write_private_turn_detail_status_locked(
            &layout,
            other_detail.turn_id(),
            "failed",
            "io_error",
            &config,
        )
        .unwrap();
        drop(guard);
        assert_eq!(
            lookup_private_turn_detail(&root, detail.turn_id()),
            PrivateTurnDetailLookup::Failed("storage_budget")
        );
        assert_eq!(
            lookup_private_turn_detail(&root, other_detail.turn_id()),
            PrivateTurnDetailLookup::Failed("io_error")
        );

        let status_directory = private_turn_detail_status_directory(&layout);
        let status_path =
            private_turn_detail_status_path(&status_directory, other_detail.turn_id()).unwrap();
        write_private_test_file(&status_path, b"{broken");
        assert_eq!(
            lookup_private_turn_detail(&root, other_detail.turn_id()),
            PrivateTurnDetailLookup::Failed("status_artifact_invalid")
        );

        let directory = layout.state.join(super::PRIVATE_TURN_DETAIL_DIRECTORY);
        ensure_private_directory_tree(&layout.state, &directory).unwrap();
        let path = private_turn_detail_path(&directory, detail.turn_id()).unwrap();
        write_private_test_file(&path, b"{broken");
        assert_eq!(
            lookup_private_turn_detail(&root, detail.turn_id()),
            PrivateTurnDetailLookup::Failed("artifact_invalid")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn private_turn_detail_status_uses_bounded_diagnostic_headroom() {
        let root = test_root("private-turn-detail-diagnostic-headroom");
        let _ = fs::remove_dir_all(&root);
        let layout = install(&root).unwrap();
        set_private_turn_details(&root, true);
        let guard = ConfigMutationGuard::acquire(&layout).unwrap();
        let mut config = load(&layout.config).unwrap();
        config.collection.local_storage_budget_bytes = 256 * 1024 * 1024;
        save(&guard, &config).unwrap();
        drop(guard);
        inflate_allocated_accounting(&root);
        let (_, detail) = project_notify_with_private_detail(&raw_notify(
            "thread-diagnostic-headroom",
            "turn-diagnostic-headroom",
        ))
        .unwrap();
        let _mutation = MutationGuard::acquire(&layout.runtime).unwrap();
        write_private_turn_detail_status_locked(
            &layout,
            detail.turn_id(),
            "failed",
            "storage_budget",
            &config,
        )
        .unwrap();
        assert_eq!(
            lookup_private_turn_detail(&root, detail.turn_id()),
            PrivateTurnDetailLookup::Failed("storage_budget")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn invalid_private_capture_config_is_an_explicit_failure() {
        let root = test_root("private-turn-detail-invalid-config");
        let _ = fs::remove_dir_all(&root);
        let layout = install(&root).unwrap();
        write_private_test_file(&layout.config, b"{broken");
        let error = capture_private_turn_detail_if_enabled(
            &root,
            &raw_notify("thread-invalid-config", "turn-invalid-config"),
        )
        .unwrap_err();
        assert_eq!(private_turn_detail_error_code(&error), "runtime_error");
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
        assert_published_report_view(&root, 2);
        assert!(!layout.logs.join(REPORT_FILE_NAME).exists());
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
        assert_published_report_view(&root, 10);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn report_refresh_contention_waits_for_render_sized_quiet_period() {
        let root = test_root("report-refresh-slow-snapshot-contention");
        let state = app_state(&root);
        state
            .report_snapshot_test
            .delay_ms
            .store(120, Ordering::Release);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            {
                let mut collector = state.collector.lock().await;
                ingest_notify_locked(&mut collector, &projected_notify("initial", "turn-1"))
                    .unwrap();
            }
            super::schedule_report_refresh_with_timing(&state, fast_report_timing());
            tokio::time::timeout(Duration::from_secs(2), async {
                while !state.report_snapshot_test.started.load(Ordering::Acquire) {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            })
            .await
            .unwrap();
            // Commits are farther apart than debounce but closer than a slow snapshot.
            // The first conflict must not start repeated full scans during the stream.
            for event in 0..8 {
                {
                    let mut collector = state.collector.lock().await;
                    ingest_notify_locked(
                        &mut collector,
                        &projected_notify(&format!("slow-{event}"), "turn-1"),
                    )
                    .unwrap();
                }
                super::schedule_report_refresh_with_timing(&state, fast_report_timing());
                tokio::time::sleep(Duration::from_millis(45)).await;
            }
            assert_eq!(state.report_refresh_attempts.load(Ordering::Acquire), 1);
            assert!(
                state
                    .collector
                    .lock()
                    .await
                    .store
                    .report_status()
                    .unwrap()
                    .pending()
            );
            assert!(state.collector.lock().await.report_degraded);
            assert_eq!(state.collector.lock().await.report_refresh_failures, 0);
            assert!(state.report_contention_quiet_ms.load(Ordering::Acquire) >= 480);
            state
                .report_snapshot_test
                .delay_ms
                .store(0, Ordering::Release);
            wait_for_report_refresh_completion(&state).await;
            let collector = state.collector.lock().await;
            assert!(!collector.store.report_status().unwrap().pending());
            assert_eq!(collector.report_refresh_failures, 0);
            assert!(!collector.report_degraded);
            drop(collector);
            // A successful cycle and a new wakeup must not discard the learned quiet window.
            let attempts = state.report_refresh_attempts.load(Ordering::Acquire);
            super::schedule_report_refresh_with_timing(&state, fast_report_timing());
            tokio::time::sleep(Duration::from_millis(100)).await;
            assert_eq!(
                state.report_refresh_attempts.load(Ordering::Acquire),
                attempts
            );
        });
        assert_published_report_view(&root, 9);
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

        assert_published_report_view(&root, 2);
        assert_eq!(state.report_refresh_attempts.load(Ordering::Acquire), 2);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn report_refresh_retries_transient_failure_and_converges_to_latest_generation() {
        let root = test_root("report-retry");
        let _ = fs::remove_dir_all(&root);
        let state = app_state(&root);
        let report_views = root.join("state/store/report-views.v1");
        fs::write(&report_views, b"occupied").unwrap();
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
            // Keep the obstruction until the scheduler has recorded a real failure.
            // A sleep shorter than debounce could otherwise test only a successful first try.
            let failed = tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let collector = state.collector.lock().await;
                    if collector.report_refresh_failures > 0
                        && collector.report_failure == Some(super::ReportFailure::Snapshot)
                    {
                        break;
                    }
                    drop(collector);
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await;
            assert!(
                failed.is_ok(),
                "injected failure was not observed: {}",
                report_refresh_diagnostics(&state)
            );
            assert!(state.report_refresh_scheduled.load(Ordering::Acquire));
            let failed_attempts = state.report_refresh_attempts.load(Ordering::Acquire);
            assert!(failed_attempts > 0);

            {
                let mut collector = state.collector.lock().await;
                ingest_notify_locked(&mut collector, &projected_notify("thread-2", "turn-2"))
                    .unwrap();
            }
            schedule_report_refresh(&state);
            fs::remove_file(&report_views).unwrap();

            wait_for_report_refresh_completion(&state).await;
            assert!(state.report_refresh_attempts.load(Ordering::Acquire) > failed_attempts);
            let collector = state.collector.lock().await;
            assert_eq!(collector.report_refresh_failures, 0);
            assert!(!collector.report_degraded);
            assert!(collector.report_failure.is_none());
        });

        assert_published_report_view(&root, 2);
        assert!(!report_dirty_path(&state.collector.blocking_lock().layout).exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn report_authority_watcher_converges_an_external_store_commit() {
        assert_external_commit_converges(0);
    }

    #[test]
    fn report_authority_watcher_respects_a_learned_quiet_window() {
        assert_external_commit_converges(2_500);
    }

    fn assert_external_commit_converges(quiet_ms: u64) {
        let root = test_root(&format!("report-external-store-commit-{quiet_ms}"));
        let _ = fs::remove_dir_all(&root);
        let state = app_state(&root);
        state
            .report_contention_quiet_ms
            .store(quiet_ms, Ordering::Release);
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

            // This is a convergence test, not a two-second publication SLO. A learned
            // quiet window can legally outlast that old harness deadline. Keep a
            // finite guard beyond the production quiet ceiling; performance has its
            // own measured protocol rather than this shared-runner wall clock.
            let convergence = tokio::time::timeout(
                super::REPORT_CONTENTION_QUIET_LIMIT + Duration::from_secs(5),
                async {
                    loop {
                        let published = {
                            let collector = state.collector.lock().await;
                            current_report_view(&collector.store)
                                .is_ok_and(|view| view.is_some_and(|view| view.records() == 1))
                        };
                        if state.report_refresh_attempts.load(Ordering::Acquire) > 0
                            && !state.report_refresh_scheduled.load(Ordering::Acquire)
                            && published
                        {
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                },
            )
            .await;
            watcher.abort();
            let _ = watcher.await;
            assert!(
                convergence.is_ok(),
                "external commit convergence timed out: attempts={} scheduled={} quiet_ms={} state={:?}",
                state.report_refresh_attempts.load(Ordering::Acquire),
                state.report_refresh_scheduled.load(Ordering::Acquire),
                state.report_contention_quiet_ms.load(Ordering::Acquire),
                state.collector.try_lock().ok().map(|collector| (
                    collector.report_dirty,
                    collector.report_degraded,
                    collector.report_refresh_failures,
                    collector.report_failure,
                )),
            );
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
        assert_published_report_view(&root, 1);
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
        assert_published_report_view(&root, 2);
        assert!(
            fs::read_to_string(&report_path)
                .unwrap()
                .contains(r#""generatedSpans":1"#)
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn report_refresh_does_not_wait_for_busy_runtime_mutation_guard() {
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
        let refresh = thread::spawn(move || refresh_dashboard_snapshot(&refresh_root));

        assert!(!refresh.join().unwrap().unwrap());
        assert!(!projection.exists());
        drop(render_guard);
        assert!(!refresh_dashboard_snapshot(&root).unwrap());
        drop(mutation);
        assert!(refresh_dashboard_snapshot(&root).unwrap());
        assert!(!projection.exists());
        assert_eq!(collector.store.record_count().unwrap(), 1);
        assert!(!collector.store.report_status().unwrap().pending());
        assert_published_report_view(&root, 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn report_refresh_does_not_hold_runtime_mutation_guard_during_projection() {
        let root = test_root("report-reservation-short-mutation-lock");
        let mut collector = collector_state(&root);
        ingest_notify_locked(&mut collector, &projected_notify("thread-1", "turn-1")).unwrap();
        let mut observed = false;
        assert!(
            super::refresh_report_from_root_observing(&root, |_| {
                let guard = MutationGuard::try_acquire(&collector.layout.runtime)
                    .expect("projection must not hold the ingest mutation lock");
                let metadata: serde_json::Value = serde_json::from_slice(
                    &fs::read(collector.layout.runtime.join("report-reservation.meta")).unwrap(),
                )
                .unwrap();
                assert_eq!(
                    metadata["version"], 2,
                    "staging must already be durably bound"
                );
                let config = load(&collector.layout.config).unwrap();
                let control = RuntimeControl::new(&config).unwrap();
                let allocated = StorageBudget::allocated_tree_bytes(&root).unwrap();
                let unreserved = control.storage_budget().writable_limit() - allocated;
                assert_eq!(control.admit(&root, unreserved).unwrap(), Admission::Denied);
                assert!(matches!(
                    control.admit(&root, 1).unwrap(),
                    Admission::Allowed { .. }
                ));
                observed = true;
                drop(guard);
            })
            .unwrap()
        );
        assert!(observed);
        drop(collector);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn report_refresh_honors_initialized_accounting_between_bounded_writes() {
        use agent_observability_local_runtime::storage_coherence::{
            StorageBarrier, StorageCoherenceError,
        };
        let root = test_root("report-accounting-permit-boundary");
        let mut collector = collector_state(&root);
        ingest_notify_locked(&mut collector, &projected_notify("thread-1", "turn-1")).unwrap();
        let mutation = MutationGuard::try_acquire(&collector.layout.runtime).unwrap();
        let barrier = StorageBarrier::initialize(&collector.layout.root, &mutation).unwrap();
        drop(mutation);
        let blocked_writer = barrier.try_begin_write().unwrap();
        assert!(!refresh_dashboard_snapshot(&root).unwrap());
        assert!(
            !collector
                .layout
                .runtime
                .join("report-reservation.meta")
                .exists()
        );
        drop(blocked_writer);
        let mut observed = false;
        assert!(
            super::refresh_report_from_root_observing(&root, |_| {
                let mutation = MutationGuard::try_acquire(&collector.layout.runtime).unwrap();
                assert!(matches!(
                    barrier.try_freeze(&mutation),
                    Err(StorageCoherenceError::Busy)
                ));
                observed = true;
            })
            .unwrap()
        );
        assert!(observed);
        assert_published_report_view(&root, 1);
        let mutation = MutationGuard::try_acquire(&collector.layout.runtime).unwrap();
        barrier.try_freeze(&mutation).unwrap().revalidate().unwrap();
        assert!(
            !collector
                .layout
                .runtime
                .join("report-reservation.meta")
                .exists()
        );
        drop(mutation);
        drop(collector);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn report_barrier_replacement_stops_projection_and_preserves_recovery_promise() {
        use agent_observability_local_runtime::storage_coherence::StorageBarrier;
        use std::os::unix::fs::OpenOptionsExt;
        let root = test_root("report-accounting-replacement");
        let mut collector = collector_state(&root);
        for index in 0..129 {
            ingest_notify_locked(
                &mut collector,
                &projected_notify(&format!("thread-{index}"), "turn-1"),
            )
            .unwrap();
        }
        let mutation = MutationGuard::try_acquire(&collector.layout.runtime).unwrap();
        let barrier = StorageBarrier::initialize(&collector.layout.root, &mutation).unwrap();
        drop(mutation);
        let path = collector.layout.runtime.join("storage-accounting.lock");
        let original = collector.layout.runtime.join("original-accounting.lock");
        let metadata = collector.layout.runtime.join("report-reservation.meta");
        let mut observed = 0;
        let mut promise = None;
        let result = super::refresh_report_from_root_observing(&root, |_| {
            observed += 1;
            if promise.is_none() {
                promise = Some(fs::read(&metadata).unwrap());
                fs::rename(&path, &original).unwrap();
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(&path)
                    .unwrap();
            }
        });
        assert_eq!(result.unwrap_err(), super::ReportFailure::Publish);
        assert_eq!(observed, 128);
        assert_eq!(fs::read(&metadata).unwrap(), promise.unwrap());
        assert!(current_report_view(&collector.store).unwrap().is_none());
        assert!(collector.store.report_status().unwrap().pending());
        assert!(barrier.revalidate().is_err());
        // Restore only this test's explicitly retained lock, then exercise guarded recovery.
        fs::remove_file(&path).unwrap();
        fs::rename(&original, &path).unwrap();
        barrier.revalidate().unwrap();
        assert!(refresh_dashboard_snapshot(&root).unwrap());
        assert_published_report_view(&root, 129);
        assert!(!metadata.exists());
        drop(collector);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn report_cleanup_defers_while_accounting_writer_is_active() {
        use agent_observability_local_runtime::storage_coherence::StorageBarrier;
        let root = test_root("report-cleanup-accounting-contention");
        let collector = collector_state(&root);
        let mutation = MutationGuard::try_acquire(&collector.layout.runtime).unwrap();
        let barrier = StorageBarrier::initialize(&collector.layout.root, &mutation).unwrap();
        drop(mutation);
        let writer = barrier.try_begin_write().unwrap();
        assert_eq!(
            super::cleanup_report_reservation(&collector.layout),
            Err(super::ReportFailure::RenderGuard)
        );
        drop(writer);
        assert_eq!(super::cleanup_report_reservation(&collector.layout), Ok(()));
        let mutation = MutationGuard::try_acquire(&collector.layout.runtime).unwrap();
        barrier.try_freeze(&mutation).unwrap().revalidate().unwrap();
        drop(mutation);
        drop(collector);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn report_finalization_contention_preserves_reservation_until_guarded_recovery() {
        for coordinated in [false, true] {
            assert_report_finalization_recovers(coordinated);
        }
    }

    fn assert_report_finalization_recovers(coordinated: bool) {
        let root = test_root(&format!(
            "report-reservation-finalization-contention-{coordinated}"
        ));
        let mut collector = collector_state(&root);
        ingest_notify_locked(&mut collector, &projected_notify("thread-1", "turn-1")).unwrap();
        if coordinated {
            let mutation = MutationGuard::try_acquire(&collector.layout.runtime).unwrap();
            agent_observability_local_runtime::storage_coherence::StorageBarrier::initialize(
                &collector.layout.root,
                &mutation,
            )
            .unwrap();
        }
        let mut blocker = None;
        let result = super::refresh_report_from_root_observing(&root, |_| {
            blocker = Some(MutationGuard::try_acquire(&collector.layout.runtime).unwrap());
        });
        assert_eq!(result.unwrap_err(), super::ReportFailure::RenderGuard);
        let control = RuntimeControl::new(&load(&collector.layout.config).unwrap()).unwrap();
        assert!(
            control
                .claim_stale_report_reservation(&root, blocker.as_ref().unwrap())
                .unwrap()
                .is_some()
        );
        assert!(collector.store.report_status().unwrap().pending());
        assert!(current_report_view(&collector.store).unwrap().is_none());
        drop(blocker);
        assert!(refresh_dashboard_snapshot(&root).unwrap());
        let mutation = MutationGuard::acquire(&collector.layout.runtime).unwrap();
        assert!(
            control
                .claim_stale_report_reservation(&root, &mutation)
                .unwrap()
                .is_none()
        );
        drop(mutation);
        assert_published_report_view(&root, 1);
        drop(collector);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn report_refresh_rejects_replaced_bound_staging_without_deleting_the_replacement() {
        use std::os::unix::fs::OpenOptionsExt;
        let root = test_root("report-bound-staging-replaced");
        let mut collector = collector_state(&root);
        ingest_notify_locked(&mut collector, &projected_notify("thread-1", "turn-1")).unwrap();
        assert!(refresh_dashboard_snapshot(&root).unwrap());
        let previous = current_report_view(&collector.store).unwrap();
        collector.store.invalidate_report().unwrap();
        let directory = collector.layout.state.join("store/report-views.v1");
        let mut replacement = None;
        let result = super::refresh_report_from_root_observing(&root, |_| {
            if replacement.is_some() {
                return;
            }
            let path = fs::read_dir(&directory)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .find(|path| {
                    path.file_name()
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .starts_with(".report-view.sqlite3.staging.")
                        && !path.to_string_lossy().ends_with("-journal")
                })
                .unwrap();
            fs::remove_file(&path).unwrap();
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .open(&path)
                .unwrap();
            file.write_all(b"replacement-must-survive").unwrap();
            file.sync_all().unwrap();
            replacement = Some(path);
        });
        assert!(result.is_err());
        assert_eq!(current_report_view(&collector.store).unwrap(), previous);
        assert!(collector.store.report_status().unwrap().pending());
        assert_eq!(
            fs::read(replacement.as_ref().unwrap()).unwrap(),
            b"replacement-must-survive"
        );
        let metadata_before =
            fs::read(collector.layout.runtime.join("report-reservation.meta")).unwrap();
        assert!(refresh_dashboard_snapshot(&root).is_err());
        assert_eq!(
            fs::read(collector.layout.runtime.join("report-reservation.meta")).unwrap(),
            metadata_before
        );
        assert_eq!(
            fs::read(replacement.unwrap()).unwrap(),
            b"replacement-must-survive"
        );
        drop(collector);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn report_acknowledgement_mismatch_keeps_reservation_for_recovery() {
        let root = test_root("report-reservation-ack-mismatch");
        let collector = collector_state(&root);
        let config = load(&collector.layout.config).unwrap();
        let control = RuntimeControl::new(&config).unwrap();
        let mutation = MutationGuard::acquire(&collector.layout.runtime).unwrap();
        let reservation = control
            .reserve_report_build(&root, &mutation, 1024 * 1024)
            .unwrap();
        let wrong_generation = collector.store.report_status().unwrap().generation + 1;
        assert_eq!(
            super::acknowledge_report_reservation(
                &collector.layout,
                &collector.store,
                wrong_generation,
                &mutation,
                reservation,
            )
            .unwrap_err(),
            super::ReportFailure::SnapshotChanged
        );
        assert!(
            control
                .claim_stale_report_reservation(&root, &mutation)
                .unwrap()
                .is_some()
        );
        drop(mutation);
        assert!(refresh_dashboard_snapshot(&root).unwrap());
        let mutation = MutationGuard::acquire(&collector.layout.runtime).unwrap();
        assert!(
            control
                .claim_stale_report_reservation(&root, &mutation)
                .unwrap()
                .is_none()
        );
        drop(mutation);
        drop(collector);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn report_publication_rechecks_budget_changed_during_projection() {
        let root = test_root("report-reservation-budget-change");
        let mut collector = collector_state(&root);
        ingest_notify_locked(&mut collector, &projected_notify("thread-1", "turn-1")).unwrap();
        assert!(refresh_dashboard_snapshot(&root).unwrap());
        let current = current_report_view(&collector.store).unwrap();
        collector.store.invalidate_report().unwrap();
        let result = super::refresh_report_from_root_observing(&root, |_| {
            let guard = ConfigMutationGuard::acquire(&collector.layout).unwrap();
            let mut config = load(&collector.layout.config).unwrap();
            config.collection.local_storage_budget_bytes = 256 * 1024 * 1024;
            save(&guard, &config).unwrap();
            inflate_allocated_accounting(&root);
        });
        assert_eq!(result.unwrap_err(), super::ReportFailure::Capacity);
        assert_eq!(current_report_view(&collector.store).unwrap(), current);
        assert!(collector.store.report_status().unwrap().pending());
        drop(collector);
        fs::remove_dir_all(root).unwrap();
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
            assert_published_report_view(&root, count);
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
        let private_detail_failures = Arc::new(AtomicU64::new(0));
        let state = AppState {
            collector: Arc::new(Mutex::new(restarted)),
            auth_token: Arc::from("a".repeat(64)),
            private_detail_failures,
            lifecycle_failures: Arc::new(AtomicU64::new(0)),
            lifecycle_storage_pressure: Arc::new(AtomicBool::new(false)),
            report_refresh_scheduled: Arc::new(AtomicBool::new(false)),
            report_refresh_requested: Arc::new(AtomicU64::new(0)),
            report_contention_quiet_ms: Arc::new(AtomicU64::new(0)),
            report_refresh_attempts: Arc::new(AtomicU64::new(0)),
            report_snapshot_test: Arc::default(),
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
        assert_published_report_view(&root, 1);
        assert!(!root.join("logs").join(REPORT_FILE_NAME).exists());
        assert!(!report_dirty_path(&install(&root).unwrap()).exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn lifecycle_invalidation_recovers_before_any_destructive_commit() {
        for replace_html in [false, true] {
            let root = test_root(if replace_html {
                "lifecycle-after-placeholder"
            } else {
                "lifecycle-before-placeholder"
            });
            let state = collector_state(&root);
            let layout = state.layout.clone();
            assert!(refresh_report_from_root(&root).unwrap());
            let guard = state.store.acquire_report_render_guard().unwrap();
            state.store.invalidate_report().unwrap();
            if replace_html {
                agent_observability_static_report::write_refresh_pending(
                    &layout.logs.join(REPORT_FILE_NAME),
                )
                .unwrap();
            }
            drop(guard);
            drop(state);
            // Simulate process loss at either publication boundary: no sidecar wakeup exists.
            assert!(!report_dirty_path(&layout).exists());
            let reopened = collector_state(&root);
            assert!(reopened.store.report_status().unwrap().pending());
            assert!(reconcile_report_state(
                &layout,
                reopened.store.report_status().unwrap().pending()
            ));
            drop(reopened);
            assert!(refresh_report_from_root(&root).unwrap());
            assert_published_report_view(&root, 0);
            if replace_html {
                assert!(
                    fs::read_to_string(layout.logs.join(REPORT_FILE_NAME))
                        .unwrap()
                        .contains("리포트 갱신 대기")
                );
            }
            let reopened = collector_state(&root);
            assert!(!reopened.store.report_status().unwrap().pending());
            drop(reopened);
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    #[cfg(unix)]
    fn stale_v6_reservation_recovers_catalog_before_authority_migration() {
        for invalid_staging in [false, true] {
            let root = test_root(&format!("v6-reservation-catalog-{invalid_staging}"));
            let mut collector = collector_state(&root);
            ingest_notify_locked(&mut collector, &projected_notify("v6-thread", "v6-turn"))
                .unwrap();
            assert!(refresh_dashboard_snapshot(&root).unwrap());
            let layout = collector.layout.clone();
            assert!(current_report_view(&collector.store).unwrap().is_some());
            let view_dir = layout.state.join("store/report-views.v1");
            let published_files: Vec<_> = fs::read_dir(&view_dir)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .filter(|path| path.extension().is_some_and(|value| value == "sqlite3"))
                .collect();
            assert_eq!(published_files.len(), 1);
            let published_file = &published_files[0];
            assert!(published_file.exists());
            drop(collector);
            let database = layout.state.join("store/local-store.sqlite3");
            // The historical fixture has epoch7; this epoch0 catalog must be retired.
            write_private_test_file(
                &database,
                include_bytes!("../../local-store/tests/fixtures/local_state_v6.sqlite3"),
            );
            let before = fs::read(&database).unwrap();
            let config = load(&layout.config).unwrap();
            let control = RuntimeControl::new(&config).unwrap();
            let mutation = MutationGuard::acquire(&layout.runtime).unwrap();
            drop(
                control
                    .reserve_report_build(&root, &mutation, 1024 * 1024)
                    .unwrap(),
            );
            let staging = view_dir.join(".report-view.sqlite3.staging.v6-interrupted");
            if invalid_staging {
                fs::create_dir(&staging).unwrap();
            } else {
                write_private_test_file(&staging, b"content-free interrupted staging");
            }
            let unrelated = layout.logs.join("operator-note");
            write_private_test_file(&unrelated, b"preserve");
            if invalid_staging {
                assert!(
                    super::recover_report_reservation_for_startup(&layout, &config, &mutation)
                        .is_err()
                );
                assert!(
                    control
                        .claim_stale_report_reservation(&root, &mutation)
                        .unwrap()
                        .is_some()
                );
                assert!(
                    fs::read(&database).unwrap() == before,
                    "failed recovery changed authority"
                );
                fs::remove_dir(&staging).unwrap();
                write_private_test_file(&staging, b"content-free interrupted staging");
            }
            super::recover_report_reservation_for_startup(&layout, &config, &mutation).unwrap();
            assert!(!staging.exists());
            assert!(!published_file.exists());
            assert!(!view_dir.join("catalog.json").exists());
            assert_eq!(fs::read(&unrelated).unwrap(), b"preserve");
            assert!(
                fs::read(&database).unwrap() == before,
                "cleanup migrated or changed authority"
            );
            assert!(
                control
                    .claim_stale_report_reservation(&root, &mutation)
                    .unwrap()
                    .is_none()
            );
            let store = super::open_store(&mutation, &layout, &config).unwrap();
            assert_eq!(store.report_status().unwrap().generation, 2);
            assert_eq!(store.report_status().unwrap().acknowledged_generation, 1);
            assert_eq!(store.report_visibility_epoch().unwrap(), 7);
            assert_eq!(store.record_count().unwrap(), 2);
            drop(store);
            drop(mutation);
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn terminal_cleanup_retries_contention_without_rebuilding() {
        for lock_kind in ["mutation", "catalog", "reservation"] {
            let root = test_root(&format!("report-terminal-cleanup-{lock_kind}"));
            let state = app_state(&root);
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async {
                let layout = state.collector.lock().await.layout.clone();
                let config = load(&layout.config).unwrap();
                let control = RuntimeControl::new(&config).unwrap();
                let mutation = MutationGuard::acquire(&layout.runtime).unwrap();
                let reservation = control
                    .reserve_report_build(&root, &mutation, 65536)
                    .unwrap();
                let reservation = if lock_kind == "reservation" {
                    Some(reservation)
                } else {
                    drop(reservation);
                    None
                };
                let blocker = LocalStore::open_current(layout.state.join("store")).unwrap();
                let catalog = if lock_kind == "catalog" {
                    Some(blocker.acquire_report_render_guard().unwrap())
                } else {
                    None
                };
                let mutation = if lock_kind == "mutation" {
                    Some(mutation)
                } else {
                    drop(mutation);
                    None
                };
                let cleanup_state = state.clone();
                let cleanup_layout = layout.clone();
                let cleanup = tokio::spawn(async move {
                    super::cleanup_report_reservation_with_retry(
                        &cleanup_state,
                        &cleanup_layout,
                        super::ReportRefreshTiming {
                            debounce: Duration::ZERO,
                            retry_initial: Duration::from_millis(50),
                        },
                    )
                    .await
                });
                // Wait until at least one busy attempt has completed; the held lock cannot
                // be acquired by the second attempt either until explicitly released here.
                tokio::time::timeout(Duration::from_secs(2), async {
                    while state
                        .report_snapshot_test
                        .cleanup_attempts
                        .load(Ordering::Acquire)
                        < 2
                    {
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .unwrap();
                assert!(!cleanup.is_finished());
                drop(mutation);
                drop(catalog);
                drop(reservation);
                assert_eq!(cleanup.await.unwrap(), Ok(()));
                assert_eq!(state.report_refresh_attempts.load(Ordering::Acquire), 0);
                let mutation = MutationGuard::acquire(&layout.runtime).unwrap();
                assert!(
                    control
                        .claim_stale_report_reservation(&root, &mutation)
                        .unwrap()
                        .is_none()
                );
            });
            drop(state);
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn terminal_cleanup_exhaustion_is_bounded_and_preserves_recovery() {
        let root = test_root("report-terminal-cleanup-exhaustion");
        let state = app_state(&root);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let layout = state.collector.lock().await.layout.clone();
            let config = load(&layout.config).unwrap();
            let control = RuntimeControl::new(&config).unwrap();
            let mutation = MutationGuard::acquire(&layout.runtime).unwrap();
            drop(
                control
                    .reserve_report_build(&root, &mutation, 65536)
                    .unwrap(),
            );
            let result = super::cleanup_report_reservation_with_retry(
                &state,
                &layout,
                super::ReportRefreshTiming {
                    debounce: Duration::ZERO,
                    retry_initial: Duration::from_millis(1),
                },
            )
            .await;
            assert_eq!(result, Err(ReportFailure::RenderGuard));
            assert_eq!(
                state
                    .report_snapshot_test
                    .cleanup_attempts
                    .load(Ordering::Acquire),
                u64::from(super::REPORT_RETRY_LIMIT)
            );
            assert_eq!(state.report_refresh_attempts.load(Ordering::Acquire), 0);
            assert!(
                control
                    .claim_stale_report_reservation(&root, &mutation)
                    .unwrap()
                    .is_some()
            );
            drop(mutation);
            assert_eq!(super::cleanup_report_reservation(&layout), Ok(()));
        });
        drop(state);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn terminal_refresh_failure_cleans_stale_reservation_without_rebuilding() {
        let root = test_root("report-terminal-reservation-cleanup");
        let state = app_state(&root);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let current = {
                let mut collector = state.collector.lock().await;
                ingest_notify_locked(&mut collector, &projected_notify("thread-1", "turn-1"))
                    .unwrap();
                assert!(refresh_dashboard_snapshot(&root).unwrap());
                let current = current_report_view(&collector.store).unwrap();
                collector.store.invalidate_report().unwrap();
                current
            };
            state
                .report_snapshot_test
                .fail_after_reservation
                .store(true, Ordering::Release);
            schedule_report_refresh(&state);
            for _ in 0..200 {
                if !state.report_refresh_scheduled.load(Ordering::Acquire) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            assert!(!state.report_refresh_scheduled.load(Ordering::Acquire));
            let collector = state.collector.lock().await;
            assert_eq!(collector.report_refresh_failures, super::REPORT_RETRY_LIMIT);
            assert_eq!(current_report_view(&collector.store).unwrap(), current);
            assert!(collector.store.report_status().unwrap().pending());
            let config = load(&collector.layout.config).unwrap();
            let control = RuntimeControl::new(&config).unwrap();
            let mutation = MutationGuard::acquire(&collector.layout.runtime).unwrap();
            assert!(
                control
                    .claim_stale_report_reservation(&root, &mutation)
                    .unwrap()
                    .is_none()
            );
            let allocated = StorageBudget::allocated_tree_bytes(&root).unwrap();
            let request = control.storage_budget().writable_limit() - allocated - 8192;
            assert!(matches!(
                control.admit(&root, request).unwrap(),
                Admission::Allowed { .. }
            ));
        });
        drop(state);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn external_report_ack_recovers_health_after_retry_exhaustion() {
        let root = test_root("report-persistent-failure");
        let _ = fs::remove_dir_all(&root);
        let state = app_state(&root);
        let report_views = root.join("state/store/report-views.v1");
        fs::write(&report_views, b"occupied").unwrap();
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
            fs::remove_file(&report_views).unwrap();
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
        assert_published_report_view(&root, 1);
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
            let transport = super::TransportListener::new(
                listener,
                server_config,
                Duration::from_secs(1),
                super::MAX_CONNECTIONS,
            );
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
            // Privacy acceptance is not the callback's 250ms fail-open latency test.
            // Use the same projector and authenticated transport with a bounded test deadline.
            let response = tokio::task::spawn_blocking(move || {
                let projected =
                    super::project_notify_json(&raw_notify("RAW_THREAD_SECRET", "RAW_TURN_SECRET"))
                        .unwrap();
                let body = serde_json::to_vec(&projected).unwrap();
                super::authenticated_request(
                    &notify_root,
                    "POST",
                    "/v1/notify",
                    Some(&body),
                    Duration::from_secs(1),
                    Duration::from_secs(5),
                )
            })
            .await
            .unwrap();
            assert_eq!(response.unwrap().status, 200);
            server.abort();
            let _ = server.await;
        });
        let store = LocalStore::open_current(root.join("state/store")).unwrap();
        assert_eq!(store.observation_count().unwrap(), 2);
        assert_eq!(store.record_count().unwrap(), 2);
        drop(store);
        refresh_report_from_root(&root).unwrap();
        assert_published_report_view(&root, 2);

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
