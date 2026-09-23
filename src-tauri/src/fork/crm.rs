//! Phase 9: the Rebound CRM sidebar. Read-only: Skim never writes to the CRM.
//!
//! Three pieces, all in this file:
//! - config: base URL, Supabase URL and anon key from the `fork_crm_*` settings,
//!   falling back to what the build baked in (`SKIM_REBOUND_*` via
//!   `option_env!`), so a build with nothing set still answers
//!   `crm_not_configured` instead of failing.
//! - session: a Supabase password login whose refresh token lives at
//!   `fork:rebound` in Credential Manager; the access token is cached in memory
//!   and renewed from the refresh token when it is about to expire.
//! - lookup: `POST {base}/api/v1/extension/lookup` with the user JWT and the
//!   chosen workspace, cached ten minutes per lowercased address.
//!
//! Every command returns a typed status or a coded error; nothing here panics
//! when the CRM is unreachable or unconfigured.

use crate::db::queries;
use crate::error::{Result, SkimError};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::State;

/// Credential Manager key holding the Supabase refresh token.
pub const REFRESH_KEY: &str = "fork:rebound";

/// Settings keys (listed under `## settings ALLOWED` in the pending file).
pub const BASE_URL_KEY: &str = "fork_crm_base_url";
pub const SUPABASE_URL_KEY: &str = "fork_crm_supabase_url";
pub const ANON_KEY_KEY: &str = "fork_crm_anon_key";
pub const WORKSPACE_KEY: &str = "fork_crm_workspace_id";

/// How long one address's lookup is served from memory.
pub const LOOKUP_TTL: Duration = Duration::from_secs(600);
/// An access token is renewed this long before Supabase says it expires.
const TOKEN_MARGIN: Duration = Duration::from_secs(30);
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

/// The hosted instance, for links when no base URL is configured at all.
pub const DEFAULT_BASE_URL: &str = "https://rebound.patricknesbitt.ai";

fn http() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .build()
            .unwrap_or_default()
    })
}

// ---- config ----------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub base_url: String,
    pub supabase_url: String,
    pub anon_key: String,
}

/// What the build baked in, if anything. Read at compile time, so a build with
/// no `SKIM_REBOUND_*` in its environment simply has no defaults.
fn baked(key: &str) -> Option<&'static str> {
    match key {
        BASE_URL_KEY => option_env!("SKIM_REBOUND_BASE_URL"),
        SUPABASE_URL_KEY => option_env!("SKIM_REBOUND_SUPABASE_URL"),
        ANON_KEY_KEY => option_env!("SKIM_REBOUND_SUPABASE_ANON_KEY"),
        _ => None,
    }
}

fn clean(value: Option<String>) -> Option<String> {
    value
        .map(|s| s.trim().trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty())
}

/// One value: the stored setting when it is non-empty, else the baked default.
/// An empty stored value means "use the default", so clearing a field in
/// Settings puts the build-time value back.
pub fn resolve_value(stored: Option<String>, default: Option<&str>) -> Option<String> {
    clean(stored).or_else(|| clean(default.map(str::to_string)))
}

/// The config from a settings reader, or `None` when any of the three is
/// missing everywhere. Pure over `stored` so the fallback order is testable.
pub fn resolve_config(stored: impl Fn(&str) -> Option<String>) -> Option<Config> {
    resolve_config_with(stored, baked)
}

fn resolve_config_with(
    stored: impl Fn(&str) -> Option<String>,
    default: impl Fn(&str) -> Option<&'static str>,
) -> Option<Config> {
    let get = |key: &str| resolve_value(stored(key), default(key));
    Some(Config {
        base_url: get(BASE_URL_KEY)?,
        supabase_url: get(SUPABASE_URL_KEY)?,
        anon_key: get(ANON_KEY_KEY)?,
    })
}

/// Read the three settings plus the workspace id in one trip.
async fn load(state: &AppState) -> Result<(Option<Config>, Option<String>)> {
    state
        .db
        .read("fork_crm_config", |conn| {
            let mut stored = HashMap::new();
            for key in [BASE_URL_KEY, SUPABASE_URL_KEY, ANON_KEY_KEY, WORKSPACE_KEY] {
                if let Some(v) = queries::get_setting(conn, key)? {
                    stored.insert(key.to_string(), v);
                }
            }
            let cfg = resolve_config(|k| stored.get(k).cloned());
            let ws = stored
                .get(WORKSPACE_KEY)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            Ok((cfg, ws))
        })
        .await
}

fn not_configured() -> SkimError {
    SkimError::other("crm_not_configured", "Rebound is not configured")
}

fn not_connected() -> SkimError {
    SkimError::other("crm_not_connected", "Not signed in to Rebound")
}

fn no_workspace() -> SkimError {
    SkimError::other("crm_no_workspace", "No Rebound workspace chosen")
}

fn http_err(what: &str, e: impl std::fmt::Display) -> SkimError {
    tracing::warn!(error = %e, what, "rebound request failed");
    SkimError::other("crm_http", format!("{what}: {e}"))
}

