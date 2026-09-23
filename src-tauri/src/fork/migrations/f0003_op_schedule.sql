-- Phase 6.4: one scheduler for undo send and send later. A row holds the
-- pending_ops row it points at out of the drain until `not_before`
-- (sync.rs::drain_ops adds `AND id NOT IN (SELECT op_id FROM fork_op_schedule
-- WHERE not_before > unixepoch())`). The row goes with its op (cascade), so a
-- drained or cancelled op leaves nothing behind. `kind` mirrors the op kind
-- ('send' today); `label` is what the user picked ("Tomorrow 08:00"), for the
-- Scheduled list.
CREATE TABLE IF NOT EXISTS fork_op_schedule (
  op_id      INTEGER PRIMARY KEY REFERENCES pending_ops(id) ON DELETE CASCADE,
  not_before INTEGER NOT NULL,
  kind       TEXT NOT NULL,
  label      TEXT
);
CREATE INDEX IF NOT EXISTS fork_op_schedule_not_before ON fork_op_schedule(not_before);
