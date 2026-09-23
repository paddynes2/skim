# Skim fork (paddynes2/skim, branch `paddy`): the full build plan

Written 2026-09-23 against upstream `nikserg/skim` @ `cd60077` (v1.0.30). Every
file:line below was read in the source for this plan, not taken from a summary.
Line numbers drift once work starts; the symbol names are what to search for.

Owner: Patrick Nesbitt. Daily account: `patrick@autospark.ai` (Google Workspace,
IMAP app password, `accounts.provider = 'custom'`, theme `warm-dark`).
Live data: `%APPDATA%\com.skim.app\skim.db` (28,827 messages, 24 folders).

---

## 0. Ground rules for this fork

These bind every phase. A phase is not done if it breaks one.

1. **Upstream stays mergeable.** `main` mirrors `nikserg/skim`; all work lands on
   `paddy`. New code goes in new files wherever possible:
   - Rust: `src-tauri/src/fork/` (one module per feature, registered from `fork/mod.rs`).
   - Svelte/TS: `src/fork/` (components, stores, lib).
   - Every edit to an upstream file is logged in `docs/fork/TOUCHLIST.md`
     (file, symbol, why, one line). Keep upstream edits small: a hook call,
     a parameter, a prop. The touch list is what makes `git merge upstream/main`
     survivable.
2. **Never alter an upstream table.** Fork state lives in `fork_*` tables created
   by a separate fork migration runner (Phase 0.2). Upstream's
   `PRAGMA user_version` sequence (0001-0015, `db/mod.rs:13-29`) is left alone,
   so an upstream 0016 can never collide with ours. The only exceptions are the
   one-time data repairs in Phase 1, which are UPDATEs on existing columns.
3. **Offline-first, like upstream.** Every mail mutation goes through
   `pending_ops` via `queue_op` (`commands/mail.rs:484`). Calendar mutations go
   through their own `fork_cal_ops` queue with the same shape. The UI updates
   optimistically; the network catches up.
4. **All outside HTTP happens in Rust.** The app CSP (`tauri.conf.json`
   `connect-src ipc: http://ipc.localhost`) blocks the webview from any other
   host. Google, Meet, Rebound and AI calls are Rust commands.
5. **Secrets only in Windows Credential Manager** via `secrets::set/get`
   (`secrets.rs`), keys prefixed `fork:`. Never in the DB, never in settings,
   never in logs.
6. **Violet (`--accent`) stays AI-only** (upstream rule, `CLAUDE.md`). New state
   colours get their own tokens.
7. **i18n:** new strings go in `src/lib/i18n/locales/en.json` only. Other
   locales fall back to English (`i18n/index.svelte.ts:78-80`, verified). The
   `MsgKey` type means a key missing from `en.json` fails `npm run check`.
8. **Nothing sends mail on its own.** No feature, timer or MCP tool may submit an
   email or an external calendar invite without Patrick pressing Send (or a send
   he scheduled himself). The MCP server creates drafts; it never sends.
9. **Gates, every phase:** `npm run check`, `npm run build`,
   `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
   `cargo test` (all with `--manifest-path src-tauri/Cargo.toml`, per
   `CONTRIBUTING.md`). Every new Rust module ships unit tests. Every new UI
   state gets a mock in `demo/mock/` and a screenshot check (Phase 0.6).
10. **Commit per phase step**, message prefixed `fork(<phase>):`.

---

## 1. What the code does today (the facts this plan rests on)

| Area | Fact | Where |
|---|---|---|
| Unread | 7px dot in `--text` + bold sender (700) + subject 600. Read subject is the same colour as unread. | `MessageRow.svelte:56,159-169,199-209` |
| Star | `★` in `--text-faint` before the subject | `MessageRow.svelte:66,210-213` |
| Contrast | `--text-faint` on `--bg`: cold-light 2.34, cold-dark 2.93, warm-light 2.31, **warm-dark 3.17** (his theme). WCAG AA text needs 4.5. `--text-dim` passes (4.65-6.77). | `tokens.css` 32/64/97/135 |
| Fonts | All px, no rem; body 14px. A single variable cannot scale type. | `base.css:16-18` |
| Focus | inputs `outline:none`; `:focus-visible` in only 4 components | `base.css:39-46` |
| List | Windowed; `rowH` only ratchets UP (freeze fix). Density change must reset it explicitly. | `MessageList.svelte:55-79` |
| Archive/delete | Local rows **deleted** at once (`remove_messages_local`, cascades bodies/attachments), op queued, `run_ops()` fired immediately | `bodies.rs:464`, `commands/mail.rs:484-531` |
| Gmail archive | Only when `provider == "gmail"` AND folder is INBOX: `\Deleted` + UID EXPUNGE. Else `role_folder("archive","Archive")`, which **CREATEs "Archive"** if no archive-role folder exists | `sync.rs:1952-1962, 2485-2525` |
| His account | `provider='custom'`; no archive-role folder. Pressing E today would create a Gmail label "Archive". Sent mirror appends a copy because provider != gmail (Gmail already files SMTP sends): duplicates. 0 duplicates yet (7 sends since the account was added 2026-09-22). | DB read 2026-09-23; `sync.rs:2357` |
| Important | `detect_role` maps name "important" to role `starred`: `[Gmail]/Important` (7,881 msgs) and `[Gmail]/Starred` (1,141) both carry role `starred` | `sync.rs:2807` |
| Starred view | Sidebar hides role `starred` and `all` | `Sidebar.svelte:22-24` |
| After archive | Selection is cleared; nothing advances to the next row | `mail.svelte.ts:686-697` |
| Undo | None anywhere. Comments say so (`mail.svelte.ts:427`, `commands/mail.rs:764`) | |
| Op queue | `pending_ops(id, account_id, kind, payload, created_at, attempts, state)`; drain picks `state='pending' ORDER BY id LIMIT 1`, FIFO, stops on network error | `0001_init.sql`, `sync.rs:1704-1867` |
| Send | `send_draft` enqueues `send` and runs ops at once; built as text/plain only (multipart/mixed with attachments) | `compose.rs:672-701`, `smtp.rs:68-146` |
| Compose | `<textarea>`; reply quote built in Rust as `> ` lines under `On {date}, {who} wrote:`; signature `\n\n-- \n{sig}` above the quote; AI co-author splits body at `SIG_MARK`/attribution (`splitTail`) | `ComposeForm.svelte:194-243,655`, `compose.rs:411-430`, `smtp.rs:42-47` |
| Compose window ✕ | Deletes a never-saved draft ("The window's ✕ saves nothing") | `ComposeForm.svelte:481-491` |
| Send key | No Ctrl+Enter send (Ctrl+Enter only submits the AI instruction box) | `ComposeForm.svelte:615-627` |
| Reply | Always opens a separate 720x680 window by draft id | `compose.rs:703-727` |
| Reading pane | One focused message rendered; others collapsed as snippet rows | `ReadingPane.svelte:538-586` |
| Sanitiser | ammonia allowlist; no `class`/`id`/`type` survive, so Gmail/Outlook quote markers are lost; `blockquote` survives | `sanitize.rs:146-248` |
| Viewer | iframe `sandbox="allow-same-origin"`, no scripts, parent can reach `contentDocument`; ResizeObserver re-measures height | `HtmlViewer.svelte:74-92,181-238,313-320` |
| Plain text | `<pre class="skim-plain">` escaped + linkified; no quote handling | `sanitize.rs:256-263` |
| Quote rules | `parse::strip_quoted` already knows attribution lines (EN/RU/DE/FR), `-----Original Message-----`, `-- `, `>` lines | `parse.rs:514-532` |
| Search | FTS5 prefix AND, palette only, 12 hits, no operators | `search.rs:20-97`, `CommandPalette.svelte:139-156` |
| Structured filters | The AI agent's `search_emails` already builds SQL for from/subject/folder role/has_attachment/unread/starred/after/before | `ai/agent.rs:808-919` |
| Settings | Allowlist `ALLOWED`; generic key/value table | `commands/settings.rs:8-29` |
| OAuth | Google scopes hard-coded to mail; `resolve_email` rejects tokens without `https://mail.google.com/`; client id baked at compile time via `option_env!` | `oauth.rs:68-109,332-361` |
| Service account | `GOOGLE_CLIENT_EMAIL/PRIVATE_KEY` in OS `.env` has **no** domain-wide delegation (tested 2026-09-23: `unauthorized_client`). Calendar needs a user OAuth grant. | |
| Updater | Upstream pubkey + `nikserg/skim` endpoint; would overwrite a fork build. `createUpdaterArtifacts:true` makes the build fail without `TAURI_SIGNING_PRIVATE_KEY`. | `tauri.conf.json`, `release.yml` |
| Install | Same identifier `com.skim.app`, NSIS `currentUser`: a fork build installs over the current app and keeps its data dir and Credential Manager entries | `tauri.conf.json` |
| Local server | None. Tokio `net` is enabled. | `Cargo.toml` |
| AI plumbing | `ai_context(db)` resolves provider/key/model; `spawn_stream` streams; `anthropic::stream` / `openai_compat::stream` are the one-shot primitives | `commands/ai.rs:246-443` |
| Rebound | `POST /api/v1/extension/lookup` takes `{email}` and returns `{person, company, deals[], activities[]}`; auth = Supabase user JWT as Bearer + `X-Workspace-ID`. Hosted at `https://rebound.patricknesbitt.ai`. OS `.env` has `REBOUND_API_BASE_URL`, `REBOUND_SUPABASE_URL`, `REBOUND_SUPABASE_ANON_KEY`. | `OS/apps/internal/rebound/src/app/api/v1/extension/lookup/route.ts`, `src/lib/api/auth.ts` |
| Demo harness | `demo/vite.demo.config.ts` aliases every Tauri IPC module to `demo/mock/*` for browser rendering at port 1421 | `demo/` |

