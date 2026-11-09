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
    /// IANA zone of the end, when it differs from the start's (a flight).
    /// `None` keeps the stored end zone on a patch, else uses `time_zone`.
    #[serde(default)]
    pub end_time_zone: Option<String>,
    /// Guest addresses. `Some(vec![])` clears the guest list.
    pub attendees: Option<Vec<String>>,
    /// Ask Google to attach a Meet link.
    pub add_meet: Option<bool>,
    #[serde(default)]
    pub options: super::options::EventOptions,
    #[serde(default)]
    pub series: bool,
    #[serde(default)]
    pub following: bool,
    pub optional_attendees: Option<Vec<String>>,
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

/// The event types the windowed pull asks for, explicitly.
pub const EVENT_TYPES: &[&str] = &[
    "default",
    "focusTime",
    "outOfOffice",
    "workingLocation",
    "fromGmail",
    "birthday",
];

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
    // Every type, named: alerts need to see (and skip) out-of-office,
    // focus-time and working-location entries rather than rely on a default.
    for t in EVENT_TYPES {
        u.query_pairs_mut().append_pair("eventTypes", t);
    }
    if let Some(t) = page_token {
        u.query_pairs_mut().append_pair("pageToken", t);
    }
    u
}

/// How far either side of "now" a remote event search looks.
pub const SEARCH_SPAN: i64 = 730 * 86_400;
/// Remote hits per calendar: one page, no paging.
pub const SEARCH_PAGE: u32 = 50;

/// `events.list` with Google's free-text `q`, instances expanded, over
/// `[now - SEARCH_SPAN, now + SEARCH_SPAN]`, one page of [`SEARCH_PAGE`].
pub fn events_search_url(calendar_google_id: &str, query: &str, now: i64) -> url::Url {
    let mut u = events_url(calendar_google_id);
    u.query_pairs_mut()
        .append_pair("q", query)
        .append_pair("singleEvents", "true")
        .append_pair("orderBy", "startTime")
        .append_pair("timeMin", &super::model::to_rfc3339(now - SEARCH_SPAN))
        .append_pair("timeMax", &super::model::to_rfc3339(now + SEARCH_SPAN))
        .append_pair("maxResults", &SEARCH_PAGE.to_string());
    u
}

pub fn events_insert_url(calendar_google_id: &str, send: SendUpdates) -> url::Url {
    let mut u = events_url(calendar_google_id);
    u.query_pairs_mut()
        .append_pair("conferenceDataVersion", "1")
        .append_pair("supportsAttachments", "true")
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
        .append_pair("supportsAttachments", "true")
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

/// `events.move`: `POST /calendars/{from}/events/{id}/move?destination={to}`.
/// Google tells the guests about the new organiser calendar per `send`.
pub fn events_move_url(
    from_calendar_google_id: &str,
    event_google_id: &str,
    to_calendar_google_id: &str,
    send: SendUpdates,
) -> url::Url {
    let mut u = event_url(from_calendar_google_id, event_google_id);
    u.path_segments_mut().expect("base url").push("move");
    u.query_pairs_mut()
        .append_pair("destination", to_calendar_google_id)
        .append_pair("sendUpdates", send.as_str());
    u
}

fn calendar_list_root() -> url::Url {
    url::Url::parse(&format!("{BASE}/users/me/calendarList")).expect("static url")
}

/// `calendarList.get` for one calendar of the signed-in user.
pub fn calendar_list_entry_url(calendar_google_id: &str) -> url::Url {
    let mut u = calendar_list_root();
    u.path_segments_mut()
        .expect("base url")
        .push(calendar_google_id);
    u
}

/// `calendarList.patch` with colours given as `#rrggbb` (`colorRgbFormat`).
pub fn calendar_list_patch_url(calendar_google_id: &str) -> url::Url {
    let mut u = calendar_list_entry_url(calendar_google_id);
    u.query_pairs_mut().append_pair("colorRgbFormat", "true");
    u
}

/// `calendarList.insert`: add a calendar someone shared to the user's list.
pub fn calendar_list_insert_url() -> url::Url {
    calendar_list_root()
}

/// `calendars.insert`: a new secondary calendar owned by the user.
pub fn calendars_insert_url() -> url::Url {
    url::Url::parse(&format!("{BASE}/calendars")).expect("static url")
}

/// `#rrggbb` in lower case, or `None` when `s` is not one.
pub fn parse_hex_color(s: &str) -> Option<String> {
    let hex = s.trim().strip_prefix('#')?;
    (hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| format!("#{}", hex.to_ascii_lowercase()))
}

/// WCAG relative luminance of a `#rrggbb` colour.
fn luminance(hex: &str) -> f64 {
    let channel = |i: usize| {
        let v = f64::from(u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0)) / 255.0;
        if v <= 0.040_45 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5)
}

