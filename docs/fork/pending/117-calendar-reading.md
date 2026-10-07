# Calendar and conversation handoff for 1.1.7

## Implemented

ReadingPane has one conversation toolbar, a labelled More menu for Move, Spam and Delete, clear sender and date hierarchy, expandable To/Cc details and a compact header layout when the reading pane is narrow. Escape closes More and retains the selected email. Conversation search uses readable matching passages and escaped highlights. Thread files use the shared attachment component and link to their exact source message. Plain prose is limited to 76ch; tables and wide newsletters retain full width. Existing iframe identity and cached body behavior remain intact.

Calendar events use separate time and title lines; short events give the title priority. Accepted, tentative, declined and cancelled states have visible marks and styles, full detail tooltips and preview labels. The all-day row scrolls after 120px and shows a count hint when crowded. The optional second time-zone column uses the existing fork_cal_second_tz setting. Its legend explains that the selected date determines the offset; Day view is appropriate across a DST transition within a week. Drag and resize retain guest confirmation and give explicit queued or no-notification feedback after the local save.

Event attachments and meeting notes accept HTTPS links, with a maximum of 25. Existing links display in preview and editor. New links can be removed before save. Existing links remain read-only in Skim; remove them in Google Calendar. Additions are disabled for whole-series and following-series edits. No upload or file sharing changes occur. The existing calendar.events scope and options_json persistence suffice: no auth or migration change. The backend merges additions with a freshly fetched remote attachment list before patch, so stale local metadata cannot remove a remote file. Existing remote metadata is retained and retry is idempotent.

## Verification

- npm run check: zero errors; only the existing ComposeForm smell warning remained at the last complete run.
- node --experimental-strip-types --test src/fork/tests/cal117-presentation.test.mjs: 2 passed.
- cargo test --manifest-path src-tauri/Cargo.toml fork::calendar::options_tests --lib: 4 passed after restoration.
- Semantic mutation dropping remote attachments: KILLED by expected count 2 versus actual 1.
- Semantic mutation allowing HTTP: KILLED by HTTPS validation assertion.
- options.rs restored byte for byte after mutations, SHA256 39327a9ed13f2619d3aa88518c0341836148550f443516040c99243a0be37964.
- node demo/v117-reading-calendar.mjs: Base dark and light passed. Covers compact toolbar, prose and table widths, recipients, More Escape, exact search highlights, event states, time zones, all-day scrolling, drag confirmation feedback, invalid URL rejection, link save/reopen, and 1200/900 layout.
- node demo/refresh-check.mjs: zero reloads, loading rows, refresh notes or movement; same iframe, document and scroll.
- Screenshots inspected in docs/fork/shots/1.1.7: reading and calendar at 1600, reading/editor at 1200 and 900 in both Base themes. Fictional demo data only.

The browser suite uses the current worktree static server at http://127.0.0.1:1427. It builds demo/dist-demo before tests. Set SKIM_CAPTURE=1 to refresh screenshots. Port 1421 belongs to the older worktree.

## Integration

Merge the flat map in 117-calendar-reading-strings.json into en.json. No new settings whitelist key is needed. The source_message and remove_new_link keys were added after the first map handoff. The v115 file-chip selector now matches the shared flattened attachment list.

The shared attachment and search components are owned by the search/compose agent. No live calendar writes, notifications or email sends were performed. Google attachment acceptance is not live-tested. Root owns full gates, post-commit proof, installation and final review.

Official contract references: https://developers.google.com/workspace/calendar/api/v3/reference/events and https://developers.google.com/workspace/calendar/api/guides/create-events. The existing Google path already uses supportsAttachments=true. fileUrl is writable; fileId is preserved from remote metadata.
