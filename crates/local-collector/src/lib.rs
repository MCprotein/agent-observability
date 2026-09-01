#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc)]

use agent_observability_adapter_codex::{
    AdapterBatch, AdapterItem, MAX_HANDOFF_BYTES, parse_notify_json, parse_otlp_http_json,
};
use agent_observability_application::project_report;
use agent_observability_local_runtime::{
    Admission, InstalledLayout, LocalRuntimeConfigV2, PressureSample, RuntimeControl, Singleton,
    StorageBudget, install, load,
};
use agent_observability_local_store::{IngestStatus, LocalStore};
use agent_observability_static_report::write_private;
use axum::{
    Router,
    body::Bytes,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{net::TcpListener, sync::Mutex};

pub const TOKEN_HEADER: &str = "x-agent-observability-token";
pub const REPORT_FILE_NAME: &str = "agent-observability-report.html";
pub const COLLECTOR_SETTINGS_VERSION: &str = "local_collector.v1";

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CollectorSettings {
    pub schema_version: String,
    pub port: u16,
    pub token: String,
    pub source_generation: String,
}

impl CollectorSettings {
    #[must_use]
    pub fn endpoint(&self) -> String {
        format!("http://127.0.0.1:{}/v1/logs", self.port)
    }

    #[must_use]
    pub fn options(&self, root: &Path) -> CollectorOptions {
        CollectorOptions {
            root: root.to_path_buf(),
            port: self.port,
            token: self.token.clone(),
            source_generation: self.source_generation.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct CollectorOptions {
    pub root: PathBuf,
    pub port: u16,
    pub token: String,
    pub source_generation: String,
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
    Unavailable,
}

#[derive(Debug)]
pub enum CollectorError {
    Io(std::io::Error),
    Runtime(String),
}

/// Creates or loads the private, idempotent local collector settings.
pub fn install_settings(root: &Path) -> Result<CollectorSettings, CollectorError> {
    let layout = install(root).map_err(runtime_error)?;
    let path = settings_path(&layout);
    if path.exists() {
        return load_settings(root);
    }
    let mut random = [0_u8; 32];
    getrandom::fill(&mut random)
        .map_err(|error| CollectorError::Runtime(format!("collector entropy failed: {error}")))?;
    let token = random
        .iter()
        .fold(String::with_capacity(64), |mut token, byte| {
            use std::fmt::Write as _;
            write!(token, "{byte:02x}").expect("writing to String cannot fail");
            token
        });
    let settings = CollectorSettings {
        schema_version: COLLECTOR_SETTINGS_VERSION.into(),
        port: available_port()?,
        token,
        source_generation: "codex-otel-v1".into(),
    };
    write_private_json(&path, &settings)?;
    Ok(settings)
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
    let mut file = open_private_read(&path)?;
    let metadata = file.metadata()?;
    if metadata.len() > 64 * 1024 {
        return Err(CollectorError::Runtime(
            "collector settings exceed 64 KiB".into(),
        ));
    }
    let mut body = String::new();
    file.read_to_string(&mut body)?;
    let settings: CollectorSettings = serde_json::from_str(&body)
        .map_err(|_| CollectorError::Runtime("invalid collector settings".into()))?;
    validate_settings(&settings)?;
    Ok(settings)
}

fn settings_path(layout: &InstalledLayout) -> PathBuf {
    layout.runtime.join("collector.json")
}

fn validate_settings(settings: &CollectorSettings) -> Result<(), CollectorError> {
    if settings.schema_version != COLLECTOR_SETTINGS_VERSION
        || settings.port == 0
        || settings.token.len() != 64
        || !settings.token.bytes().all(|byte| byte.is_ascii_hexdigit())
        || settings.source_generation.is_empty()
    {
        return Err(CollectorError::Runtime(
            "invalid local collector settings".into(),
        ));
    }
    Ok(())
}

fn write_private_json(path: &Path, settings: &CollectorSettings) -> Result<(), CollectorError> {
    let parent = path
        .parent()
        .ok_or_else(|| CollectorError::Runtime("collector settings have no parent".into()))?;
    let temporary = parent.join(format!(".collector.json.tmp.{}", std::process::id()));
    let _ = fs::remove_file(&temporary);
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary)?;
    serde_json::to_writer_pretty(&mut file, settings)
        .map_err(|_| CollectorError::Runtime("collector settings serialization failed".into()))?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    fs::rename(&temporary, path)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}

fn open_private_read(path: &Path) -> Result<File, CollectorError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(CollectorError::Runtime(
            "collector settings must be a private regular file".into(),
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(CollectorError::Runtime(
                "collector settings permissions are too broad".into(),
            ));
        }
    }
    OpenOptions::new().read(true).open(path).map_err(Into::into)
}

impl std::fmt::Display for CollectorError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "local collector I/O error: {error}"),
            Self::Runtime(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for CollectorError {}

impl From<std::io::Error> for CollectorError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug)]
struct CollectorState {
    layout: InstalledLayout,
    store: LocalStore,
    source_generation: String,
    token: String,
    last_cursor: Option<String>,
    accepted_requests: u64,
    rejected_requests: u64,
    suppressed_requests: u64,
    last_ingest_unix_ms: Option<u64>,
    report_generation: u64,
}

