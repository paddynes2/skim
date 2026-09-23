//! IPC surface of the calendar (Phase 7.4 / 7.7 backend). Registered by the
//! main session from `docs/fork/pending/7.md`. Every command answers a typed
//! error (`gcal_not_configured`, `gcal_not_connected`, `gcal_input`) instead
//! of panicking when Google is not set up.

use super::gapi::{self, EventInput, SendUpdates};
use super::model::{self, CalendarRow, EventRow};
use super::store;
use crate::db::accounts as db_accounts;
use crate::error::{Result, SkimError};
use crate::fork::google;
use crate::state::AppState;
use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, State};

#[derive(Debug, Clone, Serialize)]
pub struct CalStatus {
    /// A Google OAuth client is available (stored or built in).
    pub configured: bool,
    /// This account has a calendar grant.
    pub connected: bool,
    /// The sync engine for it is running.
    pub engine_running: bool,
    pub pending_ops: i64,
}

async fn status(db: &crate::db::Db, account_id: &str) -> Result<CalStatus> {
    let aid = account_id.to_string();
    let pending = db
        .read("fork_cal_status", move |conn| {
            store::pending_op_count(conn, &aid)
        })
        .await
        .unwrap_or(0);
    Ok(CalStatus {
        configured: google::client_config()?.is_some(),
        connected: google::is_connected(account_id)?,
        engine_running: super::handle(account_id).is_some(),
        pending_ops: pending,
    })
}

fn require_connected(account_id: &str) -> Result<()> {
    if google::client_config()?.is_none() {
        return Err(SkimError::other(
            "gcal_not_configured",
            "Google client ID is not configured (Settings → Calendar)",
        ));
    }
    if !google::is_connected(account_id)? {
        return Err(SkimError::other(
            "gcal_not_connected",
            "Google Calendar is not connected for this account",
        ));
    }
    Ok(())
}

fn emit_updated(app: &AppHandle, account_id: &str) {
    let _ = app.emit(super::EVT_UPDATED, json!({ "account_id": account_id }));
}

#[tauri::command]
pub async fn fork_cal_status(state: State<'_, AppState>, account_id: String) -> Result<CalStatus> {
    status(&state.db, &account_id).await
}

/// Run the consent flow for the account, start its engine and pull.
#[tauri::command]
pub async fn fork_cal_connect(
    app: AppHandle,
    state: State<'_, AppState>,
    account_id: String,
) -> Result<CalStatus> {
    let aid = account_id.clone();
    let account = state
        .db
        .read("fork_cal_connect", move |conn| db_accounts::get(conn, &aid))
        .await?
        .ok_or_else(|| SkimError::other("gcal_input", "unknown account"))?;
    google::connect(&app, &account.id, &account.email).await?;
    let handle = super::spawn(
        app.clone(),
        state.db.clone(),
        account.id.clone(),
        account.email,
    );
    handle.sync_now();
    status(&state.db, &account_id).await
}

#[tauri::command]
pub async fn fork_cal_disconnect(
    app: AppHandle,
    state: State<'_, AppState>,
    account_id: String,
) -> Result<CalStatus> {
    super::stop(&account_id);
    google::disconnect(&account_id).await?;
    let aid = account_id.clone();
    state
        .db
        .call(move |conn| store::delete_account_data(conn, &aid))
        .await?;
    emit_updated(&app, &account_id);
    status(&state.db, &account_id).await
}

