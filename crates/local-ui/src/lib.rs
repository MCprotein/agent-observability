#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc)]

mod dashboard_request;

use agent_observability_codex_integration::{
    CodexIntegrationStatus, IntegrationError, connect as connect_codex,
    disconnect as disconnect_codex, status as codex_status,
};
use agent_observability_contracts::MAX_REPORT_ARTIFACT_BYTES;
use agent_observability_contracts::dashboard::{
    DASHBOARD_QUERY_VERSION, DASHBOARD_RESPONSE_MAX_SERIALIZED_UTF8_BYTES, DashboardQueryRequestV1,
    DashboardQueryResponseV1, DashboardRequestKindV1, DashboardStatusKindV1,
    DashboardStatusReasonV1, DashboardStatusResponseV1,
};
use agent_observability_local_collector::{
    PrivateTurnDetailLookup, REPORT_FILE_NAME, lookup_private_turn_detail,
};
use agent_observability_local_runtime::{
    ConfigServiceError, CoordinatedSingletonScope, InstalledLayout, LocalConfigService,
    LocalRuntimeConfigV3, ProductionSingleton, VersionedLocalConfig,
};
use agent_observability_local_store::{DashboardQueryService, LocalStore};
use axum::{
    Json, Router,
    body::Body,
    extract::{
        DefaultBodyLimit, FromRequest, Path as AxumPath, RawQuery, State, rejection::JsonRejection,
    },
    http::{HeaderMap, HeaderValue, Method, Request, StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use hyper::{body::Incoming, server::conn::http1, service::service_fn};
use hyper_util::rt::{TokioIo, TokioTimer};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    env,
    ffi::OsStr,
    fs,
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddrV4, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{
    net::TcpListener,
    sync::{Notify, Semaphore, watch},
    task::JoinSet,
};
use tower::ServiceExt;

const SESSION_HEADER: &str = "x-agent-observability-session";
const MAX_REQUEST_BYTES: usize = 64 * 1024;
const DASHBOARD_PORT_BASE: u16 = 49_152;
const DASHBOARD_PORT_SPAN: u16 = 12_000;
const DASHBOARD_START_TIMEOUT: Duration = Duration::from_secs(5);
const DASHBOARD_PROBE_INTERVAL: Duration = Duration::from_millis(25);
const DASHBOARD_CAPABILITY_FILE: &str = "capability";
const DASHBOARD_IDENTITY_HEADER: &str = "x-agent-observability-dashboard";
const IDLE_TIMEOUT: Duration = Duration::from_mins(10);
const MAX_SESSION_LIFETIME: Duration = Duration::from_hours(1);
const IDLE_POLL_INTERVAL: Duration = Duration::from_secs(5);
const HEADER_READ_TIMEOUT: Duration = Duration::from_secs(5);
const GRACEFUL_DRAIN_TIMEOUT: Duration = Duration::from_secs(1);
#[cfg(target_os = "macos")]
const PLATFORM_OPEN_TIMEOUT: Duration = Duration::from_secs(5);
#[cfg(any(target_os = "macos", test))]
const PLATFORM_OPENER_PATH: &str = "/usr/bin/open";
#[cfg(any(target_os = "macos", test))]
const PLATFORM_REAP_TIMEOUT: Duration = Duration::from_secs(1);
const MAX_CONNECTIONS: usize = 64;
const SETTINGS_SHELL: &str = include_str!("generated/settings-shell.html");
const SETTINGS_SCRIPT: &str = include_str!("generated/settings-ui.js");
const SETTINGS_STYLE: &str = include_str!("generated/settings-ui.css");
#[cfg(test)]
static PLATFORM_OPEN_CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(test)]
static PLATFORM_OPEN_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Debug)]
pub enum UiError {
    Io(std::io::Error),
    Runtime(String),
    Random(String),
    DashboardArtifact(DashboardArtifactError),
    DashboardCapabilityCleanup { primary: Box<UiError> },
}

impl std::fmt::Display for UiError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "local settings UI I/O error: {error}"),
            Self::Runtime(message) => formatter.write_str(message),
            Self::Random(message) => write!(formatter, "local settings session error: {message}"),
            Self::DashboardArtifact(error) => {
                write!(formatter, "local dashboard artifact error: {error}")
            }
            Self::DashboardCapabilityCleanup { primary } => {
                write!(formatter, "{primary}; dashboard capability cleanup failed")
            }
        }
    }
}

impl std::error::Error for UiError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::DashboardCapabilityCleanup { primary } => Some(primary.as_ref()),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DashboardArtifactError {
    Missing,
    Unsafe,
    TooLarge,
    Unsupported,
    Io,
}

impl std::fmt::Display for DashboardArtifactError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Missing => "report is missing",
            Self::Unsafe => "report is not a private regular file",
            Self::TooLarge => "report exceeds the 32 MiB contract",
            Self::Unsupported => "private report serving is unsupported on this platform",
            Self::Io => "report could not be read",
        })
    }
}

impl std::error::Error for DashboardArtifactError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlatformOpenError {
    UnsupportedPlatform,
    SpawnFailed,
    ExitFailed,
    WaitFailed,
    TimedOut,
    TerminateFailed,
    ReapFailed,
    ReapTimedOut,
}

impl std::fmt::Display for PlatformOpenError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::UnsupportedPlatform => "automatic open is unsupported on this platform",
            Self::TimedOut => "automatic open timed out",
            Self::SpawnFailed
            | Self::ExitFailed
            | Self::WaitFailed
            | Self::TerminateFailed
            | Self::ReapFailed
            | Self::ReapTimedOut => "automatic open failed",
        })
    }
}

impl std::error::Error for PlatformOpenError {}

impl From<std::io::Error> for UiError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug)]
pub struct PreparedUi {
    listener: TcpListener,
    router: Router,
    url: String,
    shutdown: Arc<Notify>,
    last_seen: Arc<Mutex<Instant>>,
    _ui_singleton: ProductionSingleton,
}

impl PreparedUi {
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    pub async fn serve(self) -> Result<(), UiError> {
        self.serve_with_limits(HEADER_READ_TIMEOUT, GRACEFUL_DRAIN_TIMEOUT, MAX_CONNECTIONS)
            .await
    }

    async fn serve_with_limits(
        self,
        header_read_timeout: Duration,
        drain_timeout: Duration,
        max_connections: usize,
    ) -> Result<(), UiError> {
        let Self {
            listener,
            router,
            shutdown,
            last_seen,
            _ui_singleton,
            ..
        } = self;
        serve_transport(
            listener,
            router,
            shutdown,
            last_seen,
            header_read_timeout,
            drain_timeout,
            max_connections,
        )
        .await
    }
}

#[derive(Debug)]
pub struct PreparedDashboard {
    listener: TcpListener,
    router: Router,
    url: String,
    shutdown: Arc<Notify>,
    last_seen: Arc<Mutex<Instant>>,
    _dashboard_singleton: ProductionSingleton,
}

impl PreparedDashboard {
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    pub async fn serve(self) -> Result<(), UiError> {
        let Self {
            listener,
            router,
            shutdown,
            last_seen,
            _dashboard_singleton: dashboard_singleton,
            ..
        } = self;
        serve_transport(
            listener,
            router,
            shutdown,
            last_seen,
            HEADER_READ_TIMEOUT,
            GRACEFUL_DRAIN_TIMEOUT,
            MAX_CONNECTIONS,
        )
        .await?;
        drop(dashboard_singleton);
        Ok(())
    }
}

async fn serve_transport(
    listener: TcpListener,
    router: Router,
    shutdown: Arc<Notify>,
    last_seen: Arc<Mutex<Instant>>,
    header_read_timeout: Duration,
    drain_timeout: Duration,
    max_connections: usize,
) -> Result<(), UiError> {
    let (stop_tx, stop_rx) = watch::channel(false);
    let mut connections = JoinSet::new();
    let connection_slots = Arc::new(Semaphore::new(max_connections));
    let shutdown = shutdown_signal(shutdown, last_seen);
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            () = &mut shutdown => break,
            joined = connections.join_next(), if !connections.is_empty() => {
                let _ = joined;
            }
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                let Ok(connection_slot) = Arc::clone(&connection_slots).try_acquire_owned() else {
                    drop(stream);
                    continue;
                };
                let router = router.clone();
                let mut stop_rx = stop_rx.clone();
                connections.spawn(async move {
                    let _connection_slot = connection_slot;
                    let service = service_fn(move |request: Request<Incoming>| {
                        router.clone().oneshot(request.map(Body::new))
                    });
                    let mut builder = http1::Builder::new();
                    builder
                        .timer(TokioTimer::new())
                        .header_read_timeout(header_read_timeout);
                    let connection = builder.serve_connection(TokioIo::new(stream), service);
                    tokio::pin!(connection);
                    tokio::select! {
                        _ = &mut connection => {}
                        changed = stop_rx.changed() => {
                            if changed.is_ok() {
                                connection.as_mut().graceful_shutdown();
                                let _ = tokio::time::timeout(drain_timeout, &mut connection).await;
                            }
                        }
                    }
                });
            }
        }
    }

    let _ = stop_tx.send(true);
    drop(listener);
    if tokio::time::timeout(drain_timeout, async {
        while connections.join_next().await.is_some() {}
    })
    .await
    .is_err()
    {
        connections.abort_all();
        while connections.join_next().await.is_some() {}
    }
    Ok(())
}

#[derive(Clone, Debug)]
struct AppState {
    config: LocalConfigService,
    root: PathBuf,
    runtime: PathBuf,
    host: String,
    origin: String,
    token: String,
    shutdown: Arc<Notify>,
    last_seen: Arc<Mutex<Instant>>,
    dashboard_child: Arc<Mutex<Option<Child>>>,
}

#[derive(Clone, Debug)]
struct DashboardState {
    host: String,
    origin: String,
    token: String,
    report: PathBuf,
    paged: bool,
    query_service: Arc<Mutex<DashboardQueryService>>,
    query_slots: Arc<Semaphore>,
    initial_snapshot_failure: Option<DashboardStatusReasonV1>,
    root: PathBuf,
    last_seen: Arc<Mutex<Instant>>,
}

#[derive(Debug, Serialize)]
struct ConfigEnvelope {
    config: LocalRuntimeConfigV3,
    defaults: LocalRuntimeConfigV3,
    revision: String,
    collection_mode: &'static str,
}

#[derive(Debug, Serialize)]
struct DashboardLaunchResponse {
    url: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateRequest {
    config: LocalRuntimeConfigV3,
    revision: String,
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    code: &'static str,
    message: String,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl ApiError {
    fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }

    fn unauthorized() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            "invalid_session",
            "설정 세션이 만료되었거나 올바르지 않습니다.",
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorBody {
                code: self.code,
                message: self.message,
            }),
        )
            .into_response()
    }
}

pub async fn prepare(layout: &InstalledLayout) -> Result<PreparedUi, UiError> {
    let ui_singleton =
        ProductionSingleton::acquire(&layout.root, CoordinatedSingletonScope::SettingsUi)
            .map_err(|error| UiError::Runtime(error.to_string()))?;
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
    let address = listener.local_addr()?;
    let host = address.to_string();
    let origin = format!("http://{host}");
    let token = session_token()?;
    let shutdown = Arc::new(Notify::new());
    let last_seen = Arc::new(Mutex::new(Instant::now()));
    let state = AppState {
        config: LocalConfigService::new(layout),
        root: layout.root.clone(),
        runtime: layout.runtime.clone(),
        host,
        origin: origin.clone(),
        token: token.clone(),
        shutdown: Arc::clone(&shutdown),
        last_seen: Arc::clone(&last_seen),
        dashboard_child: Arc::new(Mutex::new(None)),
    };
    let router = router(state);

    Ok(PreparedUi {
        listener,
        router,
        url: format!("{origin}/#session={token}"),
        shutdown,
        last_seen,
        _ui_singleton: ui_singleton,
    })
}

pub async fn prepare_dashboard(layout: &InstalledLayout) -> Result<PreparedDashboard, UiError> {
    prepare_dashboard_with_status(layout, None).await
}

pub async fn prepare_dashboard_with_status(
    layout: &InstalledLayout,
    initial_snapshot_failure: Option<DashboardStatusReasonV1>,
) -> Result<PreparedDashboard, UiError> {
    let dashboard_singleton =
        ProductionSingleton::acquire(&layout.root, CoordinatedSingletonScope::DashboardUi)
            .map_err(|error| UiError::Runtime(error.to_string()))?;
    prepare_dashboard_path(layout, dashboard_singleton, initial_snapshot_failure).await
}