#[derive(Clone, Debug)]
struct AppState {
    collector: Arc<Mutex<CollectorState>>,
    report_refresh_scheduled: Arc<AtomicBool>,
}

#[derive(Debug, Serialize)]
struct Health {
    status: &'static str,
    accepted_requests: u64,
    rejected_requests: u64,
    suppressed_requests: u64,
    last_ingest_unix_ms: Option<u64>,
}

/// Runs the authenticated OTLP/HTTP receiver until the process is terminated.
pub async fn serve(options: CollectorOptions) -> Result<(), CollectorError> {
    validate_options(&options)?;
    let layout = install(&options.root).map_err(runtime_error)?;
    let config = load(&layout.config).map_err(runtime_error)?;
    let singleton = Singleton::acquire(&layout.runtime.join("collector")).map_err(runtime_error)?;
    let store = open_store(&layout, &config)?;
    let last_cursor = store
        .cursor("codex", &options.source_generation)
        .map_err(runtime_error)?;
    let listener = TcpListener::bind(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        options.port,
    ))
    .await?;
    let collector = Arc::new(Mutex::new(CollectorState {
        layout,
        store,
        source_generation: options.source_generation,
        token: options.token,
        last_cursor,
        accepted_requests: 0,
        rejected_requests: 0,
        suppressed_requests: 0,
        last_ingest_unix_ms: None,
        report_generation: 0,
    }));
    let state = AppState {
        collector,
        report_refresh_scheduled: Arc::new(AtomicBool::new(false)),
    };
    let app = router(state);
    let result = axum::serve(listener, app).await;
    drop(singleton);
    result.map_err(CollectorError::Io)
}

fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/logs", post(ingest_logs))
        .route("/v1/notify", post(ingest_notify))
        .layer(DefaultBodyLimit::max(
            usize::try_from(MAX_HANDOFF_BYTES).expect("handoff bound fits usize"),
        ))
        .with_state(state)
}

async fn ingest_notify(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let mut collector = state.collector.lock().await;
    if !authorized(&headers, &collector.token) {
        collector.rejected_requests = collector.rejected_requests.saturating_add(1);
        return StatusCode::UNAUTHORIZED;
    }
    if !is_json(&headers) {
        collector.rejected_requests = collector.rejected_requests.saturating_add(1);
        return StatusCode::UNSUPPORTED_MEDIA_TYPE;
    }
    let (outcome, committed) = match ingest_notify_locked(&mut collector, &body) {
        Ok(IngestOutcome::Committed) => {
            collector.accepted_requests = collector.accepted_requests.saturating_add(1);
            (StatusCode::OK, true)
        }
        Ok(IngestOutcome::Disabled) => {
            collector.suppressed_requests = collector.suppressed_requests.saturating_add(1);
            (StatusCode::OK, false)
        }
        Err(error) => {
            collector.rejected_requests = collector.rejected_requests.saturating_add(1);
            (error.status(), false)
        }
    };
    drop(collector);
    if committed {
        schedule_report_refresh(&state);
    }
    outcome
}

