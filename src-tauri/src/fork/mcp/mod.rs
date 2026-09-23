//! Phase 12: an MCP server inside the app, so Claude Code can search mail,
//! read threads, draft, file and look at the calendar through the same code
//! paths the UI uses. Streamable HTTP transport (`POST /mcp`, JSON-RPC 2.0)
//! on loopback only; bearer token from Credential Manager. There is no send
//! tool and no invite tool, by design (PLAN.md rule 8).
//!
//! Why this is hand-rolled and not `rmcp` 3.4.0: its
//! `transport-streamable-http-server` feature yields a `tower::Service` over
//! `http::Request` and ships no HTTP server. Driving it needs `hyper` (with
//! its `server` feature), `http` and `http-body-util` as direct dependencies,
//! none of which this crate may add, and it pulls `rmcp-macros`, `schemars`,
//! `sse-stream`, `rand 0.10` and `async-trait` for a binary that must stay
//! small. The subset of MCP the tools need (`initialize`, `ping`,
//! `tools/list`, `tools/call`) is a few hundred lines over
//! `tokio::net::TcpListener`, adds no dependency, and answers each POST with a
//! plain `application/json` body, which the spec allows in place of an SSE
//! stream. `GET /mcp` (the server-initiated stream) answers 405, also allowed.
//!
//! Register once with `claude mcp add --transport http skim
//! http://127.0.0.1:8342/mcp --header "Authorization: Bearer <token>" --scope
//! user`; Settings shows the exact line (`fork_mcp_status`).

pub mod tools;

use crate::db::queries;
use crate::db::Db;
use crate::error::{Result, SkimError};
use crate::state::AppState;
use futures::future::BoxFuture;
use serde::Serialize;
use serde_json::{json, Value};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;

/// Registered in `C:\Users\Patrick\OS\meta\PORTS.md` (HR15). Only the
/// production `start` binds it; tests bind port 0.
pub const PORT: u16 = 8342;
/// The one endpoint. Everything else is 404.
pub const PATH: &str = "/mcp";
/// Credential Manager entry holding the bearer token.
pub const TOKEN_KEY: &str = "fork:mcp_token";
/// Settings row: `on` (default when absent) or `off`.
pub const SETTING_KEY: &str = "fork_mcp";
pub const SERVER_NAME: &str = "skim";

/// Newest protocol revision this server speaks. Older ones in `SUPPORTED` are
/// echoed back when a client asks for them, per the spec's negotiation rule.
const PROTOCOL_VERSION: &str = "2025-06-18";
const SUPPORTED: &[&str] = &["2024-11-05", "2025-03-26", "2025-06-18"];

const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;
const READ_TIMEOUT: Duration = Duration::from_secs(30);

const INSTRUCTIONS: &str = "Skim is the user's mail client. Tools read the local mail cache \
(search_mail, get_thread, list_unread, list_court), the local calendar cache (get_calendar, \
find_free_slots), and queue the same offline-first operations the UI uses (archive, star, \
mark_read, create_draft, create_event). Nothing here sends mail or invites: create_draft saves \
to Drafts for the user to review and send; create_event has no attendees.";

// ---- runtime state ------------------------------------------------------------

/// What the app does after a tool wrote something, and the one tool that
/// needs the app's live session (CRM). Behind a trait object on purpose: the
/// tools must never name `AppHandle`, because any code that reaches the wry
/// runtime pulls `comctl32!TaskDialogIndirect` into whatever binary links it,
/// and the `cargo test` binary carries no Common Controls v6 manifest, so it
/// would die at load with `STATUS_ENTRYPOINT_NOT_FOUND`. Only the app binary
/// (via `tauri-build`'s manifest) may link that path, and only `launch` does.
pub trait Hooks: Send + Sync {
    /// After archive/star/read: poke the accounts' engines, `mail:updated`,
    /// badge, exactly what `commands::mail::queue_op` does.
    fn mail_changed(&self, account_ids: Vec<String>) -> BoxFuture<'_, ()>;
    /// After a draft was queued to Drafts: poke the engine, `drafts:updated`.
    fn drafts_changed(&self, account_id: String) -> BoxFuture<'_, ()>;
    /// After a calendar op was queued: `calendar:updated`, poke the engine.
    fn calendar_changed(&self, account_id: String) -> BoxFuture<'_, ()>;
    /// Phase 9 lookup as JSON; `Err` when Rebound is not connected.
    fn crm_lookup(&self, email: String) -> BoxFuture<'_, Result<Value>>;
}

