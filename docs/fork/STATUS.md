# Fork status

The one file that says where the fork stands. Everything else under
`docs/fork/` is reference and changes only when the design changes. Update
this file, and its date, whenever the state moves.

**As of 2026-09-25.**

## Estate reply (installed September 25)

`feat/estate-reply` adds Draft reply beside Reply for patrick@autospark.ai.
An optional direction goes through the configured SSH route to a private adapter
on hel-work, running the existing cc-nesbitt Codex profile in read-only mode.
The agent reads the estate and returns draft text and source references. The
adapter alone creates a Gmail draft through the same profile's os-google
credential, verifies its body/recipient/reply target, and returns its RFC ID.
Skim syncs Drafts and offers Open draft in the normal composer.

Repeated requests reuse a private receipt; a per-thread lock prevents two adapter
workers drafting the same conversation at once. Existing drafts and changed
threads stop the save. A save with an unknown outcome is never automatically
repeated. There is a small unavoidable race with edits in other mail clients
between the last check and Gmail draft creation; Gmail exposes no conditional
create here. No AI draft-update operation is claimed: ordinary edits use Skim.

Verified: all 503 Rust tests; warning-free clippy; Svelte check (only the existing
ComposeForm warning); frontend build; 34 Node tests; 96 contrast checks; nine
Python adapter tests on Linux. Live own-profile Google read and a read-only
Codex estate/voice probe passed. Demo UI working/ready/context/error states
were exercised, with no added horizontal overflow at 768px. Desktop UI minimum
size still follows the existing app. No screen-reader audit is claimed.

The helper is installed at
`/home/cc-nesbitt/.local/share/skim/estate_reply.py`; private receipts are under
`~/.local/state/skim-replies`. No credentials, gateway policy, units or database
schema were changed. A real reply draft was generated for Jon Perper after
reading the ZLed client files, complete thread and writing/voice guidance. The
adapter verified the Gmail draft's body, recipient and reply target. Two other
conversations were correctly declined because Patrick had already replied.
Nothing was sent. This short acknowledgement validates the plumbing and source
retrieval; it does not establish quality on complex commercial replies.

The signed 1.1.0 installer built from code commit `e2124a4` exited 0. Skim
relaunched responsive with three established IMAP connections and its MCP
listener; the panic log remained 385 bytes. The installed executable matches
the release build except Tauri's expected UNK-to-NSS bundle marker.
Installed SHA256: `8bb4cf6d713783f9cfe2f59e744a9baf3788dd249129fbb4b9dc145d4a75c2e6`.
Previous executable and database backup:
`C:\Users\Patrick\.skim-fork\backups\estate-reply-20260925-135936`.
Native desktop click-through of the new action has not been automated; its UI
states were exercised using fictional demo data, and real SSH/model/Gmail
integration was exercised separately.
Build contract and operations: `ESTATE-REPLY-PLAN.md`.

## Inbox latency fix (installed September 25)

The `fix/inbox-latency` worktree changes notification recovery in
`src-tauri/src/mail/sync.rs`: 60-second IDLE heartbeats, 10-second bounds on
SELECT/IDLE/DONE, a 30-second maximum reconnect delay reset after a successful
subscription, worker wake-up before DONE, and list refresh immediately after
new headers are committed. Disconnect error codes go to the bounded
`skim-sync.log`, without account identifiers or message content.

The updated 1.1.0 executable from code commit `288df1d` was installed and
relaunched on September 25. The prior executable and database/WAL were backed
up to `C:\Users\Patrick\.skim-fork\backups\inbox-latency-20260925-130703`.
The cause of Patrick's original reported delay and real-world end-to-end
arrival latency are not established. Full-folder
sweeps and queued operations still share the sync worker; this change is not an
end-to-end latency guarantee. Normal notifications do not wait for the heartbeat.

Validation on September 25: full `cargo test --locked` passed all 501 tests,
including four scripted IMAP tests for a silent IDLE start, arrival followed by
a stalled DONE, catch-up without a push, and successful notification/backoff
reset. `npm run check` passed (one pre-existing ComposeForm reactivity warning),
`npm run build` passed, all 34 Node tests passed, all 96 contrast checks passed,
and `cargo fmt --check` passed. `cargo clippy --all-targets --locked -- -D warnings`
passed with no warnings.
The NSIS build and updater signature succeeded; silent installation exited 0.
Installed binary SHA256 is
`8cd80244f50d15313b771b597957dd5413fe13c2935fbc3ee86a685068e5a482`.
Byte comparison with the release executable found only Tauri's expected
three-byte bundle marker difference (`UNK` to `NSS`). The relaunched process was
responsive, with three established IMAP/993 connections and its local MCP/8342
listener. The existing panic log remained 385 bytes; no sync error log was
created during this startup check. No test email was sent. Live arrival latency
validation remains pending; these checks establish installation and connection,
not an instant-delivery guarantee. No new GitHub release was published.
The OS graph accepted
verification receipt `verif:359907031586102e57bf`, but reports `no_view` because
this external worktree is outside its enrolled source views.

