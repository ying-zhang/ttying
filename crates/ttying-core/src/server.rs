use crate::session::{OutputChunk, SessionHandle, SessionManager};
use anyhow::Result;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, OriginalUri, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use base64::Engine;
use rand::rngs::SysRng;
use rand::TryRng;
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use subtle::ConstantTimeEq;
use tokio::sync::{broadcast, oneshot, Mutex, OwnedSemaphorePermit, RwLock, Semaphore};
use url::Url;

const MAX_MESSAGE_BYTES: usize = 1024 * 1024;
const MAX_ATTACH_BYTES: usize = 2048;
const LOGIN_WINDOW: Duration = Duration::from_secs(60);
const LOGIN_MAX_FAILURES: u32 = 10;

const CMD_INPUT: u8 = b'0';
const CMD_RESIZE: u8 = b'1';
const CMD_ATTACH: u8 = b'2';
const CMD_PING: u8 = b'3';
const SRV_OUTPUT: u8 = b'0';
const SRV_SESSION: u8 = b'1';
const SRV_PONG: u8 = b'2';
const SRV_ERROR: u8 = b'3';

#[derive(RustEmbed)]
#[folder = "assets"]
struct Assets;

#[derive(Clone, Debug)]
pub struct ServerConfig {
    pub bind: String,
    pub port: u16,
    pub token: String,
    pub command: Vec<String>,
    pub read_only: bool,
    pub max_clients: usize,
    pub allowed_origins: Vec<String>,
    pub session_ttl_secs: u64,
    pub secure_cookie: bool,
}

pub struct ServerHandle {
    pub addr: SocketAddr,
    shutdown_tx: Option<oneshot::Sender<()>>,
    join: Option<std::thread::JoinHandle<()>>,
}

impl ServerHandle {
    pub fn stop(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}
struct AppState {
    token: Arc<String>,
    secure_cookie: bool,
    stopping: AtomicBool,
    auth_sessions: Arc<RwLock<HashMap<String, Instant>>>,
    session_manager: Arc<SessionManager>,
    read_only: bool,
    session_ttl: Duration,
    allowed_origins: Arc<Vec<String>>,
    client_sem: Arc<Semaphore>,
    login_failures: Arc<Mutex<HashMap<String, (u32, Instant)>>>,
}

#[derive(Deserialize)]
struct ResizeMsg {
    cols: u16,
    rows: u16,
}

#[derive(Deserialize)]
struct AttachMsg {
    session: Option<String>,
    cols: Option<u16>,
    rows: Option<u16>,
}

#[derive(Serialize)]
struct SessionInfo<'a> {
    id: &'a str,
    reattached: bool,
    history_truncated: bool,
    read_only: bool,
    cols: u16,
    rows: u16,
}

#[derive(Serialize)]
struct ErrorInfo<'a> {
    code: &'a str,
    message: &'a str,
}

#[derive(Serialize)]
struct Health {
    ok: bool,
    version: &'static str,
    read_only: bool,
    session_grace_seconds: u64,
}

pub fn random_token() -> String {
    let mut bytes = [0u8; 32];
    let mut rng = SysRng;
    rng.try_fill_bytes(&mut bytes).expect("OS RNG failed");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn constant_eq(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.as_bytes().ct_eq(b.as_bytes()).into()
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
}

fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    for part in raw.split(';') {
        let mut it = part.trim().splitn(2, '=');
        if it.next()? == name {
            return it.next().map(ToString::to_string);
        }
    }
    None
}
async fn session_valid(state: &AppState, headers: &HeaderMap) -> bool {
    let Some(sid) = cookie_value(headers, "ttyd_session") else {
        return false;
    };
    let now = Instant::now();
    let mut sessions = state.auth_sessions.write().await;
    sessions.retain(|_, exp| *exp > now);
    sessions.get(&sid).is_some_and(|exp| *exp > now)
}

async fn is_authorized(state: &AppState, headers: &HeaderMap) -> bool {
    if let Some(token) = bearer(headers) {
        if constant_eq(token, state.token.as_str()) {
            return true;
        }
    }
    session_valid(state, headers).await
}