/// What the server needs to answer a request: the app database, the token,
/// and (in production) the hooks into the running app. Tests build one
/// without hooks.
#[derive(Clone)]
pub struct Server {
    pub(crate) db: Db,
    token: Arc<String>,
    hooks: Option<Arc<dyn Hooks>>,
}

impl Server {
    pub fn new(db: Db, token: String, hooks: Option<Arc<dyn Hooks>>) -> Self {
        Self {
            db,
            token: Arc::new(token),
            hooks,
        }
    }

    pub(crate) fn hooks(&self) -> Option<&dyn Hooks> {
        self.hooks.as_deref()
    }
}

/// The production hooks: the running app.
struct AppHooks(AppHandle);

impl Hooks for AppHooks {
    fn mail_changed(&self, account_ids: Vec<String>) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            let state = self.0.state::<AppState>();
            {
                let engines = state.engines.lock().await;
                for id in account_ids {
                    if let Some(h) = engines.get(&id) {
                        h.run_ops();
                    }
                }
            }
            let _ = self.0.emit("mail:updated", json!({}));
            crate::badge::refresh(&self.0).await;
        })
    }

    fn drafts_changed(&self, account_id: String) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            let state = self.0.state::<AppState>();
            if let Some(h) = state.engines.lock().await.get(&account_id) {
                h.run_ops();
            }
            let _ = self.0.emit("drafts:updated", json!({}));
        })
    }

    fn calendar_changed(&self, account_id: String) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            let _ = self.0.emit(
                crate::fork::calendar::EVT_UPDATED,
                json!({ "account_id": account_id }),
            );
            if let Some(h) = crate::fork::calendar::handle(&account_id) {
                h.run_ops();
            }
        })
    }

    fn crm_lookup(&self, email: String) -> BoxFuture<'_, Result<Value>> {
        Box::pin(async move {
            let found =
                crate::fork::crm::fork_crm_lookup(self.0.state::<AppState>(), email).await?;
            serde_json::to_value(found).map_err(|e| SkimError::other("mcp", e.to_string()))
        })
    }
}

struct Running {
    stop: watch::Sender<bool>,
}

static RUNNING: Mutex<Option<Running>> = Mutex::new(None);
static LISTENING: AtomicBool = AtomicBool::new(false);

/// `fork_mcp` is on unless it is literally `off`.
pub fn is_enabled(conn: &rusqlite::Connection) -> rusqlite::Result<bool> {
    Ok(queries::get_setting(conn, SETTING_KEY)?.is_none_or(|v| v.trim() != "off"))
}

