# One-pass build prompt

Paste everything below the line into a fresh Claude Code session opened in
`C:\Users\Patrick\Projects\skim`.

---

You are building Patrick's fork of the Skim email client, end to end, in one autonomous pass.

**Repo:** `C:\Users\Patrick\Projects\skim` (fork `paddynes2/skim`, remote `upstream` = `nikserg/skim`). Work only on branch `paddy`.
**The plan is the spec:** `docs/fork/PLAN.md`. Read it in full first, then `CLAUDE.md` and `CONTRIBUTING.md`. Build **every phase, 0 through 13, every step**, in the plan's order. Do not skip, stub, defer or "leave for later" any step. If a step turns out wrong once you are in the code, fix it the way the plan's intent requires and record the change in `docs/fork/DECISIONS.md`.

**Before writing code for any step, read the code it touches.** The plan cites file:line from upstream `cd60077`; lines drift, so find the symbol and read the whole function. Never assume a shape you have not read. For third-party libraries (`squire-rte`, `@event-calendar/core`, `rmcp`, Google Calendar v3, Google Meet v2 REST, Rebound's `apps/internal/rebound/src/app/api/v1/extension/lookup/route.ts` in `C:\Users\Patrick\OS`), read the installed package docs/source or the official docs before coding against them.

**Rules (from PLAN.md section 0, all binding):** fork code in `src-tauri/src/fork/` and `src/fork/`; every upstream edit logged in `docs/fork/TOUCHLIST.md`; fork state only in `fork_*` tables via the fork migration runner; offline-first queues; all outside HTTP in Rust; secrets only in Windows Credential Manager under `fork:` keys; violet stays AI-only; new strings in `en.json`.

**Hard safety rules:**
- Never send an email, never create or change a calendar event that has guests, never post anything anywhere, during the build or the smoke test. The MCP server gets no send tool and no invite tool.
- Before the first install, back up `%APPDATA%\com.skim.app\skim.db`, `-wal` and `-shm` (the build script does this; confirm the backup exists before installing).
- Never run destructive git commands (`reset --hard`, `checkout --` on modified files, `clean -f`, force-push). Commit per step with message prefix `fork(<phase>.<step>):`. Push `paddy` to `origin` only at Phase 13.
- Do not touch `C:\Users\Patrick\OS` except: reading files the plan names, reading `.env` values for the build environment (never print them), the Phase 6.5.1 edit to `tools/outreach/slop_scrub.py` + `test_slop_scrub.py` (commit on OS branch `trunk` in the main checkout, only those files, with `git commit <paths>`, and keep `python -m pytest tools/outreach/test_slop_scrub.py tools/no-smell/test_no_smell.py -q` green), and adding the port 8342 row to `meta/PORTS.md` in Phase 12.

**Gates after every step (all must pass before the next step):** `npm run check`, `npm run build`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` (all with `--manifest-path src-tauri/Cargo.toml`), and from Phase 2 on `node scripts/fork/contrast.mjs`. Every new Rust module ships unit tests; every new UI state gets a mock in `demo/mock/` and screenshots from `demo/fork-shots.mjs` saved to `docs/fork/shots/<phase>/` in all 4 themes. Look at every screenshot you generate and fix what is visibly wrong (overlap, clipping, unreadable contrast, misalignment) before moving on. A green test that never rendered the screen is not done.

**UI:** build list variant A from Phase 2.0 as the default, and still produce the A/B/C mock file.

**Google and Rebound credentials:** the build must work without them. Calendar and CRM show their connect/setup state until Patrick completes the two human steps in PLAN.md section 5. Do not attempt those steps for him and do not stop to wait for them. Bake the Rebound URL and anon-key defaults from `C:\Users\Patrick\OS\.env` into the build environment only (never commit them).

**Finish (Phase 13):** run `scripts/fork/build-install.ps1` to install over the running Skim, run the live smoke test in the plan (read-only; no sends, no guest events), register the MCP server with `claude mcp add` and call `search_mail` once through it, trial-merge `upstream/main` into a throwaway branch and record the conflicts in `docs/fork/README.md`, push `paddy`, tag `v1.1.0`.

**Final report:** a checklist of every plan step marked done with its commit hash; gate results; the screenshot folder; the installed version; anything that did not work, stated plainly with the error; and the exact human steps left (Google OAuth client, Rebound login, variant choice).