---

## 2. Phases (in build order)

Each step: **Do** (what and where), **Done when** (checks that must pass).

### Phase 0: Foundation

**0.1 Branch and remotes.** Work on `paddy` (created). `origin` = `paddynes2/skim`,
`upstream` = `nikserg/skim`. Create `docs/fork/TOUCHLIST.md` (table: file, symbol,
change, reason) and `docs/fork/DECISIONS.md` (one entry per decision in section 3).
- Done when: both files exist and are committed.

**0.2 Fork migration runner.** New `src-tauri/src/fork/db.rs`:
- `FORK_MIGRATIONS: &[&str]` from `src-tauri/src/fork/migrations/f0001_*.sql` onward.
- Version stored in the upstream `settings` table under key `fork_schema_version`
  (a value row, not a schema change). Each step runs in one transaction with its
  version bump, mirroring `db::migrate` (`db/mod.rs:165-181`).
- Called from `Db::init` right after `migrate(&mut conn, MIGRATIONS)`
  (`db/mod.rs:81`): one-line upstream touch, logged.
- Tests: applies cleanly in memory; re-run is a no-op; a failing step rolls back
  entirely (copy `failed_migration_rolls_back_entirely`).
- Done when: `cargo test fork::db` passes.

**0.3 Module skeleton.** `src-tauri/src/fork/mod.rs` declares every fork module;
`lib.rs` gets `pub mod fork;` and the fork commands appended to
`generate_handler!` (`lib.rs:327-403`). Fork background tasks start from one
`fork::start(app_handle)` call at the end of `setup` (`lib.rs:~302`).
`AppState` gains one field `fork: fork::ForkState` (`state.rs`).

**0.4 Own updater.** Generate a keypair once:
`npx tauri signer generate -w %USERPROFILE%\.skim-fork\skim-fork.key` (store the
password alongside in Credential Manager key `fork:updater_key_password` and in
the file `%USERPROFILE%\.skim-fork\build.env`, which is outside the repo).
- `tauri.conf.json`: `plugins.updater.pubkey` = new public key; `endpoints` =
  `https://github.com/paddynes2/skim/releases/latest/download/latest.json`.
- `release.yml`: keep; set repo secrets `TAURI_SIGNING_PRIVATE_KEY` and
  `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` on `paddynes2/skim` with `gh secret set`.
  Remove the `scoop` job (the fork has no bucket).
- Version: `package.json`, `Cargo.toml`, `tauri.conf.json` to `1.1.0` (fork line
  sits above upstream 1.0.x; bump the patch per fork release).
- Done when: a local signed build succeeds (0.5) and `latest.json` in a draft
  release points at the fork.