async fn health(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    let collector = state.collector.lock().await;
    if !authorized(&headers, &collector.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    axum::Json(Health {
        status: "ready",
        accepted_requests: collector.accepted_requests,
        rejected_requests: collector.rejected_requests,
        suppressed_requests: collector.suppressed_requests,
        last_ingest_unix_ms: collector.last_ingest_unix_ms,
    })
    .into_response()
}

async fn ingest_logs(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let mut collector = state.collector.lock().await;
    if !authorized(&headers, &collector.token) {
        collector.rejected_requests = collector.rejected_requests.saturating_add(1);
        return StatusCode::UNAUTHORIZED;
    }
    if !is_json(&headers) {
        collector.rejected_requests = collector.rejected_requests.saturating_add(1);
        return StatusCode::UNSUPPORTED_MEDIA_TYPE;
    }
    let (outcome, committed) = match ingest_locked(&mut collector, &body) {
        Ok(IngestOutcome::Committed) => {
            collector.accepted_requests = collector.accepted_requests.saturating_add(1);
            (StatusCode::OK, true)
        }
        Ok(IngestOutcome::Disabled) => {
            collector.suppressed_requests = collector.suppressed_requests.saturating_add(1);
            (StatusCode::OK, false)
        }
        Err(error) => {
            collector.rejected_requests = collector.rejected_requests.saturating_add(1);
            (error.status(), false)
        }
    };
    drop(collector);
    if committed {
        schedule_report_refresh(&state);
    }
    outcome
}

fn authorized(headers: &HeaderMap, expected: &str) -> bool {
    headers
        .get(TOKEN_HEADER)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| constant_time_equal(value.as_bytes(), expected.as_bytes()))
}