#[tauri::command]
pub async fn fork_cal_list_calendars(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<Vec<CalendarRow>> {
    state
        .db
        .read("fork_cal_list_calendars", move |conn| {
            store::list_calendars(conn, &account_id)
        })
        .await
}

#[tauri::command]
pub async fn fork_cal_set_selected(
    app: AppHandle,
    state: State<'_, AppState>,
    calendar_id: i64,
    selected: bool,
) -> Result<()> {
    let cal = state
        .db
        .call(move |conn| {
            store::set_selected(conn, calendar_id, selected)?;
            store::get_calendar(conn, calendar_id)
        })
        .await?
        .ok_or_else(|| SkimError::other("gcal_input", "unknown calendar"))?;
    emit_updated(&app, &cal.account_id);
    if let Some(h) = super::handle(&cal.account_id) {
        h.sync_now();
    }
    Ok(())
}

/// Events on selected calendars overlapping `[from_ts, to_ts)`, all accounts
/// unless one is named. Cancelled rows are included; the UI hides them.
#[tauri::command]
pub async fn fork_cal_events(
    state: State<'_, AppState>,
    from_ts: i64,
    to_ts: i64,
    account_id: Option<String>,
) -> Result<Vec<EventRow>> {
    state
        .db
        .read("fork_cal_events", move |conn| {
            store::events_between(conn, account_id.as_deref(), from_ts, to_ts)
        })
        .await
}

/// A local row for a new event. Timed events need both stamps; all-day ones
/// both dates (end exclusive, defaulting to the day after start).
pub fn local_row_from_input(calendar_id: i64, input: &EventInput) -> Result<EventRow> {
    let all_day = input
        .all_day
        .unwrap_or(input.start_date.is_some() && input.start_ts.is_none());
    let (start_ts, end_ts, start_date, end_date) = if all_day {
        let sd = input
            .start_date
            .clone()
            .ok_or_else(|| SkimError::other("gcal_input", "an all-day event needs start_date"))?;
        let s = model::date_to_utc_midnight(&sd)
            .ok_or_else(|| SkimError::other("gcal_input", "start_date is not YYYY-MM-DD"))?;
        let ed = input
            .end_date
            .clone()
            .unwrap_or_else(|| model::to_date(s + 86_400));
        let e = model::date_to_utc_midnight(&ed)
            .ok_or_else(|| SkimError::other("gcal_input", "end_date is not YYYY-MM-DD"))?;
        if e <= s {
            return Err(SkimError::other(
                "gcal_input",
                "end_date must be after start_date",
            ));
        }
        (s, e, Some(sd), Some(ed))
    } else {
        let s = input
            .start_ts
            .ok_or_else(|| SkimError::other("gcal_input", "a timed event needs start_ts"))?;
        let e = input.end_ts.unwrap_or(s + 1800);
        if e <= s {
            return Err(SkimError::other("gcal_input", "end must be after start"));
        }
        (s, e, None, None)
    };
    let attendees_json = input
        .attendees
        .as_ref()
        .filter(|a| !a.is_empty())
        .map(|a| gapi::attendees_body(a, None).to_string());
    Ok(EventRow {
        id: 0,
        calendar_id,
        google_id: String::new(),
        etag: None,
        status: "confirmed".into(),
        summary: input.summary.clone().unwrap_or_default(),
        description: input.description.clone(),
        location: input.location.clone(),
        start_ts,
        end_ts,
        all_day,
        start_date,
        end_date,
        time_zone: input.time_zone.clone(),
        recurring_event_id: None,
        organizer_email: None,
        attendees_json,
        self_response: None,
        transparency: None,
        hangout_link: None,
        html_link: None,
        updated: None,
        local_only: true,
    })
}

/// Apply an edit to a stored row (optimistic copy of what the patch will do).
pub fn apply_input(row: &mut EventRow, input: &EventInput) -> Result<()> {
    if let Some(s) = &input.summary {
        row.summary = s.clone();
    }
    if let Some(d) = &input.description {
        row.description = Some(d.clone());
    }
    if let Some(l) = &input.location {
        row.location = Some(l.clone());
    }
    if let Some(tz) = &input.time_zone {
        row.time_zone = Some(tz.clone());
    }
    let all_day = input.all_day.unwrap_or(row.all_day);
    if all_day {
        if input.start_date.is_some() || input.end_date.is_some() || !row.all_day {
            let sd = input
                .start_date
                .clone()
                .or_else(|| row.start_date.clone())
                .unwrap_or_else(|| model::to_date(row.start_ts));
            let s = model::date_to_utc_midnight(&sd)
                .ok_or_else(|| SkimError::other("gcal_input", "start_date is not YYYY-MM-DD"))?;
            let ed = input
                .end_date
                .clone()
                .or_else(|| row.end_date.clone().filter(|_| row.all_day))
                .unwrap_or_else(|| model::to_date(s + 86_400));
            let e = model::date_to_utc_midnight(&ed)
                .ok_or_else(|| SkimError::other("gcal_input", "end_date is not YYYY-MM-DD"))?;
            if e <= s {
                return Err(SkimError::other(
                    "gcal_input",
                    "end_date must be after start_date",
                ));
            }
            row.start_ts = s;
            row.end_ts = e;
            row.start_date = Some(sd);
            row.end_date = Some(ed);
        }
    } else {
        if let Some(s) = input.start_ts {
            row.start_ts = s;
        }
        if let Some(e) = input.end_ts {
            row.end_ts = e;
        }
        if row.end_ts <= row.start_ts {
            return Err(SkimError::other("gcal_input", "end must be after start"));
        }
        row.start_date = None;
        row.end_date = None;
    }
    row.all_day = all_day;
    if let Some(emails) = &input.attendees {
        let existing: Option<Value> = row
            .attendees_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok());
        row.attendees_json = Some(gapi::attendees_body(emails, existing.as_ref()).to_string());
        row.self_response = model::self_response(
            row.attendees_json
                .as_deref()
                .and_then(|s| serde_json::from_str::<Value>(s).ok())
                .as_ref(),
        );
    }
    Ok(())
}