// ---- session ---------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
struct TokenUser {
    id: Option<String>,
    email: Option<String>,
}

/// Supabase `auth/v1/token` response, the fields we use.
#[derive(Debug, Clone, Deserialize)]
pub struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    user: Option<TokenUser>,
}

#[derive(Debug, Clone)]
struct Session {
    access_token: String,
    user_id: Option<String>,
    email: Option<String>,
    expires_at: Instant,
}

impl Session {
    fn from_token(t: &TokenResponse) -> Self {
        let ttl = Duration::from_secs(t.expires_in.unwrap_or(3600));
        Self {
            access_token: t.access_token.clone(),
            user_id: t.user.as_ref().and_then(|u| u.id.clone()),
            email: t.user.as_ref().and_then(|u| u.email.clone()),
            expires_at: Instant::now() + ttl,
        }
    }

    fn usable(&self) -> bool {
        self.expires_at.saturating_duration_since(Instant::now()) > TOKEN_MARGIN
    }
}

fn session_cache() -> &'static Mutex<Option<Session>> {
    static CACHE: OnceLock<Mutex<Option<Session>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

fn cached_session() -> Option<Session> {
    session_cache().lock().ok()?.clone()
}

fn set_session(s: Option<Session>) {
    if let Ok(mut g) = session_cache().lock() {
        *g = s;
    }
}

/// Supabase's error body: `{error, error_description}` on the auth endpoints,
/// `{message}` on PostgREST, `{error:{code,message}}` on Rebound's own API.
#[derive(Debug, Default, Deserialize)]
struct ErrorBody {
    error: Option<serde_json::Value>,
    error_description: Option<String>,
    message: Option<String>,
    msg: Option<String>,
}

fn error_text(status: reqwest::StatusCode, body: &str) -> String {
    let parsed: ErrorBody = serde_json::from_str(body).unwrap_or_default();
    let detail = parsed
        .error_description
        .or(parsed.message)
        .or(parsed.msg)
        .or_else(|| match parsed.error {
            Some(serde_json::Value::String(s)) => Some(s),
            Some(serde_json::Value::Object(o)) => o
                .get("message")
                .and_then(|m| m.as_str())
                .map(str::to_string),
            _ => None,
        })
        .unwrap_or_else(|| body.chars().take(200).collect());
    format!("{} {}", status.as_u16(), detail.trim())
}

async fn token_request(
    cfg: &Config,
    grant: &str,
    body: serde_json::Value,
) -> Result<TokenResponse> {
    let url = format!("{}/auth/v1/token?grant_type={grant}", cfg.supabase_url);
    let resp = http()
        .post(&url)
        .header("apikey", &cfg.anon_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| http_err("sign-in", e))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| http_err("sign-in", e))?;
    if !status.is_success() {
        let code = if status.as_u16() == 400 || status.as_u16() == 401 {
            "crm_auth"
        } else {
            "crm_http"
        };
        return Err(SkimError::other(code, error_text(status, &text)));
    }
    serde_json::from_str(&text).map_err(|e| http_err("sign-in response", e))
}

async fn password_login(cfg: &Config, email: &str, password: &str) -> Result<TokenResponse> {
    token_request(
        cfg,
        "password",
        serde_json::json!({ "email": email, "password": password }),
    )
    .await
}

async fn refresh_login(cfg: &Config, refresh_token: &str) -> Result<TokenResponse> {
    token_request(
        cfg,
        "refresh_token",
        serde_json::json!({ "refresh_token": refresh_token }),
    )
    .await
}

/// Persist what a token response gave us: the rotated refresh token (Supabase
/// issues a new one on every refresh) and the in-memory session.
fn adopt(t: &TokenResponse) -> Result<Session> {
    if let Some(rt) = t.refresh_token.as_deref().filter(|s| !s.is_empty()) {
        crate::secrets::set(REFRESH_KEY, rt)?;
    }
    let s = Session::from_token(t);
    set_session(Some(s.clone()));
    Ok(s)
}

/// A usable access token: the cached one, else a refresh from the stored
/// refresh token. `crm_not_connected` when there is nothing to refresh with.
async fn session(cfg: &Config) -> Result<Session> {
    if let Some(s) = cached_session().filter(Session::usable) {
        return Ok(s);
    }
    let Some(rt) = crate::secrets::get(REFRESH_KEY)? else {
        return Err(not_connected());
    };
    match refresh_login(cfg, &rt).await {
        Ok(t) => adopt(&t),
        Err(e) if e.code() == "crm_auth" => {
            // The refresh token is dead (revoked or rotated elsewhere): the
            // only way back is a fresh sign-in, so say so, and drop what we
            // hold so status stops claiming a connection that cannot work.
            let _ = crate::secrets::delete(REFRESH_KEY);
            set_session(None);
            Err(not_connected())
        }
        Err(e) => Err(e),
    }
}

// ---- workspaces ------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub name: Option<String>,
    pub slug: Option<String>,
    pub role: Option<String>,
}