async fn prepare_dashboard_path(
    layout: &InstalledLayout,
    dashboard_singleton: ProductionSingleton,
    initial_snapshot_failure: Option<DashboardStatusReasonV1>,
) -> Result<PreparedDashboard, UiError> {
    // The interactive shell must be available even when the manual HTML export is absent
    // or exceeds its independent size bound. Snapshot availability is a query response.
    let report = layout.root.join("logs").join(REPORT_FILE_NAME);
    let token =
        load_or_create_dashboard_token(&layout.runtime.join("dashboard-ui"), &dashboard_singleton)?;
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, dashboard_port(&layout.root))).await?;
    let address = listener.local_addr()?;
    let host = address.to_string();
    let origin = format!("http://{host}");
    let shutdown = Arc::new(Notify::new());
    let last_seen = Arc::new(Mutex::new(Instant::now()));
    let state = DashboardState {
        host,
        origin: origin.clone(),
        token: token.clone(),
        report,
        paged: true,
        query_service: Arc::new(Mutex::new(dashboard_query_service()?)),
        query_slots: Arc::new(Semaphore::new(1)),
        initial_snapshot_failure,
        root: layout.root.clone(),
        last_seen: Arc::clone(&last_seen),
    };
    Ok(PreparedDashboard {
        listener,
        router: dashboard_router(state),
        url: format!("{origin}/report/{token}"),
        shutdown,
        last_seen,
        _dashboard_singleton: dashboard_singleton,
    })
}

fn dashboard_query_service() -> Result<DashboardQueryService, UiError> {
    let mut secret = [0_u8; 32];
    getrandom::fill(&mut secret).map_err(|error| UiError::Random(error.to_string()))?;
    Ok(DashboardQueryService::new(secret))
}

fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/assets/app.js", get(script))
        .route("/assets/app.css", get(style))
        .route("/favicon.ico", get(empty))
        .route("/api/config", get(get_config).put(put_config))
        .route(
            "/api/integrations/codex",
            get(get_codex_integration)
                .post(connect_codex_integration)
                .delete(disconnect_codex_integration),
        )
        .route("/api/dashboard/open", post(open_dashboard))
        .route("/api/dashboard/launch", post(launch_dashboard))
        .route("/api/heartbeat", post(heartbeat))
        .route("/api/shutdown", post(shutdown))
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BYTES))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            security_boundary,
        ))
        .with_state(state)
}

fn dashboard_router(state: DashboardState) -> Router {
    Router::new()
        .route("/report/{token}", get(dashboard_document))
        .route("/report/{token}/query", get(dashboard_query))
        .route(
            "/report/{token}/details/{turn_id}",
            get(dashboard_turn_detail),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            dashboard_security_boundary,
        ))
        .with_state(state)
}

async fn index() -> Html<&'static str> {
    Html(SETTINGS_SHELL)
}

async fn script() -> Response {
    static_asset(SETTINGS_SCRIPT, "text/javascript; charset=utf-8")
}

async fn style() -> Response {
    static_asset(SETTINGS_STYLE, "text/css; charset=utf-8")
}

async fn empty() -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn dashboard_document(
    State(state): State<DashboardState>,
    AxumPath(token): AxumPath<String>,
) -> Result<Response, StatusCode> {
    if !constant_time_equal(token.as_bytes(), state.token.as_bytes()) {
        return Err(StatusCode::NOT_FOUND);
    }
    touch_last_seen(&state.last_seen).map_err(|()| StatusCode::INTERNAL_SERVER_ERROR)?;
    let html = if state.paged {
        agent_observability_static_report::render_paged_dashboard()
            .map(String::into_bytes)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    } else {
        let report = state.report;
        tokio::task::spawn_blocking(move || read_private_report(&report))
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .map_err(|_| StatusCode::NOT_FOUND)?
    };
    let mut response = Body::from(html).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    response
        .headers_mut()
        .insert(DASHBOARD_IDENTITY_HEADER, HeaderValue::from_static("1"));
    Ok(response)
}

async fn dashboard_query(
    State(state): State<DashboardState>,
    AxumPath(token): AxumPath<String>,
    RawQuery(raw): RawQuery,
) -> Result<Response, StatusCode> {
    if !constant_time_equal(token.as_bytes(), state.token.as_bytes()) {
        return Err(StatusCode::NOT_FOUND);
    }
    let request =
        dashboard_request::decode_query(raw.as_deref()).map_err(|()| StatusCode::BAD_REQUEST)?;
    let kind = match &request {
        DashboardQueryRequestV1::Bootstrap(_) => DashboardRequestKindV1::Bootstrap,
        DashboardQueryRequestV1::Traces(_) => DashboardRequestKindV1::Traces,
        DashboardQueryRequestV1::Spans(_) => DashboardRequestKindV1::Spans,
        DashboardQueryRequestV1::Summary(_) => DashboardRequestKindV1::Summary,
        DashboardQueryRequestV1::Span(_) => DashboardRequestKindV1::Span,
        DashboardQueryRequestV1::Facets(_) => DashboardRequestKindV1::Facets,
    };
    let Ok(permit) = state.query_slots.clone().try_acquire_owned() else {
        return query_response(&query_status(kind, DashboardStatusReasonV1::Busy));
    };
    touch_last_seen(&state.last_seen).map_err(|()| StatusCode::INTERNAL_SERVER_ERROR)?;
    let response = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let Ok(mut service) = state.query_service.try_lock() else {
            return Ok(query_status(kind, DashboardStatusReasonV1::Busy));
        };
        // No migration, repairing projection, or retained authority connection on HTTP reads.
        let store = match LocalStore::open_report_reader(state.root.join("state/store")) {
            Ok(store) => store,
            Err(error) => {
                return reader_failure_reason(&error).map(|reason| query_status(kind, reason));
            }
        };
        let response = service.query(&store, request);
        Ok(match (&response, state.initial_snapshot_failure) {
            (DashboardQueryResponseV1::Status(status), Some(reason))
                if matches!(
                    status.reason,
                    DashboardStatusReasonV1::Building | DashboardStatusReasonV1::RefreshPending
                ) =>
            {
                query_status(kind, reason)
            }
            _ => response,
        })
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)??;
    query_response(&response)
}

fn reader_failure_reason(
    error: &agent_observability_local_store::StoreError,
) -> Result<DashboardStatusReasonV1, StatusCode> {
    if error.is_contention() {
        return Ok(DashboardStatusReasonV1::Busy);
    }
    if matches!(error, agent_observability_local_store::StoreError::Io(error)
        if error.kind() == std::io::ErrorKind::NotFound)
    {
        return Ok(DashboardStatusReasonV1::RefreshPending);
    }
    // Never turn corrupt, insecure or otherwise unreadable storage into a retry loop.
    Err(StatusCode::INTERNAL_SERVER_ERROR)
}

fn query_status(
    kind: DashboardRequestKindV1,
    reason: DashboardStatusReasonV1,
) -> DashboardQueryResponseV1 {
    DashboardQueryResponseV1::Status(DashboardStatusResponseV1 {
        schema_version: DASHBOARD_QUERY_VERSION.into(),
        kind: DashboardStatusKindV1::Status,
        request_kind: kind,
        reason,
        snapshot: None,
    })
}

fn query_response(response: &DashboardQueryResponseV1) -> Result<Response, StatusCode> {
    response
        .validate()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let body = serde_json::to_vec(response).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if body.len() > DASHBOARD_RESPONSE_MAX_SERIALIZED_UTF8_BYTES {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    let mut response = Body::from(body).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    Ok(response)
}

async fn dashboard_turn_detail(
    State(state): State<DashboardState>,
    AxumPath((token, turn_id)): AxumPath<(String, String)>,
) -> Response {
    if !constant_time_equal(token.as_bytes(), state.token.as_bytes()) {
        return private_detail_not_found();
    }
    if touch_last_seen(&state.last_seen).is_err() {
        return private_detail_not_found();
    }
    let root = state.root;
    match tokio::task::spawn_blocking(move || lookup_private_turn_detail(&root, &turn_id)).await {
        Ok(PrivateTurnDetailLookup::Available(detail)) => {
            let mut response = Body::from(detail).into_response();
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json; charset=utf-8"),
            );
            response
        }
        Ok(
            PrivateTurnDetailLookup::NotCollected
            | PrivateTurnDetailLookup::Failed("invalid_turn_id"),
        ) => private_detail_not_found(),
        Ok(PrivateTurnDetailLookup::Failed(code)) => private_detail_failed(code),
        Err(_) => private_detail_failed("lookup_failed"),
    }
}

fn private_detail_failed(code: &'static str) -> Response {
    let body = format!(r#"{{"error":"capture_failed","code":"{code}"}}"#);
    let mut response = (StatusCode::SERVICE_UNAVAILABLE, body).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json; charset=utf-8"),
    );
    response
}

fn private_detail_not_found() -> Response {
    let mut response = (StatusCode::NOT_FOUND, r#"{"error":"not_found"}"#).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json; charset=utf-8"),
    );
    response
}

fn static_asset(body: &'static str, content_type: &'static str) -> Response {
    let mut response = Body::from(body).into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    response
}

async fn get_config(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ConfigEnvelope>, ApiError> {
    authorize(&state, &headers, false)?;
    touch(&state)?;
    read_envelope(state.config).await.map(Json)
}

async fn put_config(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: Request<Body>,
) -> Result<Json<ConfigEnvelope>, ApiError> {
    authorize(&state, &headers, true)?;
    touch(&state)?;
    let Json(update) = Json::<UpdateRequest>::from_request(request, &state)
        .await
        .map_err(|error| map_json_rejection(&error))?;
    let config = state.config;
    let saved = tokio::task::spawn_blocking(move || config.save(&update.revision, &update.config))
        .await
        .map_err(|_| config_error(ConfigServiceError::Unavailable))?
        .map_err(config_error)?;
    Ok(Json(envelope(saved)))
}

async fn get_codex_integration(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<CodexIntegrationStatus>, ApiError> {
    authorize(&state, &headers, false)?;
    touch(&state)?;
    run_integration(state.root, codex_status).await.map(Json)
}

async fn connect_codex_integration(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<CodexIntegrationStatus>, ApiError> {
    authorize(&state, &headers, true)?;
    touch(&state)?;
    run_integration(state.root, connect_codex).await.map(Json)
}

async fn disconnect_codex_integration(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<CodexIntegrationStatus>, ApiError> {
    authorize(&state, &headers, true)?;
    touch(&state)?;
    run_integration(state.root, disconnect_codex)
        .await
        .map(Json)
}

async fn run_integration(
    root: PathBuf,
    operation: fn(&Path, &Path) -> Result<CodexIntegrationStatus, IntegrationError>,
) -> Result<CodexIntegrationStatus, ApiError> {
    let executable = env::current_exe()
        .and_then(fs::canonicalize)
        .map_err(|_| integration_error())?;
    tokio::task::spawn_blocking(move || operation(&root, &executable))
        .await
        .map_err(|_| integration_error())?
        .map_err(|error| integration_operation_error(&error))
}

async fn open_dashboard(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    authorize(&state, &headers, true)?;
    touch(&state)?;
    let dashboard_url = ensure_dashboard_server(state).await?;
    let open_result = tokio::task::spawn_blocking(move || open_dashboard_target(&dashboard_url))
        .await
        .map_err(|_| DashboardOpenError::TaskFailed)
        .and_then(|result| result)
        .map_err(dashboard_error);
    open_result?;
    Ok(StatusCode::NO_CONTENT)
}

async fn launch_dashboard(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<DashboardLaunchResponse>, ApiError> {
    authorize(&state, &headers, true)?;
    touch(&state)?;
    let url = ensure_dashboard_server(state).await?;
    Ok(Json(DashboardLaunchResponse { url }))
}

async fn ensure_dashboard_server(state: AppState) -> Result<String, ApiError> {
    let root = state.root;
    let runtime = state.runtime;
    let dashboard_child = state.dashboard_child;
    tokio::task::spawn_blocking(move || {
        launch_or_reuse_dashboard(&root, &runtime, &dashboard_child)
    })
    .await
    .map_err(|_| dashboard_error(DashboardOpenError::TaskFailed))?
    .map_err(dashboard_error)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DashboardOpenError {
    Artifact(DashboardArtifactError),
    Platform(PlatformOpenError),
    TaskFailed,
}

#[cfg(all(target_os = "macos", not(test)))]
fn open_dashboard_target(target: &str) -> Result<(), DashboardOpenError> {
    open_local_target(target).map_err(DashboardOpenError::Platform)
}

#[cfg(all(not(target_os = "macos"), not(test)))]
fn open_dashboard_target(_target: &str) -> Result<(), DashboardOpenError> {
    Err(DashboardOpenError::Platform(
        PlatformOpenError::UnsupportedPlatform,
    ))
}

#[cfg(test)]
fn open_dashboard_target(_target: &str) -> Result<(), DashboardOpenError> {
    PLATFORM_OPEN_CALLS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    Err(DashboardOpenError::Platform(
        PlatformOpenError::UnsupportedPlatform,
    ))
}

#[cfg(target_os = "macos")]
pub fn open_local_target(target: impl AsRef<OsStr>) -> Result<(), PlatformOpenError> {
    let mut command = platform_open_command(target.as_ref());
    run_platform_opener(&mut command, PLATFORM_OPEN_TIMEOUT)
}

#[cfg(not(target_os = "macos"))]
pub fn open_local_target(_target: impl AsRef<OsStr>) -> Result<(), PlatformOpenError> {
    Err(PlatformOpenError::UnsupportedPlatform)
}

#[cfg(any(target_os = "macos", test))]
fn platform_open_command(target: &OsStr) -> Command {
    let mut command = Command::new(PLATFORM_OPENER_PATH);
    command.arg(target);
    command
}

#[cfg(any(target_os = "macos", test))]
fn run_platform_opener(command: &mut Command, timeout: Duration) -> Result<(), PlatformOpenError> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| PlatformOpenError::SpawnFailed)?;
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return status
                    .success()
                    .then_some(())
                    .ok_or(PlatformOpenError::ExitFailed);
            }
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(None) => {
                terminate_and_reap_platform_opener(child)?;
                return Err(PlatformOpenError::TimedOut);
            }
            Err(_) => {
                terminate_and_reap_platform_opener(child)?;
                return Err(PlatformOpenError::WaitFailed);
            }
        }
    }
}

#[cfg(any(target_os = "macos", test))]
fn terminate_and_reap_platform_opener(mut child: Child) -> Result<(), PlatformOpenError> {
    let termination_failed = child.kill().is_err();
    let (result_tx, result_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = result_tx.send(child.wait());
    });
    match result_rx.recv_timeout(PLATFORM_REAP_TIMEOUT) {
        Ok(Ok(_)) if termination_failed => Err(PlatformOpenError::TerminateFailed),
        Ok(Ok(_)) => Ok(()),
        Ok(Err(_)) => Err(PlatformOpenError::ReapFailed),
        Err(_) => Err(PlatformOpenError::ReapTimedOut),
    }
}

