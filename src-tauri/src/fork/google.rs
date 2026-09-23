//! The Google connection the fork adds next to the mailbox: a runtime OAuth
//! client (7.1), a calendar-scoped grant per account (7.2) and the Meet space
//! call (7.6).
//!
//! The mail account's own `auth_kind` is never touched. The calendar grant is
//! a second refresh token at `fork:gcal:{account_id}` in Credential Manager,
//! obtained through upstream's loopback flow with the fork's scopes. Nothing
//! here panics without credentials: every entry point answers
//! `gcal_not_configured` / `gcal_not_connected` instead.

use crate::error::{Result, SkimError};
use crate::mail::oauth::{self, OauthConfig, OauthProvider};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::OnceLock;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;
use tokio::sync::Mutex;

/// Credential Manager keys for the runtime client (D9).
pub const CLIENT_ID_KEY: &str = "fork:google_client_id";
pub const CLIENT_SECRET_KEY: &str = "fork:google_client_secret";

/// What the calendar grant asks for. `openid email` is what lets us check the
/// grant landed on the same Google account as the mailbox.
pub const CALENDAR_SCOPES: &str = "openid email \
    https://www.googleapis.com/auth/calendar.events \
    https://www.googleapis.com/auth/calendar.calendarlist.readonly \
    https://www.googleapis.com/auth/meetings.space.created";

/// The one scope the grant is useless without.
pub const REQUIRED_SCOPE: &str = "https://www.googleapis.com/auth/calendar.events";

const MEET_SPACES_ENDPOINT: &str = "https://meet.googleapis.com/v2/spaces";

/// Where an account's calendar refresh token lives.
pub fn token_key(account_id: &str) -> String {
    format!("fork:gcal:{account_id}")
}

/// One HTTP client for every Google API call the fork makes (calendar, Meet).
/// Same reasoning as `oauth::http`: connection reuse and a hard timeout.
pub fn http() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_default()
    })
}

// ---- 7.1 runtime client config ---------------------------------------------

/// Where the client credentials came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigSource {
    /// Pasted into Settings → Calendar (Credential Manager).
    Stored,
    /// `SKIM_GOOGLE_CLIENT_ID/SECRET` at build time.
    BuiltIn,
}

/// The OAuth client to use for the calendar grant: the stored one when Patrick
/// has pasted it, else whatever the build baked in, else `None`.
pub fn client_config() -> Result<Option<(OauthConfig, ConfigSource)>> {
    let stored_id = crate::secrets::get(CLIENT_ID_KEY)?
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if let Some(client_id) = stored_id {
        let client_secret = crate::secrets::get(CLIENT_SECRET_KEY)?
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        return Ok(Some((
            OauthConfig {
                provider: OauthProvider::Google,
                client_id,
                client_secret,
            },
            ConfigSource::Stored,
        )));
    }
    Ok(oauth::baked_in_config(OauthProvider::Google).map(|c| (c, ConfigSource::BuiltIn)))
}

/// What Settings → Calendar shows. The secret never leaves Rust unmasked.
#[derive(Debug, Clone, Serialize)]
pub struct ClientStatus {
    pub configured: bool,
    pub source: Option<ConfigSource>,
    pub client_id: Option<String>,
    /// `••••` plus the last four characters, or `None` when no secret is set.
    pub secret_masked: Option<String>,
}

/// Mask a secret for display: the tail is enough to tell two apart, and too
/// little to use.
pub fn mask(secret: &str) -> String {
    let tail: String = secret
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    if secret.chars().count() <= 4 {
        "••••".to_string()
    } else {
        format!("••••{tail}")
    }
}

pub fn client_status() -> Result<ClientStatus> {
    Ok(match client_config()? {
        Some((cfg, source)) => ClientStatus {
            configured: true,
            source: Some(source),
            client_id: Some(cfg.client_id),
            secret_masked: if cfg.client_secret.is_empty() {
                None
            } else {
                Some(mask(&cfg.client_secret))
            },
        },
        None => ClientStatus {
            configured: false,
            source: None,
            client_id: None,
            secret_masked: None,
        },
    })
}

#[tauri::command]
pub fn fork_google_client_get() -> Result<ClientStatus> {
    client_status()
}