/// The text colour on a calendar of `background` (`#rrggbb`): white or near
/// black, whichever contrasts more.
pub fn foreground_for(background: &str) -> &'static str {
    const LIGHT: &str = "#ffffff";
    const DARK: &str = "#1d1d1d";
    let bg = luminance(background);
    let ratio = |fg: f64| (bg.max(fg) + 0.05) / (bg.min(fg) + 0.05);
    if ratio(luminance(LIGHT)) >= ratio(luminance(DARK)) {
        LIGHT
    } else {
        DARK
    }
}

/// The `calendarList.patch` body for a colour. `None` when `color` is not
/// `#rrggbb`.
pub fn calendar_color_body(color: &str) -> Option<Value> {
    let bg = parse_hex_color(color)?;
    let fg = foreground_for(&bg);
    Some(json!({ "backgroundColor": bg, "foregroundColor": fg }))
}

/// The `calendarList.insert` body: the calendar id (an address for a
/// person's main calendar), shown in the list at once.
pub fn calendar_list_insert_body(calendar_google_id: &str) -> Value {
    json!({ "id": calendar_google_id, "selected": true })
}

/// The `calendars.insert` body.
pub fn calendar_insert_body(summary: &str, time_zone: &str) -> Value {
    json!({ "summary": summary, "timeZone": time_zone })
}

/// The error code for a failed `calendarList.insert`: a missing grant first,
/// then a calendar the person has not shared (Google answers 404 or 403).
/// `None` leaves the generic mapping.
pub fn calendar_list_insert_error(status: u16, body: &str) -> Option<&'static str> {
    if google::is_scope_refusal(status, body) {
        Some("gcal_scope")
    } else if matches!(status, 403 | 404) {
        Some("gcal_not_shared")
    } else {
        None
    }
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
/// For a patch of a stored event, use [`event_body_for`] so a different end
/// zone survives.
pub fn event_body(input: &EventInput, existing_attendees: Option<&Value>) -> Value {
    event_body_for(input, existing_attendees, None)
}

/// The zone written to `end.timeZone`: the input's own end zone, else the
/// stored end zone of the event being patched, else the start's zone.
pub fn end_zone<'a>(input: &'a EventInput, stored_end_zone: Option<&'a str>) -> Option<&'a str> {
    let given = |z: Option<&'a str>| z.filter(|z| !z.is_empty());
    given(input.end_time_zone.as_deref())
        .or(given(stored_end_zone))
        .or(input.time_zone.as_deref())
}

/// [`event_body`] for a patch: `stored_end_zone` is the row's `end_time_zone`,
/// so a patch that moves the times keeps an end zone that differs from the
/// start's instead of stamping the start zone on both ends.
pub fn event_body_for(
    input: &EventInput,
    existing_attendees: Option<&Value>,
    stored_end_zone: Option<&str>,
) -> Value {
    let mut body = serde_json::to_value(&input.options)
        .ok()
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    if input.options.color_id.as_deref() == Some("") {
        body.insert("colorId".into(), Value::Null);
    }
    if input
        .options
        .reminders
        .as_ref()
        .is_some_and(|r| r.use_default)
    {
        body.insert("reminders".into(), json!({"useDefault": true}));
    }
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
        let stamp = |ts: i64, zone: Option<&str>| {
            let mut v = json!({ "dateTime": super::model::to_rfc3339(ts) });
            if let Some(tz) = zone {
                v["timeZone"] = json!(tz);
            }
            v
        };
        if let Some(ts) = input.start_ts {
            body.insert("start".into(), stamp(ts, input.time_zone.as_deref()));
        }
        if let Some(ts) = input.end_ts {
            body.insert("end".into(), stamp(ts, end_zone(input, stored_end_zone)));
        }
    }
    if let Some(emails) = &input.attendees {
        body.insert(
            "attendees".into(),
            attendees_body(emails, existing_attendees),
        );
    }
    if let Some(optional) = &input.optional_attendees {
        let attendees = body
            .entry("attendees")
            .or_insert_with(|| existing_attendees.cloned().unwrap_or_else(|| json!([])));
        if let Some(list) = attendees.as_array_mut() {
            for a in list {
                let email = a["email"].as_str().unwrap_or_default();
                a["optional"] = json!(optional.iter().any(|e| e.eq_ignore_ascii_case(email)));
            }
        }
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
/// `comment` is the note to the organiser on the self entry: `None` keeps the
/// stored one, `Some("")` clears it, any other text replaces it.
pub fn rsvp_body(
    attendees_json: Option<&str>,
    self_email: &str,
    response: &str,
    comment: Option<&str>,
) -> Value {
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
            if let Some(c) = comment {
                a["comment"] = json!(c);
            }
            found = true;
        }
    }
    if !found {
        let mut me = json!({ "email": self_email, "self": true, "responseStatus": response });
        if let Some(c) = comment {
            me["comment"] = json!(c);
        }
        list.push(me);
    }
    json!({ "attendees": list })
}

