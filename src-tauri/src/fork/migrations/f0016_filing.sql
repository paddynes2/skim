-- v1.1.24 (D72): Filing. TypeSafe Jev is asked, for each rule in the local
-- `fork_filing` setting ("Folder: description" per line), whether the newest
-- Inbox message of a conversation belongs in that folder. `fork_filing_score`
-- keeps one verdict per message and rule, keyed like `fork_priority_score`
-- (account, sender, Message-ID); `rule` is the folder name, lowercased.
-- `score` is NULL when Jev could not judge the message; it is still kept so
-- the message is never billed twice.
-- `fork_filing_log` is one row per message Skim moved, by account and
-- Message-ID (thread ids do not survive a move). A logged message seen in the
-- Inbox again was moved back by the user: its sender goes to
-- `fork_filing_never` for that rule and is never filed by it again.
CREATE TABLE IF NOT EXISTS fork_filing_score (
  msg_key   TEXT NOT NULL,
  rule      TEXT NOT NULL,
  score     REAL,
  model     TEXT,
  scored_at INTEGER NOT NULL,
  PRIMARY KEY (msg_key, rule)
);
CREATE TABLE IF NOT EXISTS fork_filing_log (
  id         INTEGER PRIMARY KEY,
  account_id TEXT NOT NULL,
  message_id TEXT NOT NULL,
  from_addr  TEXT NOT NULL,
  subject    TEXT NOT NULL DEFAULT '',
  rule       TEXT NOT NULL,
  folder     TEXT NOT NULL,
  score      REAL NOT NULL,
  filed_at   INTEGER NOT NULL,
  undone_at  INTEGER
);
CREATE INDEX IF NOT EXISTS idx_fork_filing_log_msg ON fork_filing_log(account_id, message_id);
CREATE TABLE IF NOT EXISTS fork_filing_never (
  rule      TEXT NOT NULL,
  from_addr TEXT NOT NULL,
  added_at  INTEGER NOT NULL,
  PRIMARY KEY (rule, from_addr)
);
