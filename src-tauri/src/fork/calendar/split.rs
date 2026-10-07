//! A series split uses stable IDs and server markers because Google has no transaction for two events.
use super::{gapi, model};
use crate::error::{Result, SkimError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitPlan {
    pub calendar_google_id: String,
    pub master_id: String,
    pub master_etag: String,
    pub token: String,
    pub replacement_id: Option<String>,
    pub replacement: Value,
    pub master_patch: Value,
    pub original_recurrence: Value,
}

fn input_error(message: &str) -> SkimError {
    SkimError::other("gcal_input", message)
}
fn conflict() -> SkimError {
    SkimError::other("gcal_conflict", "The recurring event changed in Google Calendar. Review the series before retrying this change.")
}

pub fn original_start(instance: &Value) -> Result<i64> {
    let original = &instance["originalStartTime"];
    original["dateTime"]
        .as_str()
        .and_then(model::parse_rfc3339)
        .or_else(|| {
            original["date"]
                .as_str()
                .and_then(model::date_to_utc_midnight)
        })
        .ok_or_else(|| {
            input_error("The occurrence has no original start time. Open it in Google Calendar.")
        })
}

fn start(value: &Value) -> Result<i64> {
    value["dateTime"]
        .as_str()
        .and_then(model::parse_rfc3339)
        .or_else(|| value["date"].as_str().and_then(model::date_to_utc_midnight))
        .ok_or_else(|| input_error("The recurring event has an invalid start time."))
}

fn rule_parts(master: &Value) -> Result<Vec<(String, String)>> {
    let rules = master["recurrence"]
        .as_array()
        .ok_or_else(|| input_error("This event does not repeat."))?;
    if rules.len() != 1 {
        return Err(input_error("This schedule has recurrence exceptions. Edit this and following events in Google Calendar."));
    }
    let rule = rules[0]
        .as_str()
        .and_then(|s| s.strip_prefix("RRULE:"))
        .ok_or_else(|| input_error("This recurrence format requires Google Calendar."))?;
    let mut seen = BTreeSet::new();
    let parts: Vec<_> = rule
        .split(';')
        .map(|part| {
            let (key, value) = part
                .split_once('=')
                .ok_or_else(|| input_error("The repeat rule is invalid."))?;
            if !seen.insert(key) || value.is_empty() {
                return Err(input_error("The repeat rule is invalid."));
            }
            Ok((key.to_string(), value.to_string()))
        })
        .collect::<Result<_>>()?;
    if !parts.iter().any(|(key, value)| {
        key == "FREQ" && matches!(value.as_str(), "DAILY" | "WEEKLY" | "MONTHLY" | "YEARLY")
    }) {
        return Err(input_error(
            "This repeat frequency requires Google Calendar.",
        ));
    }
    if parts.iter().any(|(key, _)| {
        !matches!(
            key.as_str(),
            "FREQ"
                | "INTERVAL"
                | "BYDAY"
                | "BYMONTHDAY"
                | "BYMONTH"
                | "BYSETPOS"
                | "WKST"
                | "COUNT"
                | "UNTIL"
        )
    }) {
        return Err(input_error("This repeat rule requires Google Calendar."));
    }
    if parts.iter().any(|(k, _)| k == "COUNT") && parts.iter().any(|(k, _)| k == "UNTIL") {
        return Err(input_error(
            "A repeat rule cannot contain both COUNT and UNTIL.",
        ));
    }
    Ok(parts)
}

pub fn occurrence_count(master: &Value) -> Result<Option<usize>> {
    rule_parts(master)?
        .iter()
        .find(|(key, _)| key == "COUNT")
        .map(|(_, count)| {
            count
                .parse::<usize>()
                .ok()
                .filter(|n| (1..=10000).contains(n))
                .ok_or_else(|| {
                    input_error(
                        "Schedules with more than 10,000 occurrences require Google Calendar.",
                    )
                })
        })
        .transpose()
}

fn recurrence(parts: &[(String, String)]) -> Value {
    json!([format!(
        "RRULE:{}",
        parts
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(";")
    )])
}

fn marker(body: &mut Value, key: &str, token: &str) {
    if !body["extendedProperties"].is_object() {
        body["extendedProperties"] = json!({});
    }
    if !body["extendedProperties"]["private"].is_object() {
        body["extendedProperties"]["private"] = json!({});
    }
    body["extendedProperties"]["private"][key] = json!(token);
}

/// COUNT uses Google's original instance times, which include cancelled and moved occurrences.
pub fn plan(
    calendar_id: &str,
    master: &Value,
    instance: &Value,
    changes: &Value,
    counted_instances: &[Value],
    token: &str,
) -> Result<SplitPlan> {
    let mut parts = rule_parts(master)?;
    let cutoff = original_start(instance)?;
    let master_start = start(&master["start"])?;
    if cutoff < master_start {
        return Err(input_error("The occurrence is before the series start."));
    }
    let master_id = master["id"]
        .as_str()
        .ok_or_else(|| input_error("The series has no Google ID."))?;
    if instance["recurringEventId"].as_str() != Some(master_id) {
        return Err(input_error(
            "The occurrence does not belong to this series.",
        ));
    }
    let etag = master["etag"]
        .as_str()
        .ok_or_else(|| input_error("The series version is unavailable."))?;
    let all_day = instance["originalStartTime"]["date"].is_string();
    let mut replacement = json!({});
    // Copy writable settings. A new event must not reuse another event's conference credentials.
    for key in [
        "summary",
        "description",
        "location",
        "start",
        "end",
        "recurrence",
        "attendees",
        "reminders",
        "visibility",
        "colorId",
        "transparency",
        "guestsCanModify",
        "guestsCanInviteOthers",
        "guestsCanSeeOtherGuests",
        "extendedProperties",
        "attachments",
    ] {
        if let Some(value) = master.get(key) {
            replacement[key] = value.clone();
        }
    }
    replacement["start"] = instance["start"].clone();
    replacement["end"] = instance["end"].clone();
    if let Some(count) = occurrence_count(master)? {
        let times: BTreeSet<i64> = counted_instances
            .iter()
            .map(original_start)
            .collect::<Result<_>>()?;
        if times.len() != count || !times.contains(&cutoff) {
            return Err(input_error("The complete occurrence count is unavailable. Open this series in Google Calendar."));
        }
        let before = times.range(..cutoff).count();
        for (key, value) in &mut parts {
            if key == "COUNT" {
                *value = (count - before).to_string();
            }
        }
        replacement["recurrence"] = recurrence(&parts);
    }
    for (key, value) in changes
        .as_object()
        .ok_or_else(|| input_error("The event changes are invalid."))?
    {
        replacement[key] = value.clone();
    }
    // A time-only move retains the series zone, even if the caller only supplied UTC timestamps.
    for key in ["start", "end"] {
        if replacement[key]["dateTime"].is_string() && replacement[key]["timeZone"].is_null() {
            replacement[key]["timeZone"] = master[key]["timeZone"].clone();
        }
    }
    if let Some(attendees) = replacement["attendees"].as_array_mut() {
        for guest in attendees {
            if let Some(guest) = guest.as_object_mut() {
                guest.remove("self");
                guest.remove("organizer");
                guest.remove("responseStatus");
                guest.remove("comment");
            }
        }
    }
    if master.get("conferenceData").is_some() && !replacement["conferenceData"].is_object() {
        replacement["conferenceData"] = json!({"createRequest":{"requestId":token,"conferenceSolutionKey":{"type":"hangoutsMeet"}}});
    }
    let mut master_patch = if cutoff == master_start {
        changes.clone()
    } else {
        json!({})
    };
    master_patch["extendedProperties"] = master
        .get("extendedProperties")
        .cloned()
        .unwrap_or_else(|| json!({}));
    marker(&mut master_patch, "skimSplitApplied", token);
    if cutoff != master_start {
        parts.retain(|(key, _)| key != "COUNT" && key != "UNTIL");
        let until = if all_day {
            model::to_date(cutoff - 86400).replace('-', "")
        } else {
            model::to_rfc3339(cutoff - 1).replace(['-', ':'], "")
        };
        parts.push(("UNTIL".into(), until));
        master_patch["recurrence"] = recurrence(&parts);
        replacement["id"] = json!(token);
        marker(&mut replacement, "skimSplitSource", token);
    }
    Ok(SplitPlan {
        calendar_google_id: calendar_id.into(),
        master_id: master_id.into(),
        master_etag: etag.into(),
        token: token.into(),
        replacement_id: (cutoff != master_start).then(|| token.into()),
        replacement,
        master_patch,
        original_recurrence: master["recurrence"].clone(),
    })
}

pub trait Transport {
    async fn get(&mut self, id: &str) -> Result<Option<Value>>;
    async fn insert(&mut self, body: &Value, send: gapi::SendUpdates) -> Result<Value>;
    async fn patch(
        &mut self,
        id: &str,
        body: &Value,
        etag: &str,
        send: gapi::SendUpdates,
    ) -> Result<Value>;
    async fn delete(&mut self, id: &str, etag: &str, send: gapi::SendUpdates) -> Result<()>;
}

pub struct GoogleTransport<'a> {
    pub account_id: &'a str,
    pub calendar_id: &'a str,
}
impl Transport for GoogleTransport<'_> {
    async fn get(&mut self, id: &str) -> Result<Option<Value>> {
        gapi::fetch_event_optional(self.account_id, self.calendar_id, id).await
    }
    async fn insert(&mut self, body: &Value, send: gapi::SendUpdates) -> Result<Value> {
        gapi::insert_event(self.account_id, self.calendar_id, send, body).await
    }
    async fn patch(
        &mut self,
        id: &str,
        body: &Value,
        etag: &str,
        send: gapi::SendUpdates,
    ) -> Result<Value> {
        gapi::patch_event_if_match(self.account_id, self.calendar_id, id, send, body, etag).await
    }
    async fn delete(&mut self, id: &str, etag: &str, send: gapi::SendUpdates) -> Result<()> {
        gapi::delete_event_if_match(self.account_id, self.calendar_id, id, send, etag).await
    }
}