/// The four answers Google accepts.
pub fn valid_response(s: &str) -> bool {
    matches!(s, "accepted" | "declined" | "tentative" | "needsAction")
}

// ---- calls ------------------------------------------------------------------

pub async fn fetch_event(account_id: &str, calendar_id: &str, event_id: &str) -> Result<Value> {
    get_json(account_id, event_url(calendar_id, event_id)).await
}

pub async fn fetch_event_optional(
    account_id: &str,
    calendar_id: &str,
    event_id: &str,
) -> Result<Option<Value>> {
    let token = google::access_token(account_id).await?;
    let resp = google::http()
        .get(event_url(calendar_id, event_id))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| SkimError::other("network", e.to_string()))?;
    if matches!(resp.status().as_u16(), 404 | 410) {
        return Ok(None);
    }
    if !resp.status().is_success() {
        return Err(google::api_error(account_id, resp).await);
    }
    resp.json()
        .await
        .map(Some)
        .map_err(|e| SkimError::other("gcal_api", e.to_string()))
}

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
            return Ok(items);
        }
    }
    Err(SkimError::other(
        "gcal_api",
        "Calendar results exceed the page limit. Narrow the date range.",
    ))
}

pub async fn fetch_instances(
    account_id: &str,
    calendar_id: &str,
    series_id: &str,
) -> Result<Vec<Value>> {
    paged(account_id, |page| {
        let mut url = event_url(calendar_id, series_id);
        url.path_segments_mut()
            .expect("event URL")
            .push("instances");
        url.query_pairs_mut()
            .append_pair("maxResults", "2500")
            .append_pair("showDeleted", "true");
        if let Some(p) = page {
            url.query_pairs_mut().append_pair("pageToken", p);
        }
        url
    })
    .await
}

pub async fn fetch_calendar_list(account_id: &str) -> Result<Vec<Value>> {
    paged(account_id, calendar_list_url).await
}

