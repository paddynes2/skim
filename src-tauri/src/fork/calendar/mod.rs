//! Calendar sync engine (Phase 7.4): one task per connected account, modelled
//! on `mail::sync::spawn` (mpsc commands + an interval), pulling a fixed
//! window (-60d/+180d, D8) and draining the `fork_cal_ops` queue the same way
//! `pending_ops` drains: FIFO, a network error stops the drain, five failed
//! attempts mark the op `failed` and tell the UI.
//!
//! Engines register themselves in a process-wide map so the commands can find
//! them without depending on `ForkState`'s shape.

pub mod commands;
pub mod gapi;
pub mod model;
pub mod store;

use crate::db::Db;
use crate::error::{Result, SkimError};
use crate::fork::google;
use model::{event_from_json, is_local_id};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc;

/// The pull window around "now" (D8: windowed pull, not sync tokens).
pub const WINDOW_PAST: i64 = 60 * 86_400;
pub const WINDOW_FUTURE: i64 = 180 * 86_400;
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(300);
/// Attempts before an op is given up on, same as `pending_ops`.
pub const MAX_ATTEMPTS: i64 = 5;

pub const EVT_UPDATED: &str = "calendar:updated";
pub const EVT_OPS_FAILED: &str = "calendar:ops_failed";

pub enum CalCommand {
    SyncNow,
    RunOps,
    Stop,
}

#[derive(Clone)]
pub struct CalHandle {
    tx: mpsc::UnboundedSender<CalCommand>,
}

impl CalHandle {
    pub fn sync_now(&self) {
        let _ = self.tx.send(CalCommand::SyncNow);
    }
    pub fn run_ops(&self) {
        let _ = self.tx.send(CalCommand::RunOps);
    }
    /// The window came back to the front: refresh, the user is looking.
    pub fn on_focus(&self) {
        self.sync_now();
    }
    pub fn stop(&self) {
        let _ = self.tx.send(CalCommand::Stop);
    }
}

fn registry() -> &'static Mutex<HashMap<String, CalHandle>> {
    static R: OnceLock<Mutex<HashMap<String, CalHandle>>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The running engine for an account, if any.
pub fn handle(account_id: &str) -> Option<CalHandle> {
    registry()
        .lock()
        .ok()
        .and_then(|m| m.get(account_id).cloned())
}

/// Hook for `WindowEvent::Focused(true)` in `lib.rs` (main session wires it).
pub fn on_window_focus() {
    if let Ok(m) = registry().lock() {
        for h in m.values() {
            h.on_focus();
        }
    }
}

pub fn stop(account_id: &str) {
    if let Some(h) = registry()
        .lock()
        .ok()
        .and_then(|mut m| m.remove(account_id))
    {
        h.stop();
    }
}

/// Start engines for every account with a stored calendar grant. Called from
/// `fork::start` (main session) once at setup; safe to call again.
pub fn start_all(app: AppHandle) {
    let db = app.state::<crate::state::AppState>().db.clone();
    tauri::async_runtime::spawn(async move {
        let accounts = match db
            .read("fork_cal_start_all", |conn| crate::db::accounts::list(conn))
            .await
        {
            Ok(a) => a,
            Err(e) => {
                tracing::warn!(error = %e, "calendar: cannot list accounts");
                return;
            }
        };
        for a in accounts {
            if google::is_connected(&a.id).unwrap_or(false) && handle(&a.id).is_none() {
                spawn(app.clone(), db.clone(), a.id, a.email);
            }
        }
    });
}

/// Spawn (or replace) the engine for one account and pull at once.
pub fn spawn(app: AppHandle, db: Db, account_id: String, account_email: String) -> CalHandle {
    stop(&account_id);
    let (tx, mut rx) = mpsc::unbounded_channel::<CalCommand>();
    let handle = CalHandle { tx };
    if let Ok(mut m) = registry().lock() {
        m.insert(account_id.clone(), handle.clone());
    }
    let mut engine = Engine {
        app,
        db,
        account_id,
        account_email,
    };
    tauri::async_runtime::spawn(async move {
        let mut poll = tokio::time::interval(POLL_INTERVAL);
        poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                cmd = rx.recv() => {
                    match cmd {
                        None | Some(CalCommand::Stop) => break,
                        Some(CalCommand::SyncNow) => {
                            engine.drain_ops().await;
                            engine.run_sync().await;
                        }
                        Some(CalCommand::RunOps) => {
                            if engine.drain_ops().await {
                                engine.run_sync().await;
                            }
                        }
                    }
                }
                _ = poll.tick() => {
                    engine.drain_ops().await;
                    engine.run_sync().await;
                }
            }
        }
    });
    handle
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Error codes after which retrying now is pointless: the drain stops and the
/// next tick (or reconnect) tries again, attempts untouched.
pub fn is_transient(code: &str) -> bool {
    matches!(
        code,
        "network" | "tls" | "oauth" | "gcal_not_connected" | "gcal_not_configured"
    )
}

