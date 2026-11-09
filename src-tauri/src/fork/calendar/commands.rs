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
    /// Wanted scopes the stored grant lacks (empty when not connected). A
    /// grant from before D98 lists `calendar.calendarlist` and
    /// `calendar.calendars`: events still work, calendar list writes need a
    /// reconnect.
    pub missing_scopes: Vec<String>,
}

async fn status(db: &crate::db::Db, account_id: &str) -> Result<CalStatus> {
    let aid = account_id.to_string();
    let pending = db
        .read("fork_cal_status", move |conn| {
            store::pending_op_count(conn, &aid)
        })
        .await
        .unwrap_or(0);
    let connected = google::is_connected(account_id)?;
    let missing_scopes = if connected {
        google::missing_scopes(google::granted_scopes(account_id)?.as_deref())
    } else {
        vec![]
    };
    Ok(CalStatus {
        configured: google::client_config()?.is_some(),
        connected,
        engine_running: super::handle(account_id).is_some(),
        pending_ops: pending,
        missing_scopes,
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

async fn require_no_split(state: &State<'_, AppState>, event_id: i64) -> Result<()> {
    if state
        .db
        .read("fork_cal_split_pending", move |conn| {
            store::event_has_split(conn, event_id)
        })
        .await?
    {
        return Err(SkimError::other(
            "gcal_input",
            "Resolve the pending or failed series change before editing this series.",
        ));
    }
    Ok(())
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
    input.options.validate()?;
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
    let attendees_json = gapi::event_body(input, None)
        .get("attendees")
        .map(Value::to_string);
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
        transparency: input.options.transparency.clone(),
        hangout_link: None,
        html_link: None,
        updated: None,
        local_only: true,
        options: input.options.clone(),
        ical_uid: None,
        event_type: input.options.event_type.clone(),
        end_time_zone: input
            .end_time_zone
            .clone()
            .filter(|z| !z.is_empty() && !all_day),
    })
}

fn input_error(message: &'static str) -> SkimError {
    SkimError::other("gcal_input", message)
}

/// Whether the input describes an all-day event (the rule `event_body` uses).
fn input_all_day(input: &EventInput) -> bool {
    input
        .all_day
        .unwrap_or(input.start_date.is_some() && input.start_ts.is_none())
}

fn input_has_guests(input: &EventInput) -> bool {
    [&input.attendees, &input.optional_attendees]
        .iter()
        .any(|list| {
            list.as_ref()
                .is_some_and(|l| l.iter().any(|e| !e.trim().is_empty()))
        })
}

/// A properties object may only ride on its own event type.
fn check_type_properties(input: &EventInput, event_type: &str) -> Result<()> {
    let o = &input.options;
    let stray = (o.out_of_office_properties.is_some() && event_type != "outOfOffice")
        || (o.focus_time_properties.is_some() && event_type != "focusTime")
        || (o.working_location_properties.is_some() && event_type != "workingLocation");
    if stray {
        return Err(input_error("event_type_properties"));
    }
    Ok(())
}

/// What each event type demands of its fields, shared by create and patch.
/// `all_day` is the event's state after the change.
fn check_type_shape(input: &mut EventInput, event_type: &str, all_day: bool) -> Result<()> {
    match event_type {
        "outOfOffice" | "focusTime" => {
            if input_has_guests(input) {
                return Err(input_error("event_type_no_guests"));
            }
            if all_day {
                return Err(input_error("event_type_timed_only"));
            }
        }
        "workingLocation" => {
            input.options.transparency = Some("transparent".into());
            input.options.visibility = Some("public".into());
        }
        _ => {}
    }
    Ok(())
}

/// The event type rules for a create on `calendar` (D98): one of the four
/// writable types; anything but `default` only on the account's primary
/// calendar; out of office and focus time take no guests and are timed;
/// working location is always free and public and names its place.
pub fn apply_event_type_create(input: &mut EventInput, calendar: &CalendarRow) -> Result<()> {
    let event_type =
        super::options::event_type_or_default(input.options.event_type.as_deref()).to_string();
    if !super::options::WRITABLE_EVENT_TYPES.contains(&event_type.as_str()) {
        return Err(input_error("event_type_invalid"));
    }
    check_type_properties(input, &event_type)?;
    if event_type == "default" {
        return Ok(());
    }
    if !calendar.is_primary {
        return Err(input_error("event_type_primary_only"));
    }
    if event_type == "workingLocation" && input.options.working_location_properties.is_none() {
        return Err(input_error("event_type_properties"));
    }
    let all_day = input_all_day(input);
    check_type_shape(input, &event_type, all_day)
}

/// The event type rules for a patch of `row` (the occurrence, or the master
/// for a series edit). Google refuses to change `eventType`, so naming a
/// different one is refused here; naming the same one is dropped from the
/// body. The shape rules of the stored type still hold.
pub fn apply_event_type_patch(input: &mut EventInput, row: &EventRow) -> Result<()> {
    let current = super::options::event_type_or_default(
        row.event_type
            .as_deref()
            .or(row.options.event_type.as_deref()),
    )
    .to_string();
    if let Some(asked) = input.options.event_type.take() {
        if super::options::event_type_or_default(Some(&asked)) != current {
            return Err(input_error("event_type_fixed"));
        }
    }
    check_type_properties(input, &current)?;
    let all_day = input.all_day.unwrap_or(row.all_day);
    let touches_shape = input_has_guests(input) || input.all_day.is_some();
    if current == "workingLocation" {
        // Only force the pair when the patch names either, so a title edit
        // stays a title edit.
        if input.options.transparency.is_some() || input.options.visibility.is_some() {
            check_type_shape(input, &current, all_day)?;
        }
        return Ok(());
    }
    if touches_shape {
        check_type_shape(input, &current, all_day)?;
    }
    Ok(())
}

/// Apply an edit to a stored row (optimistic copy of what the patch will do).
pub fn apply_input(row: &mut EventRow, input: &EventInput) -> Result<()> {
    input.options.validate()?;
    row.options.merge(&input.options);
    if let Some(v) = &input.options.transparency {
        row.transparency = Some(v.clone());
    }
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
    if let Some(tz) = input.end_time_zone.as_deref().filter(|z| !z.is_empty()) {
        row.end_time_zone = Some(tz.to_string());
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
    if input.optional_attendees.is_some() {
        let existing = row
            .attendees_json
            .as_deref()
            .and_then(|s| serde_json::from_str::<Value>(s).ok());
        let body = gapi::event_body(input, existing.as_ref());
        row.attendees_json = body.get("attendees").map(Value::to_string);
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
    let mut input = input;
    let calendar = state
        .db
        .read("fork_cal_create", move |conn| {
            store::get_calendar(conn, calendar_id)
        })
        .await?
        .filter(|c| c.account_id == account_id)
        .ok_or_else(|| SkimError::other("gcal_input", "unknown calendar for this account"))?;
    apply_event_type_create(&mut input, &calendar)?;
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

/// Read the series master without adding it to the instance cache.
#[tauri::command]
pub async fn fork_cal_series(state: State<'_, AppState>, event_id: i64) -> Result<EventRow> {
    let (row, cal) = state
        .db
        .read("fork_cal_series", move |conn| {
            let Some(row) = store::get_event(conn, event_id)? else {
                return Ok(None);
            };
            Ok(store::get_calendar(conn, row.calendar_id)?.map(|c| (row, c)))
        })
        .await?
        .ok_or_else(|| SkimError::other("gcal_input", "Unknown event"))?;
    require_connected(&cal.account_id)?;
    let series_id = row.recurring_event_id.as_deref().unwrap_or(&row.google_id);
    let value = gapi::fetch_event(&cal.account_id, &cal.google_id, series_id).await?;
    let mut master = model::event_from_json(cal.id, &value)
        .ok_or_else(|| SkimError::other("gcal_api", "The series could not be loaded"))?;
    master.id = row.id;
    master.recurring_event_id = Some(series_id.to_string());
    Ok(master)
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
    require_no_split(&state, event_id).await?;
    if input.following && input.series {
        return Err(SkimError::other(
            "gcal_input",
            "Choose one recurrence edit scope.",
        ));
    }
    if input.following {
        return patch_following(app, state, event_id, input, send).await;
    }
    if input.series {
        let master = fork_cal_series(state.clone(), event_id).await?;
        let mut input = input;
        apply_event_type_patch(&mut input, &master)?;
        let mut validated = master.clone();
        apply_input(&mut validated, &input)?;
        let existing = master
            .attendees_json
            .as_deref()
            .and_then(|s| serde_json::from_str::<Value>(s).ok());
        let body = gapi::event_body_for(&input, existing.as_ref(), master.end_time_zone.as_deref());
        let (account_id, row) = state
            .db
            .call(move |conn| {
                let Some(row) = store::get_event(conn, event_id)? else {
                    return Ok(None);
                };
                let Some(cal) = store::get_calendar(conn, row.calendar_id)? else {
                    return Ok(None);
                };
                store::queue_op(
                    conn,
                    &cal.account_id,
                    "patch",
                    &json!({
                        "event_id": event_id, "google_id": master.google_id,
                        "send_updates": send.as_str(), "body": body
                    }),
                )?;
                Ok(Some((cal.account_id, row)))
            })
            .await?
            .ok_or_else(|| SkimError::other("gcal_input", "Unknown event"))?;
        emit_updated(&app, &account_id);
        if let Some(h) = super::handle(&account_id) {
            h.run_ops();
        }
        return Ok(row);
    }
    let (account_id, row) = state
        .db
        .call(move |conn| {
            let Some(mut row) = store::get_event(conn, event_id)? else {
                return Ok(None);
            };
            let Some(cal) = store::get_calendar(conn, row.calendar_id)? else {
                return Ok(None);
            };
            if row.recurring_event_id.is_some() && input.options.recurrence.is_some() {
                return Ok(Some(Err(SkimError::other(
                    "gcal_input",
                    "Edit the series to change recurrence",
                ))));
            }
            let mut input = input;
            if let Err(e) = apply_event_type_patch(&mut input, &row) {
                return Ok(Some(Err(e)));
            }
            let existing: Option<Value> = row
                .attendees_json
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok());
            let body =
                gapi::event_body_for(&input, existing.as_ref(), row.end_time_zone.as_deref());
            if let Err(e) = require_connected(&cal.account_id) {
                return Ok(Some(Err(e)));
            }
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

async fn patch_following(
    app: AppHandle,
    state: State<'_, AppState>,
    event_id: i64,
    input: EventInput,
    send: SendUpdates,
) -> Result<EventRow> {
    let (row, cal) = state
        .db
        .read("fork_cal_following", move |conn| {
            let Some(row) = store::get_event(conn, event_id)? else {
                return Ok(None);
            };
            Ok(store::get_calendar(conn, row.calendar_id)?.map(|cal| (row, cal)))
        })
        .await?
        .ok_or_else(|| SkimError::other("gcal_input", "Unknown event"))?;
    require_connected(&cal.account_id)?;
    if !matches!(cal.access_role.as_str(), "owner" | "writer") {
        return Err(SkimError::other(
            "gcal_input",
            "This calendar is read only.",
        ));
    }
    let series_id = row.recurring_event_id.as_deref().ok_or_else(|| {
        SkimError::other("gcal_input", "Select an occurrence of a recurring event.")
    })?;
    let master = gapi::fetch_event(&cal.account_id, &cal.google_id, series_id).await?;
    let instance = gapi::fetch_event(&cal.account_id, &cal.google_id, &row.google_id).await?;
    if master["organizer"]["self"] == false {
        return Err(SkimError::other(
            "gcal_input",
            "Only the organizer can split this recurring event.",
        ));
    }
    let mut input = input;
    apply_event_type_patch(&mut input, &row)?;
    let mut validation = row.clone();
    apply_input(&mut validation, &input)?;
    let body = gapi::event_body_for(
        &input,
        master.get("attendees"),
        master["end"]["timeZone"].as_str(),
    );
    let instances = if super::split::occurrence_count(&master)?.is_some() {
        gapi::fetch_instances(&cal.account_id, &cal.google_id, series_id).await?
    } else {
        vec![]
    };
    let token = uuid::Uuid::new_v4().simple().to_string();
    let plan = super::split::plan(
        &cal.google_id,
        &master,
        &instance,
        &body,
        &instances,
        &token,
    )?;
    let aid = cal.account_id.clone();
    let queued = state
        .db
        .call(move |conn| {
            if store::event_has_ops(conn, event_id)? {
                return Ok(false);
            }
            store::queue_op(
                conn,
                &aid,
                "split",
                &json!({ "event_id": event_id, "send_updates": send.as_str(), "plan": plan }),
            )?;
            Ok(true)
        })
        .await?;
    if !queued {
        return Err(SkimError::other(
            "gcal_input",
            "Wait for this event to sync before splitting its series.",
        ));
    }
    emit_updated(&app, &cal.account_id);
    if let Some(handle) = super::handle(&cal.account_id) {
        handle.run_ops();
    }
    Ok(row)
}

#[tauri::command]
pub async fn fork_cal_availability(
    state: State<'_, AppState>,
    account_id: String,
    from_ts: i64,
    to_ts: i64,
    attendees: Vec<String>,
    exclude_event_id: Option<i64>,
) -> Result<super::availability::Availability> {
    let aid = account_id.clone();
    let (account, exclude, selected) = state
        .db
        .read("fork_cal_availability", move |conn| {
            let account = db_accounts::get(conn, &aid)?;
            let exclude = exclude_event_id
                .map(|id| store::get_event(conn, id))
                .transpose()?
                .flatten();
            let selected = store::list_calendars(conn, &aid)?
                .into_iter()
                .filter(|cal| cal.selected)
                .map(|cal| cal.google_id)
                .collect::<Vec<_>>();
            Ok((account, exclude, selected))
        })
        .await?;
    let account = account.ok_or_else(|| SkimError::other("gcal_input", "Unknown account"))?;
    let exclude_id = exclude.as_ref().map(|event| event.google_id.as_str());
    super::availability::fetch(
        &account_id,
        &account.email,
        from_ts,
        to_ts,
        attendees,
        exclude_id,
        &selected,
    )
    .await
}

#[tauri::command]
pub async fn fork_cal_sync_state(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<store::SyncState> {
    let health = super::sync_health(&account_id);
    let mut state = state
        .db
        .read("fork_cal_sync_state", move |conn| {
            store::sync_state(conn, &account_id)
        })
        .await?;
    state.syncing = health.syncing;
    state.last_synced_at = health.last_synced_at;
    state.last_sync_error = health.last_sync_error;
    Ok(state)
}

#[tauri::command]
pub async fn fork_cal_retry(
    app: AppHandle,
    state: State<'_, AppState>,
    account_id: String,
    event_id: Option<i64>,
) -> Result<usize> {
    require_connected(&account_id)?;
    let aid = account_id.clone();
    let count = state
        .db
        .call(move |conn| store::retry_failed(conn, &aid, event_id))
        .await?;
    emit_updated(&app, &account_id);
    if let Some(handle) = super::handle(&account_id) {
        handle.run_ops();
    }
    Ok(count)
}

#[tauri::command]
pub async fn fork_cal_discard_split(
    app: AppHandle,
    state: State<'_, AppState>,
    account_id: String,
    event_id: i64,
    send_updates: String,
) -> Result<()> {
    require_connected(&account_id)?;
    let send = SendUpdates::parse(&send_updates)?;
    let aid = account_id.clone();
    let (op_id, payload) = state
        .db
        .call(move |conn| store::claim_failed_split(conn, &aid, event_id))
        .await?
        .ok_or_else(|| {
            SkimError::other(
                "gcal_input",
                "This event has no failed series change to discard.",
            )
        })?;
    let result = async {
        let payload: Value = serde_json::from_str(&payload)
            .map_err(|_| SkimError::other("gcal_op", "The failed operation is invalid."))?;
        let plan: super::split::SplitPlan = serde_json::from_value(payload["plan"].clone())
            .map_err(|_| SkimError::other("gcal_op", "The series split is invalid."))?;
        let mut transport = super::split::GoogleTransport {
            account_id: &account_id,
            calendar_id: &plan.calendar_google_id,
        };
        super::split::discard(&mut transport, &plan, send).await
    }
    .await;
    let message = result.as_ref().err().map(ToString::to_string);
    let success = result.is_ok();
    state
        .db
        .call(move |conn| {
            if let Some(message) = message {
                store::record_op_error(conn, op_id, &message)?;
            }
            store::finish_op(conn, op_id, success)
        })
        .await?;
    emit_updated(&app, &account_id);
    if success {
        if let Some(handle) = super::handle(&account_id) {
            handle.sync_now();
        }
    }
    result
}

/// Delete removes the local row and queues the server deletion.
/// A stable create ID also covers a create whose reply was lost.
#[tauri::command]
pub async fn fork_cal_delete(
    app: AppHandle,
    state: State<'_, AppState>,
    event_id: i64,
    send_updates: String,
    series: Option<bool>,
) -> Result<()> {
    let send = SendUpdates::parse(&send_updates)?;
    require_no_split(&state, event_id).await?;
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
                // The server can accept a create even when its reply never reaches this device.
                let remote_id = store::create_operation_token(conn, event_id)?;
                store::drop_ops_for_event(conn, event_id)?;
                if let Some(remote_id) = remote_id {
                    store::queue_op(conn, &cal.account_id, "delete", &json!({
                        "calendar_google_id": cal.google_id, "google_id": remote_id,
                        "send_updates": send.as_str()
                    }))?;
                }
            } else {
                store::drop_ops_for_event(conn, event_id)?;
                store::queue_op(
                    conn,
                    &cal.account_id,
                    "delete",
                    &json!({
                        "calendar_google_id": cal.google_id,
                        "google_id": if series == Some(true) { row.recurring_event_id.as_ref().unwrap_or(&row.google_id) } else { &row.google_id },
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

/// The queued `rsvp` op for `row`. With `series` on an instance, the op
/// names the series master (`google_id`), so the engine patches the master
/// and the answer covers every occurrence. `comment` rides along when given.
pub fn rsvp_op_payload(
    row: &EventRow,
    response: &str,
    send: SendUpdates,
    comment: Option<&str>,
    series: bool,
) -> Value {
    let mut payload =
        json!({ "event_id": row.id, "response": response, "send_updates": send.as_str() });
    if let Some(c) = comment {
        payload["comment"] = json!(c);
    }
    if let Some(master) = row.recurring_event_id.as_deref().filter(|_| series) {
        payload["google_id"] = json!(master);
        payload["series"] = json!(true);
    }
    payload
}

/// Answer an invitation: `accepted` / `declined` / `tentative`.
/// `comment` is the note to the organiser (`""` clears it, omitted keeps it).
/// `series: true` on an occurrence answers for the whole series.
#[tauri::command]
pub async fn fork_cal_rsvp(
    app: AppHandle,
    state: State<'_, AppState>,
    event_id: i64,
    response: String,
    send_updates: String,
    comment: Option<String>,
    series: Option<bool>,
) -> Result<EventRow> {
    if !gapi::valid_response(&response) {
        return Err(SkimError::other(
            "gcal_input",
            "response must be accepted, declined or tentative",
        ));
    }
    let send = SendUpdates::parse(&send_updates)?;
    let series = series.unwrap_or(false);
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
            let note = comment.as_deref();
            let master = row.recurring_event_id.clone().filter(|_| series);
            let payload = rsvp_op_payload(&row, &resp, send, note, series);
            let answer = |r: &mut EventRow| {
                let body =
                    gapi::rsvp_body(r.attendees_json.as_deref(), &account_email, &resp, note);
                r.attendees_json = Some(body["attendees"].to_string());
                r.self_response = Some(resp.clone());
            };
            answer(&mut row);
            store::update_local_event(conn, &row)?;
            if let Some(master) = master {
                for mut other in store::series_rows(conn, row.calendar_id, &master)? {
                    if other.id != row.id {
                        answer(&mut other);
                        store::update_local_event(conn, &other)?;
                    }
                }
            }
            store::queue_op(conn, &aid, "rsvp", &payload)?;
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

fn writable(cal: &CalendarRow) -> bool {
    matches!(cal.access_role.as_str(), "owner" | "writer")
}

/// The rules for moving `row` from `from` to `to` (D98). Google moves only
/// the organiser's copy of a single event or a whole series, of the default
/// type, between calendars of one account that the user can write.
pub fn check_move(row: &EventRow, from: &CalendarRow, to: &CalendarRow) -> Result<()> {
    if row.recurring_event_id.is_some() {
        return Err(input_error("move_instance"));
    }
    let event_type = row
        .event_type
        .as_deref()
        .or(row.options.event_type.as_deref());
    if super::options::event_type_or_default(event_type) != "default" {
        return Err(input_error("move_event_type"));
    }
    if from.account_id != to.account_id {
        return Err(input_error("move_other_account"));
    }
    if from.id == to.id {
        return Err(input_error("move_same_calendar"));
    }
    if !writable(from) || !writable(to) {
        return Err(input_error("move_read_only"));
    }
    if model::is_local_id(&row.google_id) || row.local_only {
        return Err(input_error("move_not_synced"));
    }
    Ok(())
}

/// The queued `move` op's payload. `calendar_google_id` and `google_id` name
/// the source copy, so a pull of the source calendar leaves it alone while
/// the move waits (see `store::replace_window`).
pub fn move_op_payload(
    row: &EventRow,
    from: &CalendarRow,
    to: &CalendarRow,
    send: SendUpdates,
) -> Value {
    json!({
        "event_id": row.id,
        "calendar_google_id": from.google_id,
        "google_id": row.google_id,
        "to_calendar_id": to.id,
        "to_calendar_google_id": to.google_id,
        "send_updates": send.as_str(),
    })
}

/// Move an event to another calendar of the same account: the row moves at
/// once and a `move` op is queued. `send_updates` is required, as for every
/// write that can reach guests.
#[tauri::command]
pub async fn fork_cal_move(
    app: AppHandle,
    state: State<'_, AppState>,
    event_id: i64,
    to_calendar_id: i64,
    send_updates: String,
) -> Result<EventRow> {
    let send = SendUpdates::parse(&send_updates)?;
    require_no_split(&state, event_id).await?;
    let (account_id, row) = state
        .db
        .call(move |conn| {
            let Some(row) = store::get_event(conn, event_id)? else {
                return Ok(None);
            };
            let (Some(from), Some(to)) = (
                store::get_calendar(conn, row.calendar_id)?,
                store::get_calendar(conn, to_calendar_id)?,
            ) else {
                return Ok(None);
            };
            if let Err(e) =
                check_move(&row, &from, &to).and_then(|()| require_connected(&from.account_id))
            {
                return Ok(Some(Err(e)));
            }
            if store::event_has_ops(conn, event_id)? {
                return Ok(Some(Err(input_error("move_pending"))));
            }
            if store::event_by_google_id(conn, to.id, &row.google_id)?.is_some() {
                return Ok(Some(Err(input_error("move_exists"))));
            }
            store::queue_op(
                conn,
                &from.account_id,
                "move",
                &move_op_payload(&row, &from, &to, send),
            )?;
            store::set_event_calendar(conn, event_id, to.id)?;
            Ok(store::get_event(conn, event_id)?.map(|r| Ok((from.account_id, r))))
        })
        .await?
        .ok_or_else(|| SkimError::other("gcal_input", "unknown event or calendar"))??;
    emit_updated(&app, &account_id);
    if let Some(h) = super::handle(&account_id) {
        h.run_ops();
    }
    Ok(row)
}

/// Set a calendar's colour (`#rrggbb`) in the user's Google calendar list.
/// A direct call: no guest hears of it. Errors go back to the caller.
#[tauri::command]
pub async fn fork_cal_set_color(
    app: AppHandle,
    state: State<'_, AppState>,
    calendar_id: i64,
    color: String,
) -> Result<CalendarRow> {
    let body = gapi::calendar_color_body(&color).ok_or_else(|| input_error("color_invalid"))?;
    let cal = state
        .db
        .read("fork_cal_set_color", move |conn| {
            store::get_calendar(conn, calendar_id)
        })
        .await?
        .ok_or_else(|| SkimError::other("gcal_input", "unknown calendar"))?;
    require_connected(&cal.account_id)?;
    let resp = gapi::patch_calendar_list(&cal.account_id, &cal.google_id, &body).await?;
    let stored = resp["backgroundColor"]
        .as_str()
        .and_then(gapi::parse_hex_color)
        .unwrap_or_else(|| {
            body["backgroundColor"]
                .as_str()
                .unwrap_or_default()
                .to_string()
        });
    let row = state
        .db
        .call(move |conn| {
            store::set_calendar_color(conn, calendar_id, &stored)?;
            store::get_calendar(conn, calendar_id)
        })
        .await?
        .ok_or_else(|| SkimError::other("gcal_input", "unknown calendar"))?;
    emit_updated(&app, &row.account_id);
    Ok(row)
}

/// The calendar id `fork_cal_add_calendar` sends: trimmed, one token.
pub fn normalize_calendar_ref(calendar: &str) -> Result<String> {
    let id = calendar.trim();
    if id.is_empty() || id.len() > 1024 || id.chars().any(char::is_whitespace) {
        return Err(input_error("calendar_invalid"));
    }
    Ok(id.to_string())
}

/// Validate a new calendar's name and zone.
pub fn check_new_calendar(summary: &str, time_zone: &str) -> Result<(String, String)> {
    let summary = summary.trim();
    if summary.is_empty() || summary.chars().count() > 255 {
        return Err(input_error("calendar_summary_invalid"));
    }
    let zone = time_zone.trim();
    if zone.parse::<chrono_tz::Tz>().is_err() {
        return Err(input_error("time_zone_invalid"));
    }
    Ok((summary.to_string(), zone.to_string()))
}

/// Store a calendar the user added or created, selected, and pull it the way
/// a newly selected calendar is pulled (`fork_cal_set_selected`).
async fn adopt_calendar(
    app: &AppHandle,
    state: &State<'_, AppState>,
    mut row: CalendarRow,
) -> Result<CalendarRow> {
    row.selected = true;
    let stored = state
        .db
        .call(move |conn| store::upsert_selected_calendar(conn, &row))
        .await?;
    emit_updated(app, &stored.account_id);
    if let Some(h) = super::handle(&stored.account_id) {
        h.sync_now();
    }
    Ok(stored)
}

/// Add someone's shared calendar (their address or a calendar id) to the
/// account's list, selected and syncing. `gcal_not_shared` when Google says
/// it is not shared with this account.
#[tauri::command]
pub async fn fork_cal_add_calendar(
    app: AppHandle,
    state: State<'_, AppState>,
    account_id: String,
    calendar: String,
) -> Result<CalendarRow> {
    let id = normalize_calendar_ref(&calendar)?;
    require_connected(&account_id)?;
    let entry = gapi::insert_calendar_list(&account_id, &id).await?;
    let row = model::calendar_from_json(&account_id, &entry)
        .ok_or_else(|| SkimError::other("gcal_api", "Google returned no calendar"))?;
    adopt_calendar(&app, &state, row).await
}

/// The list row for a calendar `calendars.insert` just made, for when its
/// `calendarList` entry cannot be read: the user owns it.
pub fn created_calendar_row(account_id: &str, created: &Value) -> Option<CalendarRow> {
    let mut row = model::calendar_from_json(account_id, created)?;
    row.access_role = "owner".into();
    row.is_primary = false;
    Some(row)
}

/// Create a secondary calendar owned by the account, then store its list
/// entry, selected and syncing.
#[tauri::command]
pub async fn fork_cal_create_calendar(
    app: AppHandle,
    state: State<'_, AppState>,
    account_id: String,
    summary: String,
    time_zone: String,
) -> Result<CalendarRow> {
    let (summary, zone) = check_new_calendar(&summary, &time_zone)?;
    require_connected(&account_id)?;
    let created =
        gapi::insert_calendar(&account_id, &gapi::calendar_insert_body(&summary, &zone)).await?;
    let id = created["id"]
        .as_str()
        .ok_or_else(|| SkimError::other("gcal_api", "Google returned no calendar id"))?;
    // The calendar exists now: a failed read of its list entry must not
    // report failure (a retry would make a second calendar). The next pull
    // fills in the colour.
    let row = match gapi::fetch_calendar_list_entry(&account_id, id).await {
        Ok(entry) => model::calendar_from_json(&account_id, &entry),
        Err(e) => {
            tracing::warn!(error = %e, "new calendar: list entry unreadable");
            None
        }
    }
    .or_else(|| created_calendar_row(&account_id, &created))
    .ok_or_else(|| SkimError::other("gcal_api", "Google returned no calendar"))?;
    adopt_calendar(&app, &state, row).await
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
    fn rsvp_payload_targets_the_master_only_for_a_series_answer() {
        let instance = EventRow {
            id: 42,
            google_id: "series1_20261001T080000Z".into(),
            recurring_event_id: Some("series1".into()),
            ..Default::default()
        };
        let p = rsvp_op_payload(&instance, "declined", SendUpdates::All, Some("Away"), true);
        assert_eq!(
            p,
            json!({ "event_id": 42, "response": "declined", "send_updates": "all",
                    "comment": "Away", "google_id": "series1", "series": true })
        );
        // Just this occurrence: no master id, so the engine patches the instance.
        let p = rsvp_op_payload(&instance, "accepted", SendUpdates::None, None, false);
        assert_eq!(
            p,
            json!({ "event_id": 42, "response": "accepted", "send_updates": "none" })
        );
        // A single event has no master: `series` changes nothing.
        let single = EventRow {
            id: 7,
            google_id: "single".into(),
            ..Default::default()
        };
        let p = rsvp_op_payload(&single, "tentative", SendUpdates::None, Some(""), true);
        assert_eq!(
            p,
            json!({ "event_id": 7, "response": "tentative", "send_updates": "none", "comment": "" })
        );
    }

    #[test]
    fn series_rows_find_the_instances_and_the_queued_op_keeps_the_master() {
        let db = crate::db::Db::open_in_memory().unwrap();
        db.with(|conn| {
            store::upsert_calendars(
                conn,
                "a1",
                &[CalendarRow {
                    id: 0,
                    account_id: "a1".into(),
                    google_id: "primary".into(),
                    summary: "primary".into(),
                    color: None,
                    is_primary: true,
                    selected: true,
                    access_role: "owner".into(),
                }],
            )?;
            let cid = store::list_calendars(conn, "a1")?[0].id;
            let inst = |gid: &str, start: i64| EventRow {
                calendar_id: cid,
                google_id: gid.into(),
                status: "confirmed".into(),
                start_ts: start,
                end_ts: start + 900,
                recurring_event_id: Some("s1".into()),
                attendees_json: Some(
                    r#"[{"email":"me@example.test","self":true,"responseStatus":"needsAction"}]"#
                        .into(),
                ),
                ..Default::default()
            };
            let other = EventRow {
                recurring_event_id: Some("s2".into()),
                ..inst("s2_1", 50)
            };
            store::replace_window(
                conn,
                cid,
                0,
                10_000,
                &[inst("s1_1", 100), inst("s1_2", 200), other],
            )?;
            let rows = store::series_rows(conn, cid, "s1")?;
            assert_eq!(rows.len(), 2, "only the s1 instances");
            let first = &rows[0];
            let payload = rsvp_op_payload(first, "accepted", SendUpdates::None, None, true);
            assert_eq!(payload["google_id"], "s1");
            let op = store::queue_op(conn, "a1", "rsvp", &payload)?;
            let stored: String = conn.query_row(
                "SELECT payload FROM fork_cal_ops WHERE id = ?1",
                [op],
                |r| r.get(0),
            )?;
            let stored: Value = serde_json::from_str(&stored).unwrap();
            assert_eq!(stored["_series_id"], "s1");
            assert_eq!(stored["google_id"], "s1");
            Ok(())
        })
        .unwrap();
    }

    fn calendar(id: i64, account: &str, primary: bool, role: &str) -> CalendarRow {
        CalendarRow {
            id,
            account_id: account.into(),
            google_id: format!("cal{id}@example.test"),
            summary: "Cal".into(),
            color: None,
            is_primary: primary,
            selected: true,
            access_role: role.into(),
        }
    }

    fn typed(event_type: &str) -> EventInput {
        let mut input = EventInput {
            summary: Some("Away".into()),
            start_ts: Some(3600),
            end_ts: Some(7200),
            ..Default::default()
        };
        input.options.event_type = Some(event_type.into());
        input
    }

    fn code_and_message(e: SkimError) -> (String, String) {
        (e.code().to_string(), e.to_string())
    }

    #[test]
    fn event_types_only_on_the_primary_calendar() {
        let primary = calendar(1, "a1", true, "owner");
        let team = calendar(2, "a1", false, "owner");
        for t in ["outOfOffice", "focusTime"] {
            assert!(
                apply_event_type_create(&mut typed(t), &primary).is_ok(),
                "{t}"
            );
            let (code, message) =
                code_and_message(apply_event_type_create(&mut typed(t), &team).unwrap_err());
            assert_eq!(code, "gcal_input");
            assert!(message.contains("event_type_primary_only"), "{message}");
        }
        // A plain event goes anywhere, named or not.
        assert!(apply_event_type_create(&mut typed("default"), &team).is_ok());
        assert!(apply_event_type_create(&mut EventInput::default(), &team).is_ok());
        let bad = apply_event_type_create(&mut typed("birthday"), &primary).unwrap_err();
        assert_eq!(bad.code(), "gcal_input");
        assert!(bad.to_string().contains("event_type_invalid"));
    }

    #[test]
    fn out_of_office_and_focus_time_are_timed_and_have_no_guests() {
        let primary = calendar(1, "a1", true, "owner");
        for t in ["outOfOffice", "focusTime"] {
            let mut guests = typed(t);
            guests.attendees = Some(vec!["guest@example.test".into()]);
            let e = apply_event_type_create(&mut guests, &primary).unwrap_err();
            assert!(e.to_string().contains("event_type_no_guests"));
            assert_eq!(e.code(), "gcal_input");
            let mut optional = typed(t);
            optional.optional_attendees = Some(vec!["guest@example.test".into()]);
            assert!(apply_event_type_create(&mut optional, &primary).is_err());
            // An empty guest list is no guests.
            let mut empty = typed(t);
            empty.attendees = Some(vec![" ".into()]);
            assert!(apply_event_type_create(&mut empty, &primary).is_ok());
            let mut all_day = typed(t);
            all_day.start_ts = None;
            all_day.start_date = Some("2026-11-10".into());
            let e = apply_event_type_create(&mut all_day, &primary).unwrap_err();
            assert!(e.to_string().contains("event_type_timed_only"));
            let mut flagged = typed(t);
            flagged.all_day = Some(true);
            assert!(apply_event_type_create(&mut flagged, &primary).is_err());
        }
    }

    #[test]
    fn working_location_is_free_public_and_names_its_place() {
        let primary = calendar(1, "a1", true, "owner");
        let mut input = typed("workingLocation");
        let e = apply_event_type_create(&mut input, &primary).unwrap_err();
        assert!(e.to_string().contains("event_type_properties"));
        input.options.working_location_properties =
            Some(json!({"type": "homeOffice", "homeOffice": {}}));
        input.options.transparency = Some("opaque".into());
        input.options.visibility = Some("private".into());
        input.start_ts = None;
        input.start_date = Some("2026-11-10".into());
        apply_event_type_create(&mut input, &primary).unwrap();
        assert_eq!(input.options.transparency.as_deref(), Some("transparent"));
        assert_eq!(input.options.visibility.as_deref(), Some("public"));
        let body = gapi::event_body(&input, None);
        assert_eq!(body["eventType"], "workingLocation");
        assert_eq!(body["workingLocationProperties"]["type"], "homeOffice");
        assert_eq!(body["transparency"], "transparent");
        assert_eq!(body["visibility"], "public");
        let row = local_row_from_input(1, &input).unwrap();
        assert_eq!(row.event_type.as_deref(), Some("workingLocation"));
        assert_eq!(row.options.event_type.as_deref(), Some("workingLocation"));
        // Properties of another type are refused.
        let mut stray = typed("focusTime");
        stray.options.out_of_office_properties = Some(json!({"autoDeclineMode": "declineNone"}));
        let e = apply_event_type_create(&mut stray, &primary).unwrap_err();
        assert!(e.to_string().contains("event_type_properties"));
        let mut plain = EventInput::default();
        plain.options.focus_time_properties = Some(json!({}));
        assert!(apply_event_type_create(&mut plain, &primary).is_err());
    }

    #[test]
    fn a_patch_cannot_change_the_event_type() {
        let ooo = EventRow {
            event_type: Some("outOfOffice".into()),
            start_ts: 3600,
            end_ts: 7200,
            ..Default::default()
        };
        let mut change = EventInput::default();
        change.options.event_type = Some("default".into());
        let e = apply_event_type_patch(&mut change, &ooo).unwrap_err();
        assert_eq!(e.code(), "gcal_input");
        assert!(e.to_string().contains("event_type_fixed"));
        // The same type is dropped from the body, not sent.
        let mut same = EventInput {
            summary: Some("Leave".into()),
            ..Default::default()
        };
        same.options.event_type = Some("outOfOffice".into());
        same.options.out_of_office_properties = Some(json!({"declineMessage": "Away"}));
        apply_event_type_patch(&mut same, &ooo).unwrap();
        let body = gapi::event_body(&same, None);
        assert!(body.get("eventType").is_none());
        assert_eq!(body["outOfOfficeProperties"]["declineMessage"], "Away");
        // The stored type's shape still holds.
        let mut guests = EventInput {
            attendees: Some(vec!["guest@example.test".into()]),
            ..Default::default()
        };
        assert!(apply_event_type_patch(&mut guests, &ooo).is_err());
        let mut all_day = EventInput {
            all_day: Some(true),
            ..Default::default()
        };
        assert!(apply_event_type_patch(&mut all_day, &ooo).is_err());
        // A default event (no stored type) cannot become one either.
        let mut become_focus = EventInput::default();
        become_focus.options.event_type = Some("focusTime".into());
        assert!(apply_event_type_patch(&mut become_focus, &EventRow::default()).is_err());
        let mut plain = EventInput {
            attendees: Some(vec!["guest@example.test".into()]),
            ..Default::default()
        };
        assert!(apply_event_type_patch(&mut plain, &EventRow::default()).is_ok());
        // A working location keeps free and public when a patch names either.
        let wl = EventRow {
            event_type: Some("workingLocation".into()),
            ..Default::default()
        };
        let mut busy = EventInput::default();
        busy.options.transparency = Some("opaque".into());
        apply_event_type_patch(&mut busy, &wl).unwrap();
        assert_eq!(busy.options.transparency.as_deref(), Some("transparent"));
        let mut title = EventInput {
            summary: Some("Office".into()),
            ..Default::default()
        };
        apply_event_type_patch(&mut title, &wl).unwrap();
        assert_eq!(gapi::event_body(&title, None), json!({"summary": "Office"}));
    }

    #[test]
    fn an_out_of_office_event_reads_back_its_type_from_google_and_the_store() {
        let db = crate::db::Db::open_in_memory().unwrap();
        db.with(|conn| {
            store::upsert_calendars(conn, "a1", &[calendar(0, "a1", true, "owner")])?;
            let cid = store::list_calendars(conn, "a1")?[0].id;
            let google = json!({
                "id": "ooo1", "eventType": "outOfOffice", "summary": "Away",
                "start": {"dateTime": "2026-11-10T08:00:00Z"},
                "end": {"dateTime": "2026-11-10T17:00:00Z"},
                "outOfOfficeProperties": {"autoDeclineMode": "declineAllConflictingInvitations",
                                          "declineMessage": "Away"}
            });
            let row = model::event_from_json(cid, &google).unwrap();
            assert_eq!(row.options.event_type.as_deref(), Some("outOfOffice"));
            store::replace_window(conn, cid, 0, i64::MAX / 2, &[row])?;
            let stored = store::event_by_google_id(conn, cid, "ooo1")?.unwrap();
            let json = serde_json::to_value(&stored.options).unwrap();
            assert_eq!(json["eventType"], "outOfOffice");
            assert_eq!(json["outOfOfficeProperties"]["declineMessage"], "Away");
            assert_eq!(stored.event_type.as_deref(), Some("outOfOffice"));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn a_patch_keeps_a_stored_end_zone_unless_it_names_one() {
        let mut row = EventRow {
            start_ts: 3600,
            end_ts: 7200,
            time_zone: Some("Africa/Johannesburg".into()),
            end_time_zone: Some("Europe/London".into()),
            ..Default::default()
        };
        // Only the start moves: no end in the body, so Google keeps its zone.
        let start_only = EventInput {
            start_ts: Some(1800),
            time_zone: Some("Africa/Johannesburg".into()),
            ..Default::default()
        };
        let body = gapi::event_body_for(&start_only, None, row.end_time_zone.as_deref());
        assert_eq!(body["start"]["timeZone"], "Africa/Johannesburg");
        assert!(body.get("end").is_none());
        apply_input(&mut row, &start_only).unwrap();
        assert_eq!(row.end_time_zone.as_deref(), Some("Europe/London"));
        // A drag sends both ends: the end keeps the stored zone.
        let drag = EventInput {
            start_ts: Some(900),
            end_ts: Some(4500),
            time_zone: Some("Africa/Johannesburg".into()),
            ..Default::default()
        };
        let body = gapi::event_body_for(&drag, None, row.end_time_zone.as_deref());
        assert_eq!(body["end"]["timeZone"], "Europe/London");
        assert_eq!(body["start"]["timeZone"], "Africa/Johannesburg");
        // Naming an end zone wins and is stored.
        let named = EventInput {
            end_ts: Some(5400),
            end_time_zone: Some("Asia/Tokyo".into()),
            ..drag.clone()
        };
        let body = gapi::event_body_for(&named, None, row.end_time_zone.as_deref());
        assert_eq!(body["end"]["timeZone"], "Asia/Tokyo");
        apply_input(&mut row, &named).unwrap();
        assert_eq!(row.end_time_zone.as_deref(), Some("Asia/Tokyo"));
        // A new event keeps its own end zone on the local row.
        let created = local_row_from_input(1, &named).unwrap();
        assert_eq!(created.end_time_zone.as_deref(), Some("Asia/Tokyo"));
        let same = local_row_from_input(1, &drag).unwrap();
        assert_eq!(same.end_time_zone, None);
    }

    #[test]
    fn move_rules() {
        let row = EventRow {
            id: 5,
            google_id: "ev1".into(),
            ..Default::default()
        };
        let from = calendar(1, "a1", true, "owner");
        let to = calendar(2, "a1", false, "writer");
        assert!(check_move(&row, &from, &to).is_ok());
        let msg = |r: Result<()>| {
            let e = r.unwrap_err();
            assert_eq!(e.code(), "gcal_input");
            e.to_string()
        };
        let instance = EventRow {
            recurring_event_id: Some("series".into()),
            ..row.clone()
        };
        assert!(msg(check_move(&instance, &from, &to)).contains("move_instance"));
        for t in ["outOfOffice", "focusTime", "workingLocation", "birthday"] {
            let typed = EventRow {
                event_type: Some(t.into()),
                ..row.clone()
            };
            assert!(
                msg(check_move(&typed, &from, &to)).contains("move_event_type"),
                "{t}"
            );
        }
        let plain = EventRow {
            event_type: Some("default".into()),
            ..row.clone()
        };
        assert!(check_move(&plain, &from, &to).is_ok());
        let other = calendar(3, "a2", false, "owner");
        assert!(msg(check_move(&row, &from, &other)).contains("move_other_account"));
        let reader = calendar(4, "a1", false, "reader");
        assert!(msg(check_move(&row, &from, &reader)).contains("move_read_only"));
        let free_busy = calendar(4, "a1", false, "freeBusyReader");
        assert!(msg(check_move(&row, &from, &free_busy)).contains("move_read_only"));
        assert!(msg(check_move(&row, &reader, &to)).contains("move_read_only"));
        assert!(msg(check_move(&row, &from, &from)).contains("move_same_calendar"));
        let local = EventRow {
            google_id: "local-1".into(),
            local_only: true,
            ..row.clone()
        };
        assert!(msg(check_move(&local, &from, &to)).contains("move_not_synced"));
        let p = move_op_payload(&row, &from, &to, SendUpdates::None);
        assert_eq!(
            p,
            json!({ "event_id": 5, "calendar_google_id": "cal1@example.test", "google_id": "ev1",
                    "to_calendar_id": 2, "to_calendar_google_id": "cal2@example.test",
                    "send_updates": "none" })
        );
        assert_eq!(
            super::super::MoveTarget::from_payload(&p)
                .unwrap()
                .to_calendar_id,
            2
        );
    }

    #[test]
    fn a_pending_move_holds_the_source_copy_against_a_pull() {
        let db = crate::db::Db::open_in_memory().unwrap();
        db.with(|conn| {
            store::upsert_calendars(
                conn,
                "a1",
                &[calendar(0, "a1", true, "owner"), {
                    let mut t = calendar(0, "a1", false, "owner");
                    t.google_id = "team@example.test".into();
                    t
                }],
            )?;
            let cals = store::list_calendars(conn, "a1")?;
            let (from, to) = (cals[0].clone(), cals[1].clone());
            let ev = |cid: i64| EventRow {
                calendar_id: cid,
                google_id: "ev1".into(),
                status: "confirmed".into(),
                start_ts: 100,
                end_ts: 200,
                ..Default::default()
            };
            store::replace_window(conn, from.id, 0, 1000, &[ev(from.id)])?;
            let row = store::event_by_google_id(conn, from.id, "ev1")?.unwrap();
            check_move(&row, &from, &to).unwrap();
            store::queue_op(
                conn,
                "a1",
                "move",
                &move_op_payload(&row, &from, &to, SendUpdates::None),
            )?;
            store::set_event_calendar(conn, row.id, to.id)?;
            // Google still lists it in the source; the destination not yet.
            store::replace_window(conn, from.id, 0, 1000, &[ev(from.id)])?;
            store::replace_window(conn, to.id, 0, 1000, &[])?;
            let all: Vec<_> = store::events_between(conn, Some("a1"), 0, 1000)?;
            assert_eq!(all.len(), 1, "no copy comes back in the source");
            assert_eq!(all[0].id, row.id);
            assert_eq!(all[0].calendar_id, to.id);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn calendar_inputs_are_checked() {
        assert_eq!(
            normalize_calendar_ref("  kim@example.test ").unwrap(),
            "kim@example.test"
        );
        for bad in ["", "   ", "two words@example.test"] {
            let e = normalize_calendar_ref(bad).unwrap_err();
            assert_eq!(e.code(), "gcal_input");
        }
        assert_eq!(
            check_new_calendar(" Projects ", "Africa/Johannesburg").unwrap(),
            ("Projects".into(), "Africa/Johannesburg".into())
        );
        assert!(check_new_calendar("", "Africa/Johannesburg").is_err());
        assert!(check_new_calendar(&"x".repeat(256), "UTC").is_err());
        let e = check_new_calendar("Projects", "Mars/Olympus").unwrap_err();
        assert!(e.to_string().contains("time_zone_invalid"));
        let made = json!({"id": "abc@group.calendar.google.com", "summary": "Projects",
                          "timeZone": "UTC"});
        let row = created_calendar_row("a1", &made).unwrap();
        assert_eq!(row.google_id, "abc@group.calendar.google.com");
        assert_eq!(row.access_role, "owner");
        assert!(!row.is_primary);
        assert!(created_calendar_row("a1", &json!({})).is_none());
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
