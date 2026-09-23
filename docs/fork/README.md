# Skim fork: front door

This is Patrick Nesbitt's fork of [nikserg/skim](https://github.com/nikserg/skim),
kept at `paddynes2/skim` on branch `paddy`. `main` mirrors upstream and is never
edited. Everything the fork adds lives in new files (`src-tauri/src/fork/`,
`src/fork/`, `scripts/fork/`, `demo/` additions, `docs/fork/`); every edit to an
upstream file is one row in `docs/fork/TOUCHLIST.md`.

Where to read next:

- `PLAN.md`: the full build plan, phase by phase, with the file and line facts it rests on.
- `DECISIONS.md`: every decision and every deviation from the plan, dated.
- `TOUCHLIST.md`: every upstream file the fork touches, one line each.
- `AGENT-RULES.md` and `pending/`: how the parallel build agents worked and what each handed back.
- `mocks/`: the static A/B/C list mock and the AI-smell state mock.
- `shots/<phase>/`: screenshots from the visual check harness, per phase.

## What the fork adds

One line per phase. The phase numbers match `PLAN.md` section 2 and the
`fork(<phase>):` commit prefixes.

| Phase | What it adds |
|---|---|
| 0 | Foundation: `paddy` branch and remotes, fork migration runner with its own `fork_schema_version` key (upstream's `user_version` sequence is untouched), `fork::ForkState` in `AppState`, own updater keypair and endpoint, version line 1.1.0, `scripts/fork/build-install.ps1`, the screenshot harness `demo/fork-shots.mjs`. |
| 1 | Correctness fixes: Gmail detected by IMAP host and MX, not only by the provider string; archive on Gmail expunges the label instead of moving; no duplicate Sent copies on Gmail-host accounts; Important is its own role, not Starred; Starred shows a real total in the sidebar; no Archive button in Sent, Trash or Spam. |
| 2 | Wave 1 visuals: unread = blue dot plus bold (variant A of the mock), amber star in a fixed gutter, contrast tokens gated by `scripts/fork/contrast.mjs` (96 ratios), compact density, zoom keys, hover actions on rows, advance after archive, a global focus ring. |
| 3 | Triage: All / Unread / Starred chips with unread-first order, undo for archive, delete, spam, move and bulk removals (8 s client hold, then server restore by Message-ID), `g` sequences with a hint, `z` / Ctrl+Z, navigation hooks for later phases. |
| 3.4 | Sent and Drafts refresh at once after a send or a draft save, and on opening or focusing either folder (20 s debounce per folder), instead of waiting for the 5-minute poll. |
| 4 | Search operators in the palette: `from:`, `to:`, `cc:`, `subject:`, `is:`, `has:`, `in:`, `before:`, `after:`, `older_than:`, `newer_than:`, quoted values, `-word` negation; results grouped by thread with removable chips. |
| 5 | Reading: quoted text and signatures fold under a pill in HTML and plain-text bodies, with the client markers (Gmail, Outlook, Yahoo, Proton, Thunderbird) surviving the sanitiser. |
| 6 | Compose: Ctrl+Enter sends, the window close button keeps an edited draft, two-click Discard, inline reply under the message, rich text (Squire) with `body_text` kept as the full text body, send later with an undo-send hold, a Scheduled list. |
| 6.5 | AI-smell underlines in the composer: the OS `slop_scrub` rules exported to `rules.json`, a TypeScript scanner with a 30-sample parity check, underlines and a health bar, a popover with Ignore / Ignore rule / Rewrite, and a send-check prompt. |
| 7 | Google connection, Calendar and Meet: OAuth Desktop flow with the client ID and secret held in Credential Manager, calendar tables under `fork_*`, a windowed pull (-60 d / +180 d) every 5 minutes, an offline op queue for create / patch / delete / RSVP, a Meet space from the toolbar. |
| 8 | Share availability: `fork_free_slots` walks working hours in a chosen zone over the selected calendars and returns slots on a 30-minute grid, for pasting into a reply. |
| 9 | CRM sidebar: a right drawer that looks the focused sender up in Rebound (person, company, open deals, activities), read-only, with Supabase login held in Credential Manager and a 10-minute lookup cache. |
| 10 | Ball in my court: a deterministic pass classifies every thread as on me / waiting / none, two sidebar views (`g o`, `g w`) with age badges, an optional AI pass (off by default, day cap), a daily nudge toast. |
| 11 | Meeting prep: for an event with external guests, a panel with each guest's CRM card and last threads, a streamed brief, and a reminder 10 minutes before. |
| 12 | MCP server: loopback HTTP on port 8342 with a bearer token, twelve tools (`search_mail`, `get_thread`, `list_unread`, `list_court`, `get_calendar`, `find_free_slots`, `create_draft`, `archive`, `star`, `mark_read`, `create_event`, `crm_lookup`). There is no send tool and no invite tool. |
| 13 | Release: this README, the touch list and decisions log brought current, gates green, shots regenerated, an installed 1.1.0 build, the live smoke test, tag `v1.1.0` on `paddy`. |

Not in this fork by decision (D12): snooze, split inbox, screener, bundles,
templates.

Two rules bind every phase and are worth repeating here: nothing sends mail on
its own (no feature, timer or MCP tool submits an email or an external invite
without Patrick pressing Send), and secrets live only in Windows Credential
Manager under `fork:` keys, never in the database, settings, logs or repo.

## Build and install

The one command:

    powershell -ExecutionPolicy Bypass -File scripts\fork\build-install.ps1 [-SkipGates] [-NoInstall]

What `build-install.ps1` does, in order:

1. Refuses a tree with modified tracked files.
2. Loads `%USERPROFILE%\.skim-fork\build.env` into the process environment only.
   That file is outside the repo and is never printed or committed. It must set
   `TAURI_SIGNING_PRIVATE_KEY_PATH` (the script reads the key content into
   `TAURI_SIGNING_PRIVATE_KEY`, because the Tauri bundler ignores a path alone)
   and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. It may set `SKIM_GOOGLE_CLIENT_ID`
   and `SKIM_GOOGLE_CLIENT_SECRET` as a build-time fallback for the Google
   client; the runtime values pasted into Settings take precedence.
3. Reads the Rebound defaults from `C:\Users\Patrick\OS\.env` and maps them to
   `SKIM_REBOUND_BASE_URL`, `SKIM_REBOUND_SUPABASE_URL` and
   `SKIM_REBOUND_SUPABASE_ANON_KEY`. `crm.rs` bakes these in with `option_env!`
   so the Settings fields arrive prefilled. Without the OS `.env` the fields are
   empty and can be typed in Settings.
4. `npm ci`, then `bash scripts/fork/gates.sh` (skip with `-SkipGates`), then
   `npm run tauri build -- --bundles nsis`. The build stops if the installer or
   its `.sig` is missing.
5. With `-NoInstall` it stops here. Otherwise it closes Skim, backs up
   `%APPDATA%\com.skim.app\skim.db`, `-wal` and `-shm` to
   `%USERPROFILE%\.skim-fork\backups\<timestamp>\`, runs the NSIS installer
   silently, relaunches `%LOCALAPPDATA%\Skim\skim.exe` and prints the installed
   version.

`scripts/fork/gates.sh [--no-build]` runs every gate in one go and stops at the
first failure: `npm run check`, `npm run build`, `cargo fmt --check`,
`cargo clippy --all-targets -- -D warnings`, `cargo test`, the contrast script,
and `node --test src/fork/tests/`. The test step strips the MSYS directories
from `PATH` first: with `/mingw64/bin` ahead of the system, the test binaries die
with `STATUS_ENTRYPOINT_NOT_FOUND` (D25). If you run `cargo test` by hand, run it
from PowerShell or cmd.

Several agents building at once serialise on the cargo target-dir lock. Wait for
it; do not kill it.

## Demo and screenshots

The demo swaps only the Tauri IPC layer for mocks (`demo/mock/`), so the real
UI runs in a browser with canned mail and scripted AI. No account, mailbox or
model is touched. `npm run demo:dev` serves it; `demo/README.md` explains the
mock files.

The fork's visual check harness renders the app against those mocks in all four
themes at 1440x900 and writes PNGs to `docs/fork/shots/<phase>/<scenario>-<theme>.png`:

    node demo/fork-shots.mjs list                     # print scenarios
    node demo/fork-shots.mjs <phase> [scenario ...]   # default: every scenario tagged for the phase

It starts its own Vite server on port 1421; if another run holds the port, wait
and retry. Look at the PNGs and fix what is visibly wrong: one defect (the rich
editor rebuilding on every keystroke) was found this way.

The static mocks are separate: `docs/fork/mocks/list-states.html` (the A/B/C
list variants, shot to `mock-list-states.png`) and `smell-states.html`.

## How the build was run: agent fan-out

From Phase 3.2 onward the phases were built by parallel agents in the same
working tree, under `docs/fork/AGENT-RULES.md` (D24). Each agent edits only its
own new files and the upstream files its phase names. The seam files
(`fork/mod.rs`, `lib.rs`, `commands.rs`, `settings.rs`, `App.svelte`, `keys.ts`,
`actions.ts`, `SettingsFork.svelte`, `en.json`, `demo/fork-shots.mjs`,
`demo/mock/tauri-core.ts`, `TOUCHLIST.md`, `DECISIONS.md`) belong to the main
session.

Each agent hands its seam edits back in one file, `docs/fork/pending/<phase>.md`,
with fixed headings (`## mod.rs`, `## generate_handler`, `## en.json`,
`## App.svelte`, `## keys`, `## tauri-core mock`, `## fork-shots`,
`## TOUCHLIST`, `## DECISIONS`, `## migrations`, `## Cargo.toml`, `## status`).
The main session merges those mechanically and commits per step with the
`fork(<phase>):` prefix. Agents never commit, never run history-changing git,
and never send anything.