struct Engine {
    app: AppHandle,
    db: Db,
    account_id: String,
    account_email: String,
}

impl Engine {
    fn emit_updated(&self) {
        let _ = self
            .app
            .emit(EVT_UPDATED, json!({ "account_id": self.account_id }));
    }

    async fn run_sync(&mut self) {
        if let Err(e) = self.sync_inner().await {
            tracing::warn!(account = %self.account_id, error = %e, "calendar sync failed");
        }
    }

    async fn sync_inner(&mut self) -> Result<()> {
        let account_id = self.account_id.clone();
        let list = gapi::fetch_calendar_list(&account_id).await?;
        let rows: Vec<_> = list
            .iter()
            .filter_map(|v| model::calendar_from_json(&account_id, v))
            .collect();
        let aid = account_id.clone();
        self.db
            .call(move |conn| store::upsert_calendars(conn, &aid, &rows))
            .await?;
        let aid = account_id.clone();
        let selected: Vec<_> = self
            .db
            .call(move |conn| store::list_calendars(conn, &aid))
            .await?
            .into_iter()
            .filter(|c| c.selected)
            .collect();
        let now = now_unix();
        let (from, to) = (now - WINDOW_PAST, now + WINDOW_FUTURE);
        for cal in selected {
            let items = gapi::fetch_events(&account_id, &cal.google_id, from, to).await?;
            let fresh: Vec<_> = items
                .iter()
                .filter_map(|v| event_from_json(cal.id, v))
                .collect();
            let cal_id = cal.id;
            self.db
                .call(move |conn| store::replace_window(conn, cal_id, from, to, &fresh))
                .await?;
        }
        self.emit_updated();
        Ok(())
    }

    /// Run queued ops FIFO. Returns whether any op reached Google.
    async fn drain_ops(&mut self) -> bool {
        let mut landed = false;
        loop {
            let aid = self.account_id.clone();
            let next = match self.db.call(move |conn| store::next_op(conn, &aid)).await {
                Ok(v) => v,
                Err(_) => break,
            };
            let Some((op_id, kind, payload, attempts)) = next else {
                break;
            };
            let parsed: Value = match serde_json::from_str(&payload) {
                Ok(v) => v,
                Err(_) => {
                    let _ = self
                        .db
                        .call(move |conn| store::finish_op(conn, op_id, false))
                        .await;
                    continue;
                }
            };
            match self.execute_op(&kind, &parsed).await {
                Ok(()) => {
                    landed = true;
                    let _ = self
                        .db
                        .call(move |conn| store::finish_op(conn, op_id, true))
                        .await;
                }
                Err(e) => {
                    tracing::warn!(op = %kind, error = %e, "calendar op failed");
                    if is_transient(e.code()) {
                        break;
                    }
                    if attempts + 1 >= MAX_ATTEMPTS {
                        let _ = self
                            .db
                            .call(move |conn| store::finish_op(conn, op_id, false))
                            .await;
                        // A create that never landed must not keep showing as
                        // an event: drop the optimistic row.
                        if kind == "create" {
                            if let Some(id) = parsed.get("event_id").and_then(Value::as_i64) {
                                let _ = self
                                    .db
                                    .call(move |conn| store::delete_event_row(conn, id))
                                    .await;
                            }
                        }
                        let _ = self.app.emit(
                            EVT_OPS_FAILED,
                            json!({ "account_id": self.account_id, "kind": kind, "message": e.to_string() }),
                        );
                        self.emit_updated();
                    } else {
                        let _ = self
                            .db
                            .call(move |conn| store::bump_attempts(conn, op_id))
                            .await;
                        break;
                    }
                }
            }
        }
        landed
    }