fn integration_error() -> ApiError {
    ApiError::new(
        StatusCode::CONFLICT,
        "integration_failed",
        "Codex 자동 수집 상태를 확인하거나 변경할 수 없습니다.",
    )
}

fn integration_operation_error(error: &IntegrationError) -> ApiError {
    use agent_observability_local_collector::CollectorError;

    let (code, message) = match error {
        IntegrationError::ConnectCommittedSettingsFinalizationUnverified { .. } => (
            "integration_connect_committed_unverified",
            "Codex 연결 변경은 완료됐지만 설정 마무리를 확인하지 못했습니다. 현재 상태를 다시 확인해야 합니다.",
        ),
        IntegrationError::DisconnectCommittedSettingsFinalizationUnverified { .. } => (
            "integration_disconnect_committed_unverified",
            "Codex 연결 해제 변경은 완료됐지만 설정 마무리를 확인하지 못했습니다. 현재 상태를 다시 확인해야 합니다.",
        ),
        IntegrationError::Collector(CollectorError::StorageWriteUnverified {
            operation_completed: true,
            ..
        }) => (
            "integration_settings_completed_unverified",
            "설정 쓰기는 완료됐지만 검증하지 못했습니다. Codex 연결 상태를 다시 확인해야 합니다.",
        ),
        IntegrationError::Collector(CollectorError::StorageWriteUnverified {
            operation_completed: false,
            ..
        })
        | IntegrationError::SettingsRollbackFailed { .. }
        | IntegrationError::SettingsRollbackUnverified { .. } => (
            "integration_outcome_uncertain",
            "변경 또는 복원 결과를 확정할 수 없습니다. 현재 상태를 다시 확인해야 합니다.",
        ),
        _ => return integration_error(),
    };
    ApiError::new(StatusCode::CONFLICT, code, message)
}

fn dashboard_error(error: DashboardOpenError) -> ApiError {
    let (code, message) = match error {
        DashboardOpenError::Artifact(DashboardArtifactError::Missing) => (
            "dashboard_missing",
            "모니터링 리포트가 아직 생성되지 않았습니다.",
        ),
        DashboardOpenError::Artifact(DashboardArtifactError::Unsafe) => (
            "dashboard_unsafe",
            "모니터링 리포트의 로컬 보안 조건을 확인할 수 없습니다.",
        ),
        DashboardOpenError::Artifact(DashboardArtifactError::TooLarge) => (
            "dashboard_too_large",
            "모니터링 리포트가 32 MiB 제한을 초과했습니다.",
        ),
        DashboardOpenError::Artifact(DashboardArtifactError::Unsupported) => (
            "dashboard_unsupported",
            "이 운영체제에서는 private 모니터링 리포트를 제공할 수 없습니다.",
        ),
        DashboardOpenError::Artifact(DashboardArtifactError::Io) => (
            "dashboard_unavailable",
            "모니터링 리포트를 안전하게 읽을 수 없습니다.",
        ),
        DashboardOpenError::Platform(PlatformOpenError::UnsupportedPlatform) => (
            "dashboard_unsupported",
            "이 운영체제에서는 모니터링 리포트를 자동으로 열 수 없습니다.",
        ),
        DashboardOpenError::Platform(PlatformOpenError::TimedOut) => (
            "dashboard_open_timeout",
            "모니터링 리포트를 여는 시간이 초과되었습니다.",
        ),
        DashboardOpenError::Platform(
            PlatformOpenError::SpawnFailed
            | PlatformOpenError::ExitFailed
            | PlatformOpenError::WaitFailed
            | PlatformOpenError::TerminateFailed
            | PlatformOpenError::ReapFailed
            | PlatformOpenError::ReapTimedOut,
        )
        | DashboardOpenError::TaskFailed => {
            ("dashboard_open_failed", "모니터링 리포트를 열 수 없습니다.")
        }
    };
    ApiError::new(StatusCode::CONFLICT, code, message)
}

async fn heartbeat(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    authorize(&state, &headers, true)?;
    touch(&state)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn shutdown(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    authorize(&state, &headers, true)?;
    let notify = Arc::clone(&state.shutdown);
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(75)).await;
        notify.notify_one();
    });
    Ok(StatusCode::NO_CONTENT)
}

async fn security_boundary(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let valid_host = request
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == state.host);
    let valid_origin = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .is_none_or(|value| value == state.origin);
    let mut response = if valid_host && valid_origin {
        next.run(request).await
    } else {
        ApiError::new(
            StatusCode::FORBIDDEN,
            "loopback_boundary",
            "요청이 로컬 설정 경계를 벗어났습니다.",
        )
        .into_response()
    };
    apply_security_headers(response.headers_mut(), false);
    response
}

async fn dashboard_security_boundary(
    State(state): State<DashboardState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let valid_method = matches!(*request.method(), Method::GET | Method::HEAD);
    let valid_host = request
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == state.host);
    let valid_origin = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .is_none_or(|value| value == state.origin);
    let mut response = if valid_method && valid_host && valid_origin {
        next.run(request).await
    } else {
        StatusCode::FORBIDDEN.into_response()
    };
    apply_security_headers(response.headers_mut(), true);
    response
}

fn apply_security_headers(headers: &mut HeaderMap, is_dashboard: bool) {
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        if is_dashboard {
            HeaderValue::from_static(
                "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'",
            )
        } else {
            HeaderValue::from_static(
                "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self' data:; frame-ancestors 'none'; base-uri 'none'; form-action 'none'",
            )
        },
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, max-age=0"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
}

fn dashboard_port(root: &Path) -> u16 {
    let digest = Sha256::digest(root.to_string_lossy().as_bytes());
    let offset = u16::from_be_bytes([digest[0], digest[1]]) % DASHBOARD_PORT_SPAN;
    DASHBOARD_PORT_BASE + offset
}

fn dashboard_url(runtime: &Path, root: &Path) -> Result<Option<String>, DashboardArtifactError> {
    let Some(token) = read_dashboard_token(&runtime.join("dashboard-ui"))? else {
        return Ok(None);
    };
    Ok(Some(format!(
        "http://127.0.0.1:{}/report/{token}",
        dashboard_port(root)
    )))
}

fn launch_or_reuse_dashboard(
    root: &Path,
    runtime: &Path,
    child_slot: &Mutex<Option<Child>>,
) -> Result<String, DashboardOpenError> {
    if let Some(url) = dashboard_url(runtime, root).map_err(DashboardOpenError::Artifact)?
        && dashboard_probe(&url)
    {
        return Ok(url);
    }

    let executable = env::current_exe()
        .and_then(fs::canonicalize)
        .map_err(|_| DashboardOpenError::TaskFailed)?;
    let mut child = Command::new(executable)
        .arg("dashboard-serve")
        .arg(root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| DashboardOpenError::TaskFailed)?;
    let deadline = Instant::now() + DASHBOARD_START_TIMEOUT;
    while Instant::now() < deadline {
        if let Some(url) = dashboard_url(runtime, root).map_err(DashboardOpenError::Artifact)?
            && dashboard_probe(&url)
        {
            *child_slot
                .lock()
                .map_err(|_| DashboardOpenError::TaskFailed)? = Some(child);
            return Ok(url);
        }
        if child
            .try_wait()
            .map_err(|_| DashboardOpenError::TaskFailed)?
            .is_some()
        {
            return Err(DashboardOpenError::TaskFailed);
        }
        std::thread::sleep(DASHBOARD_PROBE_INTERVAL);
    }
    let _ = child.kill();
    let _ = child.wait();
    Err(DashboardOpenError::TaskFailed)
}

fn dashboard_probe(url: &str) -> bool {
    let Some(authority) = url.strip_prefix("http://127.0.0.1:") else {
        return false;
    };
    let Some((port, path)) = authority.split_once('/') else {
        return false;
    };
    let Ok(port) = port.parse::<u16>() else {
        return false;
    };
    let address = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
    let Ok(mut stream) = TcpStream::connect_timeout(&address.into(), Duration::from_millis(150))
    else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(150)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(150)));
    let request =
        format!("GET /{path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }
    let mut response = Vec::with_capacity(1024);
    let mut chunk = [0_u8; 256];
    while response.len() < 2048 {
        let Ok(count) = stream.read(&mut chunk) else {
            return false;
        };
        if count == 0 {
            break;
        }
        response.extend_from_slice(&chunk[..count]);
        if response.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    response.starts_with(b"HTTP/1.1 200")
        && response
            .windows(b"x-agent-observability-dashboard: 1".len())
            .any(|window| window == b"x-agent-observability-dashboard: 1")
}

#[cfg(unix)]
fn read_dashboard_token(dir: &Path) -> Result<Option<String>, DashboardArtifactError> {
    read_dashboard_token_observing(dir, |_| {}).map(|capability| capability.map(|(_, token)| token))
}

#[cfg(unix)]
fn read_dashboard_token_observing(
    dir: &Path,
    after_metadata: impl FnOnce(&Path),
) -> Result<Option<(fs::File, String)>, DashboardArtifactError> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let path = dir.join(DASHBOARD_CAPABILITY_FILE);
    let mut file = match fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(&path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(DashboardArtifactError::Unsafe),
    };
    let metadata = file.metadata().map_err(|_| DashboardArtifactError::Io)?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o077 != 0 || metadata.len() > 65 {
        return Err(DashboardArtifactError::Unsafe);
    }
    after_metadata(&path);
    let mut token = String::new();
    (&mut file)
        .take(66)
        .read_to_string(&mut token)
        .map_err(|_| DashboardArtifactError::Io)?;
    if token.len() > 65 {
        return Err(DashboardArtifactError::Unsafe);
    }
    let token = token.trim_end();
    if token.len() != 64
        || !token
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(DashboardArtifactError::Unsafe);
    }
    Ok(Some((file, token.to_owned())))
}

#[cfg(not(unix))]
fn read_dashboard_token(_dir: &Path) -> Result<Option<String>, DashboardArtifactError> {
    Err(DashboardArtifactError::Unsupported)
}

