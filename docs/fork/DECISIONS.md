# Fork decisions

D1-D12 are copied from `PLAN.md` section 3 so this file is the single log.
Anything decided during the build is appended below with a date.
Ids are unique. The Phase 7 backend agent reused D22-D29, which the main
session had already taken; its eight were renamed D-7a to D-7h after the build.

| # | Decision | Why |
|---|---|---|
| D1 | Fork state in `fork_*` tables + own version key `fork_schema_version` in `settings` | Upstream's single `user_version` sequence would collide with any fork 0016 |
| D2 | Unread = blue dot + bold; read dimmed; star = amber in a fixed gutter (the shipped variant C also colours the subject, D46) | Two signals, not colour-only (WCAG 1.4.1); violet is reserved for AI |
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
- **2026-09-23 D-7a (7.2):** `resolve_email` matches the required scope as a whole
- **2026-09-23 D-7b (7.3):** all three tables use `CREATE TABLE IF NOT EXISTS`
- **2026-09-23 D-7c (7.4):** engine registry is a process-wide
- **2026-09-23 D-7d (7.4):** window replace = upsert the fresh rows, then delete
- **2026-09-23 D-7e (7.4):** ops reference the local **row id** (`event_id`) and
- **2026-09-23 D-7f (7.4):** `fork_cal_rsvp` also takes `send_updates`
- **2026-09-23 D-7g (7.2):** the account-mismatch error (`gcal_account_mismatch`)
- **2026-09-23 D-7h (8):** `fork_free_slots` takes the walking zone `tz` (IANA)
- **2026-09-23 D-9a (workspaces):** `GET /api/v1/workspaces` in Rebound is
- **2026-09-23 D-9b (stage names):** the lookup route returns deals as
- **2026-09-23 D-9c (reminders):** the route returns no reminders today. The
- **2026-09-23 D-9d (`fork_crm_set_config`):** one command more than the plan
- **2026-09-23 D-9e (status and the network):** `fork_crm_status` is local
- **2026-09-23 D-9f (dead refresh token):** a `crm_auth` answer to a refresh
- **2026-09-23 D-9g (wire shape):** structs deserialise Rebound's snake_case
- **2026-09-23 D-9h (address source):** the reading pane owns the focused
- **2026-09-23 D-10a ("filed" = not in the inbox):** the plan's "thread only in folders he filed as FYI/marketing" is read as: a thread with no copy in a folder of role `inbox` is filed (`reason: "filed"`, state `none`). Role NULL (user labels), `archive`/all-mail, `sent`, `important`, `starred` do not count as "in play" on their own: an archived thread is one he dealt with, and Gmail keeps Important/Starred copies after archiving. A label copy PLUS an inbox copy (Gmail) is in play. Junk-only / trash-only threads are `none` whichever side wrote last.
- **2026-09-23 D-10b (waiting has no bulk / inbox test):** a thread whose last message is from one of his addresses is `waiting` wherever it lives (Sent has no inbox copy by construction). Only junk/trash-only threads are excluded. This means every outbound mail ever is "waiting" until answered; the views sort oldest first, so the tail is old. If that is noise in practice, a `since` floor (e.g. 90 days) belongs in the view / a setting, not in the classification.
- **2026-09-23 D-10c (own addresses):** "one of the account's own addresses" = every `accounts.email` and every `imap_user` containing `@`, across ALL accounts, lowercased. Mail from one of his mailboxes to another is still from him.
- **2026-09-23 D-10d (draft rule wins):** an unsent local draft (`drafts` row whose `reply_to_message_id` or `origin_message_id` is a message of the thread) makes the thread `on_me`, reason `draft started`, `needs_reply = 1`, even over bulk / filed / waiting. Rows in `drafts` are unsent by definition (sending deletes them). Server-draft copies in a folder of role `drafts` are skipped when picking the last message (a draft in flight is not the last word).
- **2026-09-23 D-10e (never re-classify, precisely):** the row is left alone when its `last_message_id` is unchanged AND the deterministic verdict (state, since, reason) is unchanged. Same `last_message_id` but a moved verdict (draft opened / closed, thread archived) updates state / since / reason and keeps the AI columns (`needs_reply` only overwritten when the rule itself asserts one, i.e. the draft rule). A new `last_message_id` rewrites the row and resets `model` / `needs_reply` to NULL, which is what makes the thread an AI candidate again. `fork_court_recompute` (forced) rewrites everything and resets every AI verdict.
- **2026-09-23 D-10f (AI candidates = `state = 'on_me' AND model IS NULL`):** no extra column: `model` doubles as "judged at this last_message_id". The view predicate is `state = 'on_me' AND coalesce(needs_reply, 1) = 1`, so an AI "no reply needed" hides the row and the deterministic result shows until the AI has spoken. A batch whose reply does not parse keeps the deterministic verdict but is still stamped with the model (it spent the cap; not retried until the thread changes). A provider / network error leaves the batch unstamped for the next pass. The AI pass runs only after a deterministic pass that wrote something, so an idle mailbox costs nothing.
- **2026-09-23 D-10g (`fork_court_ai` default OFF):** BYOK and optional in the plan; nothing calls a model until Patrick turns it on in settings. The day cap counts threads sent, in local days, in the internal `fork_court_ai_used` row.
- **2026-09-23 D-10h (sender pattern by hand):** the crate has no `regex`; `no-?reply|notifications?|mailer-daemon|bounce` is four case-insensitive substring tests on the whole address (`is_automated_sender`). "bounce" matches `bounces.thomas@` too; accepted, same as the plan's regex would.
- **2026-09-23 D-10i (command parameter name):** `fork_court_list` takes `court_state` (wire: `courtState`), not `state`, because `state: State<'_, AppState>` is the Tauri parameter every command already has. `courtApi.list(state, offset, limit)` hides this.
- **2026-09-23 D-10j (`mail:updated` payload):** the hook takes `touched_folder_ids: Vec<i64>`; the sync emits at most one `folderId`, the rest emit `{}`. An empty list = full scope. Threads whose messages all vanished are swept by the full scope (`thread not in threads`) and by `Scope::Threads`; a folder scope cannot name them, which the next `{}` event covers.
- **2026-09-23 D-11a:** a guest's threads are `from:` OR `to:` the address. `Filters`
- **2026-09-23 D-11b:** "external guest" = every attendee minus own addresses
- **2026-09-23 D-11c:** the reminder window is `[now, now+10 min]`, timed events only,
- **2026-09-23 D-11d:** the CRM card comes from the panel's own `fork_crm_lookup` call
- **2026-09-23 D-11e:** `fork_prep_brief` reproduces `spawn_stream`'s Channel protocol
- **2026-09-23 D-12a (transport):** the MCP server is hand-rolled (JSON-RPC 2.0
- **2026-09-23 D-12b (auth):** token = 32 random bytes as 64 hex chars (not
- **2026-09-23 D-12c (tools shape):** every tool result is a JSON object,
- **2026-09-23 D-12d (create_draft):** a new draft needs `to` and `subject`; a
- **2026-09-23 D-12e (create_event):** lands on the selected primary calendar
- **2026-09-23 D-12g (test binary vs the wry runtime; READ BEFORE ADDING
- **2026-09-23 D-12f (fork_mcp toggle):** `fork_mcp_set_enabled` binds/stops at
- **2026-09-23 D-6.5a (rules as data, not a port):** the TS scanner compiles the vendored
- **2026-09-23 D-6.5b (offset-preserving mask):** OS strips fences / inline code / `>`
- **2026-09-23 D-6.5c (the dash rule):** em / en dash is `hard` on every occurrence
- **2026-09-23 D-6.5d (document layer):** a contrast frame repeated >= 3 (no-smell's
- **2026-09-23 D-6.5e (`--smell-warn` token):** the theme has no amber; the injected
- **2026-09-23 D-6.5f (fact guard, two locks, no regex):** `fork::smell::guard` and
- **2026-09-23 D-6.5g (rewrite returns, does not stream text):** `fork_smell_rewrite`
- **2026-09-23 D-6.5h (Node resolves `./scan`):** the app imports siblings without
- **2026-09-23 D-6.5i (settings key):** `fork_smell_ignored` (as pre-listed), not
- **2026-09-23 D-6a (6.3, editor block model):** one `<div>` per line, `<div><br></div>` for a blank line (Squire's default), not `<p>` per paragraph. `textToHtml`/`htmlToText` are exact inverses over that model, so `body_text` and the editor never drift by a line: the AI stream (`draft.body = streamed + tail`) re-renders through the same path, and `splitTail` keeps finding the signature and the quote.
- **2026-09-23 D-6b (6.1, where Discard lives):** in the footer bar next to Save, not in `WindowControls` (upstream, not named by the phase). Second click within 3 s discards; the first turns the icon into "Discard?".
- **2026-09-23 D-6c (6.3, "Edit quoted text"):** turns the tail (signature block + quote) into a plain textarea under the rich editor; the words stay rich. `body_text = wordsText + tail` in both modes.
- **2026-09-23 D-6d (6.3, the signature in HTML):** emitted only while its `\n\n-- \nsig` block is still in `body_text`, so a sign-off the user deleted never comes back in the HTML part. Inline `cid:` images of the quoted original are dropped (they would point at the app's own protocol).
- **2026-09-23 D-6e (6.3, HTML at send time is a main-session touch):** `fork::compose::outgoing_html(db, draft_id)` exists for `execute_send`/`execute_save_draft`; the two call-site edits are listed under App.svelte §4 because sync.rs was another agent's beyond the drain query.
- **2026-09-23 D-6f (6.4, one task):** a single scheduler task over every account (`plan()` returns the accounts with a due hold); the plan's "that account's engine" is honoured per account inside it. Sleep is capped at 1 h and re-planned, so a clock jump costs at most an hour.
- **2026-09-23 D-6g (6.4, undo toast):** shown from the Rust event `fork:send-held` in the main window (a compose window has no toast host and closes on send). The toast lasts `hold - 1 s`; a hold longer than 60 s (send later) gets a 5 s "Scheduled for …" toast with Undo instead. `Z` is not wired to it (the undo stack's `pushFlag` would leave a stale entry past the margin).
- **2026-09-23 D-6h (6.2, teardown):** an inline reply unmounted by a thread switch deletes its draft when untouched and keeps it (flushServer) when edited, mirroring Esc.
- **2026-09-23 D-6i (6.4, refusal semantics):** `cancel` refuses when `not_before - now < 1` OR the op is no longer `pending`; the list hides holds whose draft is gone.
- **2026-09-23 D-10k (the nudge is the shell's, not notify.rs):** the toast runs in TS (`CourtNudge.ts`, minute poll, `shouldNudgeNow` mirrors `fork::court::should_nudge_local`, text from `fork_court_counts` + the first on-me row). Do NOT also wire the Rust `should_nudge_now` / `nudge_line` into `notify.rs`, or he gets two toasts. Trade-off: no nudge while the window is closed (the tray-only case); if that matters later, move it to Rust and delete `startCourtNudge`.
- **2026-09-23 D-10l (badge is an overlay, not a row edit):** `MessageRow.svelte` is not in this phase's touch list and its height is what the list's windowing measures. The age + reason badge is absolutely positioned inside a wrapper around the row (bottom-right in comfortable, left of the date in compact), so a court row is pixel-identical to an inbox row apart from the badge.
- **2026-09-23 D-10m (no amber token):** `tokens.css` has `--danger` but no warning colour; the badge's amber is `--acct-2` (the amber account swatch every theme defines) rather than a new token, so no contrast run and no shared-file edit.
- **2026-09-23 D-10n (court prefs in their own store):** `fork_court_*` are read through `src/fork/court/prefs.svelte.ts`, not `stores/prefs.svelte.ts` (same reasoning as `calPrefs`: that file is shared). The main session may fold `courtPrefs.hydrate` into `prefs.hydrate` later.
- **2026-09-23 D-10o (a day with nothing on him still counts as nudged):** `startCourtNudge` stamps `nudged_at` before asking for the line, so an empty on-me list does not re-check every minute until midnight. An undo toast on screen defers the nudge to the next minute (one toast at a time, PLAN 3.2).
- **2026-09-23 D-10p (selecting a court view leaves search silently):** `selectCourt` clears `searchQuery` / `searchPrevFolderId` and selects the id, like a sidebar folder click does; it does not go through `exitSearch` (which would first select the previous folder and load a page for nothing).
- **2026-09-23 D30 (7.5):** the grid uses the package's Svelte 5 `Calendar`
- **2026-09-23 D31 (7.5):** the four calendar keys are handled by
- **2026-09-23 D32 (7.5):** the titlebar chip + Meet button ship as
- **2026-09-23 D33 (7.4/7.5):** the guests prompt returns `"all" | "none" |
- **2026-09-23 D34 (7.5):** InviteCard matching: the invite UID is compared
- **2026-09-23 D35 (7.7):** calendar prefs live in
- **2026-09-23 D36 (7.6):** Meet now hands the clipboard a
- **2026-09-23 D37 (8):** the recipient-zone abbreviation tries `en-GB`,
- **2026-09-23 D38 (audit):** a gate audit found that deleting a draft left its held or scheduled `send` op queued; `drafts.id` is not AUTOINCREMENT, so the next draft could reuse the id and the orphaned op would ship its body. `delete_draft` now drops every queued `send` / `save_draft` op for that id (the hold cascades). Tested in `fork::scheduler`.
- **2026-09-23 D39 (audit):** four smaller holes closed: the MCP enable read fails closed; a calendar op with no `send_updates` is refused, never defaulted; `fork_cal_patch` checks the Google connection before writing or queuing; MCP `create_event` falls back only to a calendar the user owns.
- **2026-09-23 D40 (6.5 fixture):** the demo fixture's signature used `--` without the trailing space, so the scanner read the signature as the user's words. Real drafts carry the RFC 3676 `-- ` marker (compose.rs tests), so the fixture was corrected, not the scanner.
- **2026-09-23 D41 (13 smoke):** the live smoke test found that Esc on an untouched inline reply saved an empty reply to the Gmail Drafts folder: the rich editor reports its first render as input, so `dirty` was true with nothing typed. "Edited" now means dirty AND the draft differs from what was loaded (to, cc, bcc, subject, body); Esc, thread switch and the server write-back all use it. The one empty draft the smoke test created (local draft 1, Drafts message 28875) was removed through the app's own delete path; nothing was sent. Checked both ways in the demo: untouched Esc = delete_draft only, edited Esc = save_server_draft only.
- **2026-09-23 D42 (13 smoke):** Meet now without Google connected opened meet.new silently; it now toasts "Opened meet.new in your browser".
- **2026-09-23 D43 (13 smoke):** smoke-test screenshots show real mail, so they live in the session scratchpad, never under `docs/fork/shots/`, never committed.
- **2026-09-23 D44 (13 smoke):** the first live install showed On me 4,013 and Waiting 2,588: the court classified the whole mailbox history, which is not a to-do list. The views, counts, nudge and MCP `list_court` now look back 30 days by default (`fork_court_window_days`, Settings: Look back; 0 = everything). Classification is unchanged; only what is shown is windowed.
- **2026-09-23 D45 (8):** `/slots` only fired in the plain-text editor; rich text is the default, so it never fired for Patrick. The rich editor now checks the caret's line on every change (`RichEditor.takeLineToken`) and removes the token before opening the popover.
- **2026-09-23 D46 (2.0, Patrick's pick):** list variant C ships: unread subjects in `--unread` blue. Because that is now text, the contrast gate holds `--unread` to 4.5:1 (was 3:1); the light themes' blue went from #1f6fd1 to #1a62c0 to pass on selected and hover rows. Dark themes unchanged.
- **2026-09-23 D47 (8):** the compose window (`ComposeRoot`) mounted no slots popover and no toast host, so `/slots` removed its token and showed nothing there. Both are now mounted in that window too.
