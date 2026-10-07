# Fork status

The one file that says where the fork stands. Everything else under
`docs/fork/` is reference and changes only when the design changes. Update
this file, and its date, whenever the state moves.

**As of 2026-10-07.**

## Version 1.1.3 (installed October 7)

Deals replaces Snooze and Follow-ups, which had zero uses (D56). One sidebar
row, Deals, lists every conversation with someone at a live deal, newest
first, each row tagged with its deal; the badge counts deal conversations
unread in the Inbox. The list is kept in Settings, Deals (local only; this
repository is public); Add to Deals on an open thread fills in the sender's
domain. Archive is not offered inside Deals. `g e` opens it.

Verified: all gates green (519 Rust tests, node tests, contrast, svelte-check
with the one existing warning, clippy, build). Seven hand mutants on
`fork::deals` each turned the tests red; the sentinel proved the cargo channel
fails red. A design critique and a code review ran before install; their
findings are fixed (D56). Demo shots in both Base themes under
`docs/fork/shots/1.1.3/`. Signed installer built from `2753c38`, installed
08:59 SAST; installed SHA256
`d513e9f798f834526096abecb9ca8e28040b54f891e0581d92bc23d1fde1322d`. Backup:
`C:/Users/Patrick/.skim-fork/backups/20261007-085933/`. Live on his mailbox
over CDP: the list of 11 deals seeded through the app's own `set_setting`,
read back with nothing ignored; 107 deal conversations, header 107, badge 3
(matches the backend), list read in about 20 ms, the open thread shows its
deal under the subject, no Archive button in Deals, `g e` in the go hint, no
page errors. Skim relaunched without the debug port. Nothing was sent; no
mail was moved or flagged.

Not exercised live: Add to Deals saving a new line (unit tests and the demo
cover it; doing it live would change his list), the Settings box saving an
edit. Rollback: run `Skim_1.1.2_x64-setup.exe /S` from the bundle folder;
1.1.2 ignores the newer fork schema rows and the `fork_deals` setting.

## Version 1.1.2 (installed October 7)

Patrick reported the Inbox showing 38 unread, which was wrong (D55).

- Read and starred state now follows the server for the whole folder. Before,
  Skim re-read flags only for the newest 500 messages of a folder, so mail read
  in Gmail on the web or phone after it had aged out stayed unread in Skim.
- Folder counts are unread conversations, as Gmail counts them, not unread
  messages.
- Reading or starring a message on a Gmail account applies to every copy of
  it under other labels straight away.

Verified: 511 Rust tests (3 new), clippy, fmt, svelte-check, build, contrast
and all node tests green when `gates.sh` is run directly; inside the
PowerShell wrapper clippy exited 1 with no diagnostic, so the install ran with
`-SkipGates` after the direct run. Signed installer built from `dad10ab`,
installed 07:45 SAST; installed SHA256
`e2dc75a470cfb902f5cf89b021413a90b2ad9c88ec3e04fa06900f0edc223154`. Backup:
`C:/Users/Patrick/.skim-fork/backups/20261007-074524/`. Live on his mailbox:
fork schema 8 applied; the first Inbox sync flipped 17 of the 19 unread messages in one
long client thread to read from the server's answer; Inbox went from 38 (messages) to
8 (conversations, 21 messages the server still holds unread).

Not exercised live: a read or star propagating to label copies (unit tests
cover it; testing it live would change his mail). Snooze and Follow-ups had
zero uses in eight days; what replaces them is open (his call).

## Version 1.1.1 (installed September 29)

Patrick's six asks of September 29, five built (he declined the new AI features):

- A conversation opens on its newest message, wherever it is filed, with the
  rest listed below it newest first (D50). His Investec reply in Sent had been
  hidden behind "Later in thread (1)".
- Labels fold (folded by default) and single labels hide (D51).
- Base theme, both lightnesses, from the Paddy x Wes Base tokens (D52). His
  theme is now Base dark.
- Calendar: typed time field with durations, Accept / Maybe / Decline at the
  top of the event panel, Propose new time from the panel and from inbox invite
  cards, which opens a draft to the organiser (D53).
- Snooze (H) and Follow-ups (B, or the picker beside Send) replace On me and
  Waiting (D54).