**0.5 Build + install script.** `scripts/fork/build-install.ps1`:
1. Stop if `git status` is dirty on tracked files.
2. Back up `%APPDATA%\com.skim.app\skim.db*` to `%USERPROFILE%\.skim-fork\backups\<timestamp>\`
   (checkpoint first is not possible from outside; copy all three files while the
   app is closed, see 4).
3. Load `%USERPROFILE%\.skim-fork\build.env` (signing key path/password, optional
   `SKIM_GOOGLE_CLIENT_ID/SECRET`).
4. `npm ci`; run all gates from rule 9; `npm run tauri build -- --bundles nsis`.
5. Close Skim (`Stop-Process -Name skim` after asking it to quit via tray is not
   scriptable; use `taskkill /IM skim.exe` then wait), run the NSIS installer
   `/S` (silent, currentUser), relaunch `%LOCALAPPDATA%\Skim\skim.exe`.
6. Print the installed version from the exe file properties.
- Done when: the script builds and installs over the current app, the inbox
  still shows 28k+ messages, and the account still syncs.

**0.6 Visual check harness.** Extend `demo/mock/data.ts` and `demo/mock/tauri-core.ts`
with every new command (returning deterministic fixtures). Add
`demo/fork-shots.mjs` that opens `npm run demo:dev` (port 1421) with Playwright
(already a devDependency) at 1440x900 in each of the 4 themes and saves PNGs to
`docs/fork/shots/<phase>/`. Every UI step below lists the states it must shoot.
- Done when: the harness produces a shot of today's inbox in all 4 themes.

### Phase 1: Correctness fixes (before any UI work)

**1.1 Gmail detection by host (B1).** Add `fn is_gmail(account)`:
`provider == "gmail" || imap_host.eq_ignore_ascii_case("imap.gmail.com")`. Use it at
`sync.rs:1954` (archive) and `sync.rs:2357` (Sent mirror) and
`commands/invites.rs:199` (web calendar URL). Onboarding: when MX lookup
(`mail/dns.rs`) shows Google (`aspmx.l.google.com`, `*.googlemail.com`, `*.google.com`),
set `provider = "gmail"`. Fork migration `f0001_gmail_provider.sql`:
`UPDATE accounts SET provider='gmail' WHERE lower(imap_host)='imap.gmail.com' AND provider='custom';`
- Tests: archive op on a gmail-host custom account takes the expunge path; Sent
  mirror skipped.
- Done when: his account row reads `gmail`, pressing E on a test thread removes it
  from INBOX on Gmail web and no "Archive" label appears.

**1.2 Gmail archive from labels (B3).** For Gmail, archiving from any label folder
(role NULL) = expunge from that folder (removes the label), never a move to
"Archive". From INBOX unchanged. From Sent/Trash/Spam: archive is not offered
(hide the E action in those roles).
- Done when: unit test covers INBOX, label, and sent cases.

**1.3 Important is not Starred (B2).** `detect_role`: map `important` to a new role
`important` (only `starred` maps to `starred`; also match `\Flagged` attr as today).
Fork migration `f0002_important_role.sql`:
`UPDATE folders SET role='important' WHERE role='starred' AND lower(imap_name) LIKE '%/important';`
Sidebar hides `important` (like `all`). Unified folder id: add `Some("important") => -9`
in `virtual_folder_id` (`queries.rs:338`).
- Done when: only `[Gmail]/Starred` has role `starred`.

**1.4 Starred in the sidebar.** Show the `starred` role folder in `mainFolders`
(`Sidebar.svelte:22-24`) with a star icon (`folders.ts` `folderIcon`) and its
**total** count, not unread (Mimestream rule). Keep it out of move targets (already
excluded, `commands/mail.rs:843`).
- Shots: sidebar with Starred.

### Phase 2: Wave 1 visuals (unread, star, contrast, density, zoom, hover, advance)

**2.0 Mock first, then wire.** Patrick picks UI from real mocks. Build
`docs/fork/mocks/list-states.html` (static, self-contained) showing 3 variants of
the message list using 12 real-shaped rows (fixtures only, no real mail content):
- **A (recommended):** fixed 20px left gutter; unread = 8px blue dot
  (`--unread`) + bold sender and subject; read rows sender+subject at `--text-dim`,
  snippet at `--text-faint` (raised, see 2.1); starred = filled amber star in the
  gutter below the dot; hover shows a hollow star.
- **B:** A plus a 3px blue left bar on unread rows and a faint row tint.
- **C:** A plus the unread subject in blue (Outlook style) instead of the bar.
Render all 4 themes side by side. Default build ships **A**; B/C are one token
swap each (document it). Save the mock and its shots in `docs/fork/mocks/`.

**2.1 Tokens.** `tokens.css`, all four theme blocks:
- Raise `--text-faint` so it clears 4.5:1 on both `--bg` and `--surface`. Keep the
  hue family; compute with a script. Add `scripts/fork/contrast.mjs` that parses
  `tokens.css` and asserts: `--text` >= 7, `--text-dim` >= 4.5, `--text-faint` >= 4.5,
  `--unread` and `--star` >= 3 (non-text, WCAG 1.4.11) against `--bg`, `--surface`,
  `--selected`, `--hover`-over-bg. Wire it into the gates.
- New: `--unread` (blue: light `#1f6fd1`-ish, dark `#6aa6ff`-ish, tune to pass),
  `--star` (amber: light `#b7791f`-ish, dark `#f2b544`-ish), `--focus` (2px ring colour,
  3:1 minimum), `--row-unread-tint` (for variant B).
- Done when: `node scripts/fork/contrast.mjs` exits 0 for all themes.

**2.2 Row redesign.** `MessageRow.svelte`:
- Grid: `[gutter 20px][content]`. Gutter holds the unread dot (click toggles read:
  `api.markRead`, stopPropagation) and the star (click toggles star). Both are
  real buttons with `aria-label` and `aria-pressed`.
- Keep the checkbox slot logic (`MessageRow.svelte:102-125`) intact.
- Unread: sender + subject bold; read: sender + subject `--text-dim`, weight 400.
- Accessible name for the row button: "Unread, starred, from X, subject, N messages,
  has attachment, date" (`aria-label`), so state is not colour-only (WCAG 1.4.1).
- Attachment glyph (12px paperclip) before the date when `hasAttachments`.
- Shots: row states (unread, read, starred, both, selected, ticked, hover) x 4 themes.

**2.3 Focus and cursor.** `base.css`: global `:focus-visible { outline: 2px solid var(--focus); outline-offset: 2px }`
(remove only for elements that draw their own). Make the keyboard cursor row
(`selected`) distinct from hover: selected keeps `--selected` plus a 2px left bar in `--text`.

**2.4 Hover actions.** On row hover (and when the row is the keyboard cursor),
show 3 icon buttons over the right end of line 1, replacing the date: Archive (E),
Star (S), Mark read/unread (U). Each shows its key in the tooltip
(`title="Archive  E"`). They call the same functions as the keys (see 3.2 shared
action module).

**2.5 Auto-advance.** After archive/delete/spam/move of the open thread, select the
next row below (or the one above if it was last), matching Superhuman. Setting
`fork_after_archive`: `next` (default) | `previous` | `list`. Implement once in the
shared action module (3.2), not per caller.

**2.6 Density and avatars.** Settings (Rust `ALLOWED` + Settings.svelte toggles):
- `fork_density`: `comfortable` (today's 3 lines) | `compact` (one line:
  sender, subject, snippet in `--text-faint`, date). Compact row height ~36px.
- `fork_avatars`: on/off. Avatar = 28px initials disc, colour from a stable hash of
  the sender address over a 6-colour palette with no violet.
- On density change, reset `rowH` in `MessageList.svelte` to a new baseline
  (keyed effect), never switch to two-way measuring (freeze risk, `:66-79`).
- Shots: both densities x avatars on/off.