/// 32 bytes of OS randomness as 64 hex characters: shell-safe inside the
/// double quotes of the `claude mcp add` line, no padding to trip on.
fn random_token() -> String {
    let mut buf = [0u8; 32];
    getrandom::fill(&mut buf).expect("OS randomness unavailable");
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

/// The stored token, created on first use. Never logged.
pub fn ensure_token() -> Result<String> {
    if let Some(t) = crate::secrets::get(TOKEN_KEY)?.filter(|t| !t.trim().is_empty()) {
        return Ok(t);
    }
    let t = random_token();
    crate::secrets::set(TOKEN_KEY, &t)?;
    Ok(t)
}

/// The stored token for Settings to show. `None` until the server has run once.
pub fn token_for_display() -> Result<Option<String>> {
    Ok(crate::secrets::get(TOKEN_KEY)?.filter(|t| !t.trim().is_empty()))
}

/// The registration line Settings shows next to the token.
pub fn add_command(token: &str) -> String {
    format!(
        "claude mcp add --transport http {SERVER_NAME} http://127.0.0.1:{PORT}{PATH} \
         --header \"Authorization: Bearer {token}\" --scope user"
    )
}

/// Start the server if `fork_mcp` allows it. Called once from `fork::start`.
pub fn start(app: AppHandle) {
    let db = app.state::<AppState>().db.clone();
    tauri::async_runtime::spawn(async move {
        let enabled = db
            .read("fork_mcp_enabled", |conn| is_enabled(conn))
            .await
            .unwrap_or(false);
        if !enabled {
            tracing::info!("mcp: disabled by fork_mcp");
            return;
        }
        if let Err(e) = launch(app, db) {
            tracing::warn!(error = %e, "mcp: not started");
        }
    });
}

/// Bind `127.0.0.1:8342` now (so `listening` is true on return) and serve on
/// the app runtime. A second call while running is a no-op.
fn launch(app: AppHandle, db: Db) -> Result<()> {
    let mut running = RUNNING.lock().expect("mcp mutex poisoned");
    if running.is_some() {
        return Ok(());
    }
    let token = ensure_token()?;
    let std_listener =
        std::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, PORT)))
            .map_err(|e| SkimError::other("mcp_bind", format!("port {PORT} unavailable: {e}")))?;
    std_listener.set_nonblocking(true)?;
    let (tx, rx) = watch::channel(false);
    *running = Some(Running { stop: tx });
    LISTENING.store(true, Ordering::SeqCst);
    tauri::async_runtime::spawn(async move {
        match TcpListener::from_std(std_listener) {
            Ok(listener) => {
                tracing::info!(port = PORT, "mcp: listening");
                let hooks: Arc<dyn Hooks> = Arc::new(AppHooks(app));
                serve(Server::new(db, token, Some(hooks)), listener, rx).await;
            }
            Err(e) => tracing::warn!(error = %e, "mcp: listener handoff failed"),
        }
        LISTENING.store(false, Ordering::SeqCst);
        tracing::info!("mcp: stopped");
    });
    Ok(())
}

/// Stop accepting. In-flight requests finish; the port is released.
pub fn stop() {
    if let Some(r) = RUNNING.lock().expect("mcp mutex poisoned").take() {
        let _ = r.stop.send(true);
    }
}

pub fn is_listening() -> bool {
    LISTENING.load(Ordering::SeqCst)
}

/// Accept loop until `stop` flips. One task per connection; each connection
/// carries one request (`Connection: close`), which keeps the parser tiny.
pub async fn serve(server: Server, listener: TcpListener, mut stop: watch::Receiver<bool>) {
    loop {
        tokio::select! {
            _ = stop.changed() => break,
            accepted = listener.accept() => match accepted {
                Ok((stream, peer)) => {
                    let s = server.clone();
                    tokio::spawn(async move { handle_conn(s, stream, peer).await });
                }
                Err(e) => {
                    tracing::warn!(error = %e, "mcp: accept failed");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            },
        }
    }
}

// ---- status command ------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub enabled: bool,
    pub listening: bool,
    pub port: u16,
    pub endpoint: String,
    /// `None` until the server has created one (first start with `fork_mcp` on).
    pub token: Option<String>,
    /// The `claude mcp add …` line, ready to copy.
    pub add_command: Option<String>,
}

async fn status(db: &Db) -> Result<Status> {
    let enabled = db.read("fork_mcp_status", |conn| is_enabled(conn)).await?;
    let token = token_for_display()?;
    Ok(Status {
        enabled,
        listening: is_listening(),
        port: PORT,
        endpoint: format!("http://127.0.0.1:{PORT}{PATH}"),
        add_command: token.as_deref().map(add_command),
        token,
    })
}