/// Create an event: local row at once, `create` op queued, engine poked.
/// `send_updates` is `"all"` or `"none"`; the UI asks when there are guests.
#[tauri::command]
pub async fn fork_cal_create(
    app: AppHandle,
    state: State<'_, AppState>,
    account_id: String,
    calendar_id: i64,
    input: EventInput,
    send_updates: String,
) -> Result<EventRow> {
    require_connected(&account_id)?;
    let send = SendUpdates::parse(&send_updates)?;
    let row = local_row_from_input(calendar_id, &input)?;
    let body = gapi::event_body(&input, None);
    let aid = account_id.clone();
    let created = state
        .db
        .call(move |conn| {
            let cal = store::get_calendar(conn, calendar_id)?;
            if cal.as_ref().is_none_or(|c| c.account_id != aid) {
                return Ok(None);
            }
            let id = store::insert_local_event(conn, &row)?;
            store::queue_op(
                conn,
                &aid,
                "create",
                &json!({ "event_id": id, "send_updates": send.as_str(), "body": body }),
            )?;
            store::get_event(conn, id)
        })
        .await?
        .ok_or_else(|| SkimError::other("gcal_input", "unknown calendar for this account"))?;
    emit_updated(&app, &account_id);
    if let Some(h) = super::handle(&account_id) {
        h.run_ops();
    }
    Ok(created)
}

/// Edit an event: local row updated at once, `patch` op queued.
#[tauri::command]
pub async fn fork_cal_patch(
    app: AppHandle,
    state: State<'_, AppState>,
    event_id: i64,
    input: EventInput,
    send_updates: String,
) -> Result<EventRow> {
    let send = SendUpdates::parse(&send_updates)?;
    let (account_id, row) = state
        .db
        .call(move |conn| {
            let Some(mut row) = store::get_event(conn, event_id)? else {
                return Ok(None);
            };
            let Some(cal) = store::get_calendar(conn, row.calendar_id)? else {
                return Ok(None);
            };
            let existing: Option<Value> = row
                .attendees_json
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok());
            let body = gapi::event_body(&input, existing.as_ref());
            if let Err(e) = apply_input(&mut row, &input) {
                return Ok(Some(Err(e)));
            }
            store::update_local_event(conn, &row)?;
            store::queue_op(
                conn,
                &cal.account_id,
                "patch",
                &json!({ "event_id": event_id, "send_updates": send.as_str(), "body": body }),
            )?;
            Ok(Some(Ok((cal.account_id, row))))
        })
        .await?
        .ok_or_else(|| SkimError::other("gcal_input", "unknown event"))??;
    require_connected(&account_id)?;
    emit_updated(&app, &account_id);
    if let Some(h) = super::handle(&account_id) {
        h.run_ops();
    }
    Ok(row)
}

/// Delete an event: row gone at once. A row whose create never left the
/// machine just drops its pending ops; anything else queues a `delete`.
#[tauri::command]
pub async fn fork_cal_delete(
    app: AppHandle,
    state: State<'_, AppState>,
    event_id: i64,
    send_updates: String,
) -> Result<()> {
    let send = SendUpdates::parse(&send_updates)?;
    let account_id = state
        .db
        .call(move |conn| {
            let Some(row) = store::get_event(conn, event_id)? else {
                return Ok(None);
            };
            let Some(cal) = store::get_calendar(conn, row.calendar_id)? else {
                return Ok(None);
            };
            if model::is_local_id(&row.google_id) {
                store::drop_ops_for_event(conn, event_id)?;
            } else {
                store::drop_ops_for_event(conn, event_id)?;
                store::queue_op(
                    conn,
                    &cal.account_id,
                    "delete",
                    &json!({
                        "calendar_google_id": cal.google_id,
                        "google_id": row.google_id,
                        "send_updates": send.as_str()
                    }),
                )?;
            }
            store::delete_event_row(conn, event_id)?;
            Ok(Some(cal.account_id))
        })
        .await?
        .ok_or_else(|| SkimError::other("gcal_input", "unknown event"))?;
    emit_updated(&app, &account_id);
    if let Some(h) = super::handle(&account_id) {
        h.run_ops();
    }
    Ok(())
}

