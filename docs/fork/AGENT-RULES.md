# Rules for build agents (fanned out from the main session)

You are one of several agents building `docs/fork/PLAN.md` phases in the SAME
working tree at the same time. Read `PLAN.md` (your phase, plus section 0),
`CLAUDE.md`, `CONTRIBUTING.md`, and `docs/fork/DECISIONS.md` first.

## Files you may edit

- Your own new files under `src-tauri/src/fork/<yours>/` or `src-tauri/src/fork/<yours>.rs`,
  `src/fork/<yours>/`, `demo/mock/` additions named for your phase, and the
  upstream files your phase NAMES (read the whole function before editing;
  use the Edit tool: files are CRLF).
- Nothing else. In particular these are OWNED BY THE MAIN SESSION and you
  must NOT edit them (another agent is editing them too and you would
  clobber each other):
  `src-tauri/src/fork/mod.rs`, `src-tauri/src/fork/db.rs`,
  `src-tauri/src/fork/commands.rs`, `src-tauri/src/fork/settings.rs`,
  `src-tauri/src/lib.rs`, `src-tauri/Cargo.toml`, `package.json`,
  `src/lib/i18n/locales/en.json`, `src/App.svelte`, `src/fork/keys.ts`,
  `src/fork/actions.ts`, `src/fork/SettingsFork.svelte`, `demo/fork-shots.mjs`,
  `demo/mock/tauri-core.ts`, `docs/fork/TOUCHLIST.md`, `docs/fork/DECISIONS.md`.

## How to hand the main session what it must merge

Write ONE file `docs/fork/pending/<phase>.md` with these sections, exact
headings, so it can be merged mechanically:

    ## mod.rs            (lines to add to fork/mod.rs, e.g. `pub mod search_query;`)
    ## generate_handler  (fully qualified command paths, one per line)
    ## commands.rs       (nothing: put commands in your own module)
    ## settings ALLOWED  (fork_* keys you read/write, one per line, with a comment)
    ## en.json           (a JSON object of new "fork.*" keys → English strings)
    ## App.svelte        (exact snippets + where, if your phase needs the shell or keys)
    ## keys              (key bindings for src/fork/keys.ts: key, guard, what it calls)
    ## tauri-core mock   (cases for demo/mock/tauri-core.ts: command → fixture)
    ## fork-shots        (scenario entries for demo/fork-shots.mjs, phase-tagged)
    ## TOUCHLIST         (rows: file | symbol | change | reason)
    ## DECISIONS         (dated bullets for anything you changed against the plan)
    ## migrations        (file name of the SQL you wrote under fork/migrations/, if any)
    ## Cargo.toml        (dependency lines, if any; say why)
    ## status            (what is done, what is not, gate results)

Your module's `pub mod` line is pre-declared in `fork/mod.rs` by the main
session where the plan names one; check `fork/mod.rs` before assuming.

## Strings

Use `t("fork.<phase>.<key>")` freely; `npm run check` accepts unknown keys.
List every key with its English text under `## en.json` in your pending file.

## Commands (Rust)

Put `#[tauri::command]` functions in your own module. They are registered by
the main session from your pending file. Until then they are unreachable from
the UI: that is expected. Frontend wrappers go in `src/fork/<yours>/api.ts`.

## Gates

Run them yourself before you report: `npm run check`, `cargo fmt`,
`cargo clippy --all-targets --manifest-path src-tauri/Cargo.toml -- -D warnings`,
`cargo test --manifest-path src-tauri/Cargo.toml <your module>`, and
`node scripts/fork/contrast.mjs` if you touched tokens. Cargo serialises on
the target-dir lock when several agents build at once: wait, do not kill it.
The screenshot harness is `node demo/fork-shots.mjs <phase> <scenario>`; if
port 1421 is busy with another agent's run, wait and retry. Look at the PNGs
you make and fix what is visibly wrong. Put PNGs in `docs/fork/shots/<phase>/`.

## Safety (binding)

Never send an email, never create or change a calendar event with guests,
never post anything anywhere. No secrets in the DB, settings, logs or repo:
Windows Credential Manager via `crate::secrets` with `fork:` keys only. Do
not run git commands that change history; do not commit (the main session
commits per step). Do not touch `C:\Users\Patrick\OS` unless your phase says
so explicitly.
