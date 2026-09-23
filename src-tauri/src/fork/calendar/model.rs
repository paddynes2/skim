//! Rows of the calendar store and the mapping from Google Calendar v3 JSON.
//! Pure: no I/O, so every shape (timed, all-day, cancelled, recurring
//! instance) is tested against a fixture.

use chrono::{DateTime, NaiveDate, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One calendar from `calendarList.list`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CalendarRow {
    #[serde(default)]
    pub id: i64,
    pub account_id: String,
    pub google_id: String,
    pub summary: String,
    pub color: Option<String>,
    pub is_primary: bool,
    pub selected: bool,
    pub access_role: String,
}

/// One event instance from `events.list` (`singleEvents=true`), or a row
/// created locally and not yet pushed (`local_only`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct EventRow {
    #[serde(default)]
    pub id: i64,
    pub calendar_id: i64,
    pub google_id: String,
    pub etag: Option<String>,
    pub status: String,
    pub summary: String,
    pub description: Option<String>,
    pub location: Option<String>,
    /// Unix seconds. All-day: midnight UTC of `start_date`.
    pub start_ts: i64,
    /// Unix seconds, exclusive. All-day: midnight UTC of `end_date`.
    pub end_ts: i64,
    pub all_day: bool,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub time_zone: Option<String>,
    pub recurring_event_id: Option<String>,
    pub organizer_email: Option<String>,
    /// The `attendees` array as Google sent it, verbatim JSON.
    pub attendees_json: Option<String>,
    /// `responseStatus` of the attendee marked `self`, when Patrick is a guest.
    pub self_response: Option<String>,
    pub transparency: Option<String>,
    pub hangout_link: Option<String>,
    pub html_link: Option<String>,
    pub updated: Option<String>,
    pub local_only: bool,
}

/// Prefix of the placeholder `google_id` a local create carries until its op
/// lands. Never sent to Google.
pub const LOCAL_ID_PREFIX: &str = "local-";

pub fn is_local_id(google_id: &str) -> bool {
    google_id.starts_with(LOCAL_ID_PREFIX)
}

fn str_field(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Unix seconds of an RFC 3339 `dateTime`.
pub fn parse_rfc3339(s: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(s).ok().map(|d| d.timestamp())
}

/// Unix seconds of midnight UTC on a `YYYY-MM-DD` date.
pub fn date_to_utc_midnight(s: &str) -> Option<i64> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok().map(|d| {
        d.and_hms_opt(0, 0, 0)
            .expect("midnight")
            .and_utc()
            .timestamp()
    })
}

