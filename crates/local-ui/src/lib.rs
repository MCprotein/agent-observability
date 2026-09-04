#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc)]

use agent_observability_codex_integration::{
    CodexIntegrationStatus, IntegrationError, connect as connect_codex,
    disconnect as disconnect_codex, status as codex_status,
};
use agent_observability_local_collector::REPORT_FILE_NAME;
use agent_observability_local_runtime::{
    ConfigServiceError, InstalledLayout, LocalConfigService, LocalRuntimeConfigV2, Singleton,
    VersionedLocalConfig,
};
use axum::{
    Json, Router,
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, FromRequest, Path as AxumPath, State, rejection::JsonRejection},
    http::{HeaderMap, HeaderValue, Method, Request, StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use hyper::{body::Incoming, server::conn::http1, service::service_fn};
use hyper_util::rt::{TokioIo, TokioTimer};
use serde::{Deserialize, Serialize};
#[cfg(any(target_os = "macos", test))]
use std::process::{Child, Command, Stdio};
use std::{
    env,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
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
const MAX_REPORT_BYTES: u64 = 32 * 1024 * 1024;
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

#[derive(Debug)]
pub enum UiError {
    Io(std::io::Error),
    Runtime(String),
    Random(String),
}

impl std::fmt::Display for UiError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "local settings UI I/O error: {error}"),
            Self::Runtime(message) => formatter.write_str(message),
            Self::Random(message) => write!(formatter, "local settings session error: {message}"),
        }
    }
}

impl std::error::Error for UiError {}

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
    _ui_singleton: Singleton,
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
}

impl PreparedDashboard {
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    pub async fn serve(self) -> Result<(), UiError> {
        serve_transport(
            self.listener,
            self.router,
            self.shutdown,
            self.last_seen,
            HEADER_READ_TIMEOUT,
            GRACEFUL_DRAIN_TIMEOUT,
            MAX_CONNECTIONS,
        )
        .await
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
    host: String,
    origin: String,
    token: String,
    shutdown: Arc<Notify>,
    last_seen: Arc<Mutex<Instant>>,
}

#[derive(Clone, Debug)]
struct DashboardState {
    host: String,
    origin: String,
    token: String,
    html: Arc<[u8]>,
    last_seen: Arc<Mutex<Instant>>,
}

#[derive(Debug, Serialize)]
struct ConfigEnvelope {
    config: LocalRuntimeConfigV2,
    defaults: LocalRuntimeConfigV2,
    revision: String,
    collection_mode: &'static str,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateRequest {
    config: LocalRuntimeConfigV2,
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
    let ui_singleton = Singleton::acquire(&layout.runtime.join("settings-ui"))
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
        host,
        origin: origin.clone(),
        token: token.clone(),
        shutdown: Arc::clone(&shutdown),
        last_seen: Arc::clone(&last_seen),
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
    prepare_dashboard_path(layout.root.join("logs").join(REPORT_FILE_NAME)).await
}

async fn prepare_dashboard_path(path: PathBuf) -> Result<PreparedDashboard, UiError> {
    let html = tokio::task::spawn_blocking(move || read_private_report(&path))
        .await
        .map_err(|_| UiError::Runtime("local dashboard artifact task failed".into()))??;
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
    let address = listener.local_addr()?;
    let host = address.to_string();
    let origin = format!("http://{host}");
    let token = session_token()?;
    let shutdown = Arc::new(Notify::new());
    let last_seen = Arc::new(Mutex::new(Instant::now()));
    let state = DashboardState {
        host,
        origin: origin.clone(),
        token: token.clone(),
        html: Arc::from(html),
        last_seen: Arc::clone(&last_seen),
    };
    Ok(PreparedDashboard {
        listener,
        router: dashboard_router(state),
        url: format!("{origin}/report/{token}"),
        shutdown,
        last_seen,
    })
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
    let mut response = Body::from(Bytes::from_owner(Arc::clone(&state.html))).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    Ok(response)
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
        .map_err(|_| integration_error())
}

async fn open_dashboard(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    authorize(&state, &headers, true)?;
    touch(&state)?;
    let report = state.root.join("logs").join(REPORT_FILE_NAME);
    let dashboard = prepare_dashboard_path(report)
        .await
        .map_err(|_| dashboard_error(DashboardOpenError::MissingReport))?;
    let dashboard_url = dashboard.url().to_owned();
    let dashboard_shutdown = Arc::clone(&dashboard.shutdown);
    tokio::spawn(async move {
        let _ = dashboard.serve().await;
    });
    let open_result = tokio::task::spawn_blocking(move || open_dashboard_target(&dashboard_url))
        .await
        .map_err(|_| DashboardOpenError::TaskFailed)
        .and_then(|result| result)
        .map_err(dashboard_error);
    if open_result.is_err() {
        dashboard_shutdown.notify_one();
    }
    open_result?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DashboardOpenError {
    MissingReport,
    Platform(PlatformOpenError),
    TaskFailed,
}

#[cfg(target_os = "macos")]
fn open_dashboard_target(target: &str) -> Result<(), DashboardOpenError> {
    open_local_target(target).map_err(DashboardOpenError::Platform)
}

#[cfg(not(target_os = "macos"))]
fn open_dashboard_target(_target: &str) -> Result<(), DashboardOpenError> {
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

fn dashboard_error(error: DashboardOpenError) -> ApiError {
    let (code, message) = match error {
        DashboardOpenError::MissingReport => (
            "dashboard_missing",
            "모니터링 리포트가 아직 생성되지 않았습니다.",
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
                "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; connect-src 'none'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'",
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

#[cfg(unix)]
fn read_private_report(path: &Path) -> Result<Vec<u8>, std::io::Error> {
    use std::io::Read;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.permissions().mode() & 0o077 != 0
        || metadata.len() > MAX_REPORT_BYTES
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "report path is not a bounded private regular file",
        ));
    }
    let mut html = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or_default());
    file.take(MAX_REPORT_BYTES + 1).read_to_end(&mut html)?;
    if u64::try_from(html.len()).unwrap_or(u64::MAX) > MAX_REPORT_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "report exceeds the delivery bound",
        ));
    }
    Ok(html)
}

