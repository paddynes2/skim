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
| `src/styles/tokens.css` | all four theme blocks | `--text-dim`/`--text-faint` raised to clear 4.5:1; new `--unread`, `--star`, `--focus`, `--row-unread-tint` | contrast + state colours (2.1) |
| `src/components/MessageRow.svelte` | whole component | rewritten: 20px gutter (dot + star buttons), dimmed read rows, aria-label, paperclip, hover actions, compact density, avatars; checkbox slot logic kept | 2.2 / 2.4 / 2.6 |
| `src/styles/base.css` | global | `:focus-visible` ring in `--focus` | 2.3 |
| `src/components/MessageList.svelte` | `rowH` | effect resets the baseline on density change; `prefs` import | 2.6 |
| `src/components/ReadingPane.svelte` | `archive/remove/reportSpam/toggleStar/toggleRead` | delegate to `fork/actions` | one action path + auto-advance (2.5 / 3.2) |
| `src/App.svelte` | `actOnSelected`, boot, `onKeydown` | delegate to `fork/actions.act`; `prefs.hydrate` + `applyZoom`; `zoomKey` before the Ctrl guard | 2.5 / 2.7 |
| `src/ComposeRoot.svelte`, `src/AiChatRoot.svelte` | boot, keydown | `prefs.hydrate` + `applyZoom`; `zoomKey` | zoom in every window (2.7) |
| `src/components/settings/Settings.svelte` | after the toggles section | `<SettingsFork />` | fork settings panel (2.5-2.7) |
| `src/components/ShortcutsOverlay.svelte` | global group | zoom rows | 2.7 |
| `src-tauri/src/commands/settings.rs` | `get_settings`, `set_setting` | chain `fork::settings::ALLOWED` | fork settings keys |
| `src-tauri/capabilities/default.json` | permissions | `core:webview:allow-set-webview-zoom` | 2.7 |
| `demo/vite.demo.config.ts`, `demo/mock/tauri-core.ts` | aliases, `get_settings` | webview mock; `skimdemo.fork_*` served as settings | harness |
| `src-tauri/src/db/queries.rs` | `list_threads`, `list_messages`, `list_unified_threads`, `list_unified_messages` | each now wraps a `*_opts` twin that splices `fork::list` filter/order into the same SQL; `LIST_THREADS_SQL` is `pub(crate)` | filter chips + unread first (3.1) |
| `src-tauri/src/commands/mail.rs` | the four list commands | optional `filter`, `order` args → `ListOpts::parse` | 3.1 |
| `src/lib/api.ts` | the four list wrappers | optional `filter`, `order` | 3.1 |
| `src/lib/stores/mail.svelte.ts` | `state.listFilter`, `fetchPage`, `setListFilter/Order`, `reloadList` | filter rides every page read; reload on change | 3.1 |
| `src/components/MessageList.svelte` | header, styles | unread microlabel → All/Unread/Starred chips | 3.1 |
| `src/App.svelte` | `onKeydown` | `forkKey(e)` before upstream's switch | fork key map (3.1/3.3) |
| `src/components/ShortcutsOverlay.svelte` | navigation group | Shift U / Shift S rows | 3.1 |
| `demo/mock/tauri-core.ts` | list commands | `listOpts` over fixtures | harness |