**2.7 Zoom.** `fork_zoom` setting (0.8-1.5, default 1.0). Apply with
`getCurrentWebview().setZoom(v)` from `@tauri-apps/api/webview`; add permission
`core:webview:allow-set-webview-zoom` to `capabilities/default.json`. Keys:
Ctrl+= / Ctrl+- / Ctrl+0 handled before the Ctrl guard in `App.svelte:onKeydown`
(like Ctrl+A). Apply to compose and chat windows too (read the setting at their
root). Settings slider with the current %.

### Phase 3: Triage (filters, undo, keyboard)

**3.1 Filter chips + unread first.** List header (`MessageList.svelte:113-129`):
chips `All · Unread · Starred`. The unread count microlabel becomes the Unread chip
(click filters). Implementation: add `filter: 'all'|'unread'|'starred'` and
`order: 'date'|'unread_first'` to `list_threads`, `list_messages`,
`list_unified_threads`, `list_unified_messages` (commands + `queries.rs`):
- unread (grouped): `AND EXISTS (SELECT 1 FROM messages m3 WHERE m3.thread_id=t.id AND m3.folder_id=?1 AND m3.is_read=0)`
- starred (grouped): `AND t.starred = 1`; flat: `m.is_read=0` / `m.is_starred=1`
- unread_first: `ORDER BY (is_read = 0) DESC, date DESC` (grouped uses the computed unread column)
- Keep `list_threads_seeks_the_thread_index` green; add a plan test per new variant.
- Store: `mail.svelte.ts` state `listFilter`, `listOrder`; `fetchPage` passes them;
  reset scroll + paging on change; a row that stops matching (read while filtered
  to unread) stays until the next refresh (no jump under the cursor).
- Keys: `Shift+U` toggles Unread filter, `Shift+S` Starred. Persist `fork_list_order`.
- Shots: each chip state.

**3.2 One action module + Undo.** New `src/fork/actions.ts`: the only place that
archives, deletes, spams, moves, stars, marks read. `App.svelte:239-275`,
`ReadingPane.svelte:382-433`, `bulk.ts:32-55` and `FolderPicker` call it (upstream
touches, logged). It records an undo entry for every action:
- **Flags (star, read):** undo = the inverse op (`setStarred`/`markRead` with the old value). Always available.
- **Removals (archive, delete, spam, move):** two layers:
  1. **Grace window, 8s:** the row leaves the list at once (optimistic), but the
     api call is held client-side for 8s. `z` / Ctrl+Z / the toast's Undo within
     the window cancels the timer and restores the row snapshot at its old index.
     The store keeps a `pendingRemoval` set of row keys and filters them out of
     every `shown()` / `loadMoreThreads` result so a refresh cannot resurrect them.
     If the window closes or the app quits before 8s, the call is simply never
     made (safe failure: the mail stays where it was). On window `beforeunload`
     flush all pending removals immediately (they were intended).
  2. **After the grace window:** Rust command `fork_restore(message_ids_snapshot)`.
     The snapshot (taken before removal: account, source folder imap name,
     RFC822 Message-IDs) is kept in the undo stack for the session. Op kind
     `fork_restore` executed in `execute_op` (upstream touch: one match arm that
     delegates to `fork::restore::execute`): for Gmail archive, SELECT
     `[Gmail]/All Mail` (role `all`), `UID SEARCH HEADER Message-ID <id>`, then
     `UID COPY` to the source folder (Gmail adds the label back); for moves/trash,
     SELECT the destination, search by Message-ID, `UID MOVE` back. Then resync the
     source folder. Undo stack depth 20 per session.
- **Toast:** new `src/fork/Toast.svelte` bottom-centre, one at a time,
  `aria-live="polite"`, "Archived · Undo (Z)", stays 8s, pauses on hover,
  no auto-dismiss while focused (Material rule).
- Remove the "Skim has no undo" rationale comments where the behaviour changes.
- Tests: Rust restore op with a scripted IMAP server (pattern exists:
  `sync.rs` `scripted_server`); TS unit for the pendingRemoval filter.
- Shots: toast.

**3.3 Keyboard additions.** In `App.svelte:onKeydown` (via a fork key map module
called first):
- `z` undo; `Ctrl+Z` undo when not typing.
- Go-to sequences: `g` then `i` inbox, `s` starred, `t` sent, `d` drafts, `a`
  archive/all-mail view if present, `c` calendar (Phase 7), `o` "On me" (Phase 10),
  `w` "Waiting" (Phase 10). 1s window; show a small hint listing valid next keys.
- `Shift+U`, `Shift+S` (3.1), `i` toggle CRM sidebar (Phase 9), `m` Meet now (Phase 7).
- Every new key goes into `ShortcutsOverlay.svelte` groups and into the command
  palette rows with their `hint`.
- Palette teaches keys: every command row already has `hint`; add hints to all
  new commands and make hover tooltips on toolbar buttons show "Label  Key".

### Phase 4: Search operators

**4.1 Parser.** `src-tauri/src/fork/search_query.rs`:
`parse(input) -> ParsedQuery { text: String, filters: Filters }` supporting
`from:`, `to:`, `cc:`, `subject:`, `is:unread|read|starred|unstarred`,
`has:attachment`, `in:inbox|sent|drafts|trash|spam|starred|<label name>`,
`before:YYYY-MM-DD`, `after:YYYY-MM-DD`, `older_than:Nd|Nw|Nm|Ny`,
`newer_than:…`, quoted values (`from:"Jane Doe"`), `-word` negation for free
text. Unknown `x:y` tokens stay free text. Tests: a table of 30+ inputs.

**4.2 One filter builder.** Extract the SQL clause building from
`ai/agent.rs:842-883` into `fork::search_query::filter_sql(&Filters) -> (String, Vec<SqlValue>)`,
extended with to/cc (JSON `LIKE` on `to_addrs`/`cc_addrs`), in:label, negation.
`agent.rs` calls it (upstream touch). `search.rs::search_messages` parses the
input and uses it, so the palette gets operators. FTS stays the text engine
(`build_fts_query`).

**4.3 Search results in the list.** Pressing Enter in the palette with no row
highlighted (or `Shift+Enter`) shows the results in the message list as a virtual
folder "Search: <query>" (id `-900`), grouped by thread, with the parsed filters
shown as removable chips in the list header. Esc returns to the previous folder.
New command `fork_search_threads(query, offset, limit)`.
- Shots: palette with operator chips, list in search mode.

### Phase 5: Reading (quote and signature folding)

**5.1 HTML markers survive sanitising.** `sanitize.rs`:
- `add_tag_attributes("div", ["class","id"])`, `("blockquote", ["class","type"])`.
- `attribute_filter`: `class` on div keeps only a rewrite:
  `gmail_quote|gmail_quote_container|yahoo_quoted|moz-cite-prefix|protonmail_quote` → `skim-quote`;
  `gmail_signature|moz-signature` → `skim-sig`; anything else dropped.
  `id` kept only if exactly `divRplyFwdMsg` or `appendonsend` (Outlook reply
  header), otherwise dropped. `type` on blockquote kept only if `cite`.