/// Shape of `GET /api/v1/workspaces`: `{data: [{id, name, slug, created_at, role}]}`.
#[derive(Debug, Deserialize)]
struct RouteWorkspaces {
    data: Vec<Workspace>,
}

pub fn parse_route_workspaces(body: &str) -> Option<Vec<Workspace>> {
    serde_json::from_str::<RouteWorkspaces>(body)
        .ok()
        .map(|r| r.data)
}

/// Shape of the same query through PostgREST:
/// `[{role, workspace: {id, name, slug, created_at}}]`.
#[derive(Debug, Deserialize)]
struct MembershipRow {
    role: Option<String>,
    workspace: Option<MembershipWorkspace>,
}

#[derive(Debug, Deserialize)]
struct MembershipWorkspace {
    id: String,
    name: Option<String>,
    slug: Option<String>,
}

pub fn parse_membership_workspaces(body: &str) -> Option<Vec<Workspace>> {
    let rows: Vec<MembershipRow> = serde_json::from_str(body).ok()?;
    Some(
        rows.into_iter()
            .filter_map(|r| {
                let w = r.workspace?;
                Some(Workspace {
                    id: w.id,
                    name: w.name,
                    slug: w.slug,
                    role: r.role,
                })
            })
            .collect(),
    )
}

/// The workspaces this user belongs to. Rebound's `GET /api/v1/workspaces`
/// reads its session from cookies only (`workspaces/route.ts` never looks at
/// the Authorization header), so it is tried first in case that changes and
/// the same membership query is then run straight against PostgREST, which
/// does honour the JWT and RLS.
async fn list_workspaces(cfg: &Config, s: &Session) -> Result<Vec<Workspace>> {
    let route = http()
        .get(format!("{}/api/v1/workspaces", cfg.base_url))
        .bearer_auth(&s.access_token)
        .header("apikey", &cfg.anon_key)
        .send()
        .await;
    if let Ok(resp) = route {
        if resp.status().is_success() {
            if let Ok(text) = resp.text().await {
                if let Some(ws) = parse_route_workspaces(&text) {
                    return Ok(ws);
                }
            }
        }
    }
    let mut req = http()
        .get(format!(
            "{}/rest/v1/workspace_memberships",
            cfg.supabase_url
        ))
        .query(&[("select", "role,workspace:workspaces(id,name,slug)")])
        .header("apikey", &cfg.anon_key)
        .bearer_auth(&s.access_token);
    if let Some(uid) = &s.user_id {
        req = req.query(&[("user_id", format!("eq.{uid}"))]);
    }
    let resp = req.send().await.map_err(|e| http_err("workspaces", e))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| http_err("workspaces", e))?;
    if !status.is_success() {
        return Err(SkimError::other("crm_http", error_text(status, &text)));
    }
    parse_membership_workspaces(&text)
        .ok_or_else(|| http_err("workspaces", "unexpected response shape"))
}

// ---- lookup response (mirrors extension/lookup/route.ts) -------------------
//
// The route returns `select('*')` rows for `people` / `companies` /
// `activities` and `{id, name, value, status, stage_id}` for deals. Fields are
// the DB columns (snake_case on the wire) and go to the UI as camelCase; every
// one is optional so a column added or dropped in Rebound never breaks the
// drawer. Unknown fields are ignored.

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all(serialize = "camelCase"))]
pub struct Person {
    pub id: Option<String>,
    pub workspace_id: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub full_name: Option<String>,
    pub primary_email: Option<String>,
    pub primary_phone: Option<String>,
    pub linkedin_url: Option<String>,
    pub twitter_handle: Option<String>,
    pub avatar_url: Option<String>,
    pub bio: Option<String>,
    pub city: Option<String>,
    pub state: Option<String>,
    pub country: Option<String>,
    pub timezone: Option<String>,
    pub tags: Option<Vec<String>>,
    pub owner_user_id: Option<String>,
    pub custom_fields: Option<serde_json::Value>,
    pub external_ids: Option<serde_json::Value>,
    pub merged_into_id: Option<String>,
    pub last_activity_at: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all(serialize = "camelCase"))]
pub struct Company {
    pub id: Option<String>,
    pub workspace_id: Option<String>,
    pub name: Option<String>,
    pub domain: Option<String>,
    pub website: Option<String>,
    pub industry: Option<String>,
    pub description: Option<String>,
    pub employee_count_range: Option<String>,
    pub linkedin_url: Option<String>,
    pub logo_url: Option<String>,
    pub address_line1: Option<String>,
    pub address_line2: Option<String>,
    pub city: Option<String>,
    pub state: Option<String>,
    pub postal_code: Option<String>,
    pub country: Option<String>,
    pub tags: Option<Vec<String>>,
    pub owner_user_id: Option<String>,
    pub custom_fields: Option<serde_json::Value>,
    pub external_ids: Option<serde_json::Value>,
    pub merged_into_id: Option<String>,
    pub last_activity_at: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all(serialize = "camelCase"))]