/// Settings → MCP: what to show, including the registration line.
#[tauri::command]
pub async fn fork_mcp_status(state: State<'_, AppState>) -> Result<Status> {
    status(&state.db).await
}

/// Settings → MCP toggle: writes `fork_mcp` and starts or stops the listener
/// at once, so the panel's "listening" reflects the switch without a restart.
#[tauri::command]
pub async fn fork_mcp_set_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    on: bool,
) -> Result<Status> {
    state
        .db
        .call(move |conn| queries::set_setting(conn, SETTING_KEY, if on { "on" } else { "off" }))
        .await?;
    if on {
        launch(app, state.db.clone())?;
    } else {
        stop();
    }
    status(&state.db).await
}

// ---- HTTP/1.1, the little that is needed ---------------------------------------

/// One parsed request. Header names are lowercased.
pub struct Request {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
    pub content_type: Option<&'static str>,
    pub extra: Vec<(&'static str, String)>,
}

impl Response {
    pub fn empty(status: u16) -> Self {
        Self {
            status,
            body: Vec::new(),
            content_type: None,
            extra: Vec::new(),
        }
    }

    pub fn text(status: u16, msg: &str) -> Self {
        Self {
            status,
            body: msg.as_bytes().to_vec(),
            content_type: Some("text/plain; charset=utf-8"),
            extra: Vec::new(),
        }
    }

    pub fn json(status: u16, value: &Value) -> Self {
        Self {
            status,
            body: serde_json::to_vec(value).unwrap_or_default(),
            content_type: Some("application/json"),
            extra: Vec::new(),
        }
    }

    fn with(mut self, name: &'static str, value: impl Into<String>) -> Self {
        self.extra.push((name, value.into()));
        self
    }