- A test per client marker, plus a test that arbitrary classes/ids never survive.

**5.2 Plain text.** `text_to_html` (`sanitize.rs:256`): find the fold point with the
same rules as `strip_quoted` (attribution line, `-----Original Message-----`,
`-- ` signature, a run of `>` lines) and wrap everything from there in
`<div class="skim-quote">`. If folding would leave nothing visible above it
(bottom-posted or forward), do not fold.

**5.3 Fold in the viewer.** `RenderedBody` gains `has_fold: bool` (Rust struct +
`types.ts`; computed after sanitising: any marker present and non-empty content
above it). `HtmlViewer` takes `folded` prop; `buildDoc` adds, when folded:
`.skim-quote, .skim-sig, #divRplyFwdMsg, #divRplyFwdMsg ~ *, #appendonsend ~ *, blockquote[type=cite] { display:none }`
(top-level cite only: `body > blockquote[type=cite], body > div > blockquote[type=cite]`).
ReadingPane shows a "•••" pill under the iframe when `has_fold`; click toggles.
Default folded for every message except when the thread has one message and the
whole body is a forward. Height re-measures on its own (ResizeObserver).
- Shots: folded and unfolded, Gmail-style and Outlook-style fixtures.

### Phase 6: Compose (inline reply, rich text, send safety)

**6.1 Ctrl+Enter sends; ✕ keeps the draft.** `ComposeForm.svelte`: Ctrl/Cmd+Enter
in the body or subject calls `send()`. Window ✕ with edits saves to Drafts
(`save()`), without edits deletes the empty local draft. Add an explicit Discard
(trash icon) to the window bar with confirm-on-click-twice.

**6.2 Inline reply.** ReadingPane: `R`/`A`/`F` and the footer buttons open the
composer **inside the thread**, below the focused message
(`ReadingPane.svelte:559-563` area), using `ComposeForm` with `chrome=false` and a
new prop `variant="reply"` (hides From picker, shows a pop-out button that calls
`openComposeWindow` and unmounts the inline copy). Scroll it into view, focus the
body. Esc with no edits closes it; with edits keeps it as a draft (existing
`flushServer`). `Ctrl+N` still opens the window. Setting `fork_reply_inline`
(default on).
- Guard: while the inline composer is focused, list shortcuts do nothing
  (`isTyping()` already covers it); J/K move only when focus is outside.
- Shots: inline reply open, with AI bar.

**6.3 Rich text.** Editor: **Squire** (`squire-rte` 2.4.9, MIT, Fastmail's own
email editor, verified on npm 2026-09-23). Wrapped in `src/fork/RichEditor.svelte`
(contenteditable host, toolbar: bold, italic, underline, link, bulleted list,
numbered list, quote, clear formatting; keys Ctrl+B/I/U/K, Ctrl+Shift+7/8).
Data model, chosen to keep every upstream text path working:
- `drafts.body_text` stays the **full plain-text** body (words + signature +
  quoted original) exactly as today, regenerated from the editor on each save.
  Upstream code that reads it (AI `splitTail`, server draft save, send) keeps working.
- New `fork_draft_html(draft_id PK REFERENCES drafts ON DELETE CASCADE, words_html TEXT, updated_at)`
  holds only the user's own words as HTML. The signature and the quote are not
  in the editor: they render below it read-only (quote collapsed behind "•••",
  expandable, editable only after "Edit quoted text"), Superhuman-style.
- AI co-author: `runCompose` writes plain text; the editor converts it to
  paragraphs (`<p>` per blank-line block, `<br>` per line). `splitTail` keeps
  operating on `body_text`.
- Send/save: `smtp::build_message` (upstream touch: take an optional html) builds
  `multipart/alternative[text/plain, text/html]` (wrapped in `multipart/mixed`
  with attachments), following `build_calendar_reply` (`smtp.rs:150-178`). The
  HTML part = sanitized `words_html` + `<div class="skim-sig">-- <br>sig</div>` +
  `<div class="gmail_quote">attribution<blockquote type="cite">original sanitized html</blockquote></div>`.
  Composer HTML is sanitized with the same ammonia allowlist before sending.
  When there is no `fork_draft_html` row the message is text/plain as today.
- Paste: HTML paste is sanitized (Squire hook), images pasted still become
  attachments (existing `onPaste`).
- Setting `fork_rich_text` (default on).
- Tests: Rust MIME build produces valid alternative parts, and the text part equals
  `body_text`; round-trip with lettre parse.

**6.4 Undo send and send later (one scheduler).** Fork migration
`f0003_op_schedule.sql`:
`fork_op_schedule(op_id INTEGER PRIMARY KEY REFERENCES pending_ops(id) ON DELETE CASCADE, not_before INTEGER NOT NULL, kind TEXT NOT NULL, label TEXT)`.
- `sync.rs::drain_ops` query (upstream touch) adds
  `AND id NOT IN (SELECT op_id FROM fork_op_schedule WHERE not_before > unixepoch())`.
  Held ops no longer block the ops behind them.
- `fork::scheduler` task: sleeps until the earliest `not_before`, then calls
  `run_ops()` on that account's engine; re-arms on every schedule change; on
  startup re-arms from the table (sends survive a restart while the app runs in
  the tray; state this in the UI).
- **Undo send:** `send()` enqueues with `not_before = now + fork_undo_send_secs`
  (setting 0/5/10/20/30, default 10). Toast "Sending… Undo". Undo =
  `fork_cancel_scheduled(op_id)`: deletes the op and schedule row **only if**
  `not_before - now >= 1` (1s margin; the drain never picks an op before
  `not_before`, so there is no overlap), then reopens the draft (inline or window).
- **Send later:** a split Send button menu: "Tomorrow 08:00", "Monday 08:00",
  "In 2 hours", "Pick…" (date + time, and a free-text parser: `3d`, `tomorrow 9am`,
  `fri 14:00`). Same op, `not_before` = chosen time. A "Scheduled" virtual folder
  (id `-910`) lists pending scheduled sends (join schedule + drafts) with
  Edit (cancel + reopen) and Send now (set `not_before = now`).
- Tests: drain skips held ops but processes the ones behind; cancel refused inside
  the margin; scheduler re-arms after restart.

### Phase 6.5: AI-smell underlines in the composer ("humanizer")

**Why not an OSS humanizer:** measured in the OS estate (memory
`reference_de_ai_measurement_2026_09_17`, `reference_anti_slop_skills_survey`): the
public humanizers and detectors score within 0.45 points of each other and rated
the two artefacts real buyers called AI-generated as human. The rules that do
separate them already live in OS: `tools/outreach/slop_scrub.py` (113 regex rules,
hard/warn/info tiers, each with a suggestion) and `tools/no-smell/nosmell/structural.py`
(contrast-frame repetition, disclaimers/1k, positions/1k). Reuse them; do not
install another tool.