pub struct Deal {
    pub id: Option<String>,
    pub name: Option<String>,
    pub value: Option<f64>,
    pub status: Option<String>,
    pub stage_id: Option<String>,
    /// Not in the route's response: resolved from `pipeline_stages` after the
    /// lookup, best effort, so the drawer can show a stage name and not an id.
    pub stage_name: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all(serialize = "camelCase"))]
pub struct Activity {
    pub id: Option<String>,
    pub workspace_id: Option<String>,
    pub entity_type: Option<String>,
    pub entity_id: Option<String>,
    pub activity_type: Option<String>,
    pub actor_type: Option<String>,
    pub actor_user_id: Option<String>,
    pub occurred_at: Option<String>,
    pub created_at: Option<String>,
    pub payload: Option<serde_json::Value>,
    pub related_entities: Option<serde_json::Value>,
    pub external_source: Option<String>,
    pub external_id: Option<String>,
    pub pending_reply_id: Option<String>,
}

/// Rebound's `reminders` row. The lookup route does not return reminders
/// today; the field is here so the drawer shows them the day it does.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all(serialize = "camelCase"))]
pub struct Reminder {
    pub id: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub remind_at: Option<String>,
    pub snoozed_until: Option<String>,
    pub status: Option<String>,
    pub entity_type: Option<String>,
    pub entity_id: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all(serialize = "camelCase"))]
pub struct Lookup {
    pub person: Option<Person>,
    pub company: Option<Company>,
    pub deals: Vec<Deal>,
    pub activities: Vec<Activity>,
    pub reminders: Option<Vec<Reminder>>,
}

impl Lookup {
    pub fn found(&self) -> bool {
        self.person.is_some() || self.company.is_some()
    }
}

/// `success()` in Rebound wraps the payload as `{data: ...}`; accept a bare
/// object too so a shape change on that helper does not blank the drawer.
pub fn parse_lookup(body: &str) -> Option<Lookup> {
    #[derive(Deserialize)]
    struct Envelope {
        data: Option<Lookup>,
    }
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    if !value.is_object() {
        return None;
    }
    if let Some(data) = value.get("data") {
        if data.is_object() {
            return serde_json::from_value::<Envelope>(value).ok()?.data;
        }
        return None;
    }
    serde_json::from_value(value).ok()
}

// ---- lookup cache ----------------------------------------------------------

/// Ten-minute memory of lookups by lowercased address. Clock is injected so
/// expiry is testable without sleeping.
pub struct LookupCache {
    ttl: Duration,
    entries: HashMap<String, (Instant, Lookup)>,
}

impl LookupCache {
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            entries: HashMap::new(),
        }
    }

    pub fn key(email: &str) -> String {
        email.trim().to_lowercase()
    }

    pub fn get_at(&mut self, email: &str, now: Instant) -> Option<Lookup> {
        let key = Self::key(email);
        match self.entries.get(&key) {
            Some((at, v)) if now.saturating_duration_since(*at) < self.ttl => Some(v.clone()),
            Some(_) => {
                self.entries.remove(&key);
                None
            }
            None => None,
        }
    }

    pub fn put_at(&mut self, email: &str, value: Lookup, now: Instant) {
        self.entries.insert(Self::key(email), (now, value));
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

fn lookup_cache() -> &'static Mutex<LookupCache> {
    static CACHE: OnceLock<Mutex<LookupCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(LookupCache::new(LOOKUP_TTL)))
}

fn cache_get(email: &str) -> Option<Lookup> {
    lookup_cache().lock().ok()?.get_at(email, Instant::now())
}

fn cache_put(email: &str, value: Lookup) {
    if let Ok(mut c) = lookup_cache().lock() {
        c.put_at(email, value, Instant::now());
    }
}

fn cache_clear() {
    if let Ok(mut c) = lookup_cache().lock() {
        c.clear();
    }
}

// ---- lookup ----------------------------------------------------------------

async fn lookup_once(
    cfg: &Config,
    s: &Session,
    workspace: &str,
    email: &str,
) -> Result<reqwest::Response> {
    http()
        .post(format!("{}/api/v1/extension/lookup", cfg.base_url))
        .bearer_auth(&s.access_token)
        .header("X-Workspace-ID", workspace)
        .json(&serde_json::json!({ "email": email }))
        .send()
        .await
        .map_err(|e| http_err("lookup", e))
}