    async fn event_and_calendar(
        &self,
        event_id: i64,
    ) -> Result<(model::EventRow, model::CalendarRow)> {
        self.db
            .call(move |conn| {
                let Some(row) = store::get_event(conn, event_id)? else {
                    return Ok(None);
                };
                Ok(store::get_calendar(conn, row.calendar_id)?.map(|c| (row, c)))
            })
            .await?
            .ok_or_else(|| SkimError::other("gcal_op", "the event no longer exists locally"))
    }

    async fn execute_op(&mut self, kind: &str, p: &Value) -> Result<()> {
        // Never implicit: an op without an explicit choice is refused.
        let send =
            gapi::SendUpdates::parse(p["send_updates"].as_str().ok_or_else(|| {
                SkimError::other("gcal_input", "calendar op without send_updates")
            })?)?;
        match kind {
            "create" => {
                let event_id = p["event_id"]
                    .as_i64()
                    .ok_or_else(|| SkimError::other("gcal_op", "create op without event_id"))?;
                let (row, cal) = self.event_and_calendar(event_id).await?;
                if !is_local_id(&row.google_id) {
                    // Already landed (a crash between the POST and finish_op).
                    return Ok(());
                }
                let resp =
                    gapi::insert_event(&self.account_id, &cal.google_id, send, &p["body"]).await?;
                let server = event_from_json(row.calendar_id, &resp)
                    .ok_or_else(|| SkimError::other("gcal_api", "insert returned no event"))?;
                self.db
                    .call(move |conn| store::mark_synced(conn, event_id, &server))
                    .await
            }
            "patch" => {
                let event_id = p["event_id"]
                    .as_i64()
                    .ok_or_else(|| SkimError::other("gcal_op", "patch op without event_id"))?;
                let (row, cal) = self.event_and_calendar(event_id).await?;
                if is_local_id(&row.google_id) {
                    return Err(SkimError::other(
                        "gcal_op",
                        "the event's create has not reached Google yet",
                    ));
                }
                gapi::patch_event(
                    &self.account_id,
                    &cal.google_id,
                    &row.google_id,
                    send,
                    &p["body"],
                )
                .await
                .map(|_| ())
            }
            "delete" => {
                let cal = p["calendar_google_id"]
                    .as_str()
                    .ok_or_else(|| SkimError::other("gcal_op", "delete op without calendar"))?;
                let gid = p["google_id"]
                    .as_str()
                    .ok_or_else(|| SkimError::other("gcal_op", "delete op without event id"))?;
                gapi::delete_event(&self.account_id, cal, gid, send).await
            }
            "rsvp" => {
                let event_id = p["event_id"]
                    .as_i64()
                    .ok_or_else(|| SkimError::other("gcal_op", "rsvp op without event_id"))?;
                let response = p["response"]
                    .as_str()
                    .filter(|r| gapi::valid_response(r))
                    .ok_or_else(|| SkimError::other("gcal_op", "rsvp op without a response"))?;
                let (row, cal) = self.event_and_calendar(event_id).await?;
                if is_local_id(&row.google_id) {
                    return Err(SkimError::other("gcal_op", "cannot RSVP to a local event"));
                }
                let body =
                    gapi::rsvp_body(row.attendees_json.as_deref(), &self.account_email, response);
                gapi::patch_event(
                    &self.account_id,
                    &cal.google_id,
                    &row.google_id,
                    send,
                    &body,
                )
                .await
                .map(|_| ())
            }
            other => Err(SkimError::other(
                "gcal_op",
                format!("unknown calendar op {other}"),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_codes_stop_the_drain_without_spending_attempts() {
        for c in [
            "network",
            "tls",
            "oauth",
            "gcal_not_connected",
            "gcal_not_configured",
        ] {
            assert!(is_transient(c), "{c}");
        }
        for c in ["gcal_api", "gcal_op", "db", "gcal_scope"] {
            assert!(!is_transient(c), "{c}");
        }
    }

    #[test]
    fn window_is_sixty_days_back_and_one_eighty_forward() {
        assert_eq!(WINDOW_PAST / 86_400, 60);
        assert_eq!(WINDOW_FUTURE / 86_400, 180);
        assert_eq!(MAX_ATTEMPTS, 5);
    }
}