/// RFC 3339 in UTC, the form Google accepts for `dateTime`.
pub fn to_rfc3339(ts: i64) -> String {
    DateTime::<Utc>::from_timestamp(ts, 0)
        .unwrap_or_default()
        .to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// `YYYY-MM-DD` (UTC) for a timestamp — the inverse of [`date_to_utc_midnight`].
pub fn to_date(ts: i64) -> String {
    DateTime::<Utc>::from_timestamp(ts, 0)
        .unwrap_or_default()
        .format("%Y-%m-%d")
        .to_string()
}

/// The `responseStatus` of the attendee marked `self`, if any.
pub fn self_response(attendees: Option<&Value>) -> Option<String> {
    attendees?
        .as_array()?
        .iter()
        .find(|a| a.get("self").and_then(Value::as_bool).unwrap_or(false))
        .and_then(|a| str_field(a, "responseStatus"))
}

/// Map one `calendarList` resource. `None` when it has no id.
pub fn calendar_from_json(account_id: &str, v: &Value) -> Option<CalendarRow> {
    let google_id = str_field(v, "id")?;
    Some(CalendarRow {
        id: 0,
        account_id: account_id.to_string(),
        google_id,
        summary: str_field(v, "summaryOverride")
            .or_else(|| str_field(v, "summary"))
            .unwrap_or_default(),
        color: str_field(v, "backgroundColor"),
        is_primary: v.get("primary").and_then(Value::as_bool).unwrap_or(false),
        selected: v.get("selected").and_then(Value::as_bool).unwrap_or(false),
        access_role: str_field(v, "accessRole").unwrap_or_else(|| "reader".into()),
    })
}

/// Map one `events` resource into a row of `calendar_id`. `None` when it has
/// no id or no usable start/end. A cancelled instance of a recurring series
/// arrives with `status: cancelled` and often no times; it is kept as a row
/// (so a local copy of the instance is overwritten) with zero-length times.
pub fn event_from_json(calendar_id: i64, v: &Value) -> Option<EventRow> {
    let google_id = str_field(v, "id")?;
    let status = str_field(v, "status").unwrap_or_else(|| "confirmed".into());
    let start = v.get("start");
    let end = v.get("end");
    let start_date = start.and_then(|s| str_field(s, "date"));
    let end_date = end.and_then(|s| str_field(s, "date"));
    let all_day = start_date.is_some();
    let (start_ts, end_ts) = if all_day {
        let s = start_date.as_deref().and_then(date_to_utc_midnight)?;
        let e = end_date
            .as_deref()
            .and_then(date_to_utc_midnight)
            .unwrap_or(s + 86_400);
        (s, e)
    } else {
        let s = start.and_then(|s| str_field(s, "dateTime"));
        let e = end.and_then(|s| str_field(s, "dateTime"));
        match (
            s.as_deref().and_then(parse_rfc3339),
            e.as_deref().and_then(parse_rfc3339),
        ) {
            (Some(s), Some(e)) => (s, e),
            (Some(s), None) => (s, s),
            _ if status == "cancelled" => (0, 0),
            _ => return None,
        }
    };
    let attendees = v.get("attendees");
    Some(EventRow {
        id: 0,
        calendar_id,
        google_id,
        etag: str_field(v, "etag"),
        status,
        summary: str_field(v, "summary").unwrap_or_default(),
        description: str_field(v, "description"),
        location: str_field(v, "location"),
        start_ts,
        end_ts,
        all_day,
        start_date,
        end_date,
        time_zone: start.and_then(|s| str_field(s, "timeZone")),
        recurring_event_id: str_field(v, "recurringEventId"),
        organizer_email: v.get("organizer").and_then(|o| str_field(o, "email")),
        attendees_json: attendees.map(Value::to_string),
        self_response: self_response(attendees),
        transparency: str_field(v, "transparency"),
        hangout_link: str_field(v, "hangoutLink"),
        html_link: str_field(v, "htmlLink"),
        updated: str_field(v, "updated"),
        local_only: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn timed_event_maps_every_column() {
        let v = json!({
            "id": "ev1", "etag": "\"123\"", "status": "confirmed",
            "summary": "Call with X", "description": "agenda", "location": "Zoom",
            "start": {"dateTime": "2026-09-24T10:00:00+02:00", "timeZone": "Africa/Johannesburg"},
            "end": {"dateTime": "2026-09-24T10:30:00+02:00", "timeZone": "Africa/Johannesburg"},
            "organizer": {"email": "x@example.com"},
            "attendees": [
                {"email": "x@example.com", "organizer": true, "responseStatus": "accepted"},
                {"email": "patrick@autospark.ai", "self": true, "responseStatus": "needsAction"}
            ],
            "hangoutLink": "https://meet.google.com/abc",
            "htmlLink": "https://www.google.com/calendar/event?eid=1",
            "updated": "2026-09-20T08:00:00.000Z"
        });
        let row = event_from_json(7, &v).unwrap();
        assert_eq!(row.calendar_id, 7);
        assert_eq!(row.google_id, "ev1");
        assert_eq!(row.etag.as_deref(), Some("\"123\""));
        assert_eq!(row.summary, "Call with X");
        // 2026-09-24T08:00:00Z: 20,720 days since the epoch, plus eight hours.
        assert_eq!(row.start_ts, 20_720 * 86_400 + 8 * 3600);
        assert_eq!(row.end_ts - row.start_ts, 1800);
        assert!(!row.all_day);
        assert_eq!(row.time_zone.as_deref(), Some("Africa/Johannesburg"));
        assert_eq!(row.organizer_email.as_deref(), Some("x@example.com"));
        assert_eq!(row.self_response.as_deref(), Some("needsAction"));
        assert_eq!(
            row.hangout_link.as_deref(),
            Some("https://meet.google.com/abc")
        );
        assert!(row.attendees_json.unwrap().contains("patrick@autospark.ai"));
        assert!(!row.local_only);
        assert_eq!(row.transparency, None);
    }

    #[test]
    fn rfc3339_offset_is_applied() {
        // 10:00 at +02:00 is 08:00Z.
        assert_eq!(
            parse_rfc3339("2026-09-24T10:00:00+02:00"),
            parse_rfc3339("2026-09-24T08:00:00Z")
        );
        assert_eq!(to_rfc3339(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn all_day_event_uses_dates() {
        let v = json!({
            "id": "ad1", "summary": "Offsite",
            "start": {"date": "2026-10-01"}, "end": {"date": "2026-10-03"},
            "transparency": "transparent"
        });
        let row = event_from_json(1, &v).unwrap();
        assert!(row.all_day);
        assert_eq!(row.start_date.as_deref(), Some("2026-10-01"));
        assert_eq!(row.end_date.as_deref(), Some("2026-10-03"));
        assert_eq!(row.end_ts - row.start_ts, 2 * 86_400);
        assert_eq!(to_date(row.start_ts), "2026-10-01");
        assert_eq!(row.transparency.as_deref(), Some("transparent"));
        assert_eq!(row.self_response, None);
    }

    #[test]
    fn cancelled_instance_without_times_is_kept_as_cancelled() {
        let v = json!({
            "id": "series_20261001T080000Z", "status": "cancelled",
            "recurringEventId": "series",
            "originalStartTime": {"dateTime": "2026-10-01T08:00:00Z"}
        });
        let row = event_from_json(1, &v).unwrap();
        assert_eq!(row.status, "cancelled");
        assert_eq!(row.recurring_event_id.as_deref(), Some("series"));
        assert_eq!((row.start_ts, row.end_ts), (0, 0));
    }

    #[test]
    fn recurring_instance_carries_its_series_id() {
        let v = json!({
            "id": "series_20261001T080000Z", "status": "confirmed", "summary": "Standup",
            "recurringEventId": "series",
            "start": {"dateTime": "2026-10-01T08:00:00Z"},
            "end": {"dateTime": "2026-10-01T08:15:00Z"}
        });
        let row = event_from_json(1, &v).unwrap();
        assert_eq!(row.recurring_event_id.as_deref(), Some("series"));
        assert_eq!(row.end_ts - row.start_ts, 900);
    }

    #[test]
    fn an_event_without_id_or_times_is_skipped() {
        assert!(event_from_json(1, &json!({"summary": "x"})).is_none());
        assert!(event_from_json(1, &json!({"id": "y", "status": "confirmed"})).is_none());
    }

    #[test]
    fn calendar_list_entry_maps() {
        let v = json!({
            "id": "patrick@autospark.ai", "summary": "patrick@autospark.ai",
            "backgroundColor": "#9fe1e7", "primary": true, "selected": true,
            "accessRole": "owner"
        });
        let row = calendar_from_json("a1", &v).unwrap();
        assert_eq!(row.account_id, "a1");
        assert!(row.is_primary && row.selected);
        assert_eq!(row.access_role, "owner");
        assert_eq!(row.color.as_deref(), Some("#9fe1e7"));
        let v = json!({"id": "x", "summary": "Team", "summaryOverride": "My team"});
        let row = calendar_from_json("a1", &v).unwrap();
        assert_eq!(row.summary, "My team");
        assert!(!row.is_primary && !row.selected);
        assert_eq!(row.access_role, "reader");
    }

    #[test]
    fn local_ids_are_recognised() {
        assert!(is_local_id("local-abc"));
        assert!(!is_local_id("abc"));
    }
}
