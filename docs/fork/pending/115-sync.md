# Persistent mail queue status, 1.1.5

Mount src/fork/sync/SyncStatus.svelte in Sidebar above the Settings footer. No props. Add pub mod mail_status to fork/mod.rs and register fork::mail_status::{fork_mail_queue_status,fork_mail_retry_flag}. Merge 115-sync-strings.json. Demo returns {pending:0,scheduled:0,failed:0,failures:[]} for fork_mail_queue_status, and null for retry (or supply explicit fictional failed fixture).

Status reads the existing pending_ops and fork_op_schedule tables. The active account scopes counts; unified mode reads all. Future scheduled actions have a distinct count. Failed actions remain visible across restarts because the UI reads their durable state. Refresh follows mail:updated, sync:status, ops:failed and a 15-second poll. Cached content and pending writes are described separately. Idle is labelled Mail ready, not proof of server sync.

Retry allows only validated set_flag operations with seen/flagged, explicit boolean, nonempty valid UIDs and IMAP folder. It requires an active engine, verifies failed state and account inside the transaction, resets attempts and wakes the existing run_ops. No new operation is created. Sends, invitations, unsubscribe, move/delete/archive, drafts and folder mutations cannot be retried through this command. The UI gives them a manual-review notice and retains the failure. No failures are deleted or dismissed by this surface. Backend test exercises counts including future schedule, account scope, allowed retry, idempotent repeated retry, rejection of malformed and forbidden operations and preservation of failed sends.

No migration or live action used. Root owns final compilation after module registration, browser coverage and proof.
