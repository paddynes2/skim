-- Phase 7.3: the calendar store. One row per Google calendar the account can
-- see, one per event instance inside the sync window (-60d/+180d,
-- singleEvents=true, so a recurring series lands as its instances), and the
-- offline op queue shaped like upstream's pending_ops.
CREATE TABLE IF NOT EXISTS fork_cal_calendars (
  id          INTEGER PRIMARY KEY,
  account_id  TEXT NOT NULL,
  google_id   TEXT NOT NULL,
  summary     TEXT NOT NULL DEFAULT '',
  color       TEXT,
  is_primary  INTEGER NOT NULL DEFAULT 0,
  selected    INTEGER NOT NULL DEFAULT 0,
  access_role TEXT NOT NULL DEFAULT 'reader',
  UNIQUE(account_id, google_id)
);

CREATE TABLE IF NOT EXISTS fork_cal_events (
  id                 INTEGER PRIMARY KEY,
  calendar_id        INTEGER NOT NULL REFERENCES fork_cal_calendars(id) ON DELETE CASCADE,
  google_id          TEXT NOT NULL,
  etag               TEXT,
  status             TEXT NOT NULL DEFAULT 'confirmed',
  summary            TEXT NOT NULL DEFAULT '',
  description        TEXT,
  location           TEXT,
  start_ts           INTEGER NOT NULL,
  end_ts             INTEGER NOT NULL,
  all_day            INTEGER NOT NULL DEFAULT 0,
  start_date         TEXT,
  end_date           TEXT,
  time_zone          TEXT,
  recurring_event_id TEXT,
  organizer_email    TEXT,
  attendees_json     TEXT,
  self_response      TEXT,
  transparency       TEXT,
  hangout_link       TEXT,
  html_link          TEXT,
  updated            TEXT,
  local_only         INTEGER DEFAULT 0,
  UNIQUE(calendar_id, google_id)
);

CREATE INDEX IF NOT EXISTS fork_cal_events_start ON fork_cal_events(start_ts);

CREATE TABLE IF NOT EXISTS fork_cal_ops (
  id         INTEGER PRIMARY KEY,
  account_id TEXT NOT NULL,
  kind       TEXT NOT NULL,
  payload    TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  attempts   INTEGER NOT NULL DEFAULT 0,
  state      TEXT NOT NULL DEFAULT 'pending'
);