/// Stage names for the deals, from `pipeline_stages`. Best effort: a failure
/// leaves `stage_name` empty and the drawer shows the deal's status instead.
async fn fill_stage_names(cfg: &Config, s: &Session, deals: &mut [Deal]) {
    let mut ids: Vec<&str> = deals.iter().filter_map(|d| d.stage_id.as_deref()).collect();
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        return;
    }
    #[derive(Deserialize)]
    struct Stage {
        id: String,
        name: Option<String>,
    }
    let filter = format!("in.({})", ids.join(","));
    let resp = http()
        .get(format!("{}/rest/v1/pipeline_stages", cfg.supabase_url))
        .query(&[("select", "id,name"), ("id", filter.as_str())])
        .header("apikey", &cfg.anon_key)
        .bearer_auth(&s.access_token)
        .send()
        .await;
    let stages: Vec<Stage> = match resp {
        Ok(r) if r.status().is_success() => r.json().await.unwrap_or_default(),
        Ok(r) => {
            tracing::debug!(status = %r.status(), "pipeline_stages not readable");
            return;
        }
        Err(e) => {
            tracing::debug!(error = %e, "pipeline_stages request failed");
            return;
        }
    };
    let names: HashMap<String, Option<String>> =
        stages.into_iter().map(|s| (s.id, s.name)).collect();
    for d in deals.iter_mut() {
        if let Some(id) = &d.stage_id {
            if let Some(Some(name)) = names.get(id) {
                d.stage_name = Some(name.clone());
            }
        }
    }
}

async fn lookup(cfg: &Config, workspace: &str, email: &str) -> Result<Lookup> {
    let email = email.trim();
    if email.is_empty() {
        return Ok(Lookup::default());
    }
    if let Some(hit) = cache_get(email) {
        return Ok(hit);
    }
    let mut s = session(cfg).await?;
    let mut resp = lookup_once(cfg, &s, workspace, email).await?;
    if resp.status().as_u16() == 401 {
        // The cached token was rejected: renew it once and retry.
        set_session(None);
        s = session(cfg).await?;
        resp = lookup_once(cfg, &s, workspace, email).await?;
    }
    let status = resp.status();
    let text = resp.text().await.map_err(|e| http_err("lookup", e))?;
    if !status.is_success() {
        let code = match status.as_u16() {
            401 => "crm_not_connected",
            403 => "crm_no_workspace",
            _ => "crm_http",
        };
        return Err(SkimError::other(code, error_text(status, &text)));
    }
    let mut found =
        parse_lookup(&text).ok_or_else(|| http_err("lookup", "unexpected response shape"))?;
    if !found.deals.is_empty() {
        fill_stage_names(cfg, &s, &mut found.deals).await;
    }
    cache_put(email, found.clone());
    Ok(found)
}

// ---- status + commands -----------------------------------------------------

/// What Settings → CRM and the drawer render. The anon key is masked; the
/// refresh token never leaves Rust.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrmStatus {
    pub configured: bool,
    pub connected: bool,
    pub workspace_id: Option<String>,
    /// Present when connected and no workspace is chosen (the picker), or
    /// right after a sign-in.
    pub workspaces: Option<Vec<Workspace>>,
    pub base_url: Option<String>,
    pub supabase_url: Option<String>,
    pub anon_key_masked: Option<String>,
    /// The signed-in address, when the session knows it.
    pub email: Option<String>,
    /// Why `workspaces` could not be listed, if that was tried and failed.
    pub error: Option<String>,
}

async fn status_for(state: &AppState, list: bool) -> Result<CrmStatus> {
    let (cfg, workspace_id) = load(state).await?;
    let Some(cfg) = cfg else {
        return Ok(CrmStatus::default());
    };
    let connected = crate::secrets::get(REFRESH_KEY)?.is_some();
    let mut st = CrmStatus {
        configured: true,
        connected,
        workspace_id: workspace_id.clone(),
        workspaces: None,
        base_url: Some(cfg.base_url.clone()),
        supabase_url: Some(cfg.supabase_url.clone()),
        anon_key_masked: Some(crate::fork::google::mask(&cfg.anon_key)),
        email: cached_session().and_then(|s| s.email),
        error: None,
    };
    if connected && (list || workspace_id.is_none()) {
        match session(&cfg).await {
            Ok(s) => {
                st.email = s.email.clone();
                match list_workspaces(&cfg, &s).await {
                    Ok(ws) => st.workspaces = Some(ws),
                    Err(e) => st.error = Some(e.to_string()),
                }
            }
            Err(e) if e.code() == "crm_not_connected" => st.connected = false,
            Err(e) => st.error = Some(e.to_string()),
        }
    }
    Ok(st)
}

async fn save_workspace(state: &AppState, id: Option<String>) -> Result<()> {
    let value = id.unwrap_or_default();
    state
        .db
        .call(move |conn| queries::set_setting(conn, WORKSPACE_KEY, &value))
        .await
}

#[tauri::command]
pub async fn fork_crm_status(state: State<'_, AppState>) -> Result<CrmStatus> {
    status_for(&state, false).await
}

