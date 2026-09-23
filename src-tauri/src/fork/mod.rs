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
pub fn start(_app: AppHandle) {}
