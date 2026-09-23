# Pending: fork Settings panel (Compose, AI-tell, Court, Calendar mounts)

Agent: settings, 2026-09-23. Files touched (delegated by the main session for
this task): `src/fork/SettingsFork.svelte`, `src/lib/i18n/locales/en.json`.
`src/fork/stores/prefs.svelte.ts` already carried every getter / setter /
hydration line the rows need (replyInline, richText, undoSendSecs, smell,
smellBlockHard, smellIgnored); nothing added there.

## mod.rs

None.

## generate_handler

None.

## commands.rs

None.

## settings ALLOWED

All already present in `fork/settings.rs`; the panel only writes through
`prefs.set*` (`set_setting`) and `unignoreRule` (`src/fork/smell/api.ts`):

    fork_reply_inline      // on | off, default on (6.2)
    fork_rich_text         // on | off, default on (6.3)
    fork_undo_send_secs    // 0 | 5 | 10 | 20 | 30, default 10 (6.4)
    fork_smell             // on | off, default on (6.5.5)
    fork_smell_block_hard  // on | off, default on (6.5.5)
    fork_smell_ignored     // JSON array of rule ids (6.5.5), read on panel open

## en.json

Already added to `en.json` (next to the other `fork.settings.*` keys):

    {
      "fork.settings.compose": "Compose",
      "fork.settings.reply_inline": "Reply inside the thread",
      "fork.settings.rich_text": "Rich text editor",
      "fork.settings.undo_send": "Undo send",
      "fork.settings.undo_send_off": "Off",
      "fork.settings.undo_send_secs": "{n}s",
      "fork.settings.smell_unignore": "Stop ignoring {rule}"
    }

`fork.settings.smell*` (four keys) were already there from 6.5.

## App.svelte

None.

## keys

None.

## tauri-core mock

None new. The panel reads `get_settings` (already mocked) for the ignored list;
to shoot a populated list set the mock's `fork_smell_ignored` to e.g.
`["quantity_specific","transitions"]`.

## fork-shots

    // Settings: the fork panel end to end (Compose, AI-tell, Court, Calendar).
    "settings-fork-all": {
      phase: "settings",
      setup: async (page) => { await openSettingsTo(page, "Compose"); },
    },

## TOUCHLIST

| file | symbol | change | reason |
|---|---|---|---|
| `src/fork/SettingsFork.svelte` | template | Compose section (reply inline, rich text, undo send chips), AI-tell toggles + ignored-rule list with remove, `<SettingsCourt />`, `<SettingsCalendar />` | PLAN 6.2-6.4, 6.5.5, 10, 7.7 |
| `src/fork/SettingsFork.svelte` | script | `loadSmellSettings` on open, `removeIgnored` -> `unignoreRule` then `prefs.hydrate` (no second write) | the popover's Ignore writes the setting without touching `prefs` |
| `src/lib/i18n/locales/en.json` | `fork.settings.*` | 7 keys | labels |

## DECISIONS

- 2026-09-23: the ignored-rule list is read from `get_settings` when the panel
  opens rather than from `prefs.smellIgnored`, because `ignoreRule` (popover)
  writes `fork_smell_ignored` directly and the store would be stale within a
  session. After a remove, the store is re-hydrated via `prefs.hydrate` so
  no duplicate `set_setting` is issued.
- 2026-09-23: `SettingsCalendar` is mounted unconditionally (the file existed
  at finish time); it lives after `SettingsCourt`, both after the Compose rows.

## migrations

None.

## Cargo.toml

None.

## status

Done: Compose section (3 rows), AI-tell rows (2 toggles + ignored list with
per-id remove), Court and Calendar mounts, 7 strings, this file.
Not done: no screenshots taken (main session runs `settings-fork-all`).
Gates: see the agent report (`npm run check`).