fn is_json(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"))
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum IngestOutcome {
    Committed,
    Disabled,
}

#[derive(Debug)]
enum IngestError {
    Invalid(CollectorError),
    Policy,
    Pressure,
    Storage,
}

impl IngestError {
    const fn status(&self) -> StatusCode {
        match self {
            Self::Invalid(CollectorError::Io(_)) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Invalid(CollectorError::Runtime(_)) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Policy => StatusCode::PAYLOAD_TOO_LARGE,
            Self::Pressure => StatusCode::SERVICE_UNAVAILABLE,
            Self::Storage => StatusCode::INSUFFICIENT_STORAGE,
        }
    }
}

impl From<CollectorError> for IngestError {
    fn from(error: CollectorError) -> Self {
        Self::Invalid(error)
    }
}

fn ingest_locked(state: &mut CollectorState, body: &[u8]) -> Result<IngestOutcome, IngestError> {
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
    let (batch, last_cursor) = parse_otlp_http_json(
        body,
        &state.source_generation,
        state.last_cursor.as_deref(),
        next_cursor,
        now,
    )
    .map_err(runtime_error)?;
    enforce_batch_policy(&batch, &config)?;
    commit_batch(state, &batch, last_cursor, now)?;
    Ok(IngestOutcome::Committed)
}

fn ingest_notify_locked(
    state: &mut CollectorState,
    body: &[u8],
) -> Result<IngestOutcome, IngestError> {
    let Some(config) = admit_request(state, body.len())? else {
        return Ok(IngestOutcome::Disabled);
    };
    let now = current_unix_ms()?;
    let cursor = next_cursor(state)?;
    let batch = parse_notify_json(
        body,
        &state.source_generation,
        state.last_cursor.as_deref(),
        cursor,
        now,
    )
    .map_err(runtime_error)?;
    enforce_batch_policy(&batch, &config)?;
    commit_batch(state, &batch, Some(cursor.to_string()), now)?;
    Ok(IngestOutcome::Committed)
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
    if batch.observations().count() > usize::from(config.collection.max_batch_records) {
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
) -> Result<(), CollectorError> {
    if batch.diagnostics().next().is_some() {
        return Err(CollectorError::Runtime(
            "Codex OTLP batch did not satisfy the canonical adapter contract".into(),
        ));
    }
    let observations = batch
        .items
        .iter()
        .filter_map(|item| match item {
            AdapterItem::Observation(observation) => Some(observation.as_ref().clone()),
            AdapterItem::Disposition(_) => None,
        })
        .collect::<Vec<_>>();
    let statuses = state
        .store
        .ingest_batch_deferred_projection(&observations)
        .map_err(runtime_error)?;
    if statuses
        .iter()
        .any(|status| !matches!(status, IngestStatus::Committed | IngestStatus::Duplicate))
    {
        return Err(CollectorError::Runtime(
            "Codex OTLP batch was suppressed by the local store".into(),
        ));
    }
    state.last_cursor = last_cursor;
    state.last_ingest_unix_ms = Some(now);
    state.report_generation = state.report_generation.saturating_add(1);
    Ok(())
}

fn schedule_report_refresh(state: &AppState) {
    if state
        .report_refresh_scheduled
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    let state = state.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        loop {
            let (root, generation) = {
                let collector = state.collector.lock().await;
                (collector.layout.root.clone(), collector.report_generation)
            };
            let refreshed = tokio::task::spawn_blocking(move || refresh_report_from_root(&root))
                .await
                .is_ok_and(|result| result.is_ok());
            let collector = state.collector.lock().await;
            if refreshed && collector.report_generation == generation {
                state
                    .report_refresh_scheduled
                    .store(false, Ordering::Release);
                return;
            }
            drop(collector);
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    });
}

fn refresh_report_from_root(root: &Path) -> Result<(), CollectorError> {
    let layout = install(root).map_err(runtime_error)?;
    let config = load(&layout.config).map_err(runtime_error)?;
    let store = open_store(&layout, &config)?;
    store.rebuild_projection().map_err(runtime_error)?;
    refresh_report(&layout, &store, current_unix_ms()?)
}

/// Sends the raw notify argument to the local receiver with bounded foreground deadlines.
/// Receiver failure is represented as an outcome so the CLI helper can always fail open.
#[must_use]
pub fn submit_notify(root: &Path, payload: &[u8]) -> NotifyOutcome {
    let Ok(settings) = load_settings(root) else {
        return NotifyOutcome::Unavailable;
    };
    if payload.len() > 64 * 1024 {
        return NotifyOutcome::Rejected;
    }
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), settings.port);
    let Ok(mut stream) = TcpStream::connect_timeout(&address, Duration::from_millis(10)) else {
        return NotifyOutcome::Unavailable;
    };
    let deadline = Some(Duration::from_millis(40));
    if stream.set_write_timeout(deadline).is_err() || stream.set_read_timeout(deadline).is_err() {
        return NotifyOutcome::Unavailable;
    }
    let head = format!(
        "POST /v1/notify HTTP/1.1\r\nHost: 127.0.0.1\r\n{TOKEN_HEADER}: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        settings.token,
        payload.len()
    );
    if stream.write_all(head.as_bytes()).is_err()
        || stream.write_all(payload).is_err()
        || stream.flush().is_err()
    {
        return NotifyOutcome::Unavailable;
    }
    let mut response = [0_u8; 128];
    let Ok(bytes) = stream.read(&mut response) else {
        return NotifyOutcome::Unavailable;
    };
    let status = std::str::from_utf8(&response[..bytes]).unwrap_or_default();
    if status.starts_with("HTTP/1.1 200") {
        NotifyOutcome::Accepted
    } else {
        NotifyOutcome::Rejected
    }
}