    fn reason(status: u16) -> &'static str {
        match status {
            200 => "OK",
            202 => "Accepted",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            405 => "Method Not Allowed",
            408 => "Request Timeout",
            411 => "Length Required",
            413 => "Payload Too Large",
            431 => "Request Header Fields Too Large",
            _ => "Error",
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut head = format!(
            "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n",
            self.status,
            Self::reason(self.status),
            self.body.len()
        );
        if let Some(ct) = self.content_type {
            head.push_str(&format!("Content-Type: {ct}\r\n"));
        }
        for (k, v) in &self.extra {
            head.push_str(&format!("{k}: {v}\r\n"));
        }
        head.push_str("\r\n");
        let mut out = head.into_bytes();
        out.extend_from_slice(&self.body);
        out
    }
}

/// Only a loopback peer may talk to the server at all. The bind is loopback
/// already; this is the second lock for a listener handed in by someone else.
pub fn peer_allowed(ip: IpAddr) -> bool {
    ip.is_loopback()
}

async fn handle_conn(server: Server, mut stream: TcpStream, peer: SocketAddr) {
    let response = if !peer_allowed(peer.ip()) {
        Response::text(403, "loopback only")
    } else {
        match tokio::time::timeout(READ_TIMEOUT, read_request(&mut stream)).await {
            Ok(Ok(req)) => handle_request(&server, &req).await,
            Ok(Err(status)) => Response::text(status, Response::reason(status)),
            Err(_) => Response::text(408, "request timeout"),
        }
    };
    let _ = stream.write_all(&response.to_bytes()).await;
    let _ = stream.shutdown().await;
}

/// Read one HTTP/1.1 request. `Err(status)` names the refusal.
async fn read_request(stream: &mut TcpStream) -> std::result::Result<Request, u16> {
    const BAD: u16 = 400;
    let mut buf: Vec<u8> = Vec::with_capacity(4096);
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        if let Some(pos) = find_head_end(&buf) {
            break pos;
        }
        if buf.len() > MAX_HEADER_BYTES {
            return Err(431);
        }
        let n = stream.read(&mut chunk).await.map_err(|_| BAD)?;
        if n == 0 {
            return Err(400);
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = std::str::from_utf8(&buf[..head_end]).map_err(|_| BAD)?;
    let mut lines = head.split("\r\n");
    let request_line = lines.next().ok_or(BAD)?;
    let mut parts = request_line.split(' ');
    let method = parts.next().ok_or(BAD)?.to_string();
    let path = parts.next().ok_or(BAD)?.to_string();
    let version = parts.next().ok_or(BAD)?;
    if !version.starts_with("HTTP/1.") {
        return Err(400);
    }
    let mut headers = Vec::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let (k, v) = line.split_once(':').ok_or(BAD)?;
        headers.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
    }
    let req = Request {
        method,
        path,
        headers,
        body: Vec::new(),
    };
    if req
        .header("transfer-encoding")
        .is_some_and(|v| v.to_ascii_lowercase().contains("chunked"))
    {
        return Err(411);
    }
    let length: usize = match req.header("content-length") {
        Some(v) => v.trim().parse().map_err(|_| BAD)?,
        None => 0,
    };
    if length > MAX_BODY_BYTES {
        return Err(413);
    }
    let mut body = buf[head_end + 4..].to_vec();
    while body.len() < length {
        let n = stream.read(&mut chunk).await.map_err(|_| BAD)?;
        if n == 0 {
            return Err(400);
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(length);
    Ok(Request { body, ..req })
}

fn find_head_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

/// A browser page could reach the port with the user's cookies, so an
/// `Origin` header is only accepted from localhost itself (DNS rebinding
/// guard the spec asks for). Claude Code sends none.
pub fn origin_allowed(origin: &str) -> bool {
    let Some(rest) = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
    else {
        return false;
    };
    let authority = rest.split('/').next().unwrap_or("");
    let host = if let Some(stripped) = authority.strip_prefix('[') {
        stripped.split(']').next().unwrap_or("")
    } else {
        authority.split(':').next().unwrap_or("")
    };
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// `Authorization: Bearer <token>`, scheme case-insensitive, token exact.
pub fn authorized(header: Option<&str>, token: &str) -> bool {
    let Some((scheme, rest)) = header.and_then(|h| h.trim().split_once(' ')) else {
        return false;
    };
    scheme.eq_ignore_ascii_case("bearer")
        && constant_time_eq(rest.trim().as_bytes(), token.as_bytes())
}

/// Route one request: path, origin, token, method, then JSON-RPC.
pub async fn handle_request(server: &Server, req: &Request) -> Response {
    let path = req.path.split('?').next().unwrap_or("");
    if path != PATH {
        return Response::text(404, "not found");
    }
    if req.header("origin").is_some_and(|o| !origin_allowed(o)) {
        return Response::text(403, "origin not allowed");
    }
    if !authorized(req.header("authorization"), &server.token) {
        return Response::text(401, "unauthorized").with(
            "WWW-Authenticate",
            format!("Bearer realm=\"{SERVER_NAME}\""),
        );
    }
    if req.method != "POST" {
        // No server-initiated stream (GET) and no sessions to end (DELETE).
        return Response::text(405, "method not allowed").with("Allow", "POST");
    }
    let msg: Value = match serde_json::from_slice(&req.body) {
        Ok(v) => v,
        Err(e) => {
            return Response::json(
                400,
                &rpc_error(Value::Null, -32700, format!("parse error: {e}")),
            )
        }
    };
    match msg {
        Value::Array(items) => {
            let mut out = Vec::new();
            for item in items {
                if let Some(r) = handle_message(server, item).await {
                    out.push(r);
                }
            }
            if out.is_empty() {
                Response::empty(202)
            } else {
                Response::json(200, &Value::Array(out))
            }
        }
        other => match handle_message(server, other).await {
            Some(r) => Response::json(200, &r),
            None => Response::empty(202),
        },
    }
}

// ---- JSON-RPC 2.0 / MCP --------------------------------------------------------

fn rpc_error(id: Value, code: i64, message: impl Into<String>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message.into() },
    })
}

/// One JSON-RPC message in, at most one out (notifications and client
/// responses get nothing back, which the transport turns into a 202).
pub async fn handle_message(server: &Server, msg: Value) -> Option<Value> {
    let Some(obj) = msg.as_object() else {
        return Some(rpc_error(Value::Null, -32600, "invalid request"));
    };
    let id = obj.get("id").cloned().filter(|v| !v.is_null());
    let Some(method) = obj.get("method").and_then(Value::as_str) else {
        // A response to something we never asked (we send no requests): drop.
        if obj.contains_key("result") || obj.contains_key("error") {
            return None;
        }
        return Some(rpc_error(
            id.unwrap_or(Value::Null),
            -32600,
            "invalid request: no method",
        ));
    };
    let params = obj.get("params").cloned().unwrap_or_else(|| json!({}));
    let Some(id) = id else {
        tracing::debug!(method, "mcp: notification");
        return None;
    };
    let result: std::result::Result<Value, (i64, String)> = match method {
        "initialize" => Ok(initialize_result(&params)),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools::definitions() })),
        "tools/call" => call_tool(server, &params).await,
        _ => Err((-32601, format!("method not found: {method}"))),
    };
    Some(match result {
        Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
        Err((code, message)) => rpc_error(id, code, message),
    })
}

