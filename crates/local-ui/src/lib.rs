#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc)]

use agent_observability_local_runtime::{
    ConfigError, ConfigMutationGuard, InstalledLayout, LocalRuntimeConfigV2, Singleton,
    SingletonError, load, revision as runtime_revision, save_if_revision,
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
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{
    net::TcpListener,
    sync::{Notify, watch},
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
        self.serve_with_limits(HEADER_READ_TIMEOUT, GRACEFUL_DRAIN_TIMEOUT)
            .await
    }

    async fn serve_with_limits(
        self,
        header_read_timeout: Duration,
        drain_timeout: Duration,
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
        let shutdown = shutdown_signal(shutdown, last_seen);
        tokio::pin!(shutdown);

        loop {
            tokio::select! {
                biased;
                () = &mut shutdown => break,
                accepted = listener.accept() => {
                    let (stream, _) = accepted?;
                    let router = router.clone();
                    let mut stop_rx = stop_rx.clone();
                    connections.spawn(async move {
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
                joined = connections.join_next(), if !connections.is_empty() => {
                    let _ = joined;
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
    layout: InstalledLayout,
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
        layout: layout.clone(),
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
    envelope(&state.layout.config).map(Json)
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
    update.config.validate().map_err(|error| {
        ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_config",
            error.to_string(),
        )
    })?;
    let mutation = ConfigMutationGuard::acquire(&state.layout).map_err(|error| match error {
        SingletonError::AlreadyRunning => ApiError::new(
            StatusCode::CONFLICT,
            "runtime_busy",
            "다른 로컬 작업이 실행 중입니다. 잠시 후 다시 저장하세요.",
        ),
        _ => ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "runtime_lock_failed",
            error.to_string(),
        ),
    })?;
    save_if_revision(&mutation, &update.revision, &update.config).map_err(|error| {
        if matches!(error, ConfigError::Conflict) {
            ApiError::new(
                StatusCode::CONFLICT,
                "config_conflict",
                "설정 파일이 다른 프로세스에서 변경되었습니다. 최신 값을 다시 불러오세요.",
            )
        } else {
            ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "save_failed",
                error.to_string(),
            )
        }
    })?;
    envelope(&state.layout.config).map(Json)
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

fn envelope(path: &Path) -> Result<ConfigEnvelope, ApiError> {
    let config = load_config(path)?;
    let revision = config_revision(&config)?;
    Ok(ConfigEnvelope {
        config,
        defaults: LocalRuntimeConfigV2::default(),
        revision,
        collection_mode: "manual_import",
    })
}

fn config_revision(config: &LocalRuntimeConfigV2) -> Result<String, ApiError> {
    runtime_revision(config).map_err(|error| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "revision_failed",
            error.to_string(),
        )
    })
}

fn load_config(path: &Path) -> Result<LocalRuntimeConfigV2, ApiError> {
    load(path).map_err(|error| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "config_unavailable",
            error.to_string(),
        )
    })
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
    use super::{AppState, constant_time_equal, prepare, router, session_token};
    use agent_observability_local_runtime::{LocalRuntimeConfigV2, install, revision};
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode, header},
    };
    use serde_json::Value;
    use std::{
        fs,
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
                let server = tokio::spawn(
                    prepared
                        .serve_with_limits(Duration::from_millis(50), Duration::from_millis(50)),
                );

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
                    layout,
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
