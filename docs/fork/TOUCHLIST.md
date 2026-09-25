# Upstream touch list

Every edit to a file that exists upstream (`nikserg/skim`), one line each. New
files under `src-tauri/src/fork/`, `src/fork/`, `scripts/fork/`, `docs/fork/`
and `demo/` additions are fork-owned and are not listed. Keep upstream edits to
a hook call, a parameter, a prop; this table is what makes
`git merge upstream/main` survivable.

| File | Symbol | Change | Reason |
|---|---|---|---|
| `CLAUDE.md` | top | three-line pointer to `docs/fork/README.md` | a fresh agent on `paddy` otherwise reads the upstream "no calendar" principle as binding |
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
| `src-tauri/src/mail/sync.rs` | `Engine` (struct, `db`, `account`, `session`, `ensure_selected`) | `pub(crate)` | `fork::restore::execute` drives one op on the engine (3.2) |
| `src-tauri/src/mail/sync.rs` | `execute_op` | one arm: `fork_restore` → `fork::restore::execute` | undo after the grace window (3.2) |
| `src/lib/stores/mail.svelte.ts` | `shown`, `loadMoreThreads`, `insertThreadRow` | held rows filtered out of every page; row re-insert for undo; "no undo" comment reworded | 3.2 |
| `src/lib/bulk.ts` | `bulkAct` | removals and read/unread through `fork/actions` (held, undoable) | 3.2 |
| `src/components/FolderPicker.svelte` | `activate` | move through `fork/actions.moveRows` (held, undoable) | 3.2 |
| `src/App.svelte` | keydown, window, markup | Ctrl+Z before the Ctrl guard; `beforeunload` flushes held removals; `<Toast />` mounted | 3.2 |
| `src/components/ShortcutsOverlay.svelte` | actions group | Undo row | 3.2 |
| `src-tauri/src/mail/sync.rs` | `SyncCommand`, `SyncHandle::sync_folder`, engine loop, `drain_ops`, `resync_folder_id` | `SyncFolder` variant + handle method; drain returns the resynced set and extends it with `fork::freshness::folders_after_op_db(kind)` after each op; the id -> imap_name -> sync tail factored into `resync_folder_id` (logs + `reset_session` on failure) | 3.4: Sent (Gmail too) + Drafts resync right after send / save_draft / rsvp, and on demand |
| `src/lib/stores/mail.svelte.ts` | imports, `selectFolder` | `folderSelected(...)` call after `selectedFolderId` is set | 3.4: Sent/Drafts sync on open |
| `src/App.svelte` | window | `onfocus={windowFocused}` | 3.4: refresh Sent/Drafts on focus |
| `src-tauri/src/ai/agent.rs` | `search_emails` | clause builder replaced by `search_query::Filters` + `filter_sql` | 4.2: one filter builder |
| `src-tauri/src/commands/search.rs` | `search_messages` | parses operators with `fork::search_query::parse`, appends `filter_sql`; filters-only query runs a plain `messages` query | 4.2 |
| `src/lib/stores/mail.svelte.ts` | state, `fetchPage`, `refreshFolders`, fork section, `mail` | search as virtual folder -900 (`enterSearch`/`exitSearch`/`removeSearchToken`, getters); one-line guard so `folders:updated` never bounces an open search | 4.3 |
| `src/components/MessageList.svelte` | header | `{:else if mail.searching}` branch rendering `SearchChips` | 4.3 |
| `src/components/CommandPalette.svelte` | `onKeydown`, input row, styles | `showInList()`; ArrowUp reaches -1; Enter with no row / Shift+Enter = show in list; hint button | 4.3 |
| `src/components/ShortcutsOverlay.svelte` | nav group | search rows | 4.3 |
| `src-tauri/src/mail/sanitize.rs` | builder, `attribute_filter`, `text_to_html`, tests | quote/signature markers rewritten to `skim-quote` / `skim-sig` via `fold_marker()`, nothing arbitrary survives; plain text split at `fork::fold::plain_fold_point`; +10 tests | 5.1 / 5.2 |
| `src-tauri/src/db/models.rs` | `RenderedBody` | `+ has_fold: bool` | 5.3 |
| `src-tauri/src/commands/mail.rs` | `get_message_body` | `has_fold` computed after sanitising (the struct's only constructor) | 5.3 |
| `src/lib/types.ts` | `RenderedBody` | `+ hasFold?: boolean` | 5.3 |
| `src/components/HtmlViewer.svelte` | props, `buildDoc` | `folded` prop adds the fold rule (`display:none !important`) to the document stylesheet | 5.3 |
| `src/components/ReadingPane.svelte` | `unfolded`, `isFolded()`, `toggleFold()`, markup, style | the "..." pill under the body; default folded except a lone Fwd/FW/WG/TR | 5.3 |
| `src-tauri/src/mail/oauth.rs` | `OauthProvider::scopes` | made `pub` | callers now pass scopes explicitly |
| `src-tauri/src/mail/oauth.rs` | `OauthProvider::required_scope` | new `pub fn` (Google → `https://mail.google.com/`, Microsoft → `None`) | the mail flow's hard-coded check became a parameter |
| `src-tauri/src/mail/oauth.rs` | `authorize` | + `scopes: &str, required_scope: Option<&str>` params | 7.2: the fork's calendar grant reuses the loopback flow with its own scopes |
| `src-tauri/src/mail/oauth.rs` | `build_auth_url` | + `scopes: &str` param, used for the `scope` query pair | same |
| `src-tauri/src/mail/oauth.rs` | `resolve_email` | + `required_scope: Option<&str>`; checks it (exact token match) instead of the mail scope; skips tokeninfo when `None` | same; behaviour for mail unchanged |
| `src-tauri/src/mail/oauth.rs` | tests | `build_auth_url` calls pass `provider.scopes()`; + 2 tests | signature change |
| `src-tauri/src/commands/accounts.rs` | `start_google_oauth`, `start_microsoft_oauth` | pass `provider.scopes()`, `provider.required_scope()` | signature change; behaviour unchanged |
| `src/App.svelte` | script imports | `CrmDrawer`, `crmFocus`, `registerCrmNav()` | mount the drawer, wire `i` (9) |
| `src/App.svelte` | `main.panes` `{:else}` branch | `<CrmDrawer email=... open=... />` after `<ReadingPane />` | the drawer beside the reading pane (9) |
| `src/components/ReadingPane.svelte` | `focused` / `replyTarget` | one `$effect` calling `crmFocus.setEmail(...)` | the drawer follows the focused message's sender (9) |
| `src/components/settings/Settings.svelte` | after `<SettingsFork />` | `<SettingsCrm />` + import | Settings → CRM (9) |
| `src-tauri/src/commands/ai.rs` | `AiContext` | `struct` -> `pub(crate) struct`; fields `endpoint`, `key`, `model`, `now` -> `pub(crate)` (`provider`, `locale` stay private) | 10 AI pass: the plan says "uses `ai_context`"; the batch request in `fork::court::complete` resolves provider / key / model exactly as every AI command does instead of duplicating that 40-line resolution |
| `src-tauri/src/commands/ai.rs` | `ai_context` | `async fn` -> `pub(crate) async fn` | same |
| `src-tauri/src/lib.rs` | `generate_handler!` | the three commands above | |
| `src-tauri/src/notify.rs` (or `fork/mod.rs`) | daily nudge | at the setting's time (`fork_court_nudge`, default "09:00", "off" disables), `if fork::court::should_nudge_now(&setting, last_nudged_at, now) { if let Some(text) = fork::court::nudge_line(conn)? { toast(text, opens VF_ON_ME) ; store last_nudged_at } }`. `last_nudged_at` is the main session's to persist (suggest a `fork_court_nudged_at` settings row, internal). Both helpers are tested; `should_nudge_local` is the clock-free core | plan: "daily toast at 09:00 local (setting) ... click opens On me" |
| `src-tauri/src/commands/mail.rs` | `queue_op` | database half factored into `pub(crate) fn queue_op_local(conn, ids, kind, extra, local) -> Vec<account_id>`; `queue_op` calls it, behaviour unchanged | MCP archive/star/mark_read take the same op path as a keypress (12) |
| `src/components/ComposeForm.svelte` | props | `variant?: "default" \| "reply"`, `onPopOut?` | 6.2 inline reply |
| `src/components/ComposeForm.svelte` | script: rich block after `canPickFrom` | `richMode`, `wordsHtml`/`wordsText`/`tail`, `tailParts`, `onWordsChange`, `onTailInput`, body-change `$effect`, `flushHtml()`; `canPickFrom` also `variant !== "reply"` | 6.3 data model (D5) |
| `src/components/ComposeForm.svelte` | `discardClick()` + `discardArmed` | two-click Discard | 6.1 |
| `src/components/ComposeForm.svelte` | load `$effect` | reads `fork_draft_html`, splits the tail; reply variant scrolls into view + focuses | 6.2, 6.3 |
| `src/components/ComposeForm.svelte` | `scheduleSave` | `await flushHtml()` after `updateDraft` | 6.3 |
| `src/components/ComposeForm.svelte` | `send(when?)` | flushes HTML; `api.sendDraft(id, notBefore, label)` with the undo hold (`prefs.undoSendSecs`) or the picked time | 6.4 |
| `src/components/ComposeForm.svelte` | `sendKey`, `onFormKeydown`, `popOut`, `close`, `onDestroy` | Ctrl/Cmd+Enter sends; Esc on the reply variant; pop-out; window ✕ saves when dirty; reply teardown deletes an untouched draft | 6.1, 6.2 |
| `src/components/ComposeForm.svelte` | template | root gets `bind:this`, `class:reply`, `onkeydown`; subject `onkeydown={sendKey}`; body = `<RichEditor>` + `.tail` (sig, "•••" quote, "Edit quoted text") or the textarea; footer = split Send (`SendLater`), pop-out button, Discard for both variants | 6.1-6.4 |
| `src/components/ComposeForm.svelte` | style | `.discard.armed`, `.compose-form.reply*`, `.popout`, `.send-split`, `.tail*`, `.quote-*` | |
| `src/components/ReadingPane.svelte` | imports, `reply()`, target `$effect`, `inlineReplySlot` snippet rendered after the focused / shown `messageBlock`, `.inline-reply` style | inline reply under the open message | 6.2 |
| `src-tauri/src/commands/compose.rs` | `send_draft` | `+ not_before: Option<i64>, label: Option<String>` → `Result<i64>` (op id); `fork::scheduler::hold` inside the enqueue transaction, `announce` instead of `run_ops` when held | 6.4 |
| `src-tauri/src/mail/smtp.rs` | `build_message` → `build_message_with_html(.., html: Option<&str>)` | `build_message` delegates with `None`; `Some` = `multipart/alternative[text/plain, text/html]`, inside `multipart/mixed` with attachments | 6.3 |
| `src-tauri/src/mail/sync.rs` | `drain_ops` SELECT | `AND id NOT IN (SELECT op_id FROM fork_op_schedule WHERE not_before > unixepoch())` (already in HEAD d44ef77) | 6.4 (D4) |
| `src/lib/api.ts` | `sendDraft` | `(draftId, notBefore = null, label = null) => invoke<number>` | 6.4 |
| `src/lib/types.ts` | – | not touched (`Draft` unchanged; `ScheduledSend` lives in `src/fork/compose/api.ts`) | |
| `package.json`, `package-lock.json` | dependencies | `squire-rte ^2.4.9` (MIT). npm (older than the lock's) also dropped the `libc` arrays from 18 optional-dep entries and moved the root version to 1.1.0 in the lock; restore the lock from HEAD and re-add only the squire entry if you want a minimal diff | 6.3 |
| `src/lib/stores/mail.svelte.ts` | imports | `courtApi`, `COURT_UPDATED`, `VF_ON_ME`, `VF_WAITING`, `courtCounts`, `CourtState` | 10 views |
| `src/lib/stores/mail.svelte.ts` | `attachListeners` | `listen(COURT_UPDATED)`: refreshThreads when a court view is open, `courtCounts.refresh()` always | plan: views live; `mail:updated` names folders, not views |
| `src/lib/stores/mail.svelte.ts` | `refreshFolders` | early return when the selected id is -920/-921, beside the -900 guard | a sync must not bounce an open view to the inbox |
| `src/lib/stores/mail.svelte.ts` | `fetchPage` | route -920/-921 to `courtApi.list(state, offset, limit)` before the `< 0` unified branch | paging, refresh, reading pane, keys, undo unchanged on court rows |
| `src/lib/stores/mail.svelte.ts` | `courtStateOf`, `selectCourt` (new, fork section) | id <-> state; `selectCourt` clears search state then `selectFolder(id)` | same shape as `enterSearch` |
| `src/lib/stores/mail.svelte.ts` | `mail` export | `get courtView`, `selectCourt` | MessageList header, Sidebar selection, courtStore |
| `src/components/Sidebar.svelte` | script + calendar section | two `.item.court` buttons with `.count.total` badges, `class:selected` on -920/-921 | plan: "On me (N)" / "Waiting (N)" |
| `src/components/MessageList.svelte` | header | `{:else if mail.courtView}` branch: title + "Oldest first · N" sub-line + J/K hint; All/Unread/Starred block untouched | the view is its own filter |
| `src/components/MessageList.svelte` | rows | each `MessageRow` wrapped in `.court-wrap` (position: relative); in a court view a `.court-badge-slot` overlay renders `CourtRow` | MessageRow is upstream + fixed-height; an overlay adds no height, so the windowing arithmetic holds |
| `src/components/MessageList.svelte` | script | `nowSecs` minute ticker, `courtOf(row)` | ages recolour across a day boundary without a reload |
| `src/lib/stores/ui.svelte.ts` | `state.view`, `ui.view`, `ui.showCalendar()`, `ui.showMail()` | new field + 3 members | 7.5: the calendar view switch |
| `src/components/Sidebar.svelte` | folders section | one "Calendar" item (with `G C` caption) above the folders; folder clicks call `ui.showMail()` first; `class:selected` on folders also requires `ui.view === "mail"` | 7.5: Calendar in the sidebar; a folder click leaves the calendar |
| `src/components/InviteCard.svelte` | template, imports | one import line + one line `<InviteCardExtras {invite} />` before the closing `</div>` | 7.5: "In your calendar" + conflicts |
| `package.json`, `package-lock.json` | dependencies | `@event-calendar/core` `5.14.1` (exact) | 7.5 calendar grid (MIT) |
| `src/lib/i18n/locales/en.json` | `fork.settings.*` | 7 keys | labels |
| `src/components/ComposeForm.svelte` | script, body textarea, footer | AI-tell layer (health line, popover, send check); insert-at-caret, `/slots` trigger, Share availability + Add Meet link buttons | 6.5 / 7.6 / 8 |
| `src-tauri/src/commands/compose.rs` | `delete_draft` | also drops the draft's queued send / save ops (`fork::scheduler::drop_ops_for_draft`) | safety audit: a held send must die with its draft |
| `src-tauri/gen/schemas/capabilities.json` | `default.permissions` | `core:webview:allow-set-webview-zoom` (generated from `capabilities/default.json` by the build) | zoom keys (2.7) |
| `src/ComposeRoot.svelte` | template | mounts `<SlotsPopover />` and `<Toast />` after `<Composer>` | `/slots` and Share availability in the pop-out compose window (D47) |
| `src/components/MessageRow.svelte` | style | `.unread .subject { color: var(--unread); font-weight: 600; }` | list variant C (D46) |
| `src/styles/tokens.css` | two light theme blocks | `--unread` #1f6fd1 -> #1a62c0 | variant C uses it as text, so it must clear 4.5:1 (D46) |
| `src/components/ComposeForm.svelte` | `loadedShape` / `shapeOf` / `edited()` | Esc, teardown and `flushServer` save only when the draft differs from what was loaded | an untouched reply was saved to Gmail Drafts (D41) |
| `src/components/ComposeForm.svelte` | `onWordsChange` | a `/slots` line token in the rich editor opens Share availability | the rich editor is the default body (D45) |
| `src-tauri/src/mail/sync.rs` | IDLE watcher, `idle_cycle`, incremental header refresh | bounded notification commands, 60 s heartbeat, reset/capped retry, notify before DONE, refresh after header commit; scripted IMAP recovery tests | September 25 inbox recovery fix installed; startup and connection checks passed, real-world end-to-end arrival latency remains unverified (STATUS.md) |

## Estate reply (25 September 2026)

Campaign draft repair, 25 September 2026: `src/App.svelte` resolves only Drafts
messages and invalidates stale editor lookups; `src-tauri/src/commands/compose.rs`
checks draft membership before cache reuse and after fetching the body through
`fork::compose::require_draft_message`. `src/lib/bulk.ts` and
`src/components/CommandPalette.svelte` route Drafts actions through the fork-owned
folder-scoped message resolver. This prevents reopening sent mail as a draft and
moving sent copies to Trash while deleting grouped drafts.

| File | Hook | Why |
|---|---|---|
| `src-tauri/src/lib.rs` | Register `fork_estate_reply` | Fixed SSH start/status and exact draft lookup |
| `src/components/ReadingPane.svelte` | Fork-owned action beside Reply; footer wraps | Estate workflow available at the email without a new screen |
| `src/lib/i18n/locales/en.json` | `estate.*` labels | Localized action, progress and source disclosure |
| `demo/mock/tauri-core.ts` | Opt-in estate fixtures | Exercise working/ready/error states without real mailbox calls |
