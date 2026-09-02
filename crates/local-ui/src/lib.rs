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
    body::Body,
    extract::{DefaultBodyLimit, FromRequest, State, rejection::JsonRejection},
    http::{HeaderMap, HeaderValue, Request, StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use hyper::{body::Incoming, server::conn::http1, service::service_fn};
use hyper_util::rt::{TokioIo, TokioTimer};
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
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
const IDLE_TIMEOUT: Duration = Duration::from_mins(10);
const MAX_SESSION_LIFETIME: Duration = Duration::from_hours(1);
const IDLE_POLL_INTERVAL: Duration = Duration::from_secs(5);
const HEADER_READ_TIMEOUT: Duration = Duration::from_secs(5);
const GRACEFUL_DRAIN_TIMEOUT: Duration = Duration::from_secs(1);
const DASHBOARD_OPEN_TIMEOUT: Duration = Duration::from_secs(5);
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
    tokio::task::spawn_blocking(move || open_dashboard_file(&report))
        .await
        .map_err(|_| dashboard_error())?
        .map_err(|()| dashboard_error())?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(target_os = "macos")]
fn open_dashboard_file(path: &Path) -> Result<(), ()> {
    if !path.is_file() {
        return Err(());
    }
    let mut command = Command::new("open");
    command.arg(path);
    run_dashboard_opener(&mut command, DASHBOARD_OPEN_TIMEOUT)
}

#[cfg(not(target_os = "macos"))]
fn open_dashboard_file(_path: &Path) -> Result<(), ()> {
    Err(())
}

fn run_dashboard_opener(command: &mut Command, timeout: Duration) -> Result<(), ()> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| ())?;
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success().then_some(()).ok_or(()),
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(None) | Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(());
            }
        }
    }
}

fn integration_error() -> ApiError {
    ApiError::new(
        StatusCode::CONFLICT,
        "integration_failed",
        "Codex 자동 수집 상태를 확인하거나 변경할 수 없습니다.",
    )
}

fn dashboard_error() -> ApiError {
    ApiError::new(
        StatusCode::CONFLICT,
        "dashboard_unavailable",
        "모니터링 리포트를 열 수 없습니다.",
    )
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
    apply_security_headers(response.headers_mut());
    response
}

fn apply_security_headers(headers: &mut HeaderMap) {
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self' data:; frame-ancestors 'none'; base-uri 'none'; form-action 'none'",
        ),
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
    let mut last_seen = state.last_seen.lock().map_err(|_| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "session_state_failed",
            "설정 세션 상태를 갱신할 수 없습니다.",
        )
    })?;
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
        AppState, LocalConfigService, constant_time_equal, dashboard_error, prepare, router,
        run_dashboard_opener, run_integration, session_token,
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
        process::Command,
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
    fn dashboard_opener_failure_is_bounded_and_content_free() {
        let mut command = Command::new("sh");
        command.args([
            "-c",
            "echo '/Users/private/AUTOMATIC_RAW_PROMPT_SENTINEL' >&2; exit 9",
        ]);
        assert!(run_dashboard_opener(&mut command, Duration::from_secs(1)).is_err());

        let response = dashboard_error().into_response();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
                let error: Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(error["code"], "dashboard_unavailable");
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
