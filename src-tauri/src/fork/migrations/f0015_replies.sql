-- v1.1.19 (D70): To reply. `fork_reply_score` keeps one TypeSafe Jev verdict
-- per message on the reply question ("does the sender expect a personal reply
-- from them?"), keyed like `fork_priority_score` (account, sender,
-- Message-ID), so every folder copy of a message shares it. `score` is NULL
-- when Jev could not judge the message; it is still charged and kept.
-- `fork_reply_mark` is the user's own call on a conversation: `done` (no reply
-- needed) or `needs` (a reply is owed). `msg_key` is the key of the
-- conversation's last message when the mark was set; the mark holds while
-- that is still the last message (`fork_court.last_message_id`), so the next
-- message in the conversation, from anyone, ends it.
CREATE TABLE IF NOT EXISTS fork_reply_score (
  msg_key   TEXT PRIMARY KEY,
  score     REAL,
  model     TEXT,
  scored_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS fork_reply_mark (
  thread_id INTEGER PRIMARY KEY,
  msg_key   TEXT NOT NULL,
  mark      TEXT NOT NULL CHECK(mark IN ('done','needs')),
  marked_at INTEGER NOT NULL
);