**Gaps measured 2026-09-23** on a sample draft: `slop_scrub` returns no character
offsets (only `matched` text), and misses "I wanted to reach out", "Let me know if
you have any questions", "not just X, it is Y". `no-smell` caught the repeated
"rather than" (40/1k vs 6) but is document-level.

**6.5.1 One rule source, exported (edit in OS, trunk).** In
`C:\Users\Patrick\OS\tools\outreach\slop_scrub.py` add the three missing rules
(warn tier) with recall rows in `test_slop_scrub.py`, and a `--export-rules`
flag that writes JSON: `[{category, severity, pattern, flags, suggestion, min_hits}]`
plus the no-smell phrase lists (contrast frames, disclaimer/hedge phrases, stance
phrases) and thresholds from `nosmell/structural.py`. Keep both suites green.
Vendor the export into Skim at `src/fork/smell/rules.json` with
`scripts/fork/sync-smell-rules.ps1` (re-runnable; the JSON records the OS commit it
came from). OS stays the canonical home; Skim holds a copy.

**6.5.2 Scanner in the webview (TS).** `src/fork/smell/scan.ts`: compiles every rule
as a JS `RegExp` (V8 supports the lookbehinds `slop_scrub` uses; Rust `regex` does
not, which is why this runs in TS). Returns spans `{start, end, category, severity,
suggestion}` over the editor's plain text, applies `min_hits` density gating, skips
the quoted original and the signature (only the user's own words are scanned),
and adds the document-level checks: a contrast phrase repeated past threshold
underlines every occurrence; disclaimers/1k and positions/1k feed the health line.
Patrick's own hard rule is added as `hard`: em dash and en dash in outbound mail.
- Parity test: a node script runs the TS scanner and `slop_scrub.py --json` over the
  same 30-sample corpus (include `tools/outreach` test fixtures) and asserts the same
  findings. A pattern that fails to compile in JS fails the build.

**6.5.3 Underlines.** In the Squire editor (6.3) use the CSS Custom Highlight API
(`CSS.highlights`, `Highlight`, `Range`; WebView2 supports it) so the DOM is never
mutated: `::highlight(smell-hard)` red wavy underline, `smell-warn` amber dotted,
`smell-info` off by default. Re-scan debounced 300ms after typing. Plain-text mode
(rich text off) uses a mirrored overlay behind the textarea.
- A one-line **health bar** under the editor: "3 AI tells · 'rather than' ×2 ·
  no clear position" (only what fired), click to jump to the first.

**6.5.4 Suggestions popover.** Clicking (or `Alt+Enter` on) an underline opens a
popover: the rule's reason, its static suggestion, and actions:
- **Ignore** (this occurrence), **Ignore rule** (setting, reversible in Settings).
- **✦ Rewrite** (violet, AI, needs a key): sends the sentence plus one sentence of
  context either side, the rule reason, Patrick's writer profile (`ai_style_profile`
  from `ai_analyze_style`) and the house drafting rules (write the position first,
  no hedges, no contrast frames, no em dashes, plain words, keep it short) to the
  configured model via a new one-shot command `fork_smell_rewrite` (reuses
  `ai_context` + `anthropic::stream` / `openai_compat::stream`). Returns 3 options.
  **Fact guard:** reject any option that drops or changes a number, date, amount,
  URL, email address or capitalised name present in the original sentence; show
  only options that pass. Accept replaces the sentence as one undoable edit.
- **✦ Clean whole draft**: same, per flagged sentence, shown as a before/after diff
  to accept per sentence. Never applied without Patrick clicking accept.

**6.5.5 Send check.** On Send (and Send later): `hard` findings (em/en dash, invisible
characters, unfilled `{{placeholder}}`, homoglyphs) open a one-line prompt "2 must-fix
items" with Fix / Send anyway; `warn` findings do not block. Setting `fork_smell`
(on/off) and `fork_smell_block_hard` (default on).
- Tests: scanner spans are exact on fixtures; quote/signature excluded; fact guard
  rejects a rewrite that changes "R140,000" or "Tuesday 14:00".
- Shots: underlines in all 4 themes, popover with 3 options, health bar, send prompt.

### Phase 7: Google connection, Calendar, Meet

**Human step (Patrick, ~10 minutes, one time).** In Google Cloud (project owned by
the autospark.ai Workspace): enable Google Calendar API and Google Meet REST API;
OAuth consent screen **Internal**; create an OAuth client of type **Desktop app**.
Paste the client ID and secret into Skim Settings → Calendar (7.1). Internal apps
skip Google verification and weekly token expiry. The build must work, and show
clear setup text, before this is done.

**7.1 Runtime client config.** `fork::google::client_config()`: reads
`fork:google_client_id` / `fork:google_client_secret` from Credential Manager,
falling back to the compile-time `SKIM_GOOGLE_CLIENT_ID/SECRET`. Settings →
Calendar has two fields + Save (secret masked).