/// One page of remote search hits for a calendar (see [`events_search_url`]).
pub async fn search_events(
    account_id: &str,
    calendar_google_id: &str,
    query: &str,
    now: i64,
) -> Result<Vec<Value>> {
    let page = get_json(
        account_id,
        events_search_url(calendar_google_id, query, now),
    )
    .await?;
    Ok(page
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
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

pub async fn patch_event_if_match(
    account_id: &str,
    calendar_id: &str,
    event_id: &str,
    send: SendUpdates,
    body: &Value,
    etag: &str,
) -> Result<Value> {
    let url = events_patch_url(calendar_id, event_id, send);
    send_json(
        account_id,
        google::http()
            .patch(url)
            .header(reqwest::header::IF_MATCH, etag),
        body,
    )
    .await
}

/// `events.move` (see [`events_move_url`]). The call has no body.
pub async fn move_event(
    account_id: &str,
    from_calendar_google_id: &str,
    event_google_id: &str,
    to_calendar_google_id: &str,
    send: SendUpdates,
) -> Result<Value> {
    let token = google::access_token(account_id).await?;
    let url = events_move_url(
        from_calendar_google_id,
        event_google_id,
        to_calendar_google_id,
        send,
    );
    let resp = google::http()
        .post(url)
        .bearer_auth(token)
        .header(reqwest::header::CONTENT_LENGTH, "0")
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

/// `calendarList.get` for one calendar.
pub async fn fetch_calendar_list_entry(
    account_id: &str,
    calendar_google_id: &str,
) -> Result<Value> {
    get_json(account_id, calendar_list_entry_url(calendar_google_id)).await
}

/// `calendarList.patch` with `colorRgbFormat=true`.
pub async fn patch_calendar_list(
    account_id: &str,
    calendar_google_id: &str,
    body: &Value,
) -> Result<Value> {
    let url = calendar_list_patch_url(calendar_google_id);
    send_json(account_id, google::http().patch(url), body).await
}

/// `calendarList.insert`. A calendar already in the list (409) is read back
/// instead; one not shared with the user is `gcal_not_shared`.
pub async fn insert_calendar_list(account_id: &str, calendar_google_id: &str) -> Result<Value> {
    let token = google::access_token(account_id).await?;
    let resp = google::http()
        .post(calendar_list_insert_url())
        .bearer_auth(token)
        .json(&calendar_list_insert_body(calendar_google_id))
        .send()
        .await
        .map_err(|e| SkimError::other("network", e.to_string()))?;
    let status = resp.status().as_u16();
    if resp.status().is_success() {
        return resp
            .json()
            .await
            .map_err(|e| SkimError::other("gcal_api", e.to_string()));
    }
    if status == 409 {
        return fetch_calendar_list_entry(account_id, calendar_google_id).await;
    }
    let body = resp.text().await.unwrap_or_default();
    match calendar_list_insert_error(status, &body) {
        Some("gcal_not_shared") => Err(SkimError::other(
            "gcal_not_shared",
            "That calendar is not shared with this account.",
        )),
        _ => Err(google::error_from_parts(account_id, status, &body).await),
    }
}

/// `calendars.insert`.
pub async fn insert_calendar(account_id: &str, body: &Value) -> Result<Value> {
    send_json(
        account_id,
        google::http().post(calendars_insert_url()),
        body,
    )
    .await
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

pub async fn delete_event_if_match(
    account_id: &str,
    calendar_id: &str,
    event_id: &str,
    send: SendUpdates,
    etag: &str,
) -> Result<()> {
    let token = google::access_token(account_id).await?;
    let response = google::http()
        .delete(events_delete_url(calendar_id, event_id, send))
        .header(reqwest::header::IF_MATCH, etag)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|error| SkimError::other("network", error.to_string()))?;
    if response.status().is_success() || matches!(response.status().as_u16(), 404 | 410) {
        return Ok(());
    }
    Err(google::api_error(account_id, response).await)
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
        let types: Vec<String> = u
            .query_pairs()
            .filter(|(k, _)| k == "eventTypes")
            .map(|(_, v)| v.into_owned())
            .collect();
        assert_eq!(
            types,
            [
                "default",
                "focusTime",
                "outOfOffice",
                "workingLocation",
                "fromGmail",
                "birthday"
            ]
        );
        assert!(u.as_str().contains(
            "eventTypes=default&eventTypes=focusTime&eventTypes=outOfOffice\
             &eventTypes=workingLocation&eventTypes=fromGmail&eventTypes=birthday"
        ));
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
        let b = rsvp_body(Some(stored), "me@x.com", "accepted", None);
        let list = b["attendees"].as_array().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0]["responseStatus"], "accepted");
        assert_eq!(list[1]["responseStatus"], "accepted");
        assert_eq!(list[1]["self"], true);
        // No self entry: one is added rather than the answer being lost.
        let b = rsvp_body(
            Some(r#"[{"email":"org@x.com"}]"#),
            "me@x.com",
            "declined",
            None,
        );
        let list = b["attendees"].as_array().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[1]["email"], "me@x.com");
        assert_eq!(list[1]["responseStatus"], "declined");
        assert!(valid_response("tentative") && !valid_response("maybe"));
    }

    #[test]
    fn rsvp_body_sets_keeps_and_clears_the_comment() {
        let stored = r#"[{"email":"org@example.test","organizer":true,"comment":"theirs"},
                         {"email":"me@example.test","self":true,"comment":"old note"}]"#;
        let b = rsvp_body(
            Some(stored),
            "me@example.test",
            "declined",
            Some("Travelling"),
        );
        assert_eq!(b["attendees"][1]["comment"], "Travelling");
        assert_eq!(b["attendees"][1]["responseStatus"], "declined");
        assert_eq!(
            b["attendees"][0]["comment"], "theirs",
            "only the self entry"
        );
        let kept = rsvp_body(Some(stored), "me@example.test", "accepted", None);
        assert_eq!(kept["attendees"][1]["comment"], "old note");
        let cleared = rsvp_body(Some(stored), "me@example.test", "accepted", Some(""));
        assert_eq!(cleared["attendees"][1]["comment"], "");
        let added = rsvp_body(None, "me@example.test", "tentative", Some("Maybe late"));
        assert_eq!(
            added["attendees"],
            json!([{ "email": "me@example.test", "self": true,
                     "responseStatus": "tentative", "comment": "Maybe late" }])
        );
        let plain = rsvp_body(None, "me@example.test", "accepted", None);
        assert!(plain["attendees"][0].get("comment").is_none());
    }

    #[test]
    fn the_end_zone_is_the_inputs_then_the_stored_then_the_starts() {
        let base = EventInput {
            start_ts: Some(3600),
            end_ts: Some(7200),
            time_zone: Some("Africa/Johannesburg".into()),
            ..Default::default()
        };
        // Today's behaviour without a stored zone: the start's zone on both.
        let b = event_body(&base, None);
        assert_eq!(b["end"]["timeZone"], "Africa/Johannesburg");
        let b = event_body_for(&base, None, Some("Europe/London"));
        assert_eq!(b["end"]["timeZone"], "Europe/London");
        assert_eq!(b["start"]["timeZone"], "Africa/Johannesburg");
        let named = EventInput {
            end_time_zone: Some("Asia/Tokyo".into()),
            ..base.clone()
        };
        let b = event_body_for(&named, None, Some("Europe/London"));
        assert_eq!(b["end"]["timeZone"], "Asia/Tokyo");
        assert_eq!(event_body(&named, None)["end"]["timeZone"], "Asia/Tokyo");
        // An empty end zone means "not given".
        let empty = EventInput {
            end_time_zone: Some(String::new()),
            ..base.clone()
        };
        assert_eq!(
            event_body_for(&empty, None, None)["end"]["timeZone"],
            "Africa/Johannesburg"
        );
        // No zone anywhere: none written.
        let bare = EventInput {
            start_ts: Some(1),
            end_ts: Some(2),
            ..Default::default()
        };
        assert!(event_body(&bare, None)["end"].get("timeZone").is_none());
        // An end zone alone (no end time) writes nothing.
        let zone_only = EventInput {
            end_time_zone: Some("Asia/Tokyo".into()),
            ..Default::default()
        };
        assert_eq!(event_body(&zone_only, None), json!({}));
        let input: EventInput =
            serde_json::from_value(json!({"end_time_zone": "Europe/Paris"})).unwrap();
        assert_eq!(input.end_time_zone.as_deref(), Some("Europe/Paris"));
    }

    #[test]
    fn event_type_fields_are_camel_case_in_the_body() {
        let input: EventInput = serde_json::from_value(json!({
            "start_ts": 3600, "end_ts": 7200,
            "options": {"eventType": "focusTime",
                        "focusTimeProperties": {"autoDeclineMode": "declineNone", "chatStatus": "doNotDisturb"}}
        }))
        .unwrap();
        let b = event_body(&input, None);
        assert_eq!(b["eventType"], "focusTime");
        assert_eq!(b["focusTimeProperties"]["chatStatus"], "doNotDisturb");
        assert!(b.get("outOfOfficeProperties").is_none());
        assert!(b.get("event_type").is_none());
    }

    #[test]
    fn move_url_names_the_destination_and_send_updates() {
        let u = events_move_url(
            "me@example.test",
            "ev/1",
            "team@group.calendar.google.com",
            SendUpdates::None,
        );
        assert_eq!(
            u.path(),
            "/calendar/v3/calendars/me@example.test/events/ev%2F1/move"
        );
        let qs = q(&u);
        assert_eq!(qs["destination"], "team@group.calendar.google.com");
        assert_eq!(qs["sendUpdates"], "none");
        assert_eq!(qs.len(), 2);
        let u = events_move_url("a", "b", "c", SendUpdates::All);
        assert_eq!(q(&u)["sendUpdates"], "all");
    }

    #[test]
    fn calendar_list_urls() {
        let u = calendar_list_patch_url("team#holiday@group.v.calendar.google.com");
        assert_eq!(
            u.path(),
            "/calendar/v3/users/me/calendarList/team%23holiday@group.v.calendar.google.com"
        );
        assert_eq!(q(&u)["colorRgbFormat"], "true");
        assert_eq!(
            calendar_list_entry_url("kim@example.test").as_str(),
            "https://www.googleapis.com/calendar/v3/users/me/calendarList/kim@example.test"
        );
        assert_eq!(
            calendar_list_insert_url().as_str(),
            "https://www.googleapis.com/calendar/v3/users/me/calendarList"
        );
        assert_eq!(
            calendars_insert_url().as_str(),
            "https://www.googleapis.com/calendar/v3/calendars"
        );
        assert_eq!(
            calendar_list_insert_body("kim@example.test"),
            json!({"id": "kim@example.test", "selected": true})
        );
        assert_eq!(
            calendar_insert_body("Projects", "Africa/Johannesburg"),
            json!({"summary": "Projects", "timeZone": "Africa/Johannesburg"})
        );
    }

    #[test]
    fn colours_parse_and_pick_a_readable_foreground() {
        assert_eq!(parse_hex_color(" #A1B2C3 ").as_deref(), Some("#a1b2c3"));
        for bad in ["a1b2c3", "#abc", "#abcdeg", "#a1b2c3d4", ""] {
            assert_eq!(parse_hex_color(bad), None, "{bad}");
        }
        assert_eq!(foreground_for("#ffffff"), "#1d1d1d");
        assert_eq!(foreground_for("#000000"), "#ffffff");
        assert_eq!(foreground_for("#fbd75b"), "#1d1d1d", "yellow");
        assert_eq!(foreground_for("#1a4fd6"), "#ffffff", "deep blue");
        assert_eq!(foreground_for("#9fe1e7"), "#1d1d1d", "pale teal");
        assert_eq!(
            calendar_color_body("#1A4FD6").unwrap(),
            json!({"backgroundColor": "#1a4fd6", "foregroundColor": "#ffffff"})
        );
        assert!(calendar_color_body("blue").is_none());
    }

    #[test]
    fn an_unshared_calendar_maps_to_gcal_not_shared_and_scope_wins() {
        assert_eq!(
            calendar_list_insert_error(404, "{}"),
            Some("gcal_not_shared")
        );
        assert_eq!(
            calendar_list_insert_error(403, r#"{"error":{"errors":[{"reason":"forbidden"}]}}"#),
            Some("gcal_not_shared")
        );
        assert_eq!(
            calendar_list_insert_error(
                403,
                r#"{"error":{"errors":[{"reason":"insufficientPermissions"}]}}"#
            ),
            Some("gcal_scope")
        );
        assert_eq!(
            calendar_list_insert_error(403, "ACCESS_TOKEN_SCOPE_INSUFFICIENT"),
            Some("gcal_scope")
        );
        assert_eq!(calendar_list_insert_error(500, "{}"), None);
        assert_eq!(calendar_list_insert_error(400, "{}"), None);
    }

    #[test]
    fn search_url_asks_google_for_two_years_either_side() {
        let now = 800 * 86_400;
        let u = events_search_url("team@group.calendar.google.com", "board pack", now);
        assert_eq!(
            u.path(),
            "/calendar/v3/calendars/team@group.calendar.google.com/events"
        );
        let qs = q(&u);
        assert_eq!(qs["q"], "board pack");
        assert_eq!(qs["singleEvents"], "true");
        assert_eq!(qs["orderBy"], "startTime");
        assert_eq!(qs["timeMin"], super::super::model::to_rfc3339(70 * 86_400));
        assert_eq!(
            qs["timeMax"],
            super::super::model::to_rfc3339(1530 * 86_400)
        );
        assert_eq!(qs["maxResults"], "50");
        assert!(!qs.contains_key("pageToken"));
        assert!(u.as_str().contains("q=board+pack"));
    }
}
