//! Google Calendar v3 over reqwest: URL and payload builders (pure, tested)
//! and the handful of calls the engine makes. Every request carries the
//! account's calendar grant from `fork::google`; nothing here is reachable
//! without one.

use crate::error::{Result, SkimError};
use crate::fork::google;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const BASE: &str = "https://www.googleapis.com/calendar/v3";

/// Who Google notifies about a create / change / delete. Never implicit: the
/// UI asks and passes one of these every time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SendUpdates {
    All,
    None,
}

impl SendUpdates {
    pub fn parse(s: &str) -> Result<Self> {
        match s.trim() {
            "all" => Ok(Self::All),
            "none" => Ok(Self::None),
            other => Err(SkimError::other(
                "gcal_send_updates",
                format!("sendUpdates must be \"all\" or \"none\", got {other:?}"),
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::None => "none",
        }
    }
}

/// What the UI hands the create / patch commands. Every field optional: a
/// create fills what it has, a patch names only what changed.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct EventInput {
    pub summary: Option<String>,
    pub description: Option<String>,
    pub location: Option<String>,
    /// Unix seconds; used with `end_ts` for a timed event.
    pub start_ts: Option<i64>,
    pub end_ts: Option<i64>,
    pub all_day: Option<bool>,
    /// `YYYY-MM-DD`; used with `end_date` (exclusive) for an all-day event.
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    /// IANA zone sent alongside `dateTime`, so Google shows the event in it.
    pub time_zone: Option<String>,
    /// Guest addresses. `Some(vec![])` clears the guest list.
    pub attendees: Option<Vec<String>>,
    /// Ask Google to attach a Meet link.
    pub add_meet: Option<bool>,
}

fn events_url(calendar_google_id: &str) -> url::Url {
    let mut u = url::Url::parse(BASE).expect("static url");
    u.path_segments_mut()
        .expect("base url")
        .push("calendars")
        .push(calendar_google_id)
        .push("events");
    u
}

fn event_url(calendar_google_id: &str, event_google_id: &str) -> url::Url {
    let mut u = events_url(calendar_google_id);
    u.path_segments_mut()
        .expect("base url")
        .push(event_google_id);
    u
}

pub fn calendar_list_url(page_token: Option<&str>) -> url::Url {
    let mut u = url::Url::parse(&format!("{BASE}/users/me/calendarList")).expect("static url");
    u.query_pairs_mut()
        .append_pair("maxResults", "250")
        .append_pair("showHidden", "false");
    if let Some(t) = page_token {
        u.query_pairs_mut().append_pair("pageToken", t);
    }
    u
}

/// `events.list` for one calendar over `[time_min, time_max]` (unix seconds),
/// instances expanded and ordered by start.
pub fn events_list_url(
    calendar_google_id: &str,
    time_min: i64,
    time_max: i64,
    page_token: Option<&str>,
) -> url::Url {
    let mut u = events_url(calendar_google_id);
    u.query_pairs_mut()
        .append_pair("singleEvents", "true")
        .append_pair("orderBy", "startTime")
        .append_pair("timeMin", &super::model::to_rfc3339(time_min))
        .append_pair("timeMax", &super::model::to_rfc3339(time_max))
        .append_pair("maxResults", "2500");
    if let Some(t) = page_token {
        u.query_pairs_mut().append_pair("pageToken", t);
    }
    u
}

pub fn events_insert_url(calendar_google_id: &str, send: SendUpdates) -> url::Url {
    let mut u = events_url(calendar_google_id);
    u.query_pairs_mut()
        .append_pair("conferenceDataVersion", "1")
        .append_pair("sendUpdates", send.as_str());
    u
}

pub fn events_patch_url(
    calendar_google_id: &str,
    event_google_id: &str,
    send: SendUpdates,
) -> url::Url {
    let mut u = event_url(calendar_google_id, event_google_id);
    u.query_pairs_mut()
        .append_pair("conferenceDataVersion", "1")
        .append_pair("sendUpdates", send.as_str());
    u
}

pub fn events_delete_url(
    calendar_google_id: &str,
    event_google_id: &str,
    send: SendUpdates,
) -> url::Url {
    let mut u = event_url(calendar_google_id, event_google_id);
    u.query_pairs_mut()
        .append_pair("sendUpdates", send.as_str());
    u
}

/// The `attendees` array for a create / patch: the requested addresses, each
/// keeping the `responseStatus` (and other fields) it already had in
/// `existing`, so re-saving an event never resets a guest's answer.
pub fn attendees_body(emails: &[String], existing: Option<&Value>) -> Value {
    let existing: Vec<&Value> = existing
        .and_then(Value::as_array)
        .map(|a| a.iter().collect())
        .unwrap_or_default();
    Value::Array(
        emails
            .iter()
            .map(|e| e.trim())
            .filter(|e| !e.is_empty())
            .map(|email| {
                existing
                    .iter()
                    .find(|a| {
                        a.get("email")
                            .and_then(Value::as_str)
                            .is_some_and(|x| x.eq_ignore_ascii_case(email))
                    })
                    .map(|a| (*a).clone())
                    .unwrap_or_else(|| json!({ "email": email }))
            })
            .collect(),
    )
}

/// The JSON body of an `events.insert` / `events.patch`: only the keys the
/// input names. `existing_attendees` is the stored array, for [`attendees_body`].
pub fn event_body(input: &EventInput, existing_attendees: Option<&Value>) -> Value {
    let mut body = serde_json::Map::new();
    if let Some(s) = &input.summary {
        body.insert("summary".into(), json!(s));
    }
    if let Some(d) = &input.description {
        body.insert("description".into(), json!(d));
    }
    if let Some(l) = &input.location {
        body.insert("location".into(), json!(l));
    }
    let all_day = input
        .all_day
        .unwrap_or(input.start_date.is_some() && input.start_ts.is_none());
    if all_day {
        if let Some(d) = &input.start_date {
            body.insert("start".into(), json!({ "date": d }));
        }
        if let Some(d) = &input.end_date {
            body.insert("end".into(), json!({ "date": d }));
        }
    } else {
        let stamp = |ts: i64| {
            let mut v = json!({ "dateTime": super::model::to_rfc3339(ts) });
            if let Some(tz) = &input.time_zone {
                v["timeZone"] = json!(tz);
            }
            v
        };
        if let Some(ts) = input.start_ts {
            body.insert("start".into(), stamp(ts));
        }
        if let Some(ts) = input.end_ts {
            body.insert("end".into(), stamp(ts));
        }
    }
    if let Some(emails) = &input.attendees {
        body.insert(
            "attendees".into(),
            attendees_body(emails, existing_attendees),
        );
    }
    if input.add_meet == Some(true) {
        body.insert(
            "conferenceData".into(),
            json!({
                "createRequest": {
                    "requestId": uuid::Uuid::new_v4().to_string(),
                    "conferenceSolutionKey": { "type": "hangoutsMeet" }
                }
            }),
        );
    }
    Value::Object(body)
}

/// The `events.patch` body for an RSVP: the stored attendee list with the
/// `self` entry's `responseStatus` replaced (Google needs the whole array).
/// When no entry is marked `self`, one is added for `self_email`.
pub fn rsvp_body(attendees_json: Option<&str>, self_email: &str, response: &str) -> Value {
    let mut list: Vec<Value> = attendees_json
        .and_then(|s| serde_json::from_str::<Value>(s).ok())
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    let mut found = false;
    for a in list.iter_mut() {
        let is_self = a.get("self").and_then(Value::as_bool).unwrap_or(false)
            || a.get("email")
                .and_then(Value::as_str)
                .is_some_and(|e| e.eq_ignore_ascii_case(self_email));
        if is_self {
            a["responseStatus"] = json!(response);
            found = true;
        }
    }
    if !found {
        list.push(json!({ "email": self_email, "self": true, "responseStatus": response }));
    }
    json!({ "attendees": list })
}

/// The four answers Google accepts.
pub fn valid_response(s: &str) -> bool {
    matches!(s, "accepted" | "declined" | "tentative" | "needsAction")
}

// ---- calls ------------------------------------------------------------------

async fn get_json(account_id: &str, url: url::Url) -> Result<Value> {
    let token = google::access_token(account_id).await?;
    let resp = google::http()
        .get(url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| SkimError::other("network", e.to_string()))?;
    if !resp.status().is_success() {
        return Err(google::api_error(account_id, resp).await);
    }
    resp.json()
        .await
        .map_err(|e| SkimError::other("gcal_api", e.to_string()))
}

/// Every page of a list call, concatenating `items`.
async fn paged(
    account_id: &str,
    mut url_for: impl FnMut(Option<&str>) -> url::Url,
) -> Result<Vec<Value>> {
    let mut items = Vec::new();
    let mut token: Option<String> = None;
    for _ in 0..50 {
        let page = get_json(account_id, url_for(token.as_deref())).await?;
        if let Some(arr) = page.get("items").and_then(Value::as_array) {
            items.extend(arr.iter().cloned());
        }
        token = page
            .get("nextPageToken")
            .and_then(Value::as_str)
            .filter(|t| !t.is_empty())
            .map(str::to_string);
        if token.is_none() {
            break;
        }
    }
    Ok(items)
}

pub async fn fetch_calendar_list(account_id: &str) -> Result<Vec<Value>> {
    paged(account_id, calendar_list_url).await
}

pub async fn fetch_events(
    account_id: &str,
    calendar_google_id: &str,
    time_min: i64,
    time_max: i64,
) -> Result<Vec<Value>> {
    paged(account_id, |t| {
        events_list_url(calendar_google_id, time_min, time_max, t)
    })
    .await
}

async fn send_json(account_id: &str, req: reqwest::RequestBuilder, body: &Value) -> Result<Value> {
    let token = google::access_token(account_id).await?;
    let resp = req
        .bearer_auth(token)
        .json(body)
        .send()
        .await
        .map_err(|e| SkimError::other("network", e.to_string()))?;
    if !resp.status().is_success() {
        return Err(google::api_error(account_id, resp).await);
    }
    resp.json()
        .await
        .map_err(|e| SkimError::other("gcal_api", e.to_string()))
}

pub async fn insert_event(
    account_id: &str,
    calendar_google_id: &str,
    send: SendUpdates,
    body: &Value,
) -> Result<Value> {
    let url = events_insert_url(calendar_google_id, send);
    send_json(account_id, google::http().post(url), body).await
}

pub async fn patch_event(
    account_id: &str,
    calendar_google_id: &str,
    event_google_id: &str,
    send: SendUpdates,
    body: &Value,
) -> Result<Value> {
    let url = events_patch_url(calendar_google_id, event_google_id, send);
    send_json(account_id, google::http().patch(url), body).await
}

/// `events.delete`. An event Google already dropped (404 / 410) counts as done.
pub async fn delete_event(
    account_id: &str,
    calendar_google_id: &str,
    event_google_id: &str,
    send: SendUpdates,
) -> Result<()> {
    let token = google::access_token(account_id).await?;
    let resp = google::http()
        .delete(events_delete_url(calendar_google_id, event_google_id, send))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| SkimError::other("network", e.to_string()))?;
    if resp.status().is_success() || matches!(resp.status().as_u16(), 404 | 410) {
        return Ok(());
    }
    Err(google::api_error(account_id, resp).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(u: &url::Url) -> std::collections::HashMap<String, String> {
        u.query_pairs().into_owned().collect()
    }

    #[test]
    fn list_url_expands_instances_inside_the_window() {
        let u = events_list_url("patrick@autospark.ai", 0, 86_400, Some("tok"));
        // `@` is a legal path character (RFC 3986 pchar); the url crate leaves
        // it raw and Google accepts it either way.
        assert_eq!(
            u.path(),
            "/calendar/v3/calendars/patrick@autospark.ai/events"
        );
        assert!(u.as_str().starts_with("https://www.googleapis.com/"));
        let qs = q(&u);
        assert_eq!(qs["singleEvents"], "true");
        assert_eq!(qs["orderBy"], "startTime");
        assert_eq!(qs["timeMin"], "1970-01-01T00:00:00Z");
        assert_eq!(qs["timeMax"], "1970-01-02T00:00:00Z");
        assert_eq!(qs["maxResults"], "2500");
        assert_eq!(qs["pageToken"], "tok");
        assert!(!q(&events_list_url("c", 0, 1, None)).contains_key("pageToken"));
    }

    #[test]
    fn mutation_urls_carry_send_updates_explicitly() {
        let u = events_insert_url("c1", SendUpdates::None);
        assert_eq!(q(&u)["sendUpdates"], "none");
        assert_eq!(q(&u)["conferenceDataVersion"], "1");
        let u = events_patch_url("c1", "e/1", SendUpdates::All);
        assert!(u.path().ends_with("/calendars/c1/events/e%2F1"));
        assert_eq!(q(&u)["sendUpdates"], "all");
        let u = events_delete_url("c1", "e1", SendUpdates::None);
        assert_eq!(q(&u)["sendUpdates"], "none");
        assert!(SendUpdates::parse("externalOnly").is_err());
        assert_eq!(SendUpdates::parse(" all ").unwrap(), SendUpdates::All);
    }

    #[test]
    fn timed_create_body() {
        let input = EventInput {
            summary: Some("Call".into()),
            start_ts: Some(3600),
            end_ts: Some(5400),
            time_zone: Some("Africa/Johannesburg".into()),
            attendees: Some(vec!["a@x.com".into(), " ".into()]),
            add_meet: Some(true),
            ..Default::default()
        };
        let b = event_body(&input, None);
        assert_eq!(b["summary"], "Call");
        assert_eq!(b["start"]["dateTime"], "1970-01-01T01:00:00Z");
        assert_eq!(b["start"]["timeZone"], "Africa/Johannesburg");
        assert_eq!(b["end"]["dateTime"], "1970-01-01T01:30:00Z");
        assert_eq!(b["attendees"], json!([{ "email": "a@x.com" }]));
        assert_eq!(
            b["conferenceData"]["createRequest"]["conferenceSolutionKey"]["type"],
            "hangoutsMeet"
        );
        assert!(b["conferenceData"]["createRequest"]["requestId"]
            .as_str()
            .is_some_and(|s| !s.is_empty()));
        assert!(b.get("description").is_none());
        assert!(b.get("location").is_none());
    }

    #[test]
    fn all_day_create_body_uses_dates_only() {
        let input = EventInput {
            summary: Some("Offsite".into()),
            all_day: Some(true),
            start_date: Some("2026-10-01".into()),
            end_date: Some("2026-10-02".into()),
            start_ts: Some(1),
            ..Default::default()
        };
        let b = event_body(&input, None);
        assert_eq!(b["start"], json!({ "date": "2026-10-01" }));
        assert_eq!(b["end"], json!({ "date": "2026-10-02" }));
        assert!(b.get("conferenceData").is_none());
    }

    #[test]
    fn patch_body_names_only_what_changed_and_keeps_guest_answers() {
        let existing = json!([
            { "email": "a@x.com", "responseStatus": "accepted" },
            { "email": "me@x.com", "self": true, "responseStatus": "accepted" }
        ]);
        let input = EventInput {
            location: Some("Room 2".into()),
            attendees: Some(vec!["A@x.com".into(), "b@x.com".into()]),
            ..Default::default()
        };
        let b = event_body(&input, Some(&existing));
        assert_eq!(b.as_object().unwrap().len(), 2);
        assert_eq!(b["location"], "Room 2");
        assert_eq!(
            b["attendees"],
            json!([
                { "email": "a@x.com", "responseStatus": "accepted" },
                { "email": "b@x.com" }
            ])
        );
        let cleared = event_body(
            &EventInput {
                attendees: Some(vec![]),
                ..Default::default()
            },
            Some(&existing),
        );
        assert_eq!(cleared["attendees"], json!([]));
    }

    #[test]
    fn rsvp_body_patches_the_self_attendee() {
        let stored = r#"[{"email":"org@x.com","organizer":true,"responseStatus":"accepted"},
                         {"email":"me@x.com","self":true,"responseStatus":"needsAction"}]"#;
        let b = rsvp_body(Some(stored), "me@x.com", "accepted");
        let list = b["attendees"].as_array().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0]["responseStatus"], "accepted");
        assert_eq!(list[1]["responseStatus"], "accepted");
        assert_eq!(list[1]["self"], true);
        // No self entry: one is added rather than the answer being lost.
        let b = rsvp_body(Some(r#"[{"email":"org@x.com"}]"#), "me@x.com", "declined");
        let list = b["attendees"].as_array().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[1]["email"], "me@x.com");
        assert_eq!(list[1]["responseStatus"], "declined");
        assert!(valid_response("tentative") && !valid_response("maybe"));
    }
}