/// Store the pasted client. An empty secret keeps the previous one, so
/// re-saving the id alone does not wipe it.
#[tauri::command]
pub fn fork_google_client_set(client_id: String, client_secret: String) -> Result<ClientStatus> {
    let id = client_id.trim();
    if id.is_empty() {
        return Err(SkimError::other(
            "gcal_not_configured",
            "the client ID is empty",
        ));
    }
    crate::secrets::set(CLIENT_ID_KEY, id)?;
    let secret = client_secret.trim();
    if !secret.is_empty() {
        crate::secrets::set(CLIENT_SECRET_KEY, secret)?;
    }
    client_status()
}

#[tauri::command]
pub fn fork_google_client_clear() -> Result<ClientStatus> {
    crate::secrets::delete(CLIENT_ID_KEY)?;
    crate::secrets::delete(CLIENT_SECRET_KEY)?;
    client_status()
}

// ---- 7.2 the calendar grant ------------------------------------------------

/// Access-token cache per account, refreshed under one lock so two workers
/// (sync and an op drain) never refresh concurrently.
pub struct GoogleState {
    tokens: Mutex<HashMap<String, (String, i64)>>,
}

impl Default for GoogleState {
    fn default() -> Self {
        Self::new()
    }
}

impl GoogleState {
    pub fn new() -> Self {
        Self {
            tokens: Mutex::new(HashMap::new()),
        }
    }
}

/// The process-wide state. Exposed as a static so this module does not depend
/// on `ForkState`'s shape; the main session may move it onto `ForkState` later.
pub fn state() -> &'static GoogleState {
    static STATE: OnceLock<GoogleState> = OnceLock::new();
    STATE.get_or_init(GoogleState::new)
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Whether this account has a calendar grant stored.
pub fn is_connected(account_id: &str) -> Result<bool> {
    Ok(crate::secrets::get(&token_key(account_id))?.is_some_and(|t| !t.is_empty()))
}

fn not_connected() -> SkimError {
    SkimError::other(
        "gcal_not_connected",
        "Google Calendar is not connected for this account",
    )
}

fn not_configured() -> SkimError {
    SkimError::other(
        "gcal_not_configured",
        "Google client ID is not configured (Settings → Calendar)",
    )
}

/// A valid access token for the account's calendar grant, refreshed when the
/// cached one is within a minute of expiry. Modelled on `sync::resolve_credentials`.
pub async fn access_token(account_id: &str) -> Result<String> {
    let mut cache = state().tokens.lock().await;
    let now = now_unix();
    if let Some((token, expires_at)) = cache.get(account_id) {
        if *expires_at > now {
            return Ok(token.clone());
        }
    }
    let refresh = crate::secrets::get(&token_key(account_id))?
        .filter(|t| !t.is_empty())
        .ok_or_else(not_connected)?;
    let (config, _) = client_config()?.ok_or_else(not_configured)?;
    let refreshed = oauth::refresh_access_token(&config, &refresh).await?;
    if let Some(new_rt) = refreshed.new_refresh_token {
        if new_rt != refresh {
            if let Err(e) = crate::secrets::set(&token_key(account_id), &new_rt) {
                tracing::warn!(error = %e, "cannot store the rotated calendar refresh token");
            }
        }
    }
    cache.insert(
        account_id.to_string(),
        (refreshed.access_token.clone(), refreshed.expires_at),
    );
    Ok(refreshed.access_token)
}

/// Forget a cached access token (after a 401, or on disconnect).
pub async fn forget_token(account_id: &str) {
    state().tokens.lock().await.remove(account_id);
}

/// Two addresses name the same Google account when they match ignoring case
/// and surrounding whitespace.
pub fn same_account(granted: &str, expected: &str) -> bool {
    granted.trim().eq_ignore_ascii_case(expected.trim())
}

/// Run the consent flow for `account_email` and store the grant. Refuses, and
/// stores nothing, when the browser signed in as a different Google account.
pub async fn connect(app: &AppHandle, account_id: &str, account_email: &str) -> Result<()> {
    let (config, _) = client_config()?.ok_or_else(not_configured)?;
    let opener = app.clone();
    let outcome = oauth::authorize(&config, CALENDAR_SCOPES, Some(REQUIRED_SCOPE), move |url| {
        opener
            .opener()
            .open_url(url, None::<&str>)
            .map_err(|e| SkimError::other("oauth", format!("cannot open browser: {e}")))
    })
    .await?;
    if !same_account(&outcome.email, account_email) {
        return Err(SkimError::other(
            "gcal_account_mismatch",
            format!(
                "Google signed in as {}, but this mailbox is {}. Sign out of that Google \
                 account in the browser and connect again.",
                outcome.email, account_email
            ),
        ));
    }
    crate::secrets::set(&token_key(account_id), &outcome.refresh_token)?;
    state().tokens.lock().await.insert(
        account_id.to_string(),
        (outcome.access_token, outcome.expires_at),
    );
    Ok(())
}