Verified: all gates green (508 Rust tests, 44 node tests, 144 contrast ratios,
svelte-check 0 errors with the one existing ComposeForm warning, clippy clean).
Demo shots in both Base themes under `docs/fork/shots/1.1.1/`. Signed installer
built from `ea28b10` and installed at 08:55 SAST; installed SHA256
`1b2af67a39508822e4e669e564d4a37826bee5b4f089582977d1df61e591def5`. Backup:
`C:/Users/Patrick/.skim-fork/backups/20260929-085450/`. Live smoke test over
CDP on the installed app: theme Base dark, sidebar shows Snoozed and
Follow-ups and no court views, labels folded, the Investec thread focuses its
newest message with 12 earlier rows below and no "Later in thread", a snooze
on a real thread hid it, listed it under Snoozed, and Remove brought it back,
the Follow-ups view opens, the new-event time list opens and the panel closes
unsaved, and no page errors. Nothing was sent; no event was saved.

Not exercised live: Propose new time's draft (the demo covers it), a snooze or
follow-up actually falling due (Rust tests cover it), the compose follow-up on a
real send, Accept / Maybe / Decline against Google (still never run live, see
item 3). Reminders are local to Skim: Gmail on the phone still shows a snoozed
thread. The due toast needs the window open; a due reminder pins its thread to
the top either way.


## Campaign drafts repair (September 25)

Drafts rows now open a message from the real Drafts folder, including in the
unified mailbox. A newer Sent copy in the same conversation cannot become a
draft. The backend verifies folder membership before returning a cached local
draft and again after fetching its body. Moving away invalidates an outstanding
editor lookup.

Drafts row actions, bulk actions and Move resolve only draft message IDs; deleting
a grouped draft no longer moves the conversation's sent copies to Trash. Existing
messages already in Trash are not restored by this code change.

Verified: 504 Rust tests passed (one existing ignored), warning-free clippy,
Rust formatting, Svelte check (zero errors, one existing ComposeForm warning),
frontend build, 38 Node tests and 96 contrast checks. Installed at 14:55 SAST
September 25 from `d73f561`; the app relaunched successfully. The installer Minisign
signature verifies against the configured updater public key. Installed executable
bytes match the compiled repair exactly except for Tauri's expected
`BUNDLE_TYPE_VAR_UNK` to `BUNDLE_TYPE_VAR_NSS` marker. Receipt:
`.campaign-draft-install.json`; data backup:
`C:/Users/Patrick/.skim-fork/backups/20260925-145409/`.

The PowerShell build wrapper reported a native stderr information line as an
error after the signed artifacts had been emitted; the signature and installed
binary checks above establish the delivered build independently of that wrapper.

## Visible scope of this update

The installed update adds an action inside an open conversation, not a new home
screen or sidebar. Open an incoming email for `patrick@autospark.ai` and find
**Draft reply** beside **Reply** at the bottom of the reading pane. It is hidden
in Sent, Drafts, Trash and Junk and for unsupported accounts. Optional direction,
progress, **Open draft** and **Context used** live with that action. See the
[usage guide](ESTATE-REPLY-PLAN.md#using-it).

On me and Waiting were replaced by Snooze and Follow-ups in 1.1.1. A bounded
Finish session remains unimplemented. The estate reply feature is the delivered
slice of the broader proposal. The inbox recovery fix is also installed; instant
end-to-end mail delivery has not been established.

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
The exact saved RFC identity subsequently appeared in Skim's Drafts folder
(local message 29187, thread 583) through normal IMAP sync. Repeating Start
returned the same receipt; a gateway check still found exactly one draft.
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

- Branch `paddy` on `paddynes2/skim`. The last code commit is `ea28b10` (1.1.1).
- Installed on Patrick's machine: version 1.1.1, built from `ea28b10` on September 29.
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
12. The reminder toast runs in the window. With the window closed to the tray
    there is no toast (the thread still pins to the top). The court nudge it
    replaced had the same limit and is no longer started.
13. Nested `type="cite"` blockquotes above other markers are counted by
    `has_fold` but not hidden (D-5e).
14. `htmlToText` in the rich editor has no node test; it is exercised only in the browser.

Upstream:

15. The trial merge ran on 2026-09-23 at `d44ef77`, before phases 5 to 13 were
    committed. Upstream `main` was `cd60077` ("scoop: skim 1.0.30"), 0 commits
    ahead, and the merge answered "Already up to date". Upstream has not moved
    since, so no conflict has been seen yet. Re-run the trial after upstream moves.

Housekeeping:

16. Publish a GitHub release for 1.1.1 so the fork's releases match the
    installed build. The install is already 1.1.1, so nothing updates backwards.
17. The port 8342 row is in `C:\Users\Patrick\OS\meta\PORTS.md` but not
    committed there. That file also holds another session's uncommitted edit to
    the 8350 row, and a path commit would sweep it in.