Read the `## status` section of a pending file for what that phase really
finished, what it left for the main session, and which gates it ran.

## Human steps that remain

Only these three (PLAN.md section 5):

1. Google, about ten minutes, once: in the Google Cloud project owned by the
   autospark.ai Workspace, enable the Google Calendar API and the Google Meet REST
   API, set the OAuth consent screen to Internal, create an OAuth client of type
   Desktop app. Paste the client ID and secret into Skim Settings, Calendar, then
   click Connect and approve. Internal apps skip Google verification and the
   weekly token expiry.
2. Rebound, once: Settings, CRM. The URLs and anon key are prefilled from the
   build; enter the Rebound email and password and click Connect. A workspace is
   auto-picked; open a thread from a known contact and press `i` to see the card.
3. Choose the list variant if A is not right: open
   `docs/fork/mocks/list-states.html`. A ships; B (row tint plus a 3 px left bar)
   and C (blue subject) are documented as CSS deltas in D18.

Then the Phase 13 smoke test on the real app, read-only except where the plan
says otherwise: inbox renders, unread dot and amber star visible, chips work,
J / K, E then Z restores the thread, palette `from:` search, a thread with
quotes folds, inline reply opens, Ctrl+Enter present, zoom keys, the Calendar
screen shows its connect state (or events after step 1), Meet now opens
meet.new, and `search_mail` answers from Claude Code after the `claude mcp add`
line shown in Settings. No email is sent and no event with guests is created or
changed during the smoke test.