/// Sign in with the Rebound email + password. The password is used for one
/// token request and dropped; only the refresh token is kept. When the user
/// belongs to exactly one workspace it is chosen at once.
#[tauri::command]
pub async fn fork_crm_connect(
    state: State<'_, AppState>,
    email: String,
    password: String,
) -> Result<CrmStatus> {
    let (cfg, _) = load(&state).await?;
    let cfg = cfg.ok_or_else(not_configured)?;
    let email = email.trim().to_string();
    if email.is_empty() || password.is_empty() {
        return Err(SkimError::other(
            "crm_auth",
            "Email and password are required",
        ));
    }
    let token = password_login(&cfg, &email, &password).await?;
    if token.refresh_token.as_deref().unwrap_or("").is_empty() {
        return Err(SkimError::other(
            "crm_auth",
            "Sign-in returned no refresh token",
        ));
    }
    let s = adopt(&token)?;
    cache_clear();
    let mut st = status_for(&state, true).await?;
    st.email = s.email.or(Some(email));
    if st.workspace_id.is_none() {
        if let Some(ws) = st.workspaces.as_ref() {
            if ws.len() == 1 {
                let only = ws[0].id.clone();
                save_workspace(&state, Some(only.clone())).await?;
                st.workspace_id = Some(only);
            }
        }
    }
    Ok(st)
}

#[tauri::command]
pub async fn fork_crm_pick_workspace(state: State<'_, AppState>, id: String) -> Result<CrmStatus> {
    let id = id.trim().to_string();
    if id.is_empty() {
        return Err(no_workspace());
    }
    save_workspace(&state, Some(id)).await?;
    cache_clear();
    status_for(&state, false).await
}

/// Forget the session: the refresh token, the cached access token, every
/// cached lookup and the chosen workspace. The URLs and key stay.
#[tauri::command]
pub async fn fork_crm_disconnect(state: State<'_, AppState>) -> Result<CrmStatus> {
    crate::secrets::delete(REFRESH_KEY)?;
    set_session(None);
    cache_clear();
    save_workspace(&state, None).await?;
    status_for(&state, false).await
}

/// Store the three connection values. An empty string clears a stored value
/// back to the build-time default; `None` leaves that field as it is (the
/// Settings form sends `None` for an anon key it only ever showed masked).
#[tauri::command]
pub async fn fork_crm_set_config(
    state: State<'_, AppState>,
    base_url: Option<String>,
    supabase_url: Option<String>,
    anon_key: Option<String>,
) -> Result<CrmStatus> {
    state
        .db
        .call(move |conn| {
            for (key, value) in [
                (BASE_URL_KEY, base_url),
                (SUPABASE_URL_KEY, supabase_url),
                (ANON_KEY_KEY, anon_key),
            ] {
                if let Some(v) = value {
                    queries::set_setting(conn, key, v.trim().trim_end_matches('/'))?;
                }
            }
            Ok(())
        })
        .await?;
    // A different server means a different session and different people.
    set_session(None);
    cache_clear();
    status_for(&state, false).await
}

