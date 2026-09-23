# Fork decisions

D1-D12 are copied from `PLAN.md` section 3 so this file is the single log.
Anything decided during the build is appended below with a date.

| # | Decision | Why |
|---|---|---|
| D1 | Fork state in `fork_*` tables + own version key `fork_schema_version` in `settings` | Upstream's single `user_version` sequence would collide with any fork 0016 |
| D2 | Unread = blue dot + bold; read dimmed; star = amber in a fixed gutter | Two signals, not colour-only (WCAG 1.4.1); violet is reserved for AI |
| D3 | Undo = 8s client-side hold, then server restore by Message-ID | Local rows are deleted on archive, so a pure local undo is impossible after the op runs |
| D4 | Held sends excluded from the drain, 1s cancel margin | The drain is strict FIFO; a held op must not block others |
| D5 | Rich text keeps `body_text` as the full text body; HTML only for the user's words | Every upstream path (AI splitTail, Drafts save, send) keeps working unchanged |
| D6 | Squire for the editor | Purpose-built for email by Fastmail, MIT, maintained; no UI kit |
| D7 | EventCalendar for the calendar | MIT, Svelte 5 native, maintained |
| D8 | Calendar sync = windowed pull (-60d/+180d), not sync tokens | Sync tokens cannot combine with time bounds |
| D9 | Google client ID at runtime (Credential Manager) | Build stays autonomous; Patrick pastes two values once |
| D10 | MCP inside the app on loopback with a bearer token | All writes reuse app logic and the op queue |
| D11 | MCP and every timer can draft, never send | Confirm-before-send rule |
| D12 | Snooze, split inbox, screener, bundles, templates: not in this plan | Not requested; v1.2 candidates |

