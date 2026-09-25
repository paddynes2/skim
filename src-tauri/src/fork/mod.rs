//! Patrick's fork (`paddynes2/skim`, branch `paddy`). Everything the fork adds
//! to the Rust core lives under this module; upstream files get at most a hook
//! call each, logged in `docs/fork/TOUCHLIST.md`.

pub mod availability;
pub mod calendar;
pub mod commands;
pub mod compose;
pub mod court;
pub mod crm;
pub mod db;
pub mod estate_reply;
pub mod fold;
pub mod freshness;
pub mod gmail;
pub mod google;
pub mod list;
pub mod mcp;
pub mod prep;
pub mod restore;
pub mod scheduler;
pub mod search_query;
pub mod settings;
pub mod smell;

use tauri::AppHandle;

/// Fork-owned runtime state, one field on `AppState`.
#[derive(Default)]
pub struct ForkState {}

impl ForkState {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Start every fork background task. Called once at the end of `setup`.
pub fn start(app: AppHandle) {
    use tauri::{Listener, Manager};
    // 7.4: one calendar engine per account with a stored Google grant.
    calendar::start_all(app.clone());
    // 6.4: the one scheduler for held / send-later ops.
    scheduler::start(app.clone());
    // 12: the local MCP server (127.0.0.1:8342), off when `fork_mcp` = off.
    mcp::start(app.clone());
    // 10: full ball-in-court pass at startup, then a pass per `mail:updated`.
    let db = app.state::<crate::state::AppState>().db.clone();
    court::full_pass(app.clone(), db.clone());
    let app2 = app.clone();
    app.listen("mail:updated", move |e| {
        let touched = serde_json::from_str::<serde_json::Value>(e.payload())
            .ok()
            .and_then(|v| v.get("folderId").and_then(|f| f.as_i64()))
            .map(|f| vec![f])
            .unwrap_or_default();
        court::on_mail_updated(app2.clone(), db.clone(), touched);
    });
}