#[tauri::command]
pub async fn fork_crm_lookup(state: State<'_, AppState>, email: String) -> Result<Lookup> {
    let (cfg, workspace) = load(&state).await?;
    let cfg = cfg.ok_or_else(not_configured)?;
    let workspace = workspace.ok_or_else(|| {
        if crate::secrets::get(REFRESH_KEY).ok().flatten().is_none() {
            not_connected()
        } else {
            no_workspace()
        }
    })?;
    lookup(&cfg, &workspace, &email).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stored<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| {
            pairs
                .iter()
                .find(|(key, _)| *key == k)
                .map(|(_, v)| v.to_string())
        }
    }

    fn defaults(k: &str) -> Option<&'static str> {
        match k {
            BASE_URL_KEY => Some("https://rebound.example/"),
            SUPABASE_URL_KEY => Some("https://abc.supabase.co"),
            ANON_KEY_KEY => Some("anon-default"),
            _ => None,
        }
    }

    #[test]
    fn config_prefers_settings_over_baked_defaults() {
        let cfg = resolve_config_with(
            stored(&[
                (BASE_URL_KEY, "https://rebound.local/ "),
                (ANON_KEY_KEY, "anon-set"),
            ]),
            defaults,
        )
        .expect("configured");
        assert_eq!(cfg.base_url, "https://rebound.local");
        assert_eq!(cfg.supabase_url, "https://abc.supabase.co");
        assert_eq!(cfg.anon_key, "anon-set");
    }

    #[test]
    fn an_empty_setting_falls_back_to_the_default() {
        let cfg =
            resolve_config_with(stored(&[(BASE_URL_KEY, "  ")]), defaults).expect("configured");
        assert_eq!(cfg.base_url, "https://rebound.example");
    }

    #[test]
    fn nothing_anywhere_is_not_configured() {
        assert_eq!(resolve_config_with(stored(&[]), |_| None), None);
        // One value missing everywhere is also not configured.
        assert_eq!(
            resolve_config_with(
                stored(&[(BASE_URL_KEY, "https://a"), (SUPABASE_URL_KEY, "https://b")]),
                |_| None
            ),
            None
        );
    }

    /// The exact shape `extension/lookup/route.ts` returns through `success()`:
    /// `select('*')` rows for people / companies / activities and the five
    /// deal columns, all under `data`.
    const FOUND: &str = r#"{"data":{
      "person":{"id":"p1","workspace_id":"w1","first_name":"Anna","last_name":"Weber",
        "full_name":"Anna Weber","primary_email":"anna.weber@northwind.example",
        "primary_phone":null,"linkedin_url":"https://linkedin.com/in/annaweber",
        "twitter_handle":null,"avatar_url":null,"bio":null,"city":"Berlin","state":null,
        "country":"DE","timezone":"Europe/Berlin","tags":["ops"],"owner_user_id":"u1",
        "custom_fields":{"seniority":"VP"},"external_ids":null,"merged_into_id":null,
        "last_activity_at":"2026-09-22T09:00:00Z","created_at":"2026-01-01T00:00:00Z",
        "updated_at":"2026-09-22T09:00:00Z","deleted_at":null,"created_by":"u1",
        "updated_by":null,"deleted_by":null},
      "company":{"id":"c1","workspace_id":"w1","name":"Northwind","domain":"northwind.example",
        "website":null,"industry":"Logistics","description":null,"employee_count_range":"51-200",
        "linkedin_url":null,"logo_url":null,"address_line1":null,"address_line2":null,
        "city":"Berlin","state":null,"postal_code":null,"country":"DE","tags":null,
        "owner_user_id":null,"custom_fields":null,"external_ids":null,"merged_into_id":null,
        "last_activity_at":null,"created_at":null,"updated_at":null,"deleted_at":null,
        "created_by":null,"updated_by":null,"deleted_by":null},
      "deals":[{"id":"d1","name":"Q3 rollout","value":45000,"status":"open","stage_id":"s2"},
               {"id":"d2","name":"Renewal","value":null,"status":"won","stage_id":"s9"}],
      "activities":[{"id":"a1","workspace_id":"w1","entity_type":"person","entity_id":"p1",
        "activity_type":"email_received","actor_type":"system","actor_user_id":null,
        "occurred_at":"2026-09-22T09:00:00Z","created_at":"2026-09-22T09:00:01Z",
        "payload":{"subject":"Q3 launch"},"related_entities":null,"external_source":"gmail",
        "external_id":"m1","pending_reply_id":null}]
    }}"#;

    #[test]
    fn a_found_lookup_deserialises_every_field() {
        let l = parse_lookup(FOUND).expect("parses");
        assert!(l.found());
        let p = l.person.as_ref().unwrap();
        assert_eq!(p.full_name.as_deref(), Some("Anna Weber"));
        assert_eq!(p.tags.as_deref(), Some(&["ops".to_string()][..]));
        assert_eq!(p.custom_fields.as_ref().unwrap()["seniority"], "VP");
        let c = l.company.as_ref().unwrap();
        assert_eq!(c.domain.as_deref(), Some("northwind.example"));
        assert_eq!(l.deals.len(), 2);
        assert_eq!(l.deals[0].value, Some(45000.0));
        assert_eq!(l.deals[1].value, None);
        assert_eq!(l.deals[0].stage_name, None, "not in the route's response");
        assert_eq!(l.activities.len(), 1);
        assert_eq!(
            l.activities[0].activity_type.as_deref(),
            Some("email_received")
        );
        assert_eq!(
            l.activities[0].payload.as_ref().unwrap()["subject"],
            "Q3 launch"
        );
        assert_eq!(l.reminders, None);
    }

    #[test]
    fn the_ui_sees_camel_case() {
        let l = parse_lookup(FOUND).unwrap();
        let out = serde_json::to_value(&l).unwrap();
        assert_eq!(out["person"]["fullName"], "Anna Weber");
        assert_eq!(
            out["person"]["primaryEmail"],
            "anna.weber@northwind.example"
        );
        assert_eq!(out["deals"][0]["stageId"], "s2");
        assert!(out["deals"][0]["stageName"].is_null());
        assert_eq!(out["activities"][0]["activityType"], "email_received");
        assert_eq!(out["activities"][0]["occurredAt"], "2026-09-22T09:00:00Z");
    }

    #[test]
    fn a_miss_is_all_nulls_and_empty_lists() {
        let l =
            parse_lookup(r#"{"data":{"person":null,"company":null,"deals":[],"activities":[]}}"#)
                .unwrap();
        assert!(!l.found());
        assert!(l.deals.is_empty());
        assert!(l.activities.is_empty());
    }

    #[test]
    fn missing_fields_never_fail() {
        // A person with only an id, a deal with nothing, no lists at all.
        let l = parse_lookup(r#"{"data":{"person":{"id":"p1"},"deals":[{}]}}"#).unwrap();
        assert_eq!(l.person.unwrap().id.as_deref(), Some("p1"));
        assert_eq!(l.deals, vec![Deal::default()]);
        assert!(l.activities.is_empty());
        // A bare object (no `data` envelope) parses too.
        let bare = parse_lookup(r#"{"person":{"id":"p2"},"company":null}"#).unwrap();
        assert_eq!(bare.person.unwrap().id.as_deref(), Some("p2"));
        // Unknown columns Rebound may add are ignored.
        let extra =
            parse_lookup(r#"{"data":{"person":{"id":"p3","brand_new_column":1}}}"#).unwrap();
        assert_eq!(extra.person.unwrap().id.as_deref(), Some("p3"));
    }

    #[test]
    fn reminders_show_up_when_the_route_sends_them() {
        let l = parse_lookup(
            r#"{"data":{"person":{"id":"p1"},"reminders":[{"id":"r1","title":"Chase","remind_at":"2026-09-25T08:00:00Z","status":"pending"}]}}"#,
        )
        .unwrap();
        let r = l.reminders.unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].title.as_deref(), Some("Chase"));
    }

    #[test]
    fn garbage_is_not_a_lookup() {
        assert!(parse_lookup("not json").is_none());
        assert!(parse_lookup(r#"{"data":"nope"}"#).is_none());
        assert!(parse_lookup(r#"[1,2]"#).is_none());
    }

    #[test]
    fn workspaces_parse_from_both_shapes() {
        let route = r#"{"data":[{"id":"w1","name":"Brightgro","slug":"brightgro","created_at":"2026-01-01","role":"owner"}]}"#;
        let ws = parse_route_workspaces(route).unwrap();
        assert_eq!(ws.len(), 1);
        assert_eq!(ws[0].id, "w1");
        assert_eq!(ws[0].role.as_deref(), Some("owner"));

        let rest = r#"[{"role":"owner","workspace":{"id":"w1","name":"Brightgro","slug":"brightgro"}},{"role":"member","workspace":null}]"#;
        let ws = parse_membership_workspaces(rest).unwrap();
        assert_eq!(ws.len(), 1, "a membership with no workspace row is skipped");
        assert_eq!(ws[0].name.as_deref(), Some("Brightgro"));
        assert!(parse_route_workspaces(rest).is_none());
        assert!(parse_membership_workspaces(route).is_none());
    }

    #[test]
    fn cache_serves_within_ttl_and_expires_after() {
        let mut c = LookupCache::new(Duration::from_secs(600));
        let t0 = Instant::now();
        let found = parse_lookup(FOUND).unwrap();
        c.put_at("Anna.Weber@Northwind.example", found.clone(), t0);
        // Case-insensitive, whitespace-insensitive key.
        assert_eq!(
            c.get_at(
                " anna.weber@northwind.example ",
                t0 + Duration::from_secs(1)
            ),
            Some(found.clone())
        );
        assert_eq!(
            c.get_at(
                "anna.weber@northwind.example",
                t0 + Duration::from_secs(599)
            ),
            Some(found)
        );
        assert_eq!(
            c.get_at(
                "anna.weber@northwind.example",
                t0 + Duration::from_secs(600)
            ),
            None
        );
        assert!(c.is_empty(), "an expired entry is dropped on read");
        assert_eq!(c.get_at("nobody@example.com", t0), None);
    }

    #[test]
    fn cache_clear_forgets_everything() {
        let mut c = LookupCache::new(Duration::from_secs(600));
        let t0 = Instant::now();
        c.put_at("a@x", Lookup::default(), t0);
        c.put_at("b@x", Lookup::default(), t0);
        assert_eq!(c.len(), 2);
        c.clear();
        assert!(c.is_empty());
    }

    #[test]
    fn a_session_is_renewed_before_it_expires() {
        let live = Session {
            access_token: "t".into(),
            user_id: None,
            email: None,
            expires_at: Instant::now() + Duration::from_secs(3600),
        };
        assert!(live.usable());
        let dying = Session {
            expires_at: Instant::now() + Duration::from_secs(10),
            ..live.clone()
        };
        assert!(!dying.usable());
        let dead = Session {
            expires_at: Instant::now() - Duration::from_secs(1),
            ..live
        };
        assert!(!dead.usable());
    }

    #[test]
    fn token_response_carries_the_user() {
        let t: TokenResponse = serde_json::from_str(
            r#"{"access_token":"jwt","token_type":"bearer","expires_in":3600,"refresh_token":"rt","user":{"id":"u1","email":"p@x.com","aud":"authenticated"}}"#,
        )
        .unwrap();
        let s = Session::from_token(&t);
        assert_eq!(s.user_id.as_deref(), Some("u1"));
        assert_eq!(s.email.as_deref(), Some("p@x.com"));
        assert!(s.usable());
    }

    #[test]
    fn error_bodies_read_from_every_backend() {
        let st = reqwest::StatusCode::BAD_REQUEST;
        assert_eq!(
            error_text(
                st,
                r#"{"error":"invalid_grant","error_description":"Invalid login credentials"}"#
            ),
            "400 Invalid login credentials"
        );
        assert_eq!(
            error_text(
                reqwest::StatusCode::FORBIDDEN,
                r#"{"error":{"code":"FORBIDDEN","message":"Not a member of this workspace"}}"#
            ),
            "403 Not a member of this workspace"
        );
        assert_eq!(
            error_text(st, r#"{"message":"permission denied"}"#),
            "400 permission denied"
        );
        assert_eq!(
            error_text(st, "<html>gateway</html>"),
            "400 <html>gateway</html>"
        );
    }
}