## Build-time decisions
- **2026-09-23 D13 (0.4):** the "latest.json in a draft release" check is done by the tag push at Phase 13 (the release workflow writes it); nothing is pushed before Phase 13 per the build rules. Locally 0.5 asserts the signed `.sig` exists next to the NSIS installer, which is the same signing path.
- **2026-09-23 D14 (0.5):** the backup runs after Skim is closed and before the installer, so `skim.db`, `-wal` and `-shm` are a consistent set. Build first, close second: a failed build never interrupts the running app.
- **2026-09-23 D15 (1.1/1.2):** the archive/Sent-mirror behaviour is tested at the predicate (`fork::gmail::archive_by_expunge`, `is_gmail`), which is exactly what `execute_op` and `mirror_to_sent` now call; driving `execute_op` itself needs a full `Engine` with a scripted IMAP session, which upstream's test scaffold does not build. Gmail archive from a role folder other than INBOX (Starred, All Mail) keeps upstream's move, and the UI hides E in Sent/Trash/Spam.
- **2026-09-23 D16 (1.4):** the Starred total comes from a fork command `fork_role_total(role, account_id?)` rather than `folder_message_count`, because the unified sidebar's Starred is a virtual folder (id -2) with no rows of its own.
- **2026-09-23 D17 (2.1):** raising `--text-faint` to 4.5:1 put it within a few points of `--text-dim`, so `--text-dim` was moved one step further from the background on every theme to keep the three-level hierarchy readable (text > dim > faint). All 96 ratios pass `scripts/fork/contrast.mjs`.
- **2026-09-23 D18 (2.0):** variant B = add `background: var(--row-unread-tint); box-shadow: inset 3px 0 0 var(--unread)` to `.row-wrap.unread .row`; variant C = `.unread .subject { color: var(--unread) }`. Both are documented in `docs/fork/mocks/list-states.html`; the app ships A.
- **2026-09-23 D19 (2.4):** hover actions and the gutter buttons are siblings of the row `<button>` (a button cannot nest a button), positioned absolutely like upstream's checkbox; they appear on hover, at the keyboard cursor, and on focus-within, and hide the date/paperclip underneath so nothing overlaps.
- **2026-09-23 D20 (2.6):** compact density at the list's fixed 372px width gives the snippet 30% at most and the sender 108px; the subject wins the remaining width. Avatars in compact are 24px.
- **2026-09-23 D21 (2.3):** the global focus ring also applies to text inputs (offset 0). Upstream had `outline: none` on inputs and rings in only four components.
- **2026-09-23 D22 (3.2):** a bulk removal and a move are held and undoable too (the plan named archive/delete/spam/move for single threads; the bulk bar and the folder picker were routed through the same `fork/actions` entry so there is one behaviour). The 8 s hold is client-side: the IPC call is simply not made until the window closes, so a quit inside the window loses nothing (the mail stays put) and `beforeunload` fires the held calls because they were intended.
- **2026-09-23 D23 (3.2):** the TS unit test for the pending-row filter runs under `node --test src/fork/tests` (Node 22 type-stripping); no test framework was added. Gate script runs it.
- **2026-09-23 D24 (fan-out):** from 3.2 on, phases were built by parallel agents under `docs/fork/AGENT-RULES.md`; each hands its seam edits (mod.rs, handler list, strings, mocks, shots, touch rows) to the main session through `docs/fork/pending/<phase>.md`, which merges them. Migration slots f0003-f0006 were pre-registered as placeholders so the runner's list is fixed while agents fill the SQL.
- **2026-09-23 D-3.4a:** `send` resyncs Drafts as well as Sent (sending removes the server draft; the local Drafts row lingered until the poll). `rsvp` resyncs Sent. `roles_after_op` in `fork/freshness.rs` is the one table.
- **2026-09-23 D-3.4b:** `mirror_to_sent`'s Gmail branch already looked up the `sent` role after its 1.5 s sleep, so the plan's premise was half right; the sleep stays, the role lookup is now done once in the drain via `folders_after_op`.
- **2026-09-23 D-3.4c:** `fork_sync_folder` accepts any real folder id; the UI restricts to sent/drafts and a 20 s per-folder debounce bounds the cost. A refused request is dropped, not queued.
- **2026-09-23 D-3.4d:** the targeted sync drains the op queue first and skips the folder if the drain already resynced it, so opening Drafts right after a save costs one folder sync.
- **2026-09-23 D-3.4e:** `windowFocused()` reuses the folder from the last `folderSelected()` call rather than importing the mail store (no import cycle); wired as `onfocus` on `svelte:window`.
- **2026-09-23 D-4a (4.3):** `refreshFolders` got a one-line guard so a `folders:updated` never auto-selects the inbox over an open search (the -900 search folder is never in the list).
- **2026-09-23 D-4b (4.1):** `in:starred` is the star flag in any folder; `in:spam` = role junk; `in:anywhere|all|any` lifts the Trash/Spam exclusion; anything else is a label by display name.
- **2026-09-23 D-4c (4.1):** `-word` is an FTS `NOT IN` subquery so it works with or without positive text; `-from:` etc. negate their clause; `-is:unread` reads as `is:read`.
- **2026-09-23 D-4d (4.3):** search results are always grouped by thread (`fork_search_threads`, `ThreadRow` shape, `messageId: null`), shaped by the newest matching message. `older_than:Nm` = 30 days, `Ny` = 365.
- **2026-09-23 D-4e (4.3):** chips are tokens, not a second parser: removing one drops the token and re-runs through Rust.
- **2026-09-23 D-4f (4.3):** palette "no row highlighted" is `active === -1` (ArrowUp from row 0 while a query is typed); Enter then, or Shift+Enter, shows the list.
- **2026-09-23 D-4g (4.3):** search scope is the active mailbox (every mailbox in the unified view).
- **2026-09-23 D-5a (5.3):** the fold rule is `display: none !important`: Outlook's `<hr style="display:inline-block">` beat a plain rule in the render check; mail's own `!important` is stripped by `filter_style`.
- **2026-09-23 D-5b (5.3):** default-unfolded only when the thread has one message AND the subject starts with Fwd/FW/WG/TR; a lone forward with nothing above the header has `hasFold: false` and no pill.
- **2026-09-23 D-5c (5.2):** a `>` run folds only when everything from it to the next hard marker is quoted or blank; an interleaved reply is never folded at its first quote.
- **2026-09-23 D-5d (5.1):** a div carrying both signature and quote classes becomes `skim-quote`.
- **2026-09-23 D-5e (5.3):** `has_fold` matches any `type="cite"` blockquote while the CSS hides top-level ones only; a deeply nested cite above every other marker could show a pill that hides nothing. Left as is: no real client emits that.
- **2026-09-23 D-5f (5.3):** toggling rebuilds `srcdoc`; the existing poll + ResizeObserver re-measure height.
- **2026-09-23 D25 (gates):** `cargo test` binaries exit with STATUS_ENTRYPOINT_NOT_FOUND when run from Git Bash with `/mingw64/bin` on PATH (a MinGW DLL shadows the system one once the MCP/HTTP crates joined). `scripts/fork/gates.sh` strips the MSYS dirs for the test step; run from PowerShell otherwise.
- **2026-09-23 D26 (3.3 fix):** `keys.ts` held the `g` flag in `$state` inside a plain `.ts` file, which Vite never compiles as Svelte; the demo threw `rune_outside_svelte` on boot and every screenshot after 3.3 was blocked. The flag moved to `src/fork/stores/go.svelte.ts`.