/// Drop the grant. The token is not revoked at Google (the mail grant stays
/// untouched and revoking one revokes the client's whole consent).
pub async fn disconnect(account_id: &str) -> Result<()> {
    forget_token(account_id).await;
    crate::secrets::delete(&token_key(account_id))
}

/// Turn a non-2xx Google response into a typed error. 401 drops the cached
/// token and reports `oauth` so an op drain stops instead of burning attempts.
pub async fn api_error(account_id: &str, resp: reqwest::Response) -> SkimError {
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    let detail: String = body.chars().take(400).collect();
    if status.as_u16() == 401 {
        forget_token(account_id).await;
        return SkimError::other("oauth", format!("Google rejected the token: {detail}"));
    }
    if status.as_u16() == 403 && body.contains("insufficient") {
        return SkimError::other(
            "gcal_scope",
            format!("Google refused: {detail}. Disconnect and connect again to grant the calendar scopes."),
        );
    }
    tracing::warn!(status = %status, "google api error");
    SkimError::other("gcal_api", format!("Google answered {status}: {detail}"))
}

// ---- 7.6 Meet ---------------------------------------------------------------

/// Create a Meet space and return its `meetingUri`.
pub async fn meet_create(account_id: &str) -> Result<String> {
    let token = access_token(account_id).await?;
    let resp = http()
        .post(MEET_SPACES_ENDPOINT)
        .bearer_auth(token)
        .json(&serde_json::json!({}))
        .send()
        .await
        .map_err(|e| SkimError::other("network", e.to_string()))?;
    if !resp.status().is_success() {
        return Err(api_error(account_id, resp).await);
    }
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| SkimError::other("gcal_api", e.to_string()))?;
    meeting_uri(&body)
        .ok_or_else(|| SkimError::other("gcal_api", "Meet did not return a meetingUri"))
}

/// The join link out of a `spaces.create` response.
pub fn meeting_uri(body: &serde_json::Value) -> Option<String> {
    body.get("meetingUri")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

#[tauri::command]
pub async fn fork_meet_create(account_id: String) -> Result<String> {
    if !is_connected(&account_id)? {
        return Err(not_connected());
    }
    meet_create(&account_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_keeps_only_the_tail() {
        assert_eq!(mask("GOCSPX-abcdef1234"), "••••1234");
        assert_eq!(mask("abcd"), "••••");
        assert_eq!(mask(""), "••••");
    }

    #[test]
    fn token_key_is_fork_prefixed_per_account() {
        assert_eq!(token_key("acc-1"), "fork:gcal:acc-1");
        assert!(CLIENT_ID_KEY.starts_with("fork:"));
        assert!(CLIENT_SECRET_KEY.starts_with("fork:"));
    }

    #[test]
    fn calendar_scopes_carry_the_required_one_and_not_mail() {
        let scopes: Vec<&str> = CALENDAR_SCOPES.split_whitespace().collect();
        assert!(scopes.contains(&REQUIRED_SCOPE));
        assert!(scopes.contains(&"openid"));
        assert!(scopes.contains(&"email"));
        assert!(scopes.contains(&"https://www.googleapis.com/auth/meetings.space.created"));
        assert!(scopes.contains(&"https://www.googleapis.com/auth/calendar.calendarlist.readonly"));
        assert!(!scopes.iter().any(|s| s.contains("mail.google.com")));
    }

    #[test]
    fn account_match_ignores_case_and_whitespace() {
        assert!(same_account(
            " Patrick@AutoSpark.ai ",
            "patrick@autospark.ai"
        ));
        assert!(!same_account("other@autospark.ai", "patrick@autospark.ai"));
    }

    #[test]
    fn meeting_uri_is_read_from_the_space() {
        let body = serde_json::json!({
            "name": "spaces/abc",
            "meetingUri": "https://meet.google.com/abc-defg-hij",
            "meetingCode": "abc-defg-hij"
        });
        assert_eq!(
            meeting_uri(&body).as_deref(),
            Some("https://meet.google.com/abc-defg-hij")
        );
        assert_eq!(meeting_uri(&serde_json::json!({"name": "spaces/x"})), None);
    }
}