#[cfg(unix)]
fn load_or_create_dashboard_token(
    dir: &Path,
    singleton: &ProductionSingleton,
) -> Result<String, UiError> {
    load_or_create_dashboard_token_observing(dir, singleton, |_| Ok(()))
}

#[cfg(unix)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DashboardCapabilityCleanupFailure {
    Authorization,
    Identity,
    Unlink,
    DirectorySync,
    Postcondition,
}

#[cfg(unix)]
fn dashboard_capability_has_exact_identity(file: &fs::File, path: &Path) -> std::io::Result<bool> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let held = file.metadata()?;
    let named = fs::symlink_metadata(path)?;
    Ok(held.is_file()
        && named.is_file()
        && held.permissions().mode() & 0o7777 == 0o600
        && named.permissions().mode() & 0o7777 == 0o600
        && held.nlink() == 1
        && named.nlink() == 1
        && (held.dev(), held.ino()) == (named.dev(), named.ino()))
}

#[cfg(unix)]
fn cleanup_failed_dashboard_capability(
    dir: &Path,
    directory: &fs::File,
    path: &Path,
    file: &fs::File,
    singleton: &ProductionSingleton,
    permit: Option<&agent_observability_local_runtime::storage_coherence::StorageWriteGuard<'_>>,
    before_sync: &mut impl FnMut(&Path),
) -> Result<(), DashboardCapabilityCleanupFailure> {
    let revalidate = || {
        if let Some(permit) = permit {
            permit
                .revalidate()
                .map_err(|_| DashboardCapabilityCleanupFailure::Authorization)?;
        }
        singleton
            .revalidate()
            .map_err(|_| DashboardCapabilityCleanupFailure::Authorization)
    };

    revalidate()?;
    validate_dashboard_directory(directory, dir)
        .map_err(|_| DashboardCapabilityCleanupFailure::Identity)?;
    if !dashboard_capability_has_exact_identity(file, path)
        .map_err(|_| DashboardCapabilityCleanupFailure::Identity)?
    {
        return Err(DashboardCapabilityCleanupFailure::Identity);
    }
    fs::remove_file(path).map_err(|_| DashboardCapabilityCleanupFailure::Unlink)?;
    sync_dashboard_directory(directory, dir, before_sync)
        .map_err(|_| DashboardCapabilityCleanupFailure::DirectorySync)?;
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        _ => return Err(DashboardCapabilityCleanupFailure::Postcondition),
    }
    revalidate()?;
    Ok(())
}

#[cfg(unix)]
fn load_or_create_dashboard_token_observing(
    dir: &Path,
    singleton: &ProductionSingleton,
    after_persist: impl FnOnce(&Path) -> Result<(), UiError>,
) -> Result<String, UiError> {
    load_or_create_dashboard_token_with_read_observer(dir, singleton, after_persist, |_| {})
}

#[cfg(unix)]
fn load_or_create_dashboard_token_with_read_observer(
    dir: &Path,
    singleton: &ProductionSingleton,
    after_persist: impl FnOnce(&Path) -> Result<(), UiError>,
    after_read: impl FnOnce(&Path),
) -> Result<String, UiError> {
    load_or_create_dashboard_token_with_sync_observer(
        dir,
        singleton,
        after_persist,
        after_read,
        |_| {},
    )
}

#[cfg(unix)]
fn validate_dashboard_directory(directory: &fs::File, dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let held = directory.metadata()?;
    let named = fs::symlink_metadata(dir)?;
    if !held.is_dir()
        || !named.is_dir()
        || held.permissions().mode() & 0o7777 != 0o700
        || named.permissions().mode() & 0o7777 != 0o700
        || (held.dev(), held.ino()) != (named.dev(), named.ino())
    {
        return Err(std::io::Error::from(std::io::ErrorKind::InvalidData));
    }
    Ok(())
}

#[cfg(unix)]
fn sync_dashboard_directory(
    directory: &fs::File,
    dir: &Path,
    before_sync: &mut impl FnMut(&Path),
) -> std::io::Result<()> {
    before_sync(dir);
    validate_dashboard_directory(directory, dir)?;
    directory.sync_all()?;
    validate_dashboard_directory(directory, dir)
}

#[cfg(unix)]
fn load_or_create_dashboard_token_with_sync_observer(
    dir: &Path,
    singleton: &ProductionSingleton,
    after_persist: impl FnOnce(&Path) -> Result<(), UiError>,
    after_read: impl FnOnce(&Path),
    mut before_sync: impl FnMut(&Path),
) -> Result<String, UiError> {
    use std::os::unix::fs::OpenOptionsExt;

    let permit = singleton
        .try_begin_write()
        .map_err(|error| UiError::Runtime(error.to_string()))?;
    let directory = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_DIRECTORY)
        .open(dir)?;
    validate_dashboard_directory(&directory, dir)?;

    if let Some((file, token)) =
        read_dashboard_token_observing(dir, |_| {}).map_err(UiError::DashboardArtifact)?
    {
        after_read(&dir.join(DASHBOARD_CAPABILITY_FILE));
        if let Some(permit) = permit.as_ref() {
            permit
                .revalidate()
                .map_err(|error| UiError::Runtime(error.to_string()))?;
        }
        singleton
            .revalidate()
            .map_err(|error| UiError::Runtime(error.to_string()))?;
        if !dashboard_capability_has_exact_identity(&file, &dir.join(DASHBOARD_CAPABILITY_FILE))
            .map_err(|_| UiError::DashboardArtifact(DashboardArtifactError::Io))?
        {
            return Err(UiError::DashboardArtifact(DashboardArtifactError::Unsafe));
        }
        validate_dashboard_directory(&directory, dir)?;
        return Ok(token);
    }
    let token = session_token()?;
    let path = dir.join(DASHBOARD_CAPABILITY_FILE);
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&path)?;
    let result = (|| {
        file.write_all(token.as_bytes())?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        sync_dashboard_directory(&directory, dir, &mut before_sync)?;
        after_persist(&path)?;
        if !dashboard_capability_has_exact_identity(&file, &path)
            .map_err(|_| UiError::DashboardArtifact(DashboardArtifactError::Io))?
        {
            return Err(UiError::DashboardArtifact(DashboardArtifactError::Unsafe));
        }
        if read_dashboard_token(dir)
            .map_err(UiError::DashboardArtifact)?
            .as_deref()
            != Some(token.as_str())
        {
            return Err(UiError::DashboardArtifact(DashboardArtifactError::Unsafe));
        }
        if let Some(permit) = permit.as_ref() {
            permit
                .revalidate()
                .map_err(|error| UiError::Runtime(error.to_string()))?;
        }
        singleton
            .revalidate()
            .map_err(|error| UiError::Runtime(error.to_string()))?;
        if !dashboard_capability_has_exact_identity(&file, &path)
            .map_err(|_| UiError::DashboardArtifact(DashboardArtifactError::Io))?
        {
            return Err(UiError::DashboardArtifact(DashboardArtifactError::Unsafe));
        }
        Ok(())
    })();
    if let Err(error) = result {
        if cleanup_failed_dashboard_capability(
            dir,
            &directory,
            &path,
            &file,
            singleton,
            permit.as_ref(),
            &mut before_sync,
        )
        .is_err()
        {
            return Err(UiError::DashboardCapabilityCleanup {
                primary: Box::new(error),
            });
        }
        return Err(error);
    }
    Ok(token)
}

#[cfg(not(unix))]
fn load_or_create_dashboard_token(
    _dir: &Path,
    _singleton: &ProductionSingleton,
) -> Result<String, UiError> {
    Err(UiError::DashboardArtifact(
        DashboardArtifactError::Unsupported,
    ))
}

#[cfg(unix)]
fn read_private_report(path: &Path) -> Result<Vec<u8>, DashboardArtifactError> {
    let (file, length) = open_private_report(path)?;
    let mut html = Vec::with_capacity(usize::try_from(length).unwrap_or_default());
    file.take(MAX_REPORT_ARTIFACT_BYTES + 1)
        .read_to_end(&mut html)
        .map_err(|_| DashboardArtifactError::Io)?;
    if u64::try_from(html.len()).unwrap_or(u64::MAX) > MAX_REPORT_ARTIFACT_BYTES {
        return Err(DashboardArtifactError::TooLarge);
    }
    Ok(html)
}

#[cfg(unix)]
fn open_private_report(path: &Path) -> Result<(fs::File, u64), DashboardArtifactError> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|error| {
            if error.raw_os_error() == Some(libc::ELOOP) {
                DashboardArtifactError::Unsafe
            } else {
                match error.kind() {
                    std::io::ErrorKind::NotFound => DashboardArtifactError::Missing,
                    std::io::ErrorKind::PermissionDenied => DashboardArtifactError::Unsafe,
                    _ => DashboardArtifactError::Io,
                }
            }
        })?;
    let metadata = file.metadata().map_err(|_| DashboardArtifactError::Io)?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o077 != 0 {
        return Err(DashboardArtifactError::Unsafe);
    }
    if metadata.len() > MAX_REPORT_ARTIFACT_BYTES {
        return Err(DashboardArtifactError::TooLarge);
    }
    Ok((file, metadata.len()))
}

#[cfg(not(unix))]
fn read_private_report(_path: &Path) -> Result<Vec<u8>, DashboardArtifactError> {
    Err(DashboardArtifactError::Unsupported)
}

fn authorize(state: &AppState, headers: &HeaderMap, mutation: bool) -> Result<(), ApiError> {
    let supplied = headers
        .get(SESSION_HEADER)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(ApiError::unauthorized)?;
    if !constant_time_equal(supplied.as_bytes(), state.token.as_bytes()) {
        return Err(ApiError::unauthorized());
    }
    if mutation {
        let origin = headers
            .get(header::ORIGIN)
            .and_then(|value| value.to_str().ok());
        if origin != Some(state.origin.as_str()) {
            return Err(ApiError::new(
                StatusCode::FORBIDDEN,
                "invalid_origin",
                "변경 요청의 출처를 확인할 수 없습니다.",
            ));
        }
    }
    Ok(())
}

async fn read_envelope(config: LocalConfigService) -> Result<ConfigEnvelope, ApiError> {
    let versioned = tokio::task::spawn_blocking(move || config.read())
        .await
        .map_err(|_| config_error(ConfigServiceError::Unavailable))?
        .map_err(config_error)?;
    Ok(envelope(versioned))
}

fn envelope(versioned: VersionedLocalConfig) -> ConfigEnvelope {
    ConfigEnvelope {
        config: versioned.config,
        defaults: LocalRuntimeConfigV3::default(),
        revision: versioned.revision,
        collection_mode: "automatic_codex",
    }
}

fn config_error(error: ConfigServiceError) -> ApiError {
    match error {
        ConfigServiceError::Busy => ApiError::new(
            StatusCode::CONFLICT,
            "runtime_busy",
            "다른 로컬 작업이 실행 중입니다. 잠시 후 다시 저장하세요.",
        ),
        ConfigServiceError::Conflict => ApiError::new(
            StatusCode::CONFLICT,
            "config_conflict",
            "설정 파일이 변경되었습니다. 최신 값을 다시 불러오세요.",
        ),
        ConfigServiceError::Invalid => ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_config",
            "설정값이 허용된 범위를 벗어났습니다.",
        ),
        ConfigServiceError::Unavailable => ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "config_unavailable",
            "로컬 설정을 읽거나 저장할 수 없습니다.",
        ),
    }
}

fn session_token() -> Result<String, UiError> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|error| UiError::Random(error.to_string()))?;
    Ok(hex(&bytes))
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

fn touch(state: &AppState) -> Result<(), ApiError> {
    touch_last_seen(&state.last_seen).map_err(|()| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "session_state_failed",
            "설정 세션 상태를 갱신할 수 없습니다.",
        )
    })
}

fn touch_last_seen(last_seen: &Mutex<Instant>) -> Result<(), ()> {
    let mut last_seen = last_seen.lock().map_err(|_| ())?;
    *last_seen = Instant::now();
    Ok(())
}

fn map_json_rejection(error: &JsonRejection) -> ApiError {
    ApiError::new(
        StatusCode::BAD_REQUEST,
        "invalid_request",
        format!("설정 요청 형식이 올바르지 않습니다: {}", error.body_text()),
    )
}

async fn shutdown_signal(shutdown: Arc<Notify>, last_seen: Arc<Mutex<Instant>>) {
    let started_at = Instant::now();
    tokio::select! {
        () = shutdown.notified() => {}
        () = idle_expiry(last_seen, started_at) => {}
    }
}

