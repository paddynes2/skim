use super::{gapi, model};
use crate::error::{Result, SkimError};
use chrono::{NaiveDate, TimeZone};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Serialize)]
pub struct BusyPeriod {
    pub start_ts: i64,
    pub end_ts: i64,
    pub summary: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CalendarAvailability {
    pub email: String,
    pub status: &'static str,
    pub reason: Option<String>,
    pub busy: Vec<BusyPeriod>,
}

#[derive(Debug, Serialize)]
pub struct Availability {
    pub from_ts: i64,
    pub to_ts: i64,
    pub calendars: Vec<CalendarAvailability>,
}

fn unknown(email: String, reason: &str) -> CalendarAvailability {
    CalendarAvailability {
        email,
        status: "unknown",
        reason: Some(reason.into()),
        busy: vec![],
    }
}

fn event_time(value: &Value, zone: Option<&str>) -> Option<i64> {
    if let Some(stamp) = value["dateTime"].as_str() {
        return model::parse_rfc3339(stamp);
    }
    let date = NaiveDate::parse_from_str(value["date"].as_str()?, "%Y-%m-%d").ok()?;
    let zone: chrono_tz::Tz = zone?.parse().ok()?;
    zone.from_local_datetime(&date.and_hms_opt(0, 0, 0)?)
        .earliest()
        .map(|v| v.timestamp())
}

/// A failed read remains unknown. Empty readable calendars are available.
pub fn from_events(
    email: String,
    events: Result<Vec<Value>>,
    zone: Option<&str>,
    from: i64,
    to: i64,
    exclude: Option<&str>,
) -> CalendarAvailability {
    let Ok(events) = events else {
        return unknown(
            email,
            "Calendar access or a network connection is unavailable.",
        );
    };
    let mut busy = vec![];
    for event in events {
        if event["status"] == "cancelled"
            || event["transparency"] == "transparent"
            || event["id"].as_str().is_some_and(|id| Some(id) == exclude)
            || model::self_response(event.get("attendees")).as_deref() == Some("declined")
        {
            continue;
        }
        let (Some(start), Some(end)) = (
            event_time(&event["start"], zone),
            event_time(&event["end"], zone),
        ) else {
            return unknown(
                email,
                "An event time could not be read. Availability is unknown.",
            );
        };
        if start < to && end > from {
            busy.push(BusyPeriod {
                start_ts: start.max(from),
                end_ts: end.min(to),
                summary: event["summary"].as_str().map(str::to_string),
            });
        }
    }
    busy.sort_by_key(|period| period.start_ts);
    CalendarAvailability {
        email,
        status: "available",
        reason: None,
        busy,
    }
}

pub async fn fetch(
    account_id: &str,
    self_email: &str,
    from: i64,
    to: i64,
    attendees: Vec<String>,
    exclude: Option<&str>,
    selected_calendar_ids: &[String],
) -> Result<Availability> {
    if !to
        .checked_sub(from)
        .is_some_and(|duration| duration > 0 && duration <= 31 * 86400)
        || attendees.len() > 20
    {
        return Err(SkimError::other(
            "gcal_input",
            "Choose up to 31 days and 20 guests for availability.",
        ));
    }
    let mut addresses = vec![self_email.to_string()];
    for email in attendees {
        let email = email.trim();
        if !email.is_empty() && !addresses.iter().any(|v| v.eq_ignore_ascii_case(email)) {
            addresses.push(email.into());
        }
    }
    let list = gapi::fetch_calendar_list(account_id).await;
    let mut calendars = vec![];
    for email in addresses {
        let is_self = email.eq_ignore_ascii_case(self_email);
        let matches: Vec<&Value> = list
            .as_ref()
            .ok()
            .map(|rows| {
                rows.iter()
                    .filter(|row| {
                        let id = row["id"].as_str().unwrap_or_default();
                        id.eq_ignore_ascii_case(&email)
                            || (is_self
                                && (row["primary"] == true
                                    || selected_calendar_ids.iter().any(|selected| selected == id)))
                    })
                    .collect()
            })
            .unwrap_or_default();
        if matches.is_empty()
            || (is_self
                && selected_calendar_ids
                    .iter()
                    .any(|id| !matches.iter().any(|row| row["id"] == *id)))
        {
            calendars.push(unknown(
                email,
                "A selected calendar is not shared or could not be loaded.",
            ));
            continue;
        }
        let mut combined = CalendarAvailability {
            email: email.clone(),
            status: "available",
            reason: None,
            busy: vec![],
        };
        for calendar in matches {
            if !matches!(
                calendar["accessRole"].as_str(),
                Some("owner" | "writer" | "reader")
            ) {
                combined.status = "unknown";
                combined.reason =
                    Some("This connection cannot read a selected calendar's events.".into());
                continue;
            }
            let id = calendar["id"].as_str().unwrap_or_default();
            let events = gapi::fetch_events(account_id, id, from, to).await;
            let found = from_events(
                email.clone(),
                events,
                calendar["timeZone"].as_str(),
                from,
                to,
                exclude,
            );
            combined.busy.extend(found.busy);
            if found.status == "unknown" {
                combined.status = "unknown";
                combined.reason = found.reason;
            }
        }
        combined.busy.sort_by_key(|period| period.start_ts);
        calendars.push(combined);
    }
    Ok(Availability {
        from_ts: from,
        to_ts: to,
        calendars,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn inaccessible_and_invalid_calendars_are_not_reported_free() {
        let row = from_events(
            "hidden@example.test".into(),
            Err(SkimError::other("gcal_api", "denied")),
            None,
            0,
            100,
            None,
        );
        assert_eq!(row.status, "unknown");
        assert!(row.busy.is_empty() && row.reason.is_some());
        let malformed = from_events(
            "bad@example.test".into(),
            Ok(vec![json!({"start": {}, "end": {}})]),
            None,
            0,
            100,
            None,
        );
        assert_eq!(malformed.status, "unknown");
        assert_eq!(
            from_events("ok@example.test".into(), Ok(vec![]), None, 0, 100, None).status,
            "available"
        );
    }

    #[test]
    fn availability_uses_calendar_zone_and_filters_non_busy_events() {
        let values = vec![
            json!({"id":"all", "start":{"date":"2026-10-07"},"end":{"date":"2026-10-08"}}),
            json!({"id":"excluded", "start":{"dateTime":"2026-10-07T08:00:00Z"},"end":{"dateTime":"2026-10-07T09:00:00Z"}}),
            json!({"status":"cancelled"}),
            json!({"transparency":"transparent"}),
            json!({"attendees":[{"self":true,"responseStatus":"declined"}]}),
        ];
        let from = model::parse_rfc3339("2026-10-06T00:00:00Z").unwrap();
        let row = from_events(
            "me@example.test".into(),
            Ok(values),
            Some("Africa/Johannesburg"),
            from,
            from + 3 * 86400,
            Some("excluded"),
        );
        assert_eq!(row.status, "available");
        assert_eq!(row.busy.len(), 1);
        assert_eq!(row.busy[0].start_ts, from + 22 * 3600);
        assert_eq!(row.busy[0].end_ts - row.busy[0].start_ts, 86400);
    }
}