## Where it is

- Branch `paddy` on `paddynes2/skim`. The last code commit is `e2124a4`;
  commits after it change only docs and comments.
- Installed on Patrick's machine: version 1.1.0, built from `e2124a4` on September 25.
  Backup and startup verification are recorded above.
- Release `v1.1.0` on GitHub is tagged at `9884f0d`, behind the
  installed build. It lacks the `/slots` rich-editor fix and list variant C.
  The installed copy will not update itself backwards, because both builds say
  1.1.0. The next release should be 1.1.1.
- Gates green at `75c57e0`: `bash scripts/fork/gates.sh`, including 96 contrast ratios.

## Done

All of `PLAN.md` phases 0 to 13. The phase table in `README.md` says what each
phase added. The build and the live smoke test also changed these:

- The Google connection is live. Patrick connected it and 192 events synced.
- The MCP server was tested live. It returns 401 without the token and 403 for
  a foreign Origin, lists 12 tools, and answers `search_mail`.
  `claude mcp list` shows `skim` connected.
- The live smoke test on the installed app passed: inbox, chips, J / K,
  E then Z, palette `from:`, a thread fold, inline reply, Ctrl+Enter, zoom and
  the Calendar screen. The screenshots show real mail, so they were kept out of
  the repo (D43).
- Fixes the smoke test forced:
  - Esc on an untouched reply no longer saves an empty draft (D41).
  - Court views look back 30 days. Settings has a "Look back" row, and "all"
    shows everything. On the first install the court showed 4,013 threads on me
    and 2,588 waiting; with the window it shows 248 and 41 (D44).
  - `/slots` works in the rich editor (D45) and in the pop-out compose window (D47).
- Safety audit fixes:
  - Deleting a draft drops its queued sends.
  - The MCP switch fails closed.
  - A calendar op with no `send_updates` is refused.
  - A calendar patch while disconnected is refused.
  - MCP `create_event` lands only on a calendar Patrick owns.
- Patrick picked list variant C (blue unread subject). It now ships, with the
  light-theme blue darkened to clear 4.5:1 (D46).

## Open, itemised

Nothing below is built unless it says so.

Needs Patrick:

1. **Rebound login.** Settings, CRM, enter the Rebound email and password,
   Connect. He does not have the password; use Forgot password on Rebound.
   Until then the CRM drawer, meeting-prep guest cards and MCP `crm_lookup`
   return "not connected".
2. **Theme.** Warm-dark is his choice in Settings; the default is untouched.

Built, never exercised live:

3. Google calendar event create, patch, delete, RSVP and Meet space create.
   Only connect and the pull have run live. The first real use is the test.
4. Everything past Rebound login: lookup, cache and refresh.
5. The court AI pass. It is off by default and there is no key in the build env.

Not built:

6. PLAN 6.5.4 "Clean whole draft", a one-click rewrite of every flagged span.
7. A `--smell-warn` token in `tokens.css`. It is defined per theme inside
   `src/fork/smell/highlight.ts`, with a hardcoded fallback in two components.
8. The `settings-fork-all` screenshot scenario. It was handed back and never merged.
9. A density row in the command palette.
10. `z` undoes a thread action but not a send; the send hold has its own Undo
    button (D-6g).
11. The CRM `lookup_for` seam proposed in `pending/11.md`, and making
    `spawn_stream` `pub(crate)`.
12. The court nudge runs in the window's TypeScript. With the window closed to
    the tray there is no nudge, because it is not wired to `notify.rs`.
13. Nested `type="cite"` blockquotes above other markers are counted by
    `has_fold` but not hidden (D-5e).
14. `htmlToText` in the rich editor has no node test; it is exercised only in the browser.

Upstream:

15. The trial merge ran on 2026-09-23 at `d44ef77`, before phases 5 to 13 were
    committed. Upstream `main` was `cd60077` ("scoop: skim 1.0.30"), 0 commits
    ahead, and the merge answered "Already up to date". Upstream has not moved
    since, so no conflict has been seen yet. Re-run the trial after upstream moves.

Housekeeping:

16. Release 1.1.1 so GitHub matches the installed build (see "Where it is").
17. The port 8342 row is in `C:\Users\Patrick\OS\meta\PORTS.md` but not
    committed there. That file also holds another session's uncommitted edit to
    the 8350 row, and a path commit would sweep it in.