async fn idle_expiry(last_seen: Arc<Mutex<Instant>>, started_at: Instant) {
    loop {
        tokio::time::sleep(IDLE_POLL_INTERVAL).await;
        let expired = started_at.elapsed() >= MAX_SESSION_LIFETIME
            || last_seen
                .lock()
                .map_or(true, |instant| instant.elapsed() >= IDLE_TIMEOUT);
        if expired {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AppState, DASHBOARD_CAPABILITY_FILE, DASHBOARD_IDENTITY_HEADER, DashboardArtifactError,
        DashboardOpenError, DashboardState, LocalConfigService, MAX_REPORT_ARTIFACT_BYTES,
        PLATFORM_OPEN_CALLS, PLATFORM_OPEN_TEST_LOCK, PlatformOpenError, REPORT_FILE_NAME, UiError,
        constant_time_equal, dashboard_error, dashboard_router, load_or_create_dashboard_token,
        load_or_create_dashboard_token_observing, platform_open_command, prepare,
        prepare_dashboard, read_private_report, router, run_integration, run_platform_opener,
        session_token,
    };
    use agent_observability_codex_integration::{CodexIntegrationStatus, IntegrationError};
    use agent_observability_contracts::hash_opaque_identifier;
    use agent_observability_local_runtime::{
        ConfigMutationGuard, LocalRuntimeConfigV3, install, load, revision, save,
    };
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode, header},
        response::IntoResponse,
    };
    use serde_json::Value;
    use std::{
        fs,
        path::{Path, PathBuf},
        process::{Command, Stdio},
        sync::{Arc, Mutex},
        time::{Duration, Instant},
    };
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpStream,
        sync::Notify,
    };
    use tower::ServiceExt;

    #[test]
    fn session_tokens_are_private_fixed_width_values() {
        let first = session_token().unwrap();
        let second = session_token().unwrap();
        assert_eq!(first.len(), 64);
        assert!(first.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_ne!(first, second);
        assert!(constant_time_equal(first.as_bytes(), first.as_bytes()));
        assert!(!constant_time_equal(first.as_bytes(), second.as_bytes()));
        assert!(!constant_time_equal(first.as_bytes(), b"short"));
    }

    #[test]
    fn config_revision_is_stable_and_change_sensitive() {
        let first = LocalRuntimeConfigV3::default();
        let mut second = first.clone();
        second.enabled = false;
        assert_eq!(revision(&first).unwrap(), revision(&first).unwrap());
        assert_ne!(revision(&first).unwrap(), revision(&second).unwrap());
    }

    fn path_bearing_integration_failure(
        _root: &Path,
        _executable: &Path,
    ) -> Result<CodexIntegrationStatus, IntegrationError> {
        Err(IntegrationError::Runtime(
            "/Users/private/AUTOMATIC_RAW_PROMPT_SENTINEL private-key.pem".into(),
        ))
    }

    #[test]
    fn integration_unverified_outcomes_are_explicit_and_content_free() {
        use agent_observability_local_collector::CollectorError;

        let private_error = || CollectorError::Io(std::io::Error::other("/private/SECRET_TOKEN"));
        let cases = [
            (
                IntegrationError::ConnectCommittedSettingsFinalizationUnverified {
                    finalization: private_error(),
                },
                "integration_connect_committed_unverified",
            ),
            (
                IntegrationError::DisconnectCommittedSettingsFinalizationUnverified {
                    finalization: private_error(),
                },
                "integration_disconnect_committed_unverified",
            ),
            (
                IntegrationError::Collector(CollectorError::StorageWriteUnverified {
                    operation_completed: true,
                    primary: None,
                    verification: Box::new(private_error()),
                }),
                "integration_settings_completed_unverified",
            ),
            (
                IntegrationError::Collector(CollectorError::StorageWriteUnverified {
                    operation_completed: false,
                    primary: None,
                    verification: Box::new(private_error()),
                }),
                "integration_outcome_uncertain",
            ),
            (
                IntegrationError::SettingsRollbackUnverified {
                    primary: Box::new(IntegrationError::Runtime("SECRET_TOKEN".into())),
                    rollback: private_error(),
                },
                "integration_outcome_uncertain",
            ),
            (
                IntegrationError::SettingsRollbackFailed {
                    primary: Box::new(IntegrationError::Runtime("SECRET_TOKEN".into())),
                    rollback: private_error(),
                },
                "integration_outcome_uncertain",
            ),
        ];
        let schema: Value = serde_json::from_str(include_str!(
            "../../../contracts/codex-integration-error-v1.schema.json"
        ))
        .unwrap();
        for (error, expected) in cases {
            let response = super::integration_operation_error(&error);
            assert_eq!(response.status, StatusCode::CONFLICT);
            assert_eq!(response.code, expected);
            assert!(
                schema["properties"]["code"]["enum"]
                    .as_array()
                    .unwrap()
                    .contains(&Value::String(response.code.into()))
            );
            let body = serde_json::to_string(&super::ErrorBody {
                code: response.code,
                message: response.message,
            })
            .unwrap();
            assert!(!body.contains("SECRET_TOKEN"));
            assert!(!body.contains("/private/"));
        }
    }

    #[test]
    fn integration_api_error_is_fixed_and_content_free() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let response =
                    run_integration(PathBuf::from("unused"), path_bearing_integration_failure)
                        .await
                        .unwrap_err()
                        .into_response();
                assert_eq!(response.status(), StatusCode::CONFLICT);
                let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
                let error: Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(error["code"], "integration_failed");
                assert_eq!(
                    error["message"],
                    "Codex 자동 수집 상태를 확인하거나 변경할 수 없습니다."
                );
                for sentinel in [
                    b"/Users/".as_slice(),
                    b"AUTOMATIC_RAW_PROMPT_SENTINEL",
                    b"private-key.pem",
                ] {
                    assert!(
                        !body
                            .windows(sentinel.len())
                            .any(|window| window == sentinel)
                    );
                }
            });
    }

    #[test]
    fn dashboard_opener_failure_is_content_free() {
        let mut command = Command::new("sh");
        command.args([
            "-c",
            "echo '/Users/private/AUTOMATIC_RAW_PROMPT_SENTINEL' >&2; exit 9",
        ]);
        assert!(run_platform_opener(&mut command, Duration::from_secs(1)).is_err());

        let response = dashboard_error(DashboardOpenError::Platform(PlatformOpenError::ExitFailed))
            .into_response();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
                let error: Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(error["code"], "dashboard_open_failed");
                assert_eq!(error["message"], "모니터링 리포트를 열 수 없습니다.");
                assert!(
                    !body
                        .windows(b"/Users/".len())
                        .any(|value| value == b"/Users/")
                );
                assert!(
                    !body
                        .windows(b"AUTOMATIC_RAW_PROMPT_SENTINEL".len())
                        .any(|value| value == b"AUTOMATIC_RAW_PROMPT_SENTINEL")
                );
            });
    }

    #[test]
    fn dashboard_opener_timeout_kills_and_reaps_the_process() {
        let pid_path = std::env::temp_dir().join(format!(
            "agent-observability-dashboard-opener-{}.pid",
            std::process::id()
        ));
        let _ = fs::remove_file(&pid_path);
        let mut command = Command::new("/bin/sh");
        command
            .args([
                "-c",
                "echo $$ > \"$1\"; exec /bin/sleep 30",
                "dashboard-opener-test",
            ])
            .arg(&pid_path);
        let started = Instant::now();

        assert_eq!(
            run_platform_opener(&mut command, Duration::from_millis(500)),
            Err(PlatformOpenError::TimedOut)
        );
        assert!(started.elapsed() < Duration::from_secs(2));
        let pid = fs::read_to_string(&pid_path).unwrap();
        let status = Command::new("/bin/sh")
            .args(["-c", "kill -0 \"$1\"", "dashboard-opener-test", pid.trim()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(!status.success());
        fs::remove_file(pid_path).unwrap();
    }

    #[test]
    fn platform_opener_wires_the_trusted_binary_and_target() {
        let target = std::ffi::OsStr::new("file:///private/report.html");
        let command = platform_open_command(target);
        assert_eq!(command.get_program(), std::ffi::OsStr::new("/usr/bin/open"));
        assert_eq!(command.get_args().collect::<Vec<_>>(), [target]);
    }

    #[test]
    fn test_build_dashboard_opener_is_fail_closed() {
        let _opener_guard = PLATFORM_OPEN_TEST_LOCK.lock().unwrap();
        let calls_before = PLATFORM_OPEN_CALLS.load(std::sync::atomic::Ordering::SeqCst);
        assert_eq!(
            super::open_dashboard_target("http://127.0.0.1:49152/report/private"),
            Err(DashboardOpenError::Platform(
                PlatformOpenError::UnsupportedPlatform
            ))
        );
        assert!(
            PLATFORM_OPEN_CALLS.load(std::sync::atomic::Ordering::SeqCst) > calls_before,
            "test opener sentinel must record and block the call"
        );
    }

    #[cfg(unix)]
    #[test]
    fn dashboard_launch_api_prepares_and_reuses_without_platform_opener() {
        let _opener_guard = PLATFORM_OPEN_TEST_LOCK.lock().unwrap();
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let root = std::env::temp_dir().join(format!(
                    "agent-observability-dashboard-launch-test-{}",
                    std::process::id()
                ));
                let _ = fs::remove_dir_all(&root);
                let layout = install(&root).unwrap();
                let report = layout.root.join("logs").join(REPORT_FILE_NAME);
                assert!(
                    !report.exists(),
                    "interactive launch cannot depend on a full HTML export"
                );

                let dashboard_server = prepare_dashboard(&layout).await.unwrap();
                let expected_url = dashboard_server.url().to_owned();
                assert_dashboard_pending_without_store(&dashboard_server).await;
                assert!(!report.exists());
                let dashboard_shutdown = Arc::clone(&dashboard_server.shutdown);
                let dashboard_task = tokio::spawn(dashboard_server.serve());
                tokio::task::yield_now().await;

                let state = AppState {
                    config: LocalConfigService::new(&layout),
                    root: layout.root.clone(),
                    runtime: layout.runtime.clone(),
                    host: "127.0.0.1:43191".into(),
                    origin: "http://127.0.0.1:43191".into(),
                    token: "test-session".into(),
                    shutdown: Arc::new(Notify::new()),
                    last_seen: Arc::new(Mutex::new(Instant::now())),
                    dashboard_child: Arc::new(Mutex::new(None)),
                };
                let app = router(state);
                let opener_calls_before =
                    PLATFORM_OPEN_CALLS.load(std::sync::atomic::Ordering::SeqCst);

                let unauthorized = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method("POST")
                            .uri("/api/dashboard/launch")
                            .header(header::HOST, "127.0.0.1:43191")
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

                let wrong_origin = app
                    .clone()
                    .oneshot(api_request(
                        "POST",
                        "/api/dashboard/launch",
                        "127.0.0.1:43191",
                        Some("http://example.invalid"),
                        None,
                    ))
                    .await
                    .unwrap();
                assert_eq!(wrong_origin.status(), StatusCode::FORBIDDEN);

                for _ in 0..2 {
                    let response = app
                        .clone()
                        .oneshot(api_request(
                            "POST",
                            "/api/dashboard/launch",
                            "127.0.0.1:43191",
                            Some("http://127.0.0.1:43191"),
                            None,
                        ))
                        .await
                        .unwrap();
                    assert_eq!(response.status(), StatusCode::OK);
                    assert_eq!(
                        response
                            .headers()
                            .get(header::CACHE_CONTROL)
                            .and_then(|value| value.to_str().ok()),
                        Some("no-store, max-age=0")
                    );
                    let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
                    let launch: Value = serde_json::from_slice(&body).unwrap();
                    assert_eq!(
                        launch["url"].as_str(),
                        Some(expected_url.as_str()),
                        "launch API must return the prepared dashboard URL"
                    );
                }
                assert_eq!(
                    PLATFORM_OPEN_CALLS.load(std::sync::atomic::Ordering::SeqCst),
                    opener_calls_before,
                    "launch-only API must never invoke the platform opener"
                );

                dashboard_shutdown.notify_one();
                dashboard_task.await.unwrap().unwrap();
                let _ = fs::remove_dir_all(root);
            });
    }

    #[test]
    fn dashboard_reader_classifies_absence_without_masking_durable_failures() {
        use agent_observability_local_store::StoreError;
        assert_eq!(
            super::reader_failure_reason(&StoreError::Io(std::io::Error::from(
                std::io::ErrorKind::NotFound
            ))),
            Ok(super::DashboardStatusReasonV1::RefreshPending)
        );
        for error in [
            StoreError::SchemaMismatch,
            StoreError::InsecurePermissions,
            StoreError::Symlink,
            StoreError::Io(std::io::Error::from(std::io::ErrorKind::PermissionDenied)),
            StoreError::Io(std::io::Error::from(std::io::ErrorKind::InvalidData)),
        ] {
            assert_eq!(
                super::reader_failure_reason(&error),
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn dashboard_directory_fifo_before_sync_returns_and_releases_permit() {
        use agent_observability_local_runtime::{
            CoordinatedSingletonScope, MutationGuard, ProductionSingleton,
            storage_coherence::StorageBarrier,
        };
        use std::os::unix::fs::FileTypeExt;

        const PROBE: &str = "AGENTOBS_DASHBOARD_DIRECTORY_SYNC_PROBE";
        let Ok(phase) = std::env::var(PROBE) else {
            for phase in ["publication", "cleanup"] {
                let mut child = Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "tests::dashboard_directory_fifo_before_sync_returns_and_releases_permit",
                    ])
                    .env(PROBE, phase)
                    .spawn()
                    .unwrap();
                let deadline = Instant::now() + Duration::from_secs(5);
                loop {
                    if let Some(status) = child.try_wait().unwrap() {
                        assert!(status.success(), "{phase}");
                        break;
                    }
                    if Instant::now() >= deadline {
                        child.kill().unwrap();
                        child.wait().unwrap();
                        panic!("{phase} directory sync blocked on FIFO");
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
            return;
        };
        let root = std::env::temp_dir().join(format!("dashboard-dir-sync-{}", std::process::id()));
        let layout = install(&root).unwrap();
        let mutation = MutationGuard::try_acquire(&layout.runtime).unwrap();
        let barrier = StorageBarrier::initialize(&root, &mutation).unwrap();
        let singleton =
            ProductionSingleton::acquire(&root, CoordinatedSingletonScope::DashboardUi).unwrap();
        let dir = layout.runtime.join("dashboard-ui");
        let displaced = layout.runtime.join("displaced-dashboard");
        let mut syncs = 0;
        let result = super::load_or_create_dashboard_token_with_sync_observer(
            &dir,
            &singleton,
            |_| Err(UiError::DashboardArtifact(DashboardArtifactError::Unsafe)),
            |_| {},
            |path| {
                syncs += 1;
                if syncs == if phase == "publication" { 1 } else { 2 } {
                    fs::rename(path, &displaced).unwrap();
                    assert!(
                        Command::new("mkfifo")
                            .args(["-m", "600"])
                            .arg(path)
                            .status()
                            .unwrap()
                            .success()
                    );
                }
            },
        );
        assert!(result.is_err());
        assert!(fs::symlink_metadata(&dir).unwrap().file_type().is_fifo());
        let freeze = barrier.try_freeze(&mutation).unwrap();
        drop(freeze);
        drop(singleton);
        assert!(fs::symlink_metadata(&dir).unwrap().file_type().is_fifo());
        drop(barrier);
        drop(mutation);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn dashboard_capability_growth_after_metadata_is_rejected() {
        use std::io::Write as _;
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!("dashboard-growth-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let path = root.join(DASHBOARD_CAPABILITY_FILE);
        fs::write(&path, "a".repeat(64)).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let result = super::read_dashboard_token_observing(&root, |path| {
            fs::OpenOptions::new()
                .append(true)
                .open(path)
                .unwrap()
                .write_all(&[b' '; 4096])
                .unwrap();
        });
        fs::remove_dir_all(&root).unwrap();
        assert!(matches!(result, Err(DashboardArtifactError::Unsafe)));
    }

    #[cfg(unix)]
    #[test]
    fn dashboard_capability_existing_replacement_is_rejected() {
        use agent_observability_local_runtime::{
            CoordinatedSingletonScope, MutationGuard, ProductionSingleton,
            storage_coherence::StorageBarrier,
        };
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!(
            "dashboard-existing-replacement-{}",
            std::process::id()
        ));
        let layout = install(&root).unwrap();
        let mutation = MutationGuard::try_acquire(&layout.runtime).unwrap();
        let barrier = StorageBarrier::initialize(&root, &mutation).unwrap();
        let singleton =
            ProductionSingleton::acquire(&root, CoordinatedSingletonScope::DashboardUi).unwrap();
        let dir = layout.runtime.join("dashboard-ui");
        let token = load_or_create_dashboard_token(&dir, &singleton).unwrap();
        let result = super::load_or_create_dashboard_token_with_read_observer(
            &dir,
            &singleton,
            |_| Ok(()),
            |path| {
                fs::rename(path, dir.join("original")).unwrap();
                fs::write(path, &token).unwrap();
                fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
            },
        );
        assert_eq!(
            fs::read_to_string(dir.join(DASHBOARD_CAPABILITY_FILE)).unwrap(),
            token
        );
        let freeze = barrier.try_freeze(&mutation).unwrap();
        drop(freeze);
        drop(singleton);
        drop(barrier);
        drop(mutation);
        fs::remove_dir_all(root).unwrap();
        assert!(matches!(
            result,
            Err(UiError::DashboardArtifact(DashboardArtifactError::Unsafe))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn coordinated_dashboard_capability_write_is_busy_without_partial_artifact() {
        use agent_observability_local_runtime::{
            CoordinatedSingletonScope, MutationGuard, ProductionSingleton,
            storage_coherence::StorageBarrier,
        };
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!(
            "agent-observability-dashboard-capability-busy-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let runtime = root.join("runtime");
        let mutation = MutationGuard::try_acquire(&runtime).unwrap();
        let barrier = StorageBarrier::initialize(&root, &mutation).unwrap();
        let singleton =
            ProductionSingleton::acquire(&root, CoordinatedSingletonScope::DashboardUi).unwrap();
        let freeze = barrier.try_freeze(&mutation).unwrap();
        let dashboard_dir = runtime.join("dashboard-ui");

        let error = load_or_create_dashboard_token(&dashboard_dir, &singleton).unwrap_err();
        assert!(matches!(
            error,
            UiError::Runtime(ref message)
                if message == "singleton coherence error: storage accounting barrier is busy"
        ));
        assert!(!dashboard_dir.join(DASHBOARD_CAPABILITY_FILE).exists());

        drop(freeze);
        drop(singleton);
        drop(barrier);
        drop(mutation);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn dashboard_capability_fifo_is_rejected_without_blocking_and_releases_permit() {
        use agent_observability_local_runtime::{
            CoordinatedSingletonScope, MutationGuard, ProductionSingleton,
            storage_coherence::StorageBarrier,
        };

        const PROBE: &str = "AGENTOBS_DASHBOARD_CAPABILITY_FIFO_PROBE";
        if std::env::var_os(PROBE).is_none() {
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "tests::dashboard_capability_fifo_is_rejected_without_blocking_and_releases_permit",
                ])
                .env(PROBE, "1")
                .spawn()
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                if let Some(status) = child.try_wait().unwrap() {
                    assert!(status.success());
                    return;
                }
                if Instant::now() >= deadline {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    panic!("dashboard capability read blocked on a FIFO");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        let root = std::env::temp_dir().join(format!(
            "agent-observability-dashboard-capability-fifo-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let layout = install(&root).unwrap();
        let mutation = MutationGuard::try_acquire(&layout.runtime).unwrap();
        let barrier = StorageBarrier::initialize(&root, &mutation).unwrap();
        let singleton =
            ProductionSingleton::acquire(&root, CoordinatedSingletonScope::DashboardUi).unwrap();
        let capability = layout
            .runtime
            .join("dashboard-ui")
            .join(DASHBOARD_CAPABILITY_FILE);
        assert!(
            Command::new("mkfifo")
                .args(["-m", "600"])
                .arg(&capability)
                .status()
                .unwrap()
                .success()
        );

        assert!(matches!(
            load_or_create_dashboard_token(capability.parent().unwrap(), &singleton),
            Err(UiError::DashboardArtifact(DashboardArtifactError::Unsafe))
        ));
        let freeze = barrier.try_freeze(&mutation).unwrap();

        drop(freeze);
        drop(singleton);
        drop(barrier);
        drop(mutation);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn failed_dashboard_capability_creation_removes_owned_artifact() {
        use agent_observability_local_runtime::{
            CoordinatedSingletonScope, MutationGuard, ProductionSingleton,
            storage_coherence::StorageBarrier,
        };

        let root = std::env::temp_dir().join(format!(
            "agent-observability-dashboard-capability-cleanup-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let layout = install(&root).unwrap();
        let mutation = MutationGuard::try_acquire(&layout.runtime).unwrap();
        let barrier = StorageBarrier::initialize(&root, &mutation).unwrap();
        let singleton =
            ProductionSingleton::acquire(&root, CoordinatedSingletonScope::DashboardUi).unwrap();
        let dashboard_dir = layout.runtime.join("dashboard-ui");
        let capability = dashboard_dir.join(DASHBOARD_CAPABILITY_FILE);

        let error = load_or_create_dashboard_token_observing(&dashboard_dir, &singleton, |_| {
            Err(UiError::DashboardArtifact(DashboardArtifactError::Unsafe))
        })
        .unwrap_err();
        assert!(matches!(
            error,
            UiError::DashboardArtifact(DashboardArtifactError::Unsafe)
        ));
        assert!(!capability.exists());
        let freeze = barrier.try_freeze(&mutation).unwrap();

        drop(freeze);
        drop(singleton);
        drop(barrier);
        drop(mutation);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn failed_dashboard_capability_creation_preserves_foreign_replacement() {
        use agent_observability_local_runtime::{
            CoordinatedSingletonScope, MutationGuard, ProductionSingleton,
            storage_coherence::StorageBarrier,
        };
        use std::io::Write as _;
        use std::os::unix::fs::OpenOptionsExt;

        let root = std::env::temp_dir().join(format!(
            "agent-observability-dashboard-capability-replacement-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let layout = install(&root).unwrap();
        let mutation = MutationGuard::try_acquire(&layout.runtime).unwrap();
        let barrier = StorageBarrier::initialize(&root, &mutation).unwrap();
        let singleton =
            ProductionSingleton::acquire(&root, CoordinatedSingletonScope::DashboardUi).unwrap();
        let dashboard_dir = layout.runtime.join("dashboard-ui");
        let capability = dashboard_dir.join(DASHBOARD_CAPABILITY_FILE);
        let displaced = dashboard_dir.join("displaced-capability");

        let error = load_or_create_dashboard_token_observing(&dashboard_dir, &singleton, |path| {
            fs::rename(path, &displaced)?;
            let mut foreign = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .open(path)?;
            foreign.write_all(b"FOREIGN_CAPABILITY_SENTINEL")?;
            foreign.sync_all()?;
            Err(UiError::DashboardArtifact(DashboardArtifactError::Unsafe))
        })
        .unwrap_err();
        assert!(matches!(
            error,
            UiError::DashboardCapabilityCleanup { ref primary }
                if matches!(primary.as_ref(), UiError::DashboardArtifact(DashboardArtifactError::Unsafe))
        ));
        assert_eq!(
            fs::read(&capability).unwrap(),
            b"FOREIGN_CAPABILITY_SENTINEL"
        );
        assert_eq!(
            error.to_string(),
            "local dashboard artifact error: report is not a private regular file; dashboard capability cleanup failed"
        );

        drop(singleton);
        drop(barrier);
        drop(mutation);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn dashboard_capability_cleanup_failure_is_explicit_and_sanitized() {
        use agent_observability_local_runtime::{
            CoordinatedSingletonScope, MutationGuard, ProductionSingleton,
            storage_coherence::StorageBarrier,
        };
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!(
            "agent-observability-dashboard-capability-cleanup-failure-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let layout = install(&root).unwrap();
        let mutation = MutationGuard::try_acquire(&layout.runtime).unwrap();
        let barrier = StorageBarrier::initialize(&root, &mutation).unwrap();
        let singleton =
            ProductionSingleton::acquire(&root, CoordinatedSingletonScope::DashboardUi).unwrap();
        let dashboard_dir = layout.runtime.join("dashboard-ui");
        let capability = dashboard_dir.join(DASHBOARD_CAPABILITY_FILE);

        let error = load_or_create_dashboard_token_observing(&dashboard_dir, &singleton, |path| {
            fs::set_permissions(path, fs::Permissions::from_mode(0o640))?;
            Err(UiError::DashboardArtifact(DashboardArtifactError::Unsafe))
        })
        .unwrap_err();
        let token = fs::read_to_string(&capability).unwrap();
        let rendered = error.to_string();
        assert!(matches!(
            error,
            UiError::DashboardCapabilityCleanup { ref primary }
                if matches!(primary.as_ref(), UiError::DashboardArtifact(DashboardArtifactError::Unsafe))
        ));
        assert_eq!(
            rendered,
            "local dashboard artifact error: report is not a private regular file; dashboard capability cleanup failed"
        );
        assert!(!rendered.contains(root.to_string_lossy().as_ref()));
        assert!(!rendered.contains(token.trim()));
        assert!(capability.exists());

        drop(singleton);
        drop(barrier);
        drop(mutation);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn coordinated_dashboard_releases_write_permit_and_preserves_metadata_on_busy_drop() {
        use agent_observability_local_runtime::{MutationGuard, storage_coherence::StorageBarrier};

        let root = std::env::temp_dir().join(format!(
            "agent-observability-dashboard-coordinated-lifetime-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let layout = install(&root).unwrap();
        let mutation = MutationGuard::try_acquire(&layout.runtime).unwrap();
        let barrier = StorageBarrier::initialize(&root, &mutation).unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let dashboard = runtime.block_on(prepare_dashboard(&layout)).unwrap();
        let metadata = layout.runtime.join("dashboard-ui/runtime.meta");
        let capability = layout.runtime.join("dashboard-ui/capability");
        let original_metadata = fs::read(&metadata).unwrap();
        assert!(capability.is_file());

        let freeze = barrier.try_freeze(&mutation).unwrap();
        drop(dashboard);
        assert_eq!(fs::read(&metadata).unwrap(), original_metadata);
        assert!(capability.is_file());

        drop(freeze);
        drop(barrier);
        drop(mutation);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn dashboard_queries_enforce_scope_and_single_worker_without_creating_store() {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
            let root = std::env::temp_dir().join(format!("agentobs-query-boundary-{}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            let layout = install(&root).unwrap();
            let slots = Arc::new(tokio::sync::Semaphore::new(1));
            let mut permit = Some(Arc::clone(&slots).try_acquire_owned().unwrap());
            let state = DashboardState {
                host: "127.0.0.1:43192".into(), origin: "http://127.0.0.1:43192".into(),
                token: "query-session".into(), report: root.join("absent.html"), paged: true,
                query_service: Arc::new(Mutex::new(super::DashboardQueryService::new([1; 32]))),
                query_slots: slots, initial_snapshot_failure: None, root: layout.root.clone(),
                last_seen: Arc::new(Mutex::new(Instant::now())),
            };
            let app = dashboard_router(state);
            let query = "request=%7B%22schemaVersion%22%3A%22agent_observability.dashboard_query.v1%22%2C%22kind%22%3A%22bootstrap%22%7D";
            for (method, token, host, origin, encoded, expected) in [
                ("GET", "wrong", "127.0.0.1:43192", "http://127.0.0.1:43192", query, StatusCode::NOT_FOUND),
                ("POST", "query-session", "127.0.0.1:43192", "http://127.0.0.1:43192", query, StatusCode::FORBIDDEN),
                ("GET", "query-session", "localhost:43192", "http://127.0.0.1:43192", query, StatusCode::FORBIDDEN),
                ("GET", "query-session", "127.0.0.1:43192", "http://example.invalid", query, StatusCode::FORBIDDEN),
                ("GET", "query-session", "127.0.0.1:43192", "http://127.0.0.1:43192", "request=%", StatusCode::BAD_REQUEST),
            ] {
                let response = app.clone().oneshot(Request::builder().method(method)
                    .uri(format!("/report/{token}/query?{encoded}"))
                    .header(header::HOST, host).header(header::ORIGIN, origin)
                    .body(Body::empty()).unwrap()).await.unwrap();
                assert_eq!(response.status(), expected);
                assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store, max-age=0");
            }
            for expected_reason in ["busy", "refresh_pending"] {
                let response = app.clone().oneshot(Request::builder()
                    .uri(format!("/report/query-session/query?{query}"))
                    .header(header::HOST, "127.0.0.1:43192").body(Body::empty()).unwrap()).await.unwrap();
                assert_eq!(response.status(), StatusCode::OK);
                let body = to_bytes(response.into_body(), 1024).await.unwrap();
                let body: Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(body["reason"], expected_reason);
                if expected_reason == "busy" { drop(permit.take()); }
            }
            assert!(!layout.state.join("store/local-store.sqlite3").exists());
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                drop(super::LocalStore::open(layout.state.join("store")).unwrap());
                fs::set_permissions(layout.state.join("store/local-store.sqlite3"), fs::Permissions::from_mode(0o644)).unwrap();
                let response = app.clone().oneshot(Request::builder()
                    .uri(format!("/report/query-session/query?{query}"))
                    .header(header::HOST, "127.0.0.1:43192").body(Body::empty()).unwrap()).await.unwrap();
                assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
                assert!(to_bytes(response.into_body(), 1024).await.unwrap().is_empty());
            }
            drop(permit);
            fs::remove_dir_all(root).unwrap();
        });
    }

    async fn assert_dashboard_pending_without_store(server: &super::PreparedDashboard) {
        let local_path = server.url().strip_prefix("http://127.0.0.1:").unwrap();
        let (port, path) = local_path.split_once('/').unwrap();
        let host = format!("127.0.0.1:{port}");
        let query_path = format!(
            "/{path}/query?request=%7B%22schemaVersion%22%3A%22agent_observability.dashboard_query.v1%22%2C%22kind%22%3A%22bootstrap%22%7D"
        );
        let response = server
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(query_path)
                    .header(header::HOST, host)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[header::CACHE_CONTROL],
            "no-store, max-age=0"
        );
        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let query: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(query["kind"], "status");
        assert_eq!(query["reason"], "refresh_pending");
    }

    #[test]
    fn platform_opener_stdio_helper() {
        if std::env::var_os("AGENTOBS_PLATFORM_STDIO_HELPER").is_none() {
            return;
        }
        let mut command = Command::new("/bin/sh");
        command.args([
            "-c",
            "if IFS= read -r _; then exit 9; fi; printf 'AUTOMATIC_PLATFORM_STDOUT_SENTINEL'; printf 'AUTOMATIC_PLATFORM_STDERR_SENTINEL' >&2",
        ]);
        assert_eq!(
            run_platform_opener(&mut command, Duration::from_secs(1)),
            Ok(())
        );
    }

    #[test]
    fn platform_opener_nulls_child_stdin_stdout_and_stderr() {
        use std::io::Write;

        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "tests::platform_opener_stdio_helper",
                "--nocapture",
            ])
            .env("AGENTOBS_PLATFORM_STDIO_HELPER", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(b"AUTOMATIC_PLATFORM_STDIN_SENTINEL\n")
            .unwrap();
        drop(child.stdin.take());
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        for sentinel in [
            b"AUTOMATIC_PLATFORM_STDOUT_SENTINEL".as_slice(),
            b"AUTOMATIC_PLATFORM_STDERR_SENTINEL".as_slice(),
        ] {
            let leaked = [&output.stdout, &output.stderr].iter().any(|stream| {
                stream
                    .windows(sentinel.len())
                    .any(|value| value == sentinel)
            });
            assert!(!leaked, "platform opener leaked a child standard stream");
        }
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn unsupported_platform_open_is_typed_and_content_free() {
        let error = super::open_local_target("AUTOMATIC_PRIVATE_TARGET_SENTINEL").unwrap_err();
        assert_eq!(error, PlatformOpenError::UnsupportedPlatform);
        assert_eq!(
            error.to_string(),
            "automatic open is unsupported on this platform"
        );
        assert!(
            !error
                .to_string()
                .contains("AUTOMATIC_PRIVATE_TARGET_SENTINEL")
        );

        let response = dashboard_error(DashboardOpenError::Platform(error)).into_response();
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[test]
    fn transport_bounds_partial_headers_and_shutdown_drain() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let root = std::env::temp_dir().join(format!(
                    "agent-observability-local-ui-transport-test-{}",
                    std::process::id()
                ));
                let _ = fs::remove_dir_all(&root);
                let layout = install(&root).unwrap();
                let prepared = prepare(&layout).await.unwrap();
                let address = prepared.listener.local_addr().unwrap();
                let shutdown = Arc::clone(&prepared.shutdown);
                let server = tokio::spawn(prepared.serve_with_limits(
                    Duration::from_millis(200),
                    Duration::from_millis(50),
                    1,
                ));

                let mut timed_out_header = TcpStream::connect(address).await.unwrap();
                timed_out_header
                    .write_all(b"GET / HTTP/1.1\r\nHost:")
                    .await
                    .unwrap();
                let mut byte = [0_u8; 1];
                let bytes_read = tokio::time::timeout(
                    Duration::from_millis(500),
                    timed_out_header.read(&mut byte),
                )
                .await
                .expect("partial header connection must close")
                .unwrap();
                assert_eq!(bytes_read, 0);

                let mut draining_connection = TcpStream::connect(address).await.unwrap();
                draining_connection
                    .write_all(b"GET / HTTP/1.1\r\nHost:")
                    .await
                    .unwrap();
                tokio::time::sleep(Duration::from_millis(10)).await;
                let mut overflow_connection = TcpStream::connect(address).await.unwrap();
                overflow_connection
                    .write_all(b"GET / HTTP/1.1\r\nHost:")
                    .await
                    .unwrap();
                let overflow_result = tokio::time::timeout(
                    Duration::from_millis(100),
                    overflow_connection.read(&mut byte),
                )
                .await
                .expect("connection above the fixed limit must close");
                match overflow_result {
                    Ok(0) => {}
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::BrokenPipe
                        ) => {}
                    other => panic!("unexpected overflow connection result: {other:?}"),
                }
                shutdown.notify_one();
                tokio::time::timeout(Duration::from_millis(500), server)
                    .await
                    .expect("shutdown drain must be bounded")
                    .unwrap()
                    .unwrap();
                let _ = fs::remove_dir_all(root);
            });
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn api_enforces_loopback_session_and_optimistic_revision() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let root = std::env::temp_dir().join(format!(
                    "agent-observability-local-ui-test-{}",
                    std::process::id()
                ));
                let _ = fs::remove_dir_all(&root);
                let layout = install(&root).unwrap();
                let state = AppState {
                    config: LocalConfigService::new(&layout),
                    root: layout.root.clone(),
                    runtime: layout.runtime.clone(),
                    host: "127.0.0.1:43191".into(),
                    origin: "http://127.0.0.1:43191".into(),
                    token: "test-session".into(),
                    shutdown: Arc::new(Notify::new()),
                    last_seen: Arc::new(Mutex::new(Instant::now())),
                    dashboard_child: Arc::new(Mutex::new(None)),
                };
                let app = router(state);

                let unauthorized = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .uri("/api/config")
                            .header(header::HOST, "127.0.0.1:43191")
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

                let wrong_host = app
                    .clone()
                    .oneshot(api_request(
                        "GET",
                        "/api/config",
                        "localhost:43191",
                        Some("http://127.0.0.1:43191"),
                        None,
                    ))
                    .await
                    .unwrap();
                assert_eq!(wrong_host.status(), StatusCode::FORBIDDEN);

                let wrong_origin = app
                    .clone()
                    .oneshot(api_request(
                        "GET",
                        "/api/config",
                        "127.0.0.1:43191",
                        Some("http://example.invalid"),
                        None,
                    ))
                    .await
                    .unwrap();
                assert_eq!(wrong_origin.status(), StatusCode::FORBIDDEN);

                let response = app
                    .clone()
                    .oneshot(api_request(
                        "GET",
                        "/api/config",
                        "127.0.0.1:43191",
                        Some("http://127.0.0.1:43191"),
                        None,
                    ))
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::OK);
                assert!(
                    response
                        .headers()
                        .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                        .is_none()
                );
                let bytes = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
                let envelope: Value = serde_json::from_slice(&bytes).unwrap();
                let stale_revision = envelope["revision"].as_str().unwrap().to_owned();
                let mut config: LocalRuntimeConfigV3 =
                    serde_json::from_value(envelope["config"].clone()).unwrap();
                config.enabled = false;
                config.storage_budget.workspace_budget_bytes = 805_306_368;
                let update = serde_json::json!({
                    "config": config,
                    "revision": stale_revision,
                });
                let saved = app
                    .clone()
                    .oneshot(api_request(
                        "PUT",
                        "/api/config",
                        "127.0.0.1:43191",
                        Some("http://127.0.0.1:43191"),
                        Some(update.to_string()),
                    ))
                    .await
                    .unwrap();
                assert_eq!(saved.status(), StatusCode::OK);
                let bytes = to_bytes(saved.into_body(), 64 * 1024).await.unwrap();
                let saved_envelope: Value = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(saved_envelope["config"]["enabled"], false);
                assert_eq!(saved_envelope["config"]["schema_version"], "local_runtime.v5");
                assert_eq!(saved_envelope["config"]["storage_budget"]["workspace_budget_bytes"], 805_306_368_u64);
                assert_ne!(saved_envelope["revision"], stale_revision);

                let original_bytes = fs::read(&layout.config).unwrap();
                let mut separated = saved_envelope["config"].clone();
                separated["storage_budget"]["mode"] = "separated".into();
                let denied = app.clone().oneshot(api_request(
                    "PUT", "/api/config", "127.0.0.1:43191", Some("http://127.0.0.1:43191"),
                    Some(serde_json::json!({"config": separated, "revision": saved_envelope["revision"]}).to_string()),
                )).await.unwrap();
                assert_eq!(denied.status(), StatusCode::UNPROCESSABLE_ENTITY);
                assert_eq!(fs::read(&layout.config).unwrap(), original_bytes);

                let conflict_response = app
                    .clone()
                    .oneshot(api_request(
                        "PUT",
                        "/api/config",
                        "127.0.0.1:43191",
                        Some("http://127.0.0.1:43191"),
                        Some(update.to_string()),
                    ))
                    .await
                    .unwrap();
                assert_eq!(conflict_response.status(), StatusCode::CONFLICT);
                fs::remove_file(&layout.config).unwrap();
                let unavailable_response = app
                    .oneshot(api_request(
                        "GET",
                        "/api/config",
                        "127.0.0.1:43191",
                        Some("http://127.0.0.1:43191"),
                        None,
                    ))
                    .await
                    .unwrap();
                assert_eq!(
                    unavailable_response.status(),
                    StatusCode::INTERNAL_SERVER_ERROR
                );
                let unavailable_body = to_bytes(unavailable_response.into_body(), 64 * 1024)
                    .await
                    .unwrap();
                let unavailable: Value = serde_json::from_slice(&unavailable_body).unwrap();
                assert_eq!(unavailable["code"], "config_unavailable");
                assert_eq!(
                    unavailable["message"],
                    "로컬 설정을 읽거나 저장할 수 없습니다."
                );
                assert!(!unavailable_body.windows(5).any(|window| window == b"/tmp/"));
                let _ = fs::remove_dir_all(root);
            });
    }

    #[cfg(unix)]
    #[test]
    fn dashboard_artifact_failures_remain_typed() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let root = std::env::temp_dir().join(format!(
            "agent-observability-dashboard-artifact-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let report = root.join("report.html");
        assert_eq!(
            read_private_report(&report).unwrap_err(),
            DashboardArtifactError::Missing
        );

        fs::write(&report, b"private").unwrap();
        fs::set_permissions(&report, fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(
            read_private_report(&report).unwrap_err(),
            DashboardArtifactError::Unsafe
        );
        fs::set_permissions(&report, fs::Permissions::from_mode(0o600)).unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(&report)
            .unwrap()
            .set_len(MAX_REPORT_ARTIFACT_BYTES + 1)
            .unwrap();
        assert_eq!(
            read_private_report(&report).unwrap_err(),
            DashboardArtifactError::TooLarge
        );

        fs::remove_file(&report).unwrap();
        let target = root.join("target.html");
        fs::write(&target, b"private").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
        symlink(&target, &report).unwrap();
        assert_eq!(
            read_private_report(&report).unwrap_err(),
            DashboardArtifactError::Unsafe
        );
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    #[allow(clippy::too_many_lines)]
    fn dashboard_surface_is_private_read_only_and_separate_from_settings() {
        use std::os::unix::fs::PermissionsExt;

        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let root = std::env::temp_dir().join(format!(
                    "agent-observability-dashboard-ui-test-{}",
                    std::process::id()
                ));
                let _ = fs::remove_dir_all(&root);
                let layout = install(&root).unwrap();
                let report = layout.root.join("logs").join(REPORT_FILE_NAME);
                fs::write(&report, b"<!doctype html><title>Private dashboard</title>").unwrap();
                fs::set_permissions(&report, fs::Permissions::from_mode(0o600)).unwrap();
                let settings_server = prepare(&layout).await.unwrap();
                let dashboard_server = prepare_dashboard(&layout).await.unwrap();
                assert!(settings_server.url().contains("/#session="));
                assert!(dashboard_server.url().contains("/report/"));
                drop(settings_server);
                drop(dashboard_server);
                let state = DashboardState {
                    host: "127.0.0.1:43192".into(),
                    origin: "http://127.0.0.1:43192".into(),
                    token: "dashboard-session".into(),
                    report: report.clone(),
                    paged: false,
                    query_service: Arc::new(Mutex::new(super::DashboardQueryService::new([1; 32]))),
                    query_slots: Arc::new(tokio::sync::Semaphore::new(1)),
                    initial_snapshot_failure: None,
                    root: layout.root.clone(),
                    last_seen: Arc::new(Mutex::new(Instant::now())),
                };
                let app = dashboard_router(state);

                let initial_dashboard = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .uri("/report/dashboard-session")
                            .header(header::HOST, "127.0.0.1:43192")
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                let initial_body = to_bytes(initial_dashboard.into_body(), 1024).await.unwrap();
                assert_eq!(
                    initial_body.as_ref(),
                    b"<!doctype html><title>Private dashboard</title>"
                );
                let replacement = report.with_extension("replacement");
                fs::write(&replacement, b"<!doctype html><title>Replacement</title>").unwrap();
                fs::set_permissions(&replacement, fs::Permissions::from_mode(0o600)).unwrap();
                fs::rename(replacement, &report).unwrap();

                let wrong_token = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .uri("/report/settings-session")
                            .header(header::HOST, "127.0.0.1:43192")
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(wrong_token.status(), StatusCode::NOT_FOUND);

                let wrong_host = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .uri("/report/dashboard-session")
                            .header(header::HOST, "localhost:43192")
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(wrong_host.status(), StatusCode::FORBIDDEN);

                let wrong_origin = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .uri("/report/dashboard-session")
                            .header(header::HOST, "127.0.0.1:43192")
                            .header(header::ORIGIN, "http://example.invalid")
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(wrong_origin.status(), StatusCode::FORBIDDEN);

                let dashboard = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .uri("/report/dashboard-session")
                            .header(header::HOST, "127.0.0.1:43192")
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(dashboard.status(), StatusCode::OK);
                let csp = dashboard
                    .headers()
                    .get(header::CONTENT_SECURITY_POLICY)
                    .unwrap()
                    .to_str()
                    .unwrap();
                assert!(csp.contains("connect-src 'self'"));
                assert!(csp.contains("frame-ancestors 'none'"));
                assert_eq!(
                    dashboard
                        .headers()
                        .get(DASHBOARD_IDENTITY_HEADER)
                        .and_then(|value| value.to_str().ok()),
                    Some("1")
                );
                let body = to_bytes(dashboard.into_body(), 1024).await.unwrap();
                assert_eq!(body.as_ref(), b"<!doctype html><title>Replacement</title>");

                let turn_id = hash_opaque_identifier("turn-private");
                let detail_uri = format!("/report/dashboard-session/details/{turn_id}");
                let absent = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .uri(&detail_uri)
                            .header(header::HOST, "127.0.0.1:43192")
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(absent.status(), StatusCode::NOT_FOUND);
                assert_eq!(
                    absent
                        .headers()
                        .get(header::CACHE_CONTROL)
                        .and_then(|value| value.to_str().ok()),
                    Some("no-store, max-age=0")
                );
                assert_eq!(
                    to_bytes(absent.into_body(), 128).await.unwrap().as_ref(),
                    br#"{"error":"not_found"}"#
                );

                let guard = ConfigMutationGuard::acquire(&layout).unwrap();
                let mut config = load(&layout.config).unwrap();
                config.capture_private_codex_turn_details = true;
                save(&guard, &config).unwrap();
                drop(guard);
                let digest = turn_id.strip_prefix("id:sha256:").unwrap();
                let detail_directory = layout.state.join("private-codex-turn-details");
                fs::create_dir_all(&detail_directory).unwrap();
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&detail_directory, fs::Permissions::from_mode(0o700))
                        .unwrap();
                }
                let detail_path = detail_directory.join(format!("{digest}.json"));
                fs::write(
                    &detail_path,
                    serde_json::to_vec(&serde_json::json!({
                        "schemaVersion": "agent_observability.private_turn_detail.v1",
                        "turnId": turn_id.clone(),
                        "cwd": "/Users/private/exact-project",
                        "inputMessages": ["PRIVATE_INPUT_SENTINEL"],
                        "lastAssistantMessage": "PRIVATE_OUTPUT_SENTINEL"
                    }))
                    .unwrap(),
                )
                .unwrap();
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&detail_path, fs::Permissions::from_mode(0o600)).unwrap();
                }
                let status_directory = layout.state.join("private-codex-turn-detail-statuses");
                fs::create_dir_all(&status_directory).unwrap();
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&status_directory, fs::Permissions::from_mode(0o700))
                        .unwrap();
                }
                let status_path = status_directory.join(format!("{digest}.json"));
                fs::write(
                    &status_path,
                    serde_json::to_vec(&serde_json::json!({
                        "schema_version": "private_codex_turn_detail_status.v1",
                        "turn_id": turn_id.clone(),
                        "state": "available",
                        "code": "ok"
                    }))
                    .unwrap(),
                )
                .unwrap();
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&status_path, fs::Permissions::from_mode(0o600)).unwrap();
                }

                let present = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .uri(&detail_uri)
                            .header(header::HOST, "127.0.0.1:43192")
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(present.status(), StatusCode::OK);
                assert_eq!(
                    present
                        .headers()
                        .get(header::CACHE_CONTROL)
                        .and_then(|value| value.to_str().ok()),
                    Some("no-store, max-age=0")
                );
                let detail: serde_json::Value = serde_json::from_slice(
                    &to_bytes(present.into_body(), 64 * 1024).await.unwrap(),
                )
                .unwrap();
                assert_eq!(
                    detail["schemaVersion"],
                    "agent_observability.private_turn_detail.v1"
                );
                assert_eq!(detail["turnId"], turn_id);
                assert_eq!(detail["cwd"], "/Users/private/exact-project");
                assert_eq!(detail["inputMessages"][0], "PRIVATE_INPUT_SENTINEL");
                assert_eq!(detail["lastAssistantMessage"], "PRIVATE_OUTPUT_SENTINEL");

                fs::write(detail_path, b"{broken").unwrap();
                let corrupt = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .uri(&detail_uri)
                            .header(header::HOST, "127.0.0.1:43192")
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(corrupt.status(), StatusCode::SERVICE_UNAVAILABLE);
                assert_eq!(
                    to_bytes(corrupt.into_body(), 128).await.unwrap().as_ref(),
                    br#"{"error":"capture_failed","code":"artifact_invalid"}"#
                );

                let invalid = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .uri("/report/dashboard-session/details/not-a-hash")
                            .header(header::HOST, "127.0.0.1:43192")
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(invalid.status(), StatusCode::NOT_FOUND);
                assert_eq!(
                    to_bytes(invalid.into_body(), 128).await.unwrap().as_ref(),
                    br#"{"error":"not_found"}"#
                );

                let settings_api_is_absent = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .uri("/api/config")
                            .header(header::HOST, "127.0.0.1:43192")
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(settings_api_is_absent.status(), StatusCode::NOT_FOUND);

                let mutation_is_rejected = app
                    .oneshot(
                        Request::builder()
                            .method("POST")
                            .uri("/report/dashboard-session")
                            .header(header::HOST, "127.0.0.1:43192")
                            .header(header::ORIGIN, "http://127.0.0.1:43192")
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(mutation_is_rejected.status(), StatusCode::FORBIDDEN);
                let _ = fs::remove_dir_all(root);
            });
    }

    fn api_request(
        method: &str,
        uri: &str,
        host: &str,
        origin: Option<&str>,
        body: Option<String>,
    ) -> Request<Body> {
        let mut builder = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::HOST, host)
            .header("x-agent-observability-session", "test-session");
        if let Some(origin) = origin {
            builder = builder.header(header::ORIGIN, origin);
        }
        if body.is_some() {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
        }
        builder.body(Body::from(body.unwrap_or_default())).unwrap()
    }
}