/// Discard removes only this operation's replacement while the original schedule is intact.
pub async fn discard(
    transport: &mut impl Transport,
    plan: &SplitPlan,
    send: gapi::SendUpdates,
) -> Result<()> {
    let master = transport.get(&plan.master_id).await?.ok_or_else(conflict)?;
    if master["extendedProperties"]["private"]["skimSplitApplied"] == plan.token {
        return Ok(());
    }
    if master["recurrence"] != plan.original_recurrence {
        return Err(SkimError::other("gcal_conflict", "The original schedule changed. Review both series in Google Calendar before discarding."));
    }
    if let Some(id) = &plan.replacement_id {
        if let Some(replacement) = transport.get(id).await? {
            if replacement["status"] == "cancelled" {
                return Ok(());
            }
            if replacement["extendedProperties"]["private"]["skimSplitSource"] != plan.token {
                return Err(SkimError::other(
                    "gcal_conflict",
                    "This replacement belongs to another operation. It was not deleted.",
                ));
            }
            let etag = replacement["etag"]
                .as_str()
                .ok_or_else(|| input_error("The replacement event version is unavailable."))?;
            transport.delete(id, etag, send).await?;
        }
    }
    Ok(())
}

/// Replacement first: failed creation never truncates the original series.
/// Each retry reads server markers before it performs a write.
pub async fn execute(
    transport: &mut impl Transport,
    plan: &SplitPlan,
    send: gapi::SendUpdates,
) -> Result<()> {
    let master = transport.get(&plan.master_id).await?.ok_or_else(conflict)?;
    if master["extendedProperties"]["private"]["skimSplitApplied"] == plan.token {
        return Ok(());
    }
    if master["etag"].as_str() != Some(&plan.master_etag) {
        return Err(conflict());
    }
    if let Some(id) = &plan.replacement_id {
        let existing = transport.get(id).await?;
        let replacement = match existing {
            Some(value) => value,
            None => match transport.insert(&plan.replacement, send).await {
                Ok(value) => value,
                Err(error) => match transport.get(id).await? {
                    Some(value) => value,
                    None => return Err(error),
                },
            },
        };
        if replacement["status"] == "cancelled"
            || replacement["extendedProperties"]["private"]["skimSplitSource"] != plan.token
        {
            return Err(SkimError::other(
                "gcal_conflict",
                "The replacement event changed. Review both series in Google Calendar.",
            ));
        }
    }
    transport
        .patch(&plan.master_id, &plan.master_patch, &plan.master_etag, send)
        .await?;
    Ok(())
}
