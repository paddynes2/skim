# Pending: palette + tooltips + shortcuts overlay (3.3 splice)

## en.json

Already written to `src/lib/i18n/locales/en.json` (after `fork.shortcuts.search_close`), delegated by the main session:

```json
{
  "fork.shortcuts.send": "Send (in the composer)",
  "fork.shortcuts.smell_open": "Open the AI-tell suggestion",
  "fork.shortcuts.cal_day": "Day view",
  "fork.shortcuts.cal_week": "Week view",
  "fork.shortcuts.cal_month": "Month view",
  "fork.shortcuts.cal_agenda": "Agenda view",
  "fork.shortcuts.cal_today": "Today",
  "fork.shortcuts.cal_next": "Next period",
  "fork.shortcuts.cal_prev": "Previous period",
  "fork.shortcuts.cal_new": "New event"
}
```

## fork-shots

- `palette-fork` (phase 3.3): open the palette with Ctrl+K, type "go", wait 300 ms, shoot. Expect the upstream "Go to <folder>" rows with `G I`/`G S`/`G T`/`G D`/`G A` hints, plus "Go to Calendar" (`G C`), "Go to On me" (`G O`), "Go to Waiting" (`G W`) when those hooks are registered.

## TOUCHLIST

| file | symbol | change | reason |
|---|---|---|---|
| src/components/CommandPalette.svelte | imports, `commands` | import `forkCommands`/`gotoHint`; `hint: gotoHint(folder.role)` on goto rows; `list.push(...forkCommands())` before return | 3.3: every fork key discoverable in the palette with its hint |
| src/components/ReadingPane.svelte | toolbar + footer + translate chips | `title` = "Label  Key" on archive, delete, spam, move, star, read/unread, ask, reply, reply all, forward, translate (both chips) | key discoverability on hover; titles only |
| src/components/ShortcutsOverlay.svelte | `groups` | rows `M` Meet now, `Ctrl Enter` send, `Alt Enter` AI-tell suggestion; new "Calendar" group `D W M A T J K N` | keys from phases 5, 6, 7.5, 7.6 were missing |

## DECISIONS

- 2026-09-23: palette.ts labels the Calendar / On me / Waiting rows "Go to <view>" (`palette.goto`) so they sit with the folder goto rows and match a "go" query. No density-toggle row: forkCommands has none and no density hook exists yet.

## status

Done. `npm run check` 0 errors (1 pre-existing warning in ComposeForm.svelte, not touched). `npm run build` ok. No shot taken (scenario is for demo/fork-shots.mjs, owned by main).
