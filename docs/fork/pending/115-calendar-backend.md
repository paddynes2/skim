# Calendar backend handoff for 1.1.5

Base: `9351b1f`. Worktree: `C:/Users/Patrick/Projects/skim-daily-use-20261007`.

## Registration

Add these entries beside the existing calendar commands in `lib.rs`:

```rust
fork::calendar::commands::fork_cal_availability,
fork::calendar::commands::fork_cal_sync_state,
fork::calendar::commands::fork_cal_retry,
fork::calendar::commands::fork_cal_discard_split,
```

The calendar module declares its new files. No migration, dependency, authentication, OAuth scope or credential change is needed.

## IPC contract

- `fork_cal_availability(accountId, fromTs, toTs, attendees, excludeEventId?)` returns `{from_ts, to_ts, calendars}`. Each calendar item has `{email, status, reason, busy}`. Status is `available` or `unknown`. Busy periods have `{start_ts, end_ts, summary}`. An available calendar can contain busy periods. Unknown never means free. The account item combines primary and locally selected calendars. Guests need a listed calendar with reader, writer or owner access. Reads use existing events scope, not free/busy scope. All-day periods use each calendar's time zone. Limits: 31 days and 20 guest addresses.
- `fork_cal_sync_state(accountId)` returns `{pending_ops, failed_ops, events, syncing, last_synced_at, last_sync_error}`. The last successful pull time is Unix seconds and is null until success in this app session. Event state has `{event_id, status, message, can_discard}`. Status is pending or failed. `can_discard` applies only to failed series splits.
- `fork_cal_retry(accountId, eventId?)` resets failed operations in scope and returns their count. Completed writes are absent from the queue and cannot be retried.
- `fork_cal_discard_split(accountId, eventId, sendUpdates)` discards a failed split with server marker and version checks. `sendUpdates` is explicitly all or none, as in the existing event actions. A completed split with a lost reply is retained, and its stale failed operation is cleared.
- Existing `fork_cal_patch` accepts `EventInput.following: boolean` with default false. It is mutually exclusive with `series`. Keep the selected occurrence ID and occurrence fields. The backend fetches the actual occurrence and its series master. It uses originalStartTime for the cutoff.

## Recovery UI

Root owns this remaining UI integration. Show the action when `can_discard` is true. Reuse the existing notification confirmation.

- Action: `Discard failed series change`
- Explanation: `Remove the replacement series and keep the original schedule. A completed change will be kept.`
- Notification choice: pass the user's explicit all or none choice.

## Scheduling and retry behaviour

The split preserves COUNT remainder by reading all original occurrence times, including cancelled and moved instances. UNTIL retains the original ending on the replacement. The old series stops immediately before the selected original occurrence. An edit from the first occurrence updates the original master without creating another series.

The replacement uses a stable, client-generated Google ID and private operation marker. The original trim uses its fetched ETag with If-Match and adds a completion marker. Retries read those markers before any write. Replacement creation precedes truncation, so a failed create leaves the original schedule intact.

Google does not provide a transaction for the two events. If replacement creation succeeds and trimming fails, both series can temporarily exist. Retry resumes without another insert or repeated completed notification writes. A changed master version produces a conflict. Discard deletes only the marked replacement, only while the original recurrence remains intact, and uses the replacement ETag. It refuses foreign IDs or ambiguous changed schedules. It preserves both series after a completed split.

The queue records operation errors. Failed creates remain visible. Pending and failed local edits survive cache refresh. Failed operations block later writes to the same source event or series, not unrelated calendars. Edits and deletes to a series with an unresolved split require resolving that split first. Discard claims are recoverable after an app restart. Normal creates and patches also use stable IDs or operation markers to avoid replay after lost replies.

## Limits

- Split accepts one RRULE with daily, weekly, monthly or yearly frequency. Multiple recurrence lines, EXDATE/RDATE, unsupported rule keys, COUNT over 10,000, and incomplete COUNT instance results are refused with an explicit message.
- Following edits create a new series. Future individual exception customizations are not copied. New-series guest responses reset. Existing Meet credentials are not reused; a new Meet request is created when needed.
- Availability does not request new OAuth scopes. Unlisted calendars, freeBusyReader-only calendars and failed reads remain unknown.
- A split conflict where both the original recurrence and replacement changed outside Skim requires review in Google Calendar. The discard guard does not guess which event to delete.
- No real Google reads, event writes or invitations were used for verification.

## Verification

Focused command: `cargo test --manifest-path src-tauri/Cargo.toml fork::calendar --lib` with the shared CARGO_TARGET_DIR. Last completed run: 38 passed, zero failed.

`cargo clippy --manifest-path src-tauri/Cargo.toml --lib -- -D warnings` passed. Root must run final gates and prove the added tests after integration.

`scheduling_tests.rs` covers COUNT with moved/cancelled originals, UNTIL, all-day cutoff, first occurrence, incomplete rules, failure before creation, failure after creation, lost insert and trim replies, external edits, discard identity guards, completed split preservation, interrupted discard recovery, cached failed edits and retry isolation. Availability tests cover inaccessible and malformed calendars, all-day time zones and exclusion of cancelled/free/declined events.

## Primary API evidence

Read on 2026-10-07:

- [Recurring events](https://developers.google.com/workspace/calendar/api/guides/recurringevents): originalStartTime identifies a moved occurrence. Following edits require splitting the series rather than patching every instance. Later exceptions reset under this operation.
- [Resource versions](https://developers.google.com/workspace/calendar/api/guides/version-resources): If-Match protects a conditional update or deletion. A supplied resource ID prevents a second successful insert using the same ID.
- [Event insert](https://developers.google.com/workspace/calendar/api/v3/reference/events/insert): event IDs use base32hex characters. UUID hexadecimal strings meet this restriction. The request carries sendUpdates, conferenceDataVersion and supportsAttachments explicitly.
- [Event instances](https://developers.google.com/workspace/calendar/api/v3/reference/events/instances): showDeleted includes cancelled occurrences, with pagination. COUNT validation uses the complete returned original occurrence set.

Skill used for error and handoff text: `C:/Users/Patrick/.agents/skills/asd-ste100/SKILL.md`.