## Upstream sync

### Trial merge record, 2026-09-23

Done in a throwaway worktree (`trial/upstream-merge`, cut from `paddy` at
`d44ef77`, removed afterwards; the shared tree was not touched).

- `git fetch upstream`: upstream `main` is at `cd60077` ("scoop: skim 1.0.30"),
  the same commit the plan was written against.
- `upstream/main` is 0 commits ahead of `paddy`. `paddy` is 12 commits ahead of
  `upstream/main`, and `cd60077` is their merge base.
- `git merge upstream/main --no-commit --no-ff` answered "Already up to date."
  No conflicting files, so there is nothing to classify as trivial or structural
  yet. The TOUCHLIST rows are the list of files that would conflict when
  upstream moves: hook lines in `db/mod.rs`, `state.rs`, `lib.rs`, `mail/sync.rs`,
  `commands/invites.rs`, `mail/autoconfig.rs`, `commands/accounts.rs`,
  `db/queries.rs`, `mail/sanitize.rs`, `mail/oauth.rs`, and the Svelte
  components listed there; plus the version line in `package.json`,
  `Cargo.toml`, `Cargo.lock` and `tauri.conf.json`, which will conflict on every
  upstream release and is always resolved in the fork's favour (1.1.x).
- Caveat: the trial ran against `paddy`'s committed state. The working tree
  holds later phases not yet committed; their upstream touches are in the
  pending files and will join the touch list as the main session merges them.