**7.2 Scoped OAuth.** Upstream touch in `oauth.rs`: `authorize` and
`build_auth_url` take `scopes: &str` and `required_scope: Option<&str>` (the mail
flow passes today's values, so behaviour is unchanged); `resolve_email` checks
`required_scope` instead of the hard-coded mail scope. Fork flow requests
`openid email https://www.googleapis.com/auth/calendar.events https://www.googleapis.com/auth/calendar.calendarlist.readonly https://www.googleapis.com/auth/meetings.space.created`.
Refresh token stored at `fork:gcal:{account_id}`. `auth_kind` of the mail account
is untouched. Userinfo email must equal the account email; if not, show which
account was used and refuse.
- Token cache + refresh like `resolve_credentials` (`sync.rs:386-420`), own mutex.

**7.3 Calendar store.** Fork migration `f0004_calendar.sql`:
- `fork_cal_calendars(id PK, account_id, google_id, summary, color, is_primary, selected, access_role, UNIQUE(account_id, google_id))`
- `fork_cal_events(id PK, calendar_id REFERENCES fork_cal_calendars ON DELETE CASCADE, google_id, etag, status, summary, description, location, start_ts, end_ts, all_day, start_date, end_date, time_zone, recurring_event_id, organizer_email, attendees_json, self_response, transparency, hangout_link, html_link, updated, local_only INTEGER DEFAULT 0, UNIQUE(calendar_id, google_id))`, index on `(start_ts)`.
- `fork_cal_ops(id PK, account_id, kind, payload, created_at, attempts, state)`.

**7.4 Calendar sync engine.** `fork::calendar::spawn` per connected account,
modelled on `sync.rs::spawn` (mpsc commands + interval):
- Calendar list: `GET /calendar/v3/users/me/calendarList`.
- Events for each selected calendar: `GET /calendar/v3/calendars/{id}/events?singleEvents=true&orderBy=startTime&timeMin=now-60d&timeMax=now+180d&maxResults=2500`
  (page through `nextPageToken`). Replace the window in one transaction (keep
  `local_only` rows until their op lands).
- Every 5 minutes, on window focus, and right after each op drains. Emit
  `calendar:updated`.
- Ops: `create` (`POST events?conferenceDataVersion=1&sendUpdates=…`), `patch`,
  `delete`, `rsvp` (patch the self attendee's `responseStatus`). Offline-first:
  local row first (`local_only=1` for creates), op queued, retries like `drain_ops`
  (network errors stop the drain; 5 attempts then `failed` + `calendar:ops_failed`).
- `sendUpdates` is never implicit: when an event has attendees other than Patrick,
  the editor asks "Send invites/updates to guests?" and passes `all` or `none`.
- Tests: JSON → row mapping (timed, all-day, cancelled, recurring instance), op
  payload builders, window replace keeps local-only rows.

**7.5 Calendar screen.** Dependency `@event-calendar/core` 5.14.1 (MIT, Svelte 5,
verified on npm 2026-09-23). Read its README in `node_modules` before coding: the
v5 API is `createCalendar(el, plugins, options)` / the Svelte component; use the
TimeGrid, DayGrid, List and Interaction plugins.
- `ui.svelte.ts`: `view: "mail" | "calendar"`. Sidebar item "Calendar" above the
  folders; `g c`. In calendar view, `App.svelte` renders `src/fork/CalendarView.svelte`
  in place of MessageList + ReadingPane (upstream touch at `App.svelte:473-500`).
- Views: Day, Week (default), Month, Agenda; keys `d w m a`, `t` today, `j/k` next/prev period, `n` new event.
- Theme: map EventCalendar CSS variables to Skim tokens in all 4 themes; event
  colour = calendar colour muted; declined events struck through; tentative dashed.
- Drag to move/resize = `patch` op (asks about guests if any). Click empty slot =
  quick create.
- Event panel (right, 360px): title, date, start/end or all-day, calendar picker,
  guests (reuse `AddressInput.svelte`), location, description, "Add Google Meet"
  toggle (`conferenceData.createRequest` with `hangoutsMeet`), RSVP buttons when
  Patrick is a guest, Join button when `hangout_link`, Delete (confirm twice).
- Titlebar next-event chip: "14:00 Call with X · in 25m · Join" when an event
  starts within 60 minutes (hidden otherwise).
- Invite cards (`InviteCard.svelte`): when the invite UID matches a synced event,
  show "In your calendar" and any overlapping events as a conflict line.
- Shots: week, month, agenda, event panel, quick create, next-event chip, 4 themes.

**7.6 Meet.** Titlebar "Meet now" button (video icon, key `m`, palette row):
- Without Google connected: open `https://meet.new` in the browser (opener plugin).
- Connected: `POST https://meet.googleapis.com/v2/spaces` → `meetingUri`; copy to
  clipboard (`navigator.clipboard.writeText`, inside the click gesture), open it in
  the browser, toast "Meet link copied".
- Compose toolbar "Add Meet link" inserts a fresh link at the cursor.
- Event panel toggle (7.5).

**7.7 Settings → Calendar.** Client ID/secret (7.1), Connect/Disconnect per account,
calendars shown (checkbox list), default event length (30), working hours
(start, end, days) and a second time zone to display, booking link (free text).

### Phase 8: Share availability (`/slots`)

`fork::availability::free_slots(account, from, days, duration, working_hours, tz)`:
busy = events on selected calendars where `status != cancelled`, `self_response != declined`,
`transparency != transparent`; merge busy intervals; walk working hours in the
chosen zone; return slots on the duration grid (30-minute steps), max 3 per day,
next 5 working days, never in the past, 15-minute buffer after "now".
- Composer: typing `/slots` at a line start, or the toolbar button, opens a popover:
  duration (30/45/60), days (3/5/10), time zone for the recipient (defaults to the
  second zone from settings), preview; Insert writes a plain list
  ("Tue 24 Sep: 10:00, 14:30 SAST (09:00, 13:30 BST)") plus the booking link if set.
- Tests: overlapping events, all-day busy, declined ignored, DST boundary, zone conversion.

### Phase 9: CRM sidebar (Rebound)

- Settings → CRM: Base URL, Supabase URL and anon key are prefilled from build-time
  defaults (`SKIM_REBOUND_BASE_URL`, `SKIM_REBOUND_SUPABASE_URL`,
  `SKIM_REBOUND_SUPABASE_ANON_KEY` via `option_env!`; `build-install.ps1` copies them
  from `C:\Users\Patrick\OS\.env` keys `REBOUND_API_BASE_URL`,
  `REBOUND_SUPABASE_URL`, `REBOUND_SUPABASE_ANON_KEY` into the build environment,
  never into the repo). Patrick enters only his Rebound email + password → Connect.
  Rust: `POST {supabase}/auth/v1/token?grant_type=password` with `apikey` header;
  store the refresh token at `fork:rebound`; refresh with `grant_type=refresh_token`.
  Workspace: `GET {base}/api/v1/workspaces`; pick the only one or show a picker; save id.
- `fork_crm_lookup(email)`: `POST {base}/api/v1/extension/lookup` with
  `Authorization: Bearer <jwt>`, `X-Workspace-ID`, body `{email}`; 10-minute
  in-memory cache; typed struct mirroring the route's response
  (read `route.ts` for the exact fields before writing the struct).
- UI: right drawer in the reading pane (`i` toggles, remembered): person, company,
  open deals with stage, last 5 activities with dates, reminders if present,
  "Open in Rebound" link. Empty state: "Not in Rebound" + "Add in Rebound" link.
  Read-only; Skim never writes to the CRM.
- Shots: found, not found, not connected.

### Phase 10: Ball in my court

- Fork migration `f0005_court.sql`:
  `fork_court(thread_id PK, account_id, state TEXT CHECK(state IN ('on_me','waiting','none')), since INTEGER, last_message_id INTEGER, needs_reply INTEGER, reason TEXT, model TEXT, computed_at INTEGER)`.
- Deterministic pass (`fork::court::compute`), on every `mail:updated` for touched
  threads and a full pass at startup: last message of the thread by date
  (deduped by Message-ID, like `get_thread`); from Patrick's address → `waiting`
  since that date; from someone else → `on_me`, unless bulk: `list_unsubscribe`
  present, sender matches `no-?reply|notifications?|mailer-daemon|bounce`, or
  thread only in folders he filed as FYI/marketing. An unsent local draft on the
  thread marks `on_me` with reason "draft started".
- AI pass (optional, BYOK, uses `ai_context` + one-shot `anthropic::stream` /
  `openai_compat::stream` collecting text): only for `on_me` threads whose
  `last_message_id` changed; batches of 20; strict JSON
  `[{thread_id, needs_reply, reason}]`; parse fails → keep deterministic result.
  Never re-classify an unchanged thread. Cost-cap setting (threads/day, default 200).
- Views: sidebar "On me (N)" and "Waiting (N)" (virtual folders `-920`, `-921`),
  sorted oldest first; row shows age ("3d") in amber past 2 days, red past 5
  (settings). Keys `g o`, `g w`.
- Nudges: a daily toast at 09:00 local (setting) "On you: N · Oldest: X (5d)";
  click opens "On me". Uses the existing toast machinery (`notify.rs`).
- Tests: last-message logic, bulk exclusion, draft rule, JSON parse fallback.
- Shots: both views with ages.

### Phase 11: Meeting prep

- In the event panel and as a toast 10 minutes before any event with external
  guests: "Prep".
- Prep panel: per external guest, the Rebound card (Phase 9) and their last 5
  threads (search `from:` / `to:` via the Phase 4 builder, last 90 days); a
  "✦ Brief me" button (AI, violet) streams a short brief over those threads + CRM
  context via `spawn_stream`.
- Tests: guest extraction (exclude own addresses), thread query.

### Phase 12: MCP server

- Crate `rmcp` (3.4.0 on crates.io 2026-09-23); before coding, read its docs for the
  Streamable HTTP server feature names and confirm it builds on Windows with the
  project's `rustls`/`ring` choice; if it drags a second TLS stack, use its
  server-only features (the server binds plain HTTP on loopback).
- Bind `127.0.0.1:8342` only (checked free; register it in
  `C:\Users\Patrick\OS\meta\PORTS.md` per OS HR15). Bearer token: 32 random bytes,
  stored at `fork:mcp_token`; required on every request. Setting `fork_mcp` on/off
  (default on for this build). Refuse non-loopback peers.
- Tools (all through the same code paths as the UI):
  - `search_mail(query, limit)`: Phase 4 operators; returns thread ids, subject, from, date, snippet.
  - `get_thread(thread_id)`: messages with plain-text bodies (quotes stripped option).
  - `list_unread(folder?, limit)`, `list_court(state)`.
  - `get_calendar(from, to)`, `find_free_slots(duration, days, tz)`.
  - `crm_lookup(email)`.
  - `create_draft(to, cc?, subject, body, reply_to_message_id?)`: local draft + save to Drafts; returns draft id. **Never sends.**
  - `archive(thread_id)`, `star(thread_id, on)`, `mark_read(thread_id, on)`: via `queue_op`, reversible.
  - `create_event(title, start, end, description?)`: **own calendar only, no
    attendees** (an attendee would send an invite).
  - There is no send tool and no invite tool, by design (rule 8).
- Register with Claude Code (user scope), run by the builder:
  `claude mcp add --transport http skim http://127.0.0.1:8342/mcp --header "Authorization: Bearer <token>" --scope user`.
- Tests: auth refused without token; each tool against an in-memory DB.

### Phase 13: Release, install, verify

1. Update `docs/fork/TOUCHLIST.md`, `docs/fork/DECISIONS.md`, and a short
   `docs/fork/README.md` (what the fork adds, how to build, how to merge upstream:
   `git fetch upstream && git merge upstream/main` on `paddy`, resolve using the touch list).
2. All gates green; contrast script green; shots regenerated for every phase into
   `docs/fork/shots/`.
3. `scripts/fork/build-install.ps1` → installed build v1.1.0.
4. Live smoke test on his real app (read-only checks except where noted):
   inbox renders; unread dot + amber star visible; filter chips work; J/K, E then Z
   restores the thread; palette `from:` search works; a thread with quotes folds;
   inline reply opens; Ctrl+Enter present; zoom keys work; Calendar screen shows
   the "connect" state (or events if Patrick has done the Google step); Meet now
   opens meet.new; MCP `search_mail` answers from Claude Code.
   **Do not send any email and do not create or modify any event with guests
   during the smoke test.**
5. Push `paddy` to `origin`; tag `v1.1.0` on `paddy` → release workflow builds the
   signed installer + `latest.json` on the fork.

---

## 3. Decisions (and why)

| # | Decision | Why |
|---|---|---|
| D1 | Fork state in `fork_*` tables + own version key | Upstream's single `user_version` sequence would collide with any fork 0016 |
| D2 | Unread = blue dot + bold; read dimmed; star = amber in a fixed gutter | Two signals, not colour-only (WCAG 1.4.1); no client users praise weight alone; violet is reserved for AI |
| D3 | Undo = 8s client-side hold, then server restore by Message-ID | Local rows are deleted on archive (`bodies.rs:464`), so a pure local undo is impossible after the op runs; the hold covers the common mistake with zero server work |
| D4 | Held sends excluded from the drain, 1s cancel margin | The drain is strict FIFO; a held op must not block others, and the margin removes the cancel-vs-send race without a new op state |
| D5 | Rich text keeps `body_text` as the full text body; HTML only for the user's words | Every upstream path (AI splitTail, Drafts save, send) keeps working unchanged |
| D6 | Squire for the editor | Purpose-built for email by Fastmail, MIT, maintained; no UI kit |
| D7 | EventCalendar for the calendar | MIT, Svelte 5 native, maintained (release 2026-09-22) |
| D8 | Calendar sync = windowed pull (-60d/+180d), not sync tokens | Sync tokens cannot combine with time bounds; a few hundred events re-pulled every 5 min is cheap and simple |
| D9 | Google client ID at runtime (Credential Manager) | Build stays autonomous; Patrick pastes two values once |
| D10 | MCP inside the app on loopback with a bearer token | All writes reuse app logic and the op queue; no second process fighting the DB writer |
| D11 | MCP and every timer can draft, never send | His confirm-before-send rule |
| D12 | Snooze, split inbox, screener, bundles, templates: not in this plan | Not in the requested list; candidates for a v1.2 plan |

## 4. Risks and how they are handled

- **Upstream merge pain:** the touch list + fork-only files; run a trial merge of
  `upstream/main` at the end of Phase 13 and record conflicts in the README.
- **Freeze from row height:** density change resets `rowH`; never two-way measure.
- **Contrast regressions:** the contrast script runs in the gates.
- **Gmail restore search misses** (Message-ID absent): the undo toast says
  "Couldn't restore; find it in All Mail" and the snapshot is logged to
  `skim-fork.log` (ids only, no content).
- **Binary size / deps:** `rmcp`, Squire, EventCalendar add weight; acceptable for
  a personal build; record installer size before/after in the README.
- **Send later needs the app running:** Skim autostarts to the tray; the Scheduled
  view says "Sends while Skim is running".

## 5. Human steps (only these)

1. Google Cloud: enable Calendar + Meet APIs, Internal consent screen, Desktop OAuth client; paste ID + secret into Settings → Calendar; click Connect and approve.
2. Settings → CRM: enter the Rebound email + password once (URLs and key are prefilled).
3. Pick a list variant (A/B/C) from `docs/fork/mocks/list-states.html` if A is not right.