/// Answer an invitation: `accepted` / `declined` / `tentative`.
#[tauri::command]
pub async fn fork_cal_rsvp(
    app: AppHandle,
    state: State<'_, AppState>,
    event_id: i64,
    response: String,
    send_updates: String,
) -> Result<EventRow> {
    if !gapi::valid_response(&response) {
        return Err(SkimError::other(
            "gcal_input",
            "response must be accepted, declined or tentative",
        ));
    }
    let send = SendUpdates::parse(&send_updates)?;
    let (account_id, account_email) = state
        .db
        .call(move |conn| {
            let Some(row) = store::get_event(conn, event_id)? else {
                return Ok(None);
            };
            let Some(cal) = store::get_calendar(conn, row.calendar_id)? else {
                return Ok(None);
            };
            Ok(db_accounts::get(conn, &cal.account_id)?.map(|a| (a.id, a.email)))
        })
        .await?
        .ok_or_else(|| SkimError::other("gcal_input", "unknown event"))?;
    require_connected(&account_id)?;
    let resp = response.clone();
    let aid = account_id.clone();
    let row = state
        .db
        .call(move |conn| {
            let Some(mut row) = store::get_event(conn, event_id)? else {
                return Ok(None);
            };
            let body = gapi::rsvp_body(row.attendees_json.as_deref(), &account_email, &resp);
            row.attendees_json = Some(body["attendees"].to_string());
            row.self_response = Some(resp.clone());
            store::update_local_event(conn, &row)?;
            store::queue_op(
                conn,
                &aid,
                "rsvp",
                &json!({ "event_id": event_id, "response": resp, "send_updates": send.as_str() }),
            )?;
            Ok(Some(row))
        })
        .await?
        .ok_or_else(|| SkimError::other("gcal_input", "unknown event"))?;
    emit_updated(&app, &account_id);
    if let Some(h) = super::handle(&account_id) {
        h.run_ops();
    }
    Ok(row)
}

#[tauri::command]
pub async fn fork_cal_sync_now(account_id: String) -> Result<()> {
    require_connected(&account_id)?;
    match super::handle(&account_id) {
        Some(h) => {
            h.sync_now();
            Ok(())
        }
        None => Err(SkimError::other(
            "gcal_not_connected",
            "the calendar engine is not running for this account",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timed_local_row_defaults_to_thirty_minutes() {
        let row = local_row_from_input(
            3,
            &EventInput {
                summary: Some("Call".into()),
                start_ts: Some(1000),
                attendees: Some(vec!["a@x.com".into()]),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!((row.start_ts, row.end_ts), (1000, 2800));
        assert!(!row.all_day && row.local_only);
        assert_eq!(row.calendar_id, 3);
        assert_eq!(
            row.attendees_json.as_deref(),
            Some(r#"[{"email":"a@x.com"}]"#)
        );
        assert!(local_row_from_input(3, &EventInput::default()).is_err());
        assert!(local_row_from_input(
            3,
            &EventInput {
                start_ts: Some(10),
                end_ts: Some(5),
                ..Default::default()
            }
        )
        .is_err());
    }

    #[test]
    fn all_day_local_row_defaults_to_one_day() {
        let row = local_row_from_input(
            1,
            &EventInput {
                start_date: Some("2026-10-01".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(row.all_day);
        assert_eq!(row.end_date.as_deref(), Some("2026-10-02"));
        assert_eq!(row.end_ts - row.start_ts, 86_400);
        assert_eq!(row.attendees_json, None);
    }

    #[test]
    fn apply_input_edits_in_place_and_keeps_answers() {
        let mut row = local_row_from_input(
            1,
            &EventInput {
                summary: Some("Old".into()),
                start_ts: Some(1000),
                end_ts: Some(2000),
                ..Default::default()
            },
        )
        .unwrap();
        row.attendees_json =
            Some(r#"[{"email":"a@x.com","responseStatus":"accepted"},{"email":"me@x.com","self":true,"responseStatus":"tentative"}]"#.into());
        apply_input(
            &mut row,
            &EventInput {
                summary: Some("New".into()),
                end_ts: Some(2600),
                attendees: Some(vec!["a@x.com".into(), "me@x.com".into(), "c@x.com".into()]),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(row.summary, "New");
        assert_eq!((row.start_ts, row.end_ts), (1000, 2600));
        assert_eq!(row.self_response.as_deref(), Some("tentative"));
        assert!(row.attendees_json.as_deref().unwrap().contains("c@x.com"));
        // Switch to all-day: dates derived from the timestamp.
        apply_input(
            &mut row,
            &EventInput {
                all_day: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(row.all_day);
        assert_eq!(row.start_date.as_deref(), Some("1970-01-01"));
        assert_eq!(row.end_date.as_deref(), Some("1970-01-02"));
        // And back, with an explicit range.
        apply_input(
            &mut row,
            &EventInput {
                all_day: Some(false),
                start_ts: Some(5000),
                end_ts: Some(5600),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!row.all_day && row.start_date.is_none());
        assert!(apply_input(
            &mut row,
            &EventInput {
                end_ts: Some(10),
                ..Default::default()
            }
        )
        .is_err());
    }
}
