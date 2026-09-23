# Upstream touch list

Every edit to a file that exists upstream (`nikserg/skim`), one line each. New
files under `src-tauri/src/fork/`, `src/fork/`, `scripts/fork/`, `docs/fork/`
and `demo/` additions are fork-owned and are not listed. Keep upstream edits to
a hook call, a parameter, a prop; this table is what makes
`git merge upstream/main` survivable.

| File | Symbol | Change | Reason |
|---|---|---|---|
| `src-tauri/src/db/mod.rs` | `Db::init` | one call `crate::fork::db::migrate(&mut conn)?` after upstream `migrate` | fork migration runner (0.2) |
| `src-tauri/src/state.rs` | `AppState` | field `fork: crate::fork::ForkState` + init | fork runtime state (0.3) |
| `src-tauri/src/lib.rs` | module list | `pub mod fork;` | register fork module (0.3) |
| `src-tauri/src/lib.rs` | `setup` | `fork::start(app.handle().clone())` at the end | fork background tasks (0.3) |
| `src-tauri/src/lib.rs` | `generate_handler!` | fork commands appended under a comment | IPC surface for fork commands (0.3) |
| `src-tauri/src/mail/suspicion.rs` | `sender_signals` (name/addr mismatch) | boolean pulled into a `let` with `#[allow(clippy::nonminimal_bool)]` | upstream fails clippy on rustc 1.91 (`-D warnings`); no behaviour change |
