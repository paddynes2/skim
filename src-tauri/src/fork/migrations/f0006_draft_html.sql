-- Phase 6.3: rich text. `drafts.body_text` stays the full plain-text body
-- (words + signature + quoted original) exactly as upstream writes it; this
-- table holds only the user's own words as HTML, when the rich editor was
-- used. No row = the message goes out text/plain as before (D5).
CREATE TABLE IF NOT EXISTS fork_draft_html (
  draft_id   INTEGER PRIMARY KEY REFERENCES drafts(id) ON DELETE CASCADE,
  words_html TEXT NOT NULL,
  updated_at INTEGER NOT NULL
);
