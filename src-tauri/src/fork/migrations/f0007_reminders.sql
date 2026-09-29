-- v1.1.1: Snooze and Follow-ups, the replacement for the On me / Waiting views.
-- Both are set by Patrick, per thread; nothing is inferred.
--   snooze:   the thread leaves every list until `due_ts`, then comes back
--             pinned to the top until opened (`seen_ts`). A reply from someone
--             else brings it back early.
--   followup: "remind me if no reply by `due_ts`". A message in the thread from
--             anyone but him after `set_ts` answers it (the row is deleted).
--             Unanswered at `due_ts`, the thread pins to the top until opened.
-- A follow-up set while composing a new email has no thread yet: `thread_id`
-- stays NULL and `subject` finds the Sent copy once it syncs.
CREATE TABLE IF NOT EXISTS fork_reminders (
  id          INTEGER PRIMARY KEY,
  kind        TEXT NOT NULL CHECK(kind IN ('snooze','followup')),
  thread_id   INTEGER,
  subject     TEXT,
  set_ts      INTEGER NOT NULL,
  due_ts      INTEGER NOT NULL,
  seen_ts     INTEGER,
  notified_ts INTEGER
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_fork_reminders_thread_kind
  ON fork_reminders(thread_id, kind) WHERE thread_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_fork_reminders_due ON fork_reminders(due_ts);
