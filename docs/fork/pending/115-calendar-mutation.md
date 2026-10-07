# Calendar semantic mutation evidence for 1.1.5

Base commit: `30bc9d1`. Date: 2026-10-07.

Each mutation changed production Rust code only. Each run used one existing focused test. Source files were restored from exact saved bytes in a finally block. No tests were removed. No live Google calls occurred.

## COUNT remainder off by one

Mutation: `(count - before).to_string()` to `(count - before + 1).to_string()`.

Command: `cargo test --manifest-path src-tauri/Cargo.toml --lib fork::calendar::scheduling_tests::following_count_uses_original_times_and_keeps_the_remaining_count -- --exact`.

Result: **PROVEN**. Exit code: 101. Exact source restoration: True.

Original and restored SHA256: `03387a99b5b04e9bcbd4f709a68c9da4b88192dec6e5a6140cd46b9ed7097a80`.

```text
failures:

---- fork::calendar::scheduling_tests::following_count_uses_original_times_and_keeps_the_remaining_count stdout ----

thread 'fork::calendar::scheduling_tests::following_count_uses_original_times_and_keeps_the_remaining_count' (25948) panicked at src\fork\calendar\scheduling_tests.rs:42:5:
assertion `left == right` failed
  left: Array [String("RRULE:FREQ=DAILY;COUNT=4")]
 right: Array [String("RRULE:FREQ=DAILY;COUNT=3")]
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    fork::calendar::scheduling_tests::following_count_uses_original_times_and_keeps_the_remaining_count

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 536 filtered out; finished in 0.00s

error: test failed, to rerun pass `--lib`
```

## Foreign replacement discard guard disabled

Mutation: `if replacement["extendedProperties"]["private"]["skimSplitSource"] != plan.token {` to `if false && replacement["extendedProperties"]["private"]["skimSplitSource"] != plan.token {`.

Command: `cargo test --manifest-path src-tauri/Cargo.toml --lib fork::calendar::scheduling_tests::discard_removes_only_the_marked_replacement_and_keeps_completed_splits -- --exact`.

Result: **PROVEN**. Exit code: 101. Exact source restoration: True.

Original and restored SHA256: `03387a99b5b04e9bcbd4f709a68c9da4b88192dec6e5a6140cd46b9ed7097a80`.

```text
failures:

---- fork::calendar::scheduling_tests::discard_removes_only_the_marked_replacement_and_keeps_completed_splits stdout ----

thread 'fork::calendar::scheduling_tests::discard_removes_only_the_marked_replacement_and_keeps_completed_splits' (29616) panicked at src\fork\calendar\scheduling_tests.rs:263:5:
assertion failed: split::discard(&mut remote, &plan, SendUpdates::None).await.is_err()
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    fork::calendar::scheduling_tests::discard_removes_only_the_marked_replacement_and_keeps_completed_splits

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 536 filtered out; finished in 0.00s

error: test failed, to rerun pass `--lib`
```

## Held local edit preservation disabled

Mutation: `if held {` to `if false && held {`.

Command: `cargo test --manifest-path src-tauri/Cargo.toml --lib fork::calendar::scheduling_tests::failed_local_edits_survive_refresh_and_retry_never_replays_completed_ops -- --exact`.

Result: **PROVEN**. Exit code: 101. Exact source restoration: True.

Original and restored SHA256: `bbd89082fc93d4c06e5aa35c8527a87bfc75236b73c9f74dd9bf54dcfd933ba5`.

```text
failures:

---- fork::calendar::scheduling_tests::failed_local_edits_survive_refresh_and_retry_never_replays_completed_ops stdout ----

thread 'fork::calendar::scheduling_tests::failed_local_edits_survive_refresh_and_retry_never_replays_completed_ops' (52820) panicked at src\fork\calendar\scheduling_tests.rs:355:9:
assertion `left == right` failed
  left: "Original"
 right: "Edited offline"
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    fork::calendar::scheduling_tests::failed_local_edits_survive_refresh_and_retry_never_replays_completed_ops

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 536 filtered out; finished in 0.01s

error: test failed, to rerun pass `--lib`
```

## Completed split marker ignored

Mutation: `if master["extendedProperties"]["private"]["skimSplitApplied"] == plan.token {` to `if false && master["extendedProperties"]["private"]["skimSplitApplied"] == plan.token {`.

Command: `cargo test --manifest-path src-tauri/Cargo.toml --lib fork::calendar::scheduling_tests::split_retries_preserve_original_and_do_not_repeat_completed_writes -- --exact`.

Result: **PROVEN**. Exit code: 101. Exact source restoration: True.

Original and restored SHA256: `03387a99b5b04e9bcbd4f709a68c9da4b88192dec6e5a6140cd46b9ed7097a80`.

```text
failures:

---- fork::calendar::scheduling_tests::split_retries_preserve_original_and_do_not_repeat_completed_writes stdout ----

thread 'fork::calendar::scheduling_tests::split_retries_preserve_original_and_do_not_repeat_completed_writes' (67164) panicked at src\fork\calendar\scheduling_tests.rs:203:10:
called `Result::unwrap()` on an `Err` value: Other { code: "gcal_conflict", message: "The recurring event changed in Google Calendar. Review the series before retrying this change." }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    fork::calendar::scheduling_tests::split_retries_preserve_original_and_do_not_repeat_completed_writes

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 536 filtered out; finished in 0.00s

error: test failed, to rerun pass `--lib`
```

## Restored calendar suite

Command: `cargo test --manifest-path src-tauri/Cargo.toml fork::calendar --lib`.

Exit code: 0.

```text
test result: ok. 38 passed; 0 failed; 0 ignored; 0 measured; 499 filtered out; finished in 0.15s
```
