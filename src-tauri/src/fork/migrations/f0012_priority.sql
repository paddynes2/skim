-- v1.1.11 (D64): Priority. `fork_priority_score` keeps one Jev verdict per
-- message, keyed by account, sender and normalized Message-ID (or
-- `id:<messages.id>` when it has none), so the Inbox and Important copies of
-- one message share it and a message is never asked about twice. `score` is
-- NULL when Jev could not judge the message; it is still charged and kept. `fork_priority` is the pass's answer: the
-- conversations that sort first in the Inbox while they are unread there.
CREATE TABLE IF NOT EXISTS fork_priority_score (
  msg_key   TEXT PRIMARY KEY,
  score     REAL,
  model     TEXT,
  scored_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS fork_priority (
  thread_id INTEGER PRIMARY KEY,
  reason    TEXT NOT NULL CHECK(reason IN ('deal','starred','jev'))
);