fn initialize_result(params: &Value) -> Value {
    let requested = params.get("protocolVersion").and_then(Value::as_str);
    let version = requested
        .filter(|v| SUPPORTED.contains(v))
        .unwrap_or(PROTOCOL_VERSION);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": {
            "name": SERVER_NAME,
            "title": "Skim",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "instructions": INSTRUCTIONS,
    })
}

async fn call_tool(server: &Server, params: &Value) -> std::result::Result<Value, (i64, String)> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or((-32602, "tools/call needs a name".to_string()))?;
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    match tools::call(server, name, &args).await {
        Ok(v) => Ok(json!({
            "content": [{ "type": "text", "text": serde_json::to_string_pretty(&v).unwrap_or_default() }],
            "structuredContent": v,
            "isError": false,
        })),
        Err(tools::ToolError::Unknown) => Err((-32602, format!("unknown tool: {name}"))),
        Err(tools::ToolError::Input(m)) | Err(tools::ToolError::Failed(m)) => Ok(json!({
            "content": [{ "type": "text", "text": m }],
            "isError": true,
        })),
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    /// A server on an ephemeral loopback port over the given database. Returns
    /// the base URL and the stop switch (dropping it stops the server).
    pub async fn spawn(db: Db, token: &str) -> (String, watch::Sender<bool>) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = watch::channel(false);
        let server = Server::new(db, token.to_string(), None);
        tokio::spawn(async move { serve(server, listener, rx).await });
        (format!("http://{addr}{PATH}"), tx)
    }

    /// One JSON-RPC request through the real HTTP path.
    pub async fn rpc(url: &str, token: Option<&str>, body: &Value) -> (u16, Value) {
        let client = reqwest::Client::new();
        let mut req = client
            .post(url)
            .header("Accept", "application/json, text/event-stream")
            .json(body);
        if let Some(t) = token {
            req = req.header("Authorization", format!("Bearer {t}"));
        }
        let resp = req.send().await.unwrap();
        let status = resp.status().as_u16();
        let text = resp.text().await.unwrap();
        // A refusal before JSON-RPC (401, 404, 405) is plain text.
        let value = if text.is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&text).unwrap_or(Value::String(text))
        };
        (status, value)
    }

    pub async fn call(url: &str, token: &str, tool: &str, args: Value) -> Value {
        let (status, v) = rpc(
            url,
            Some(token),
            &json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                     "params": { "name": tool, "arguments": args } }),
        )
        .await;
        assert_eq!(status, 200, "{v}");
        v["result"].clone()
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{rpc, spawn};
    use super::*;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn init() -> Value {
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": { "protocolVersion": "2025-06-18", "capabilities": {},
                            "clientInfo": { "name": "test", "version": "0" } } })
    }

    #[tokio::test]
    async fn refuses_without_a_token_and_with_the_wrong_one() {
        let (url, _stop) = spawn(Db::open_in_memory().unwrap(), TOKEN).await;
        let (status, body) = rpc(&url, None, &init()).await;
        assert_eq!(status, 401);
        assert_eq!(body, "unauthorized", "no JSON-RPC body before auth");
        let (status, _) = rpc(&url, Some("not-the-token"), &init()).await;
        assert_eq!(status, 401);
        // Same length, one byte off: the comparison is exact, not prefix.
        let mut near = TOKEN.to_string();
        near.replace_range(63..64, "0");
        let (status, _) = rpc(&url, Some(&near), &init()).await;
        assert_eq!(status, 401);
        let (status, body) = rpc(&url, Some(TOKEN), &init()).await;
        assert_eq!(status, 200);
        assert_eq!(body["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(body["result"]["serverInfo"]["name"], SERVER_NAME);
        assert!(body["result"]["capabilities"]["tools"].is_object());
    }

    #[tokio::test]
    async fn only_post_on_the_mcp_path() {
        let (url, _stop) = spawn(Db::open_in_memory().unwrap(), TOKEN).await;
        let client = reqwest::Client::new();
        let resp = client
            .get(&url)
            .header("Authorization", format!("Bearer {TOKEN}"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status().as_u16(), 405);
        assert_eq!(resp.headers().get("allow").unwrap(), "POST");
        let resp = client
            .delete(&url)
            .header("Authorization", format!("Bearer {TOKEN}"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status().as_u16(), 405);
        let other = url.replace(PATH, "/");
        let resp = client
            .post(&other)
            .header("Authorization", format!("Bearer {TOKEN}"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status().as_u16(), 404);
    }

    #[tokio::test]
    async fn a_browser_origin_is_refused_even_with_the_token() {
        let (url, _stop) = spawn(Db::open_in_memory().unwrap(), TOKEN).await;
        let client = reqwest::Client::new();
        let resp = client
            .post(&url)
            .header("Authorization", format!("Bearer {TOKEN}"))
            .header("Origin", "https://evil.example")
            .json(&init())
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status().as_u16(), 403);
        let resp = client
            .post(&url)
            .header("Authorization", format!("Bearer {TOKEN}"))
            .header("Origin", "http://localhost:1420")
            .json(&init())
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status().as_u16(), 200);
    }

    #[test]
    fn peer_and_origin_predicates() {
        assert!(peer_allowed("127.0.0.1".parse().unwrap()));
        assert!(peer_allowed("::1".parse().unwrap()));
        assert!(!peer_allowed("192.168.1.10".parse().unwrap()));
        assert!(!peer_allowed("100.64.0.1".parse().unwrap()));
        assert!(origin_allowed("http://localhost"));
        assert!(origin_allowed("http://127.0.0.1:8342"));
        assert!(origin_allowed("http://[::1]:8342/x"));
        assert!(!origin_allowed("http://localhost.evil.example"));
        assert!(!origin_allowed("null"));
        assert!(!origin_allowed("file://"));
        assert!(authorized(Some(&format!("bearer {TOKEN}")), TOKEN));
        assert!(!authorized(Some(&format!("Basic {TOKEN}")), TOKEN));
        assert!(!authorized(Some("Bearer"), TOKEN));
        assert!(!authorized(None, TOKEN));
    }

    #[tokio::test]
    async fn notifications_get_202_and_unknown_methods_an_error() {
        let (url, _stop) = spawn(Db::open_in_memory().unwrap(), TOKEN).await;
        let (status, body) = rpc(
            &url,
            Some(TOKEN),
            &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        )
        .await;
        assert_eq!(status, 202);
        assert!(body.is_null());
        let (status, body) = rpc(
            &url,
            Some(TOKEN),
            &json!({ "jsonrpc": "2.0", "id": 7, "method": "resources/list" }),
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(body["id"], 7);
        assert_eq!(body["error"]["code"], -32601);
        let (status, body) = rpc(
            &url,
            Some(TOKEN),
            &json!({ "jsonrpc": "2.0", "id": 8, "method": "ping" }),
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(body["result"], json!({}));
    }

    #[tokio::test]
    async fn malformed_json_is_a_parse_error_not_a_crash() {
        let (url, _stop) = spawn(Db::open_in_memory().unwrap(), TOKEN).await;
        let client = reqwest::Client::new();
        let resp = client
            .post(&url)
            .header("Authorization", format!("Bearer {TOKEN}"))
            .header("Content-Type", "application/json")
            .body("{not json")
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status().as_u16(), 400);
        let body: Value = resp.json().await.unwrap();
        assert_eq!(body["error"]["code"], -32700);
        // The server is still up afterwards.
        let (status, _) = rpc(&url, Some(TOKEN), &init()).await;
        assert_eq!(status, 200);
    }

    #[tokio::test]
    async fn tools_list_names_every_tool_and_no_send() {
        let (url, _stop) = spawn(Db::open_in_memory().unwrap(), TOKEN).await;
        let (_, body) = rpc(
            &url,
            Some(TOKEN),
            &json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
        )
        .await;
        let names: Vec<&str> = body["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        for want in [
            "search_mail",
            "get_thread",
            "list_unread",
            "list_court",
            "get_calendar",
            "find_free_slots",
            "crm_lookup",
            "create_draft",
            "archive",
            "star",
            "mark_read",
            "create_event",
        ] {
            assert!(names.contains(&want), "missing {want}: {names:?}");
        }
        for banned in ["send", "send_mail", "send_draft", "invite"] {
            assert!(!names.iter().any(|n| n.contains(banned)), "{names:?}");
        }
        for t in body["result"]["tools"].as_array().unwrap() {
            assert_eq!(t["inputSchema"]["type"], "object", "{t}");
            assert!(t["description"].as_str().is_some_and(|d| !d.is_empty()));
        }
    }

    #[test]
    fn initialize_echoes_a_supported_version_and_downgrades_an_unknown_one() {
        let r = initialize_result(&json!({ "protocolVersion": "2025-03-26" }));
        assert_eq!(r["protocolVersion"], "2025-03-26");
        let r = initialize_result(&json!({ "protocolVersion": "2031-01-01" }));
        assert_eq!(r["protocolVersion"], PROTOCOL_VERSION);
        let r = initialize_result(&json!({}));
        assert_eq!(r["protocolVersion"], PROTOCOL_VERSION);
    }

    #[test]
    fn add_command_has_the_endpoint_and_the_header() {
        let line = add_command("abc");
        assert_eq!(
            line,
            "claude mcp add --transport http skim http://127.0.0.1:8342/mcp \
             --header \"Authorization: Bearer abc\" --scope user"
        );
        let t = random_token();
        assert_eq!(t.len(), 64);
        assert!(t.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(t, random_token());
    }

    #[test]
    fn enabled_defaults_on_and_only_off_switches_it() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            assert!(is_enabled(conn)?);
            queries::set_setting(conn, SETTING_KEY, "off")?;
            assert!(!is_enabled(conn)?);
            queries::set_setting(conn, SETTING_KEY, "on")?;
            assert!(is_enabled(conn)?);
            Ok(())
        })
        .unwrap();
    }
}