#[cfg(not(unix))]
fn read_private_report(_path: &Path) -> Result<Vec<u8>, std::io::Error> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "private report serving is unsupported on this platform",
    ))
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
        defaults: LocalRuntimeConfigV2::default(),
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
        AppState, DashboardOpenError, DashboardState, LocalConfigService, PlatformOpenError,
        REPORT_FILE_NAME, constant_time_equal, dashboard_error, dashboard_router,
        platform_open_command, prepare, prepare_dashboard, read_private_report, router,
        run_integration, run_platform_opener, session_token,
    };
    use agent_observability_codex_integration::{CodexIntegrationStatus, IntegrationError};
    use agent_observability_local_runtime::{LocalRuntimeConfigV2, install, revision};
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
        let first = LocalRuntimeConfigV2::default();
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
                    host: "127.0.0.1:43191".into(),
                    origin: "http://127.0.0.1:43191".into(),
                    token: "test-session".into(),
                    shutdown: Arc::new(Notify::new()),
                    last_seen: Arc::new(Mutex::new(Instant::now())),
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
                let mut config: LocalRuntimeConfigV2 =
                    serde_json::from_value(envelope["config"].clone()).unwrap();
                config.enabled = false;
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
                assert_ne!(saved_envelope["revision"], stale_revision);

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
                    html: Arc::from(read_private_report(&report).unwrap()),
                    last_seen: Arc::new(Mutex::new(Instant::now())),
                };
                fs::write(&report, b"<!doctype html><title>Replacement</title>").unwrap();
                let app = dashboard_router(state);

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
                assert!(csp.contains("connect-src 'none'"));
                assert!(csp.contains("frame-ancestors 'none'"));
                let body = to_bytes(dashboard.into_body(), 1024).await.unwrap();
                assert_eq!(
                    body.as_ref(),
                    b"<!doctype html><title>Private dashboard</title>"
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