### Re-syncing when upstream moves

1. `git fetch upstream`, then `git log --oneline paddy..upstream/main` to see
   what came in, and `git diff --stat paddy...upstream/main -- <touchlist files>`
   to see which touched files upstream changed.
2. Rebase `paddy` on `upstream/main` (or merge, as PLAN.md Phase 13 says; the
   rebase keeps the `fork(<phase>):` history linear). Fork-owned directories
   never conflict.
3. For each conflict, open `docs/fork/TOUCHLIST.md`, find the row for that file
   and symbol, and re-apply the fork's hook line, parameter or prop on top of
   upstream's new version of the function. The row tells you what the edit was
   and why. If upstream renamed or split the function, update the row.
4. Keep the fork's version line (1.1.x) and updater keys in `tauri.conf.json`,
   and keep the `scoop` job removed from `release.yml`.
5. If upstream added a migration to its `user_version` sequence, nothing changes
   on the fork side: fork tables are under `fork_schema_version`.
6. `bash scripts/fork/gates.sh`, then `node demo/fork-shots.mjs <phase>` for any
   phase whose components upstream touched, and look at the PNGs.
7. Bump the fork patch version and build with `build-install.ps1`.

## Known limits, stated plainly

State at the v1.1.0 install, 2026-09-23:

- No live call to Google was made. No OAuth client exists yet (human step 1),
  so the Calendar v3 and Meet request shapes were written from the reference
  and tested against JSON fixtures. The first real Connect is the test. If
  Google 404s on the raw `@` in a calendar path, percent-encode the segment in
  `gapi::events_url`.
- No live call to Rebound was made. No credentials were entered (human step 2);
  login, refresh, lookup and cache are unit-tested against route-shaped
  fixtures. First check: Settings, CRM, Connect, then `i` on a known contact.
- The MCP server was checked live on the installed build: 401 without the
  token, 403 for a foreign Origin, 12 tools listed, `search_mail` answered with
  real results, and `claude mcp list` shows `skim` connected.
- The Ball-in-my-court AI pass has never run live (no key in the build
  environment). It is off by default. Every outbound mail reads as "waiting"
  until answered (D-10b); if that is noise, a `since` floor belongs in the view.
- Freshness (3.4) and the Gmail archive path (1.1, 1.2) are tested at the
  predicate, not against a scripted IMAP server (D15).
- `htmlToText` in the rich editor is exercised only in the browser.
- Run `cargo test` through `scripts/fork/gates.sh`, or from PowerShell: from
  Git Bash the test binary dies with STATUS_ENTRYPOINT_NOT_FOUND (D25).
- Send later needs the app running: Skim autostarts to the tray, and the
  Scheduled view says so.
- The port row for 8342 is in `OS/meta/PORTS.md` but not committed there: that
  file carried other sessions' uncommitted edits, so a path commit would have
  swept them in.
