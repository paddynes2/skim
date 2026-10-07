# Calendar UI 1.1.5 handoff

Implemented in the shared worktree without a commit.

- Day, Week, Workweek, Month and Agenda. Workweek hides Saturday and Sunday. Visible-hour selectors keep their settings. The title opens a six-week month navigator. Calendar checkboxes sit above the grid.
- Grid overlap is side by side; normal event titles wrap and slot height is 32px. Event click opens a 320px preview with full title, date, guests, location, description and sync state. Edit opens the existing large editor. Keyboard Enter opens the library event button. Escape closes; focus returns to the clicked event.
- Duplicate opens a populated draft. It copies guests and settings, excludes recurrence and Meet links, and never writes until Create. Existing guest confirmation remains.
- Event editor checks cached selected-calendar overlap, including date-only all-day boundaries in the selected time zone. Find a time calls the new availability API for seven days in the selected zone and working hours. Unknown guests are listed explicitly. Suggestions mean available only for calendars that were checked. Input changes invalidate results.
- Recurring editor scope selects occurrence, following or entire series. Following keeps the selected instance ID, dates and fields while reading master recurrence. Backend rejects unsupported complex schedules. Following deletion is deliberately unavailable; this task requested edits.
- Pending and failed sync status is shown per event. Failed operations have Retry in preview and at the top of the grid. A failed status read is unknown, not synced. Cached rows stay visible during refresh; stale range results cannot replace the current range.

Root integration:

- Merge `115-calendar-ui-strings.json` into en.json.
- Allow settings keys `fork_cal_visible_start` and `fork_cal_visible_end`.
- Backend contract coordinated with calendar_backend: EventInput.following, fork_cal_availability, fork_cal_sync_state and fork_cal_retry.
- Existing v114 demo test must click Edit event after clicking a grid event. Series scope now uses the Apply changes to select.

Verification: `npm run check` passes, with the existing ComposeForm smell warning. `node --experimental-strip-types --test src/fork/tests/cal115-planning.test.mjs` passes 5 tests. New test imports a new helper, so prove-test may need mutation mode. Browser checks pass in both Base themes with fictional fixtures. See demo/v115-check.mjs and docs/fork/shots/1.1.5. Captures cover 1600, 1200 and 900px. The editor occupies the full calendar region below 1100px.

Limits: no live Google writes or invitations tested. Suggestions depend on the existing grant and Google sharing. Complex recurrence behavior is gated by backend errors with Google Calendar escape hatch. Human visual acceptance is not recorded.