fn host_matches_origin(headers: &HeaderMap, origin: &str, secure: bool) -> bool {
    let Ok(url) = Url::parse(origin) else {
        return false;
    };
    let Some(host) = headers.get(header::HOST).and_then(|v| v.to_str().ok()) else {
        return false;
    };
    let scheme = if secure { "https" } else { "http" };
    let Ok(expected) = Url::parse(&format!("{scheme}://{host}")) else {
        return false;
    };
    !host.contains(['/', '@', '?', '#', '\\'])
        && url.scheme() == scheme
        && url.path() == "/"
        && url.query().is_none()
        && url.fragment().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && url.origin() == expected.origin()
}

fn origin_allowed(state: &AppState, headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) else {
        return true;
    };
    state.allowed_origins.iter().any(|o| o == origin)
        || host_matches_origin(headers, origin, state.secure_cookie)
}

async fn login_allowed(state: &AppState, peer: &str) -> bool {
    let now = Instant::now();
    let mut map = state.login_failures.lock().await;
    map.retain(|_, (_, first)| now.duration_since(*first) < LOGIN_WINDOW);
    map.get(peer)
        .is_none_or(|(count, _)| *count < LOGIN_MAX_FAILURES)
}

async fn record_login_failure(state: &AppState, peer: &str) {
    let now = Instant::now();
    let mut map = state.login_failures.lock().await;
    if map.len() >= 4096 && !map.contains_key(peer) {
        return;
    }
    let entry = map.entry(peer.to_string()).or_insert((0, now));
    entry.0 = entry.0.saturating_add(1);
}
pub fn start_background(config: ServerConfig) -> Result<ServerHandle> {
    anyhow::ensure!(
        config.token.len() >= 16 && config.token.bytes().all(|b| b.is_ascii_graphic()),
        "token must contain at least 16 printable ASCII bytes without whitespace"
    );
    anyhow::ensure!(!config.command.is_empty(), "empty command");
    anyhow::ensure!(
        (1..=1024).contains(&config.max_clients),
        "max_clients must be 1..1024"
    );
    anyhow::ensure!(
        (1..=604800).contains(&config.session_ttl_secs),
        "session TTL must be 1..604800 seconds"
    );
    let (addr_tx, addr_rx) = std::sync::mpsc::sync_channel::<Result<SocketAddr, String>>(1);
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let join = std::thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                let _ = addr_tx.send(Err(format!("runtime: {e}")));
                return;
            }
        };
        rt.block_on(async move {
            let listener =
                match tokio::net::TcpListener::bind((config.bind.as_str(), config.port)).await {
                    Ok(l) => l,
                    Err(e) => {
                        let _ = addr_tx.send(Err(format!("bind: {e}")));
                        return;
                    }
                };
            let addr = match listener.local_addr() {
                Ok(a) => a,
                Err(e) => {
                    let _ = addr_tx.send(Err(format!("local_addr: {e}")));
                    return;
                }
            };
            let _ = addr_tx.send(Ok(addr));
            let max_sessions = config.max_clients.max(2).saturating_mul(2);
            let state = Arc::new(AppState {
                token: Arc::new(config.token),
                secure_cookie: config.secure_cookie,
                stopping: AtomicBool::new(false),
                auth_sessions: Arc::new(RwLock::new(HashMap::new())),
                session_manager: Arc::new(SessionManager::new(
                    Arc::new(config.command),
                    config.read_only,
                    max_sessions,
                )),
                read_only: config.read_only,
                session_ttl: Duration::from_secs(config.session_ttl_secs),
                allowed_origins: Arc::new(config.allowed_origins),
                client_sem: Arc::new(Semaphore::new(config.max_clients.max(1))),
                login_failures: Arc::new(Mutex::new(HashMap::new())),
            });

            let cleanup_state = state.clone();
            tokio::spawn(async move {
                let mut tick = tokio::time::interval(Duration::from_secs(5));
                loop {
                    tick.tick().await;
                    cleanup_state.session_manager.cleanup();
                }
            });

            let app = build_router(state.clone());
            let shutdown_state = state.clone();
            let serve = axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
                shutdown_state.stopping.store(true, Ordering::Release);
                shutdown_state.session_manager.shutdown_all();
            });
            if let Err(e) = serve.await {
                tracing::error!("server stopped: {e}");
            }
            state.session_manager.shutdown_all();
        });
    });

    match addr_rx.recv_timeout(Duration::from_secs(10)) {
        Ok(Ok(addr)) => Ok(ServerHandle {
            addr,
            shutdown_tx: Some(shutdown_tx),
            join: Some(join),
        }),
        Ok(Err(e)) => Err(anyhow::anyhow!(e)),
        Err(e) => Err(anyhow::anyhow!("startup timeout: {e}")),
    }
}
fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/api/health", get(api_health))
        .route("/api/session", post(api_session))
        .route("/api/logout", post(api_logout))
        .route("/ws", get(ws_handler))
        .fallback(get(static_asset))
        .layer(middleware::from_fn(security_headers))
        .with_state(state)
}

