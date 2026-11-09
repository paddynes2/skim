-- Calendar round 1: the iCalUID (one meeting seen by two accounts shares it),
-- Google's eventType (meeting alerts skip out-of-office, working-location and
-- focus-time entries), the end's own time zone, and the alert keys already
-- fired, so a restart never refires one.
-- Additive: earlier app versions ignore the new columns and table. Existing
-- rows get the two columns on their next pull.
ALTER TABLE fork_cal_events ADD COLUMN ical_uid TEXT;
ALTER TABLE fork_cal_events ADD COLUMN event_type TEXT;
ALTER TABLE fork_cal_events ADD COLUMN end_time_zone TEXT;

CREATE TABLE IF NOT EXISTS fork_cal_alerts_fired (
  account_id TEXT NOT NULL,
  google_id  TEXT NOT NULL,
  start_ts   INTEGER NOT NULL,
  lead_min   INTEGER NOT NULL,
  fired_at   INTEGER NOT NULL,
  PRIMARY KEY (account_id, google_id, start_ts, lead_min)
);
