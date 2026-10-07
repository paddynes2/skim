-- Event options from Google sync and local edits feed the event editor.
-- Additive: earlier app versions ignore this column.
ALTER TABLE fork_cal_events ADD COLUMN options_json TEXT NOT NULL DEFAULT '{}';