async fn security_headers(req: axum::extract::Request, next: Next) -> Response {
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    h.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    h.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
             img-src 'self' data:; font-src 'self'; connect-src 'self' ws: wss:; \
             object-src 'none'; base-uri 'none'; frame-ancestors 'none'",
        ),
    );
    res
}

async fn index() -> Response {
    asset_response("index.html")
}

async fn static_asset(OriginalUri(uri): OriginalUri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.is_empty() {
        return asset_response("index.html");
    }
    asset_response(path)
}

fn asset_response(path: &str) -> Response {
    let Some(content) = Assets::get(path) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mime = match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "png" => "image/png",
        "svg" => "image/svg+xml",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    };
    let mut res = content.data.into_response();
    res.headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(mime));
    res
}
async fn api_health(State(state): State<Arc<AppState>>) -> Response {
    axum::Json(Health {
        ok: true,
        version: crate::VERSION,
        read_only: state.read_only,
        session_grace_seconds: crate::session::SESSION_GRACE_PERIOD.as_secs(),
    })
    .into_response()
}

async fn api_session(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    if !origin_allowed(&state, &headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let authorized_token = bearer(&headers).is_some_and(|t| constant_eq(t, state.token.as_str()));
    let existing = session_valid(&state, &headers).await;
    if !authorized_token && !existing {
        if !login_allowed(&state, &peer.ip().to_string()).await {
            return StatusCode::TOO_MANY_REQUESTS.into_response();
        }
        record_login_failure(&state, &peer.ip().to_string()).await;
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let sid = if existing {
        cookie_value(&headers, "ttyd_session").unwrap()
    } else {
        random_token()
    };
    let expires = Instant::now() + state.session_ttl;
    let mut sessions = state.auth_sessions.write().await;
    sessions.retain(|_, exp| *exp > Instant::now());
    if sessions.len() >= 4096 && !sessions.contains_key(&sid) {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    sessions.insert(sid.clone(), expires);
    drop(sessions);
    let secure = if state.secure_cookie { "; Secure" } else { "" };
    let cookie = format!(
        "ttyd_session={sid}; HttpOnly; SameSite=Strict; Path=/; Max-Age={}{secure}",
        state.session_ttl.as_secs()
    );
    let mut res = StatusCode::NO_CONTENT.into_response();
    if let Ok(value) = HeaderValue::from_str(&cookie) {
        res.headers_mut().insert(header::SET_COOKIE, value);
    }
    res
}

async fn api_logout(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if !origin_allowed(&state, &headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if let Some(sid) = cookie_value(&headers, "ttyd_session") {
        state.auth_sessions.write().await.remove(&sid);
    }
    let cookie = "ttyd_session=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0";
    let mut res = StatusCode::NO_CONTENT.into_response();
    if let Ok(value) = HeaderValue::from_str(cookie) {
        res.headers_mut().insert(header::SET_COOKIE, value);
    }
    res
}
struct Attachment {
    session: Arc<SessionHandle>,
    out_rx: broadcast::Receiver<Arc<OutputChunk>>,
    last_sent: u64,
}

async fn connection_authorized(state: &AppState, cookie: Option<&str>) -> bool {
    if state.stopping.load(Ordering::Acquire) {
        return false;
    }
    match cookie {
        None => true,
        Some(sid) => state
            .auth_sessions
            .read()
            .await
            .get(sid)
            .is_some_and(|exp| *exp > Instant::now()),
    }
}

fn frame(kind: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 1);
    out.push(kind);
    out.extend_from_slice(payload);
    out
}

async fn send_frame(socket: &mut WebSocket, kind: u8, payload: &[u8]) -> bool {
    matches!(
        tokio::time::timeout(
            Duration::from_secs(10),
            socket.send(Message::Binary(frame(kind, payload).into()))
        )
        .await,
        Ok(Ok(()))
    )
}

async fn send_json<T: Serialize>(socket: &mut WebSocket, kind: u8, value: &T) -> bool {
    match serde_json::to_vec(value) {
        Ok(payload) => send_frame(socket, kind, &payload).await,
        Err(_) => false,
    }
}

async fn send_error(socket: &mut WebSocket, code: &str, message: &str) -> bool {
    send_json(socket, SRV_ERROR, &ErrorInfo { code, message }).await
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    let Ok(permit) = state.client_sem.clone().try_acquire_owned() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    if !is_authorized(&state, &headers).await {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if !origin_allowed(&state, &headers) {
        tracing::warn!("refused websocket origin from {peer}");
        return StatusCode::FORBIDDEN.into_response();
    }
    let auth_cookie = if bearer(&headers).is_some_and(|t| constant_eq(t, state.token.as_str())) {
        None
    } else {
        cookie_value(&headers, "ttyd_session")
    };
    ws.max_message_size(MAX_MESSAGE_BYTES)
        .max_frame_size(MAX_MESSAGE_BYTES)
        .on_upgrade(move |socket| handle_socket(socket, state, peer, permit, auth_cookie))
}
async fn handle_attached_message(
    socket: &mut WebSocket,
    attachment: &Attachment,
    data: &[u8],
) -> bool {
    if data.is_empty() || data.len() > MAX_MESSAGE_BYTES {
        return false;
    }
    match data[0] {
        CMD_INPUT if !attachment.session.read_only => {
            if data.len() > 65537 || !attachment.session.write_input(data[1..].to_vec()) {
                let _ = send_error(
                    socket,
                    "input_overflow",
                    "Input queue full or input exceeds 64 KiB",
                )
                .await;
                return false;
            }
            true
        }
        CMD_INPUT => true,
        CMD_RESIZE => {
            if let Ok(size) = serde_json::from_slice::<ResizeMsg>(&data[1..]) {
                attachment
                    .session
                    .resize(size.cols.clamp(2, 1000), size.rows.clamp(2, 1000));
            }
            true
        }
        CMD_PING => send_frame(socket, SRV_PONG, &[]).await,
        CMD_ATTACH => true,
        _ => true,
    }
}

async fn handle_socket(
    mut socket: WebSocket,
    state: Arc<AppState>,
    peer: SocketAddr,
    _permit: OwnedSemaphorePermit,
    auth_cookie: Option<String>,
) {
    let mut attachment: Option<Attachment> = None;
    let attach_deadline = Instant::now() + Duration::from_secs(10);
    let mut last_received = Instant::now();
    let mut tick = tokio::time::interval(Duration::from_millis(500));

    'socket: loop {
        if !connection_authorized(&state, auth_cookie.as_deref()).await
            || last_received.elapsed() > Duration::from_secs(60)
        {
            break;
        }
        if let Some(att) = attachment.as_mut() {
            tokio::select! {
                maybe = att.out_rx.recv() => {
                    match maybe {
                        Ok(chunk) => {
                            if chunk.seq > att.last_sent {
                                if !send_frame(&mut socket, SRV_OUTPUT, &chunk.data).await {
                                    break;
                                }
                                att.last_sent = chunk.seq;
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => {
                            let (last_seq, history) = att.session.snapshot_history();
                            if history.first().is_some_and(|chunk| chunk.seq > att.last_sent + 1) {
                                let _ = send_error(&mut socket, "output_gap", "Output history overflow; reconnect to recover retained output").await;
                                break;
                            }
                            for chunk in history {
                                if chunk.seq > att.last_sent
                                    && !send_frame(&mut socket, SRV_OUTPUT, &chunk.data).await
                                {
                                    break 'socket;
                                }
                                att.last_sent = att.last_sent.max(chunk.seq);
                            }
                            att.last_sent = att.last_sent.max(last_seq);
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            let _ = send_error(&mut socket, "session_ended", "Shell exited").await;
                            break;
                        }
                    }
                }
                maybe = socket.recv() => {
                    let Some(Ok(msg)) = maybe else { break; };
                    if !connection_authorized(&state, auth_cookie.as_deref()).await { break; }
                    last_received = Instant::now();
                    match msg {
                        Message::Binary(data) => {
                            if !handle_attached_message(&mut socket, att, &data).await {
                                break;
                            }
                        }
                        Message::Close(_) | Message::Text(_) => break,
                        _ => {}
                    }
                }
                _ = tick.tick() => {
                    if att.session.is_ended() {
                        let _ = send_error(&mut socket, "session_ended", "Shell exited").await;
                        break;
                    }
                }
            }
        } else {
            if Instant::now() >= attach_deadline {
                break;
            }
            let msg = tokio::select! {
                result = socket.recv() => result,
                _ = tick.tick() => continue,
            };
            let Some(Ok(msg)) = msg else {
                break;
            };
            if !connection_authorized(&state, auth_cookie.as_deref()).await {
                break;
            }
            last_received = Instant::now();
            let Message::Binary(data) = msg else {
                if matches!(msg, Message::Close(_) | Message::Text(_)) {
                    break;
                }
                continue;
            };
            if data.is_empty() || data.len() > MAX_ATTACH_BYTES {
                let _ = send_error(&mut socket, "invalid_message", "Invalid control message").await;
                break;
            }
            match data[0] {
                CMD_PING => {
                    if !send_frame(&mut socket, SRV_PONG, &[]).await {
                        break;
                    }
                }
                CMD_ATTACH => {
                    let Ok(attach) = serde_json::from_slice::<AttachMsg>(&data[1..]) else {
                        let _ = send_error(&mut socket, "invalid_attach", "Invalid attach message")
                            .await;
                        break;
                    };
                    let cols = attach.cols.unwrap_or(80).clamp(2, 1000);
                    let rows = attach.rows.unwrap_or(24).clamp(2, 1000);
                    let manager = state.session_manager.clone();
                    let opened = tokio::task::spawn_blocking(move || {
                        manager.attach_or_create(attach.session.as_deref(), cols, rows)
                    })
                    .await
                    .unwrap_or_else(|e| Err(anyhow::anyhow!(e)));
                    let (session, reattached) = match opened {
                        Ok(value) => value,
                        Err(e) => {
                            tracing::error!("pty session open failed: {e}");
                            let _ = send_error(&mut socket, "pty_error", "Could not open terminal")
                                .await;
                            break;
                        }
                    };
                    let out_rx = session.subscribe_output();
                    let (last_sent, history) = session.snapshot_history();
                    let info = SessionInfo {
                        id: &session.id,
                        reattached,
                        history_truncated: history.first().is_some_and(|chunk| chunk.seq > 1),
                        read_only: session.read_only,
                        cols,
                        rows,
                    };
                    if !send_json(&mut socket, SRV_SESSION, &info).await {
                        session.detach();
                        break;
                    }
                    let mut ok = true;
                    for chunk in history {
                        if !send_frame(&mut socket, SRV_OUTPUT, &chunk.data).await {
                            ok = false;
                            break;
                        }
                    }
                    if !ok {
                        session.detach();
                        break;
                    }
                    attachment = Some(Attachment {
                        session,
                        out_rx,
                        last_sent,
                    });
                }
                _ => {
                    let _ = send_error(&mut socket, "attach_required", "Attach first").await;
                }
            }
        }
    }

    if let Some(att) = attachment {
        att.session.detach();
    }
    tracing::info!("websocket session closed for {peer}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_checks_scheme_ipv6_and_default_ports() {
        let mut h = HeaderMap::new();
        h.insert(header::HOST, "localhost:80".parse().unwrap());
        assert!(host_matches_origin(&h, "http://localhost", false));
        assert!(!host_matches_origin(&h, "https://localhost:80", false));
        assert!(!host_matches_origin(&h, "http://localhost/extra", false));
        h.insert(header::HOST, "[::1]:7681".parse().unwrap());
        assert!(host_matches_origin(&h, "http://[::1]:7681", false));
        h.insert(header::HOST, "localhost:bad".parse().unwrap());
        assert!(!host_matches_origin(&h, "http://localhost", false));
        h.insert(header::HOST, "example.test:443".parse().unwrap());
        assert!(host_matches_origin(&h, "https://example.test", true));
        assert!(!host_matches_origin(&h, "http://example.test:443", true));
    }
}
