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
| `src-tauri/tauri.conf.json` | `plugins.updater`, `version` | fork pubkey + `paddynes2/skim` latest.json endpoint; version 1.1.0 | own updater (0.4) |
| `package.json`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock` | `version` | 1.1.0 | fork version line (0.4) |
| `.github/workflows/release.yml` | `scoop` job | removed | the fork has no bucket (0.4) |
| `src-tauri/src/mail/sync.rs` | `execute_op` archive arm | calls `fork::gmail::archive_by_expunge(account, imap_name, role)` via new `folder_role` helper | Gmail by host; labels expunge (1.1, 1.2) |
| `src-tauri/src/mail/sync.rs` | `mirror_to_sent` | `provider != "gmail"` → `!fork::gmail::is_gmail(account)` | no duplicate Sent copies on Gmail-host accounts (1.1) |
| `src-tauri/src/mail/sync.rs` | `detect_role` | `important` maps to role `important` | Important is not Starred (1.3) |
| `src-tauri/src/commands/invites.rs` | `open_invite_ics` | provider read through `fork::gmail::effective_provider` | web calendar URL for Gmail-host accounts (1.1) |
| `src-tauri/src/mail/autoconfig.rs` | `lookup`, `lookup_async` | `GMAIL` preset const; MX probe also asks `fork::gmail::is_google_mx` and returns the Gmail preset | onboarding sets provider gmail for Google-hosted domains (1.1) |
| `src-tauri/src/commands/accounts.rs` | `add_account` | provider through `fork::gmail::effective_provider` | hand-configured Gmail host = gmail (1.1) |
| `src-tauri/src/db/queries.rs` | `virtual_folder_id` | `Some("important") => -9` | unified id for the new role (1.3) |
| `src/lib/folders.ts` | `roleKey` | `important: "nav.important"` | label for the new role (1.3) |
| `src/lib/i18n/locales/en.json` | `nav.important` | new string | (1.3) |
| `src/components/Sidebar.svelte` | `mainFolders`, count span | hide `important` not `starred`; Starred shows its total via `fork/stores/starred` | Starred in the sidebar (1.4) |
| `src/components/ReadingPane.svelte` | toolbar, `archive()` | `{#if canArchive}` from `fork/actions.archiveOffered` | no Archive in Sent/Trash/Spam (1.2) |
| `src/App.svelte` | `actOnSelected` | early return when `!archiveOffered(role)` | no Archive in Sent/Trash/Spam (1.2) |
| `src-tauri/src/lib.rs` | `generate_handler!` | `fork::commands::fork_role_total` | Starred total (1.4) |
| `demo/mock/tauri-core.ts` | `invoke` switch | `fork_role_total` fixture | demo harness (1.4) |
