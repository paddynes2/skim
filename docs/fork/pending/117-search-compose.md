# Search, files and compose handoff

## mod.rs

Root registered `pub mod attachments;`.

## generate_handler

Already registered by root:

```rust
fork::attachments::fork_attachment_preview,
fork::attachments::fork_attachment_fingerprints,
fork::attachments::fork_save_attachments,
fork::search_query::fork_search_coverage,
```

## commands.rs

No addition. The palette search command in `src-tauri/src/commands/search.rs` now obtains matching passages through the shared search helper.

## settings ALLOWED

`fork_compose_snippets` stores a validated JSON list of up to 40 named plain-text snippets. Root added the setting.

## en.json

Merge `116-search-compose-strings.json` in this directory. Root has already merged it. The `116` names on this JSON file, the compose browser harness, and the helper test file are retained from the initial task naming. Their contents belong to this release.

## App.svelte

No addition from this agent. Root fixed the native-dialog keyboard guard and added its assertions to the files browser harness.

## keys

No global shortcuts added.

## tauri-core mock

Targeted existing mock updates are present. `get_draft` returns a clone; local saves can fail or complete after a delay; staged draft attachments and rich HTML persist across page reload. Flags:

- `skimdemo.compose_save_error=on`: fail local save.
- `skimdemo.compose_save_delay=700`: delay local save by 700 ms.
- `skimdemo.compose_open_error=on`: fail window opening.
- `skimdemo.attachment_save_error=on`: fail individual file save.
- `skimdemo.search_match=older`: palette and grouped search return exact message 1009 and a matching passage.
- `skimdemo.fork_body=wide`: render a full-width four-column email table.

`fork_search_coverage` returns `{cachedMessages,totalMessages,attachmentMessages}`. Preview, fingerprint and Save all fixture commands are registered.

## fork-shots

Behavior harnesses:

```text
node demo/v116-compose-check.mjs
node demo/v117-search-files-check.mjs
```

These use the static built demo on port 1431 by default. `SKIM_DEMO_URL` overrides the URL. `SKIM_SKIP_BUILD=1` skips a redundant demo build. Both use synthetic fixture data and do not send email or change a live mailbox.

## TOUCHLIST

| File | Change | Purpose |
| --- | --- | --- |
| `src-tauri/src/fork/search_query.rs` | Literal filename filter, matching passage, exact message ID, coverage command, DB tests | Open the email that actually matched |
| `src-tauri/src/commands/search.rs` | Shared matching passage | Palette results display the match |
| `src/fork/search/{query,filters,passages}.ts` | Filename operator and highlight helpers | Preserve query semantics and escaped text |
| `src/fork/search/{HighlightedText,SearchTools}.svelte` | Safe highlights and cached filename coverage | Explain search scope |
| `src/components/CommandPalette.svelte` | Keep search before selecting exact message | Retain results when opening a match |
| `src-tauri/src/fork/attachments.rs` | Preview, SHA256 identity, safe batch saving | Reusable file operations |
| `src/fork/attachments/groups.ts` | Group only verified identical content | Keep distinct versions separate |
| `src/components/AttachmentChips.svelte` | Preview modal, Save all, source links, errors | Shared files UI for reading and companies |
| `src/fork/compose/{workflow.ts,Snippets.svelte}` | Snippets, attachment reminder, serialized saves | Reuse text and preserve draft state |
| `src/components/ComposeForm.svelte` | Save status/retry, staged-file tracking, safe transfer/close | Preserve latest edits and attachments |
| `demo/mock/{tauri-core,fork-compose}.ts` | Stateful failure/delay fixtures | Prove user flows without live writes |

The shared attachment component accepts `attachments: AttachmentMeta[]`, optional `onsource(file)` and optional `sourceLabel(file)`. Reading and Deals agents have integrated these props. Root integrated `HighlightedText` into MessageRow.

## DECISIONS

- 2026-10-07: PDF preview is explicitly a text preview. Existing `lopdf` extracts up to 50 pages and 80,000 characters from files up to 25 MB. It does not render page layout, scans or diagrams. Open original remains available. No CSP change or dependency was added.
- 2026-10-07: Image preview accepts byte signatures for PNG, JPEG, GIF and WebP. SVG/HTML do not become active image previews.
- 2026-10-07: Identical grouping uses SHA256 and size, with a 200-file and 128 MB aggregate hashing budget. Unhashed files remain separate. Save all saves one copy per verified identical group and does not overwrite existing files; name collisions receive a numbered suffix.
- 2026-10-07: Compose becomes inert during window transfer so edits cannot arrive after the final captured snapshot. Failed saves or failed window opening keep the editor and its text visible. A native close request saves before closing. Attachments finish staging before transfer.
- 2026-10-07: Search filename matching is literal, case-insensitive SQLite matching over cached attachment metadata. `%` and `_` have no wildcard meaning. Query opening retains search state and targets the actual matching message.

## migrations

None.

## Cargo.toml

No changes. Uses existing `lopdf`, `sha2`, `base64`, dialog and runtime dependencies.

## status

Implemented and integrated within the assigned scope. No source commits from this agent.

Evidence:

- `node --test src/fork/tests/search-compose116.test.mjs src/fork/tests/search115.test.mjs`: 6 passed after restoring semantic mutations.
- `cargo test --manifest-path src-tauri/Cargo.toml fork::search_query::tests`: 9 passed before mutation; restored rerun recorded in the parent handoff message.
- `cargo test --manifest-path src-tauri/Cargo.toml fork::attachments::tests`: 2 passed.
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`: passed.
- `npm run check`: zero errors, one pre-existing ComposeForm `smell` reactivity warning.
- `demo/v116-compose-check.mjs`: dark/plain and light/rich passed. Checks attachment reminder with no send, local-save failure/retry, failed popout preservation, delayed autosave ordering, latest body/file preservation after popout and reload, bold HTML preservation, and snippet save/insert.
- `demo/v117-search-files-check.mjs`: matching passage, exact older message, retained query, filename coverage, PDF text preview, Open original, Save all and source links passed. Root extended this with native-dialog shortcut isolation; the preview Save file error assertion was added after the initial pass for root's final rerun.
- Three isolated semantic mutations failed assertions, with exact source bytes restored in `finally`: filename-only grouping, completion before save/open, and selection of the wrong existing message. Full commands and output: `117-search-compose-mutations.md`. An initial None-ID mutation produced a database error and was rejected as proof; the revised wrong-message mutation failed the intended assertion.

Concrete limits:

- Filename/body search covers cached mail; it does not fetch remote mail or search file contents/OCR. The UI states cached coverage.
- Matching passage uses literal lowercase terms from the query against cached body text, subject and preview. FTS accent/tokenizer matches that do not have an identical lowercase substring can fall back to the stored preview; frontend highlighting itself folds accents.
- Image preview is visual; PDF preview is text only. The 25 MB input limit does not strictly bound PDF decompression memory in `lopdf`.
- Save all supports up to 200 distinct file groups at once. Larger lists keep individual save actions; Save all is disabled.
- Snippets are local settings and plain text. They insert into the rich editor as text, preserving the rest of the draft.
- The tested transition is inline reply to a compose window and reopening the saved draft. There is no new direct button to collapse a compose window back into the thread; the existing Drafts flow remains the return path.
- Browser tests cover mocked native calls. The final installed-app check and installer are root-owned.