/// Performs a bounded authenticated health probe against the local collector.
#[must_use]
pub fn check_health(root: &Path) -> HealthOutcome {
    let Ok(settings) = load_settings(root) else {
        return HealthOutcome::Unavailable;
    };
    let request = format!(
        "GET /health HTTP/1.1\r\nHost: 127.0.0.1\r\n{TOKEN_HEADER}: {}\r\nConnection: close\r\n\r\n",
        settings.token
    );
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), settings.port);
    let Ok(mut stream) = TcpStream::connect_timeout(&address, Duration::from_millis(50)) else {
        return HealthOutcome::Unavailable;
    };
    let timeout = Some(Duration::from_millis(100));
    if stream.set_write_timeout(timeout).is_err()
        || stream.set_read_timeout(timeout).is_err()
        || stream.write_all(request.as_bytes()).is_err()
        || stream.flush().is_err()
    {
        return HealthOutcome::Unavailable;
    }
    let mut response = [0_u8; 128];
    match stream.read(&mut response) {
        Ok(bytes)
            if std::str::from_utf8(&response[..bytes])
                .unwrap_or_default()
                .starts_with("HTTP/1.1 200") =>
        {
            HealthOutcome::Ready
        }
        _ => HealthOutcome::Unavailable,
    }
}

fn refresh_report(
    layout: &InstalledLayout,
    store: &LocalStore,
    now_unix_ms: u64,
) -> Result<(), CollectorError> {
    let records = store.current_records().map_err(runtime_error)?;
    let report = project_report(
        &records,
        timestamp_from_unix_ms(now_unix_ms)?,
        "Agent Observability Report",
        None,
    )
    .map_err(runtime_error)?;
    write_private(&layout.logs.join(REPORT_FILE_NAME), &report).map_err(runtime_error)?;
    Ok(())
}

fn open_store(
    layout: &InstalledLayout,
    config: &LocalRuntimeConfigV2,
) -> Result<LocalStore, CollectorError> {
    let control = RuntimeControl::new(config).map_err(runtime_error)?;
    let headroom = control
        .migration_headroom(&layout.root)
        .map_err(runtime_error)?;
    LocalStore::open_with_migration_headroom(layout.state.join("store"), headroom)
        .map_err(runtime_error)
}

