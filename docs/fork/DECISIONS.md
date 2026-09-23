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
