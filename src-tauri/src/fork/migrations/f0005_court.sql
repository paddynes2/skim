-- Phase 10: "Ball in my court". One row per thread the deterministic pass
-- has looked at. `state` is what the sidebar shows; `since` is the date of the
-- last message (unix seconds), which the views sort on oldest first;
-- `last_message_id` is the `messages.id` the verdict was computed from, so an
-- unchanged thread is never re-classified; `needs_reply` / `model` are the AI
-- pass's verdict (NULL until it has run for that `last_message_id`).
CREATE TABLE IF NOT EXISTS fork_court (
  thread_id       INTEGER PRIMARY KEY,
  account_id      TEXT NOT NULL,
  state           TEXT NOT NULL CHECK(state IN ('on_me','waiting','none')),
  since           INTEGER,
  last_message_id INTEGER,
  needs_reply     INTEGER,
  reason          TEXT,
  model           TEXT,
  computed_at     INTEGER
);
CREATE INDEX IF NOT EXISTS idx_fork_court_state_since ON fork_court(state, since);