fn validate_options(options: &CollectorOptions) -> Result<(), CollectorError> {
    if options.port == 0
        || options.token.len() < 32
        || options.source_generation.is_empty()
        || !options.root.is_absolute()
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
        AppState, CollectorState, IngestError, IngestOutcome, NotifyOutcome, REPORT_FILE_NAME,
        TOKEN_HEADER, admit_request, constant_time_equal, enforce_batch_policy, ingest_locked,
        ingest_notify_locked, install_settings, is_json, load_settings, open_store,
        refresh_report_from_root, router, schedule_report_refresh, settings_path, submit_notify,
        timestamp_from_unix_ms, write_private_json,
    };
    use agent_observability_adapter_codex::{MAX_HANDOFF_BYTES, parse_otlp_http_json};
    use agent_observability_local_runtime::{ConfigMutationGuard, install, load, save};
    use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
    use std::{
        fs,
        io::{Read, Write},
        net::{Ipv4Addr, TcpListener, TcpStream},
        path::{Path, PathBuf},
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        thread,
        time::{Duration, Instant},
    };
    use tokio::sync::Mutex;

    fn test_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "agent-observability-collector-{name}-{}",
            std::process::id()
        ))
    }

    fn collector_state(root: &Path) -> CollectorState {
        let layout = install(root).unwrap();
        let config = load(&layout.config).unwrap();
        let store = open_store(&layout, &config).unwrap();
        CollectorState {
            layout,
            store,
            source_generation: "codex-test".into(),
            token: "a".repeat(64),
            last_cursor: None,
            accepted_requests: 0,
            rejected_requests: 0,
            suppressed_requests: 0,
            last_ingest_unix_ms: None,
            report_generation: 0,
        }
    }

    fn app_state(root: &Path) -> AppState {
        AppState {
            collector: Arc::new(Mutex::new(collector_state(root))),
            report_refresh_scheduled: Arc::new(AtomicBool::new(false)),
        }
    }

    fn padded_json(body: &[u8], bytes: usize) -> Vec<u8> {
        assert!(body.len() <= bytes);
        let mut padded = Vec::with_capacity(bytes);
        padded.extend_from_slice(body);
        padded.resize(bytes, b' ');
        padded
    }

    async fn post(
        port: u16,
        token: Option<&str>,
        content_type: Option<&str>,
        body: Vec<u8>,
    ) -> StatusCode {
        let token = token.map(str::to_owned);
        let content_type = content_type.map(str::to_owned);
        tokio::task::spawn_blocking(move || {
            use std::fmt::Write as _;

            let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
            let timeout = Some(Duration::from_secs(2));
            stream.set_write_timeout(timeout).unwrap();
            stream.set_read_timeout(timeout).unwrap();
            let mut request = format!(
                "POST /v1/logs HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nConnection: close\r\n",
                body.len()
            );
            if let Some(token) = token {
                write!(request, "{TOKEN_HEADER}: {token}\r\n").unwrap();
            }
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

    fn configure_port(root: &Path, port: u16) {
        let mut settings = install_settings(root).unwrap();
        settings.port = port;
        let layout = install(root).unwrap();
        write_private_json(&settings_path(&layout), &settings).unwrap();
    }

    fn spawn_response_server(response: Option<&'static [u8]>) -> (u16, thread::JoinHandle<()>) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 4096];
            let _ = stream.read(&mut request);
            if let Some(response) = response {
                stream.write_all(response).unwrap();
            } else {
                thread::sleep(Duration::from_millis(250));
            }
        });
        (port, handle)
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
    fn token_comparison_rejects_length_and_content_mismatch() {
        assert!(constant_time_equal(b"same", b"same"));
        assert!(!constant_time_equal(b"same", b"diff"));
        assert!(!constant_time_equal(b"same", b"shorter"));
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
    fn http_receiver_enforces_auth_media_type_json_and_transport_body_bound() {
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
            let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
                .await
                .unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = tokio::spawn(async move {
                axum::serve(listener, app).await.unwrap();
            });
            assert_eq!(
                post(port, None, Some("application/json"), b"{}".to_vec()).await,
                StatusCode::UNAUTHORIZED
            );
            assert_eq!(
                post(
                    port,
                    Some(&"b".repeat(64)),
                    Some("application/json"),
                    b"{}".to_vec(),
                )
                .await,
                StatusCode::UNAUTHORIZED
            );
            assert_eq!(
                post(port, Some(&"a".repeat(64)), None, b"{}".to_vec()).await,
                StatusCode::UNSUPPORTED_MEDIA_TYPE
            );
            assert_eq!(
                post(
                    port,
                    Some(&"a".repeat(64)),
                    Some("text/plain"),
                    b"{}".to_vec(),
                )
                .await,
                StatusCode::UNSUPPORTED_MEDIA_TYPE
            );
            assert_eq!(
                post(
                    port,
                    Some(&"a".repeat(64)),
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
                    Some(&"a".repeat(64)),
                    Some("application/json; charset=utf-8"),
                    exact,
                )
                .await,
                StatusCode::OK
            );
            assert_eq!(
                post(
                    port,
                    Some(&"a".repeat(64)),
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
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn notify_is_fail_open_for_missing_refused_stalled_oversized_and_rejected_receivers() {
        let missing = test_root("notify-missing");
        let _ = fs::remove_dir_all(&missing);
        assert_eq!(submit_notify(&missing, b"{}"), NotifyOutcome::Unavailable);

        let refused = test_root("notify-refused");
        let _ = fs::remove_dir_all(&refused);
        let refused_port = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        configure_port(&refused, refused_port);
        assert_eq!(submit_notify(&refused, b"{}"), NotifyOutcome::Unavailable);

        let oversized = vec![b'x'; 64 * 1024 + 1];
        assert_eq!(submit_notify(&refused, &oversized), NotifyOutcome::Rejected);

        let rejected = test_root("notify-rejected");
        let _ = fs::remove_dir_all(&rejected);
        let (port, server) = spawn_response_server(Some(
            b"HTTP/1.1 422 Unprocessable Entity\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        ));
        configure_port(&rejected, port);
        assert_eq!(submit_notify(&rejected, b"{}"), NotifyOutcome::Rejected);
        server.join().unwrap();

        let accepted = test_root("notify-accepted");
        let _ = fs::remove_dir_all(&accepted);
        let (port, server) = spawn_response_server(Some(
            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        ));
        configure_port(&accepted, port);
        assert_eq!(submit_notify(&accepted, b"{}"), NotifyOutcome::Accepted);
        server.join().unwrap();

        let stalled = test_root("notify-stalled");
        let _ = fs::remove_dir_all(&stalled);
        let (port, server) = spawn_response_server(None);
        configure_port(&stalled, port);
        let started = Instant::now();
        assert_eq!(submit_notify(&stalled, b"{}"), NotifyOutcome::Unavailable);
        let elapsed = started.elapsed();
        assert!(elapsed >= Duration::from_millis(20), "elapsed={elapsed:?}");
        assert!(elapsed < Duration::from_millis(500), "elapsed={elapsed:?}");
        server.join().unwrap();

        for root in [refused, rejected, accepted, stalled] {
            let _ = fs::remove_dir_all(root);
        }
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
        assert_eq!(first.token.len(), 64);
        let path = root.join("runtime/collector.json");
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
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
        let store = open_store(&layout, &config).unwrap();
        let mut state = CollectorState {
            layout: layout.clone(),
            store,
            source_generation: "codex-test".into(),
            token: "x".repeat(32),
            last_cursor: None,
            accepted_requests: 0,
            rejected_requests: 0,
            suppressed_requests: 0,
            last_ingest_unix_ms: None,
            report_generation: 0,
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
        ingest_notify_locked(
            &mut state,
            br#"{"type":"agent-turn-complete","thread-id":"conversation-1","turn-id":"turn-1","cwd":"/private/SECRET_PATH","input-messages":["SECRET_PROMPT"],"last-assistant-message":"SECRET_OUTPUT"}"#,
        )
        .unwrap();
        refresh_report_from_root(&layout.root).unwrap();

        assert_eq!(state.last_cursor.as_deref(), Some("3"));
        assert_eq!(state.store.counts().unwrap().0, 3);
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
                ingest_notify_locked(
                    &mut collector,
                    br#"{"type":"agent-turn-complete","thread-id":"thread-1","turn-id":"turn-1"}"#,
                )
                .unwrap();
            }
            schedule_report_refresh(&state);
            tokio::time::sleep(Duration::from_millis(120)).await;
            assert!(state.report_refresh_scheduled.load(Ordering::Acquire));

            {
                let mut collector = state.collector.lock().await;
                ingest_notify_locked(
                    &mut collector,
                    br#"{"type":"agent-turn-complete","thread-id":"thread-2","turn-id":"turn-2"}"#,
                )
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
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn full_runtime_tree_excludes_raw_otlp_and_notify_content() {
        let root = test_root("privacy-tree");
        let _ = fs::remove_dir_all(&root);
        let _ = install_settings(&root).unwrap();
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
        ingest_notify_locked(
            &mut state,
            br#"{"type":"agent-turn-complete","thread-id":"RAW_THREAD_SECRET","turn-id":"RAW_TURN_SECRET","cwd":"/private/RAW_PATH_SECRET","input-messages":["RAW_PROMPT_SECRET"],"last-assistant-message":"RAW_OUTPUT_SECRET"}"#,
        )
        .unwrap();
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
                b"RAW_PROMPT_SECRET",
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
        let store = open_store(&layout, &initial).unwrap();
        let mut state = CollectorState {
            layout: layout.clone(),
            store,
            source_generation: "codex-test".into(),
            token: "x".repeat(32),
            last_cursor: None,
            accepted_requests: 0,
            rejected_requests: 0,
            suppressed_requests: 0,
            last_ingest_unix_ms: None,
            report_generation: 0,
        };
        let guard = ConfigMutationGuard::acquire(&layout).unwrap();
        let mut disabled = initial;
        disabled.enabled = false;
        save(&guard, &disabled).unwrap();

        let outcome = ingest_notify_locked(
            &mut state,
            br#"{"type":"agent-turn-complete","thread-id":"thread","turn-id":"turn","input-messages":["SECRET_PROMPT"]}"#,
        )
        .unwrap();

        assert_eq!(outcome, IngestOutcome::Disabled);
        assert_eq!(state.store.counts().unwrap().0, 0);
        assert_eq!(state.last_cursor, None);
        let _ = fs::remove_dir_all(root);
    }
}
