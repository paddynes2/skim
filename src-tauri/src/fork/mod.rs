//! Patrick's fork (`paddynes2/skim`, branch `paddy`). Everything the fork adds
//! to the Rust core lives under this module; upstream files get at most a hook
//! call each, logged in `docs/fork/TOUCHLIST.md`.

pub mod commands;
pub mod db;
pub mod gmail;
pub mod list;
pub mod settings;

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
