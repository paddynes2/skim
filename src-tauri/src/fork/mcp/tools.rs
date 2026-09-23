//! The MCP tools. Every one goes through the code the UI already runs: the
//! Phase 4 query parser and thread search, `bodies::get_thread`, the fork
//! court and calendar stores, `commands::mail::queue_op_local` for
//! archive/star/read, the drafts table plus the `save_draft` op for drafts,
//! and the calendar op queue for events. Nothing here sends.

use super::Server;
use crate::commands::mail::queue_op_local;
use crate::db::models::{Address, ThreadRow};
use crate::db::{accounts, bodies, drafts, queries};
use crate::error::SkimError;
use crate::fork::availability;
use crate::fork::calendar::{self, gapi, model, store};
use crate::fork::court;
use crate::fork::search_query;
use crate::mail::parse::{html_to_text, strip_quoted};
use crate::mail::smtp::signature_block;
use chrono::{Local, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use rusqlite::OptionalExtension;
use serde_json::{json, Value};

/// Longest body a `get_thread` reply carries per message.
const MAX_BODY_CHARS: usize = 20_000;
/// Longest window `get_calendar` reads.
const MAX_CALENDAR_SPAN: i64 = 366 * 86_400;

#[derive(Debug)]
pub enum ToolError {
    /// No such tool: a JSON-RPC error, not a tool result.
    Unknown,
    /// The caller's arguments are wrong: a tool result with `isError`.
    Input(String),
    /// The app could not do it: a tool result with `isError`.
    Failed(String),
}

impl From<SkimError> for ToolError {
    fn from(e: SkimError) -> Self {
        Self::Failed(e.to_string())
    }
}

impl From<rusqlite::Error> for ToolError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Failed(format!("database error: {e}"))
    }
}

type ToolResult = std::result::Result<Value, ToolError>;

// ---- definitions ---------------------------------------------------------------

fn tool(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": false,
        },
    })
}

/// `tools/list`. Hand-written schemas: twelve small objects, no schemars.
pub fn definitions() -> Vec<Value> {
    vec![
        tool(
            "search_mail",
            "Search the local mail cache. Supports Skim's operators: from:, to:, cc:, \
             subject:, is:unread|read|starred|unstarred, has:attachment, in:inbox|sent|drafts|\
             trash|spam|starred|<label>, before:/after:YYYY-MM-DD, older_than:/newer_than:Nd|Nw|\
             Nm|Ny, quoted values, -word negation. Returns one row per thread, newest first.",
            json!({
                "query": { "type": "string", "description": "Free text and/or operators." },
                "limit": { "type": "integer", "minimum": 1, "maximum": 100, "default": 20 },
            }),
            &["query"],
        ),
        tool(
            "get_thread",
            "Every message of a thread, oldest first, with plain-text bodies (quoted replies \
             and signatures stripped unless strip_quotes is false). A body that is not \
             downloaded yet comes back null.",
            json!({
                "thread_id": { "type": "integer" },
                "strip_quotes": { "type": "boolean", "default": true },
            }),
            &["thread_id"],
        ),
        tool(
            "list_unread",
            "Threads with unread messages, newest first. Defaults to the Inbox; `folder` \
             names a role (inbox, sent, starred, important, ...) or a folder by name.",
            json!({
                "folder": { "type": "string" },
                "limit": { "type": "integer", "minimum": 1, "maximum": 200, "default": 20 },
            }),
            &[],
        ),
        tool(
            "list_court",
            "Ball-in-court threads: `on_me` (someone else spoke last and it looks like it needs \
             a reply) or `waiting` (the user spoke last). Oldest first, with the age in days.",
            json!({
                "state": { "type": "string", "enum": ["on_me", "waiting"], "default": "on_me" },
                "limit": { "type": "integer", "minimum": 1, "maximum": 200, "default": 50 },
            }),
            &[],
        ),
        tool(
            "get_calendar",
            "Events on the selected calendars between two instants. Dates are RFC 3339 \
             (2026-09-24T09:00:00+02:00), YYYY-MM-DD (whole day, UTC), or a local \
             YYYY-MM-DDTHH:MM. Cancelled events are left out.",
            json!({
                "from": { "type": "string" },
                "to": { "type": "string" },
            }),
            &["from", "to"],
        ),
        tool(
            "find_free_slots",
            "Free slots on the user's working hours (Settings → Calendar), on a 30-minute grid, \
             at most 3 per working day, never in the past. `tz` is the IANA zone to print them \
             in; defaults to the second time zone from Settings, else UTC.",
            json!({
                "duration": { "type": "integer", "description": "Minutes.", "default": 30 },
                "days": { "type": "integer", "description": "Working days to scan.", "default": 5 },
                "tz": { "type": "string", "description": "IANA zone, e.g. Europe/London." },
            }),
            &[],
        ),
        tool(
            "crm_lookup",
            "Look an email address up in the user's CRM (Rebound): person, company, open deals, \
             recent activity. Read-only.",
            json!({ "email": { "type": "string" } }),
            &["email"],
        ),
        tool(
            "create_draft",
            "Write a draft and save it to the mailbox's Drafts folder for the user to review. \
             This never sends. With reply_to_message_id the draft is a reply on that thread \
             (subject and recipient default from the original when left empty). The account's \
             signature is appended.",
            json!({
                "to": { "type": "string", "description": "Comma-separated addresses." },
                "cc": { "type": "string" },
                "subject": { "type": "string" },
                "body": { "type": "string", "description": "Plain text." },
                "reply_to_message_id": { "type": "integer", "description": "A message id from get_thread." },
            }),
            &["to", "subject", "body"],
        ),
        tool(
            "archive",
            "Archive every message of a thread (queued to the server like the E key; the UI's \
             undo window does not apply).",
            json!({ "thread_id": { "type": "integer" } }),
            &["thread_id"],
        ),
        tool(
            "star",
            "Star or unstar every message of a thread.",
            json!({
                "thread_id": { "type": "integer" },
                "on": { "type": "boolean", "default": true },
            }),
            &["thread_id"],
        ),
        tool(
            "mark_read",
            "Mark every message of a thread read or unread.",
            json!({
                "thread_id": { "type": "integer" },
                "on": { "type": "boolean", "default": true },
            }),
            &["thread_id"],
        ),
        tool(
            "create_event",
            "Create an event on the user's own primary calendar, with no attendees (an attendee \
             would send an invite, which this server never does). Times are RFC 3339 with an \
             offset, or local YYYY-MM-DDTHH:MM.",
            json!({
                "title": { "type": "string" },
                "start": { "type": "string" },
                "end": { "type": "string" },
                "description": { "type": "string" },
            }),
            &["title", "start", "end"],
        ),
    ]
}

/// `tools/call`.
pub async fn call(server: &Server, name: &str, args: &Value) -> ToolResult {
    let a = Args(args);
    match name {
        "search_mail" => search_mail(server, &a).await,
        "get_thread" => get_thread(server, &a).await,
        "list_unread" => list_unread(server, &a).await,
        "list_court" => list_court(server, &a).await,
        "get_calendar" => get_calendar(server, &a).await,
        "find_free_slots" => find_free_slots(server, &a).await,
        "crm_lookup" => crm_lookup(server, &a).await,
        "create_draft" => create_draft(server, &a).await,
        "archive" => thread_op(server, &a, "archive").await,
        "star" => thread_op(server, &a, "star").await,
        "mark_read" => thread_op(server, &a, "mark_read").await,
        "create_event" => create_event(server, &a).await,
        _ => Err(ToolError::Unknown),
    }
}

// ---- argument helpers ----------------------------------------------------------

struct Args<'a>(&'a Value);

impl Args<'_> {
    fn opt_str(&self, key: &str) -> Option<String> {
        self.0
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    }

    fn str(&self, key: &str) -> std::result::Result<String, ToolError> {
        self.opt_str(key)
            .ok_or_else(|| ToolError::Input(format!("`{key}` is required")))
    }

    fn int(&self, key: &str, default: i64) -> std::result::Result<i64, ToolError> {
        match self.0.get(key) {
            None | Some(Value::Null) => Ok(default),
            Some(v) => v
                .as_i64()
                .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
                .ok_or_else(|| ToolError::Input(format!("`{key}` must be an integer"))),
        }
    }

    fn opt_int(&self, key: &str) -> std::result::Result<Option<i64>, ToolError> {
        match self.0.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => v
                .as_i64()
                .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
                .map(Some)
                .ok_or_else(|| ToolError::Input(format!("`{key}` must be an integer"))),
        }
    }

    fn bool(&self, key: &str, default: bool) -> std::result::Result<bool, ToolError> {
        match self.0.get(key) {
            None | Some(Value::Null) => Ok(default),
            Some(Value::Bool(b)) => Ok(*b),
            Some(Value::String(s)) => match s.trim().to_ascii_lowercase().as_str() {
                "true" | "on" | "yes" | "1" => Ok(true),
                "false" | "off" | "no" | "0" => Ok(false),
                _ => Err(ToolError::Input(format!("`{key}` must be a boolean"))),
            },
            Some(_) => Err(ToolError::Input(format!("`{key}` must be a boolean"))),
        }
    }
}

/// RFC 3339 with offset, `YYYY-MM-DD` (midnight UTC), or a naive
/// `YYYY-MM-DDTHH:MM[:SS]` / `YYYY-MM-DD HH:MM[:SS]` taken as local time.
pub fn parse_when(s: &str) -> Option<i64> {
    let s = s.trim();
    if let Some(ts) = model::parse_rfc3339(s) {
        return Some(ts);
    }
    if let Some(ts) = model::date_to_utc_midnight(s) {
        return Some(ts);
    }
    for fmt in [
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
    ] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(s, fmt) {
            return Local
                .from_local_datetime(&naive)
                .earliest()
                .map(|d| d.timestamp());
        }
    }
    None
}

fn when_arg(a: &Args, key: &str) -> std::result::Result<i64, ToolError> {
    let s = a.str(key)?;
    parse_when(&s).ok_or_else(|| {
        ToolError::Input(format!(
            "`{key}` is not a date: use RFC 3339 (2026-09-24T09:00:00+02:00), \
             YYYY-MM-DD, or YYYY-MM-DDTHH:MM"
        ))
    })
}

fn rfc3339(ts: i64) -> String {
    model::to_rfc3339(ts)
}

fn address(a: &Address) -> Value {
    json!({ "name": a.name, "email": a.addr })
}

fn thread_row(r: &ThreadRow) -> Value {
    json!({
        "thread_id": r.id,
        "account_id": r.account_id,
        "subject": r.subject,
        "from": { "name": r.from_name, "email": r.from_addr },
        "date": rfc3339(r.date),
        "snippet": r.snippet,
        "unread": !r.is_read,
        "starred": r.is_starred,
        "has_attachments": r.has_attachments,
        "message_count": r.message_count,
    })
}

// ---- read tools -----------------------------------------------------------------

async fn search_mail(server: &Server, a: &Args<'_>) -> ToolResult {
    let query = a.str("query")?;
    let limit = a.int("limit", 20)?.clamp(1, 100);
    let parsed = search_query::parse(&query);
    let rows = server
        .db
        .read("mcp_search_mail", move |conn| {
            search_query::search_threads(conn, &parsed, 0, limit, None)
        })
        .await?;
    Ok(json!({
        "query": query,
        "count": rows.len(),
        "threads": rows.iter().map(thread_row).collect::<Vec<_>>(),
    }))
}

/// The readable text of a stored body: the text part, else the HTML flattened;
/// quotes and signature cut when asked (falling back to the whole text when the
/// cut leaves nothing, as a bottom-posted reply does).
fn body_text(html: Option<&str>, text: Option<&str>, strip: bool) -> Option<String> {
    let full = match (text.filter(|t| !t.trim().is_empty()), html) {
        (Some(t), _) => t.to_string(),
        (None, Some(h)) if !h.trim().is_empty() => html_to_text(h),
        _ => return None,
    };
    let mut out = if strip {
        let cut = strip_quoted(&full);
        if cut.trim().is_empty() {
            full
        } else {
            cut
        }
    } else {
        full
    };
    if out.chars().count() > MAX_BODY_CHARS {
        out = out.chars().take(MAX_BODY_CHARS).collect::<String>() + "\n[truncated]";
    }
    Some(out)
}

async fn get_thread(server: &Server, a: &Args<'_>) -> ToolResult {
    let thread_id = a.int("thread_id", 0)?;
    let strip = a.bool("strip_quotes", true)?;
    let detail = server
        .db
        .read("mcp_get_thread", move |conn| {
            let Some(detail) = bodies::get_thread(conn, thread_id)? else {
                return Ok(None);
            };
            let mut messages = Vec::new();
            for m in &detail.messages {
                let body = bodies::get_body(conn, m.id)?
                    .and_then(|(html, text)| body_text(html.as_deref(), text.as_deref(), strip));
                messages.push(json!({
                    "message_id": m.id,
                    "from": address(&m.from),
                    "to": m.to.iter().map(address).collect::<Vec<_>>(),
                    "cc": m.cc.iter().map(address).collect::<Vec<_>>(),
                    "date": rfc3339(m.date),
                    "subject": m.subject,
                    "unread": !m.is_read,
                    "starred": m.is_starred,
                    "has_attachments": m.has_attachments,
                    "body": body,
                }));
            }
            Ok(Some(json!({
                "thread_id": detail.id,
                "subject": detail.subject,
                "messages": messages,
            })))
        })
        .await?;
    detail.ok_or_else(|| ToolError::Input(format!("thread {thread_id} not found")))
}

async fn list_unread(server: &Server, a: &Args<'_>) -> ToolResult {
    let folder = a.opt_str("folder").map(|f| f.to_lowercase());
    let limit = a.int("limit", 20)?.clamp(1, 200);
    let rows = server
        .db
        .read("mcp_list_unread", move |conn| {
            let mut stmt = conn.prepare_cached(
                "SELECT m.thread_id, m.account_id, m.from_name, m.from_addr, m.subject, m.snippet,
                        max(m.date), f.display_name,
                        (SELECT count(*) FROM messages m2 WHERE m2.thread_id = m.thread_id)
                 FROM messages m JOIN folders f ON f.id = m.folder_id
                 WHERE m.is_read = 0 AND m.thread_id IS NOT NULL
                   AND ((?1 IS NULL AND f.role = 'inbox')
                        OR (?1 IS NOT NULL AND (f.role = ?1 OR lower(f.display_name) = ?1
                                                OR lower(f.imap_name) = ?1)))
                 GROUP BY m.thread_id
                 ORDER BY max(m.date) DESC
                 LIMIT ?2",
            )?;
            let rows = stmt
                .query_map(rusqlite::params![folder, limit], |r| {
                    let from_name: Option<String> = r.get(2)?;
                    let from_addr: Option<String> = r.get(3)?;
                    Ok(json!({
                        "thread_id": r.get::<_, i64>(0)?,
                        "account_id": r.get::<_, String>(1)?,
                        "from": {
                            "name": from_name.filter(|s| !s.is_empty())
                                .or_else(|| from_addr.clone()).unwrap_or_default(),
                            "email": from_addr.unwrap_or_default(),
                        },
                        "subject": r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                        "snippet": r.get::<_, Option<String>>(5)?.unwrap_or_default(),
                        "date": rfc3339(r.get::<_, i64>(6)?),
                        "folder": r.get::<_, Option<String>>(7)?.unwrap_or_default(),
                        "message_count": r.get::<_, i64>(8)?,
                    }))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await?;
    Ok(json!({ "count": rows.len(), "threads": rows }))
}

async fn list_court(server: &Server, a: &Args<'_>) -> ToolResult {
    let state = a
        .opt_str("state")
        .unwrap_or_else(|| court::STATE_ON_ME.to_string());
    if state != court::STATE_ON_ME && state != court::STATE_WAITING {
        return Err(ToolError::Input("`state` must be on_me or waiting".into()));
    }
    let limit = a.int("limit", 50)?.clamp(1, 200);
    let now = Utc::now().timestamp();
    let rows = server
        .db
        .read("mcp_list_court", move |conn| {
            court::list(conn, &state, 0, limit)
        })
        .await?;
    Ok(json!({
        "count": rows.len(),
        "threads": rows.iter().map(|r| {
            let mut v = thread_row(&r.row);
            v["since"] = json!(rfc3339(r.since));
            v["age_days"] = json!(court::age_days(r.since, now));
            v["reason"] = json!(r.reason);
            v
        }).collect::<Vec<_>>(),
    }))
}

fn event_json(e: &model::EventRow) -> Value {
    let attendees: Vec<Value> = e
        .attendees_json
        .as_deref()
        .and_then(|j| serde_json::from_str::<Value>(j).ok())
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .map(|a| json!({ "email": a["email"], "response": a["responseStatus"] }))
        .collect();
    json!({
        "event_id": e.id,
        "calendar_id": e.calendar_id,
        "title": e.summary,
        "start": if e.all_day { json!(e.start_date) } else { json!(rfc3339(e.start_ts)) },
        "end": if e.all_day { json!(e.end_date) } else { json!(rfc3339(e.end_ts)) },
        "all_day": e.all_day,
        "location": e.location,
        "description": e.description,
        "organizer": e.organizer_email,
        "attendees": attendees,
        "my_response": e.self_response,
        "meet_link": e.hangout_link,
        "link": e.html_link,
        "status": e.status,
        "pending_sync": e.local_only,
    })
}

async fn get_calendar(server: &Server, a: &Args<'_>) -> ToolResult {
    let from = when_arg(a, "from")?;
    let mut to = when_arg(a, "to")?;
    // A bare date as the upper bound means the whole of that day.
    if a.opt_str("to")
        .as_deref()
        .and_then(model::date_to_utc_midnight)
        == Some(to)
    {
        to += 86_400;
    }
    if to <= from {
        return Err(ToolError::Input("`to` must be after `from`".into()));
    }
    if to - from > MAX_CALENDAR_SPAN {
        return Err(ToolError::Input("window longer than a year".into()));
    }
    let events = server
        .db
        .read("mcp_get_calendar", move |conn| {
            store::events_between(conn, None, from, to)
        })
        .await?;
    let events: Vec<Value> = events
        .iter()
        .filter(|e| e.status != "cancelled")
        .map(event_json)
        .collect();
    Ok(json!({
        "from": rfc3339(from),
        "to": rfc3339(to),
        "count": events.len(),
        "events": events,
    }))
}

async fn find_free_slots(server: &Server, a: &Args<'_>) -> ToolResult {
    let duration = a.int("duration", 30)?.clamp(5, 480) as u32;
    let days = a.int("days", 5)?.clamp(1, 30) as u32;
    let tz_arg = a.opt_str("tz");
    let now = Utc::now().timestamp();
    // Same window as `fork_free_slots`: the working days can spread over more
    // than twice as many calendar days, plus long all-day events.
    let (from, to) = (
        now - 7 * 86_400,
        now + i64::from(days) * 3 * 86_400 + 7 * 86_400,
    );
    let (events, work, tz_setting) = server
        .db
        .read("mcp_find_free_slots", move |conn| {
            let events = store::events_between(conn, None, from, to)?;
            let work =
                availability::working_hours_from(|k| queries::get_setting(conn, k).ok().flatten());
            let tz = queries::get_setting(conn, "fork_cal_second_tz")?;
            Ok((events, work, tz))
        })
        .await?;
    let tz_name = tz_arg
        .or(tz_setting.filter(|s| !s.trim().is_empty()))
        .unwrap_or_else(|| "UTC".to_string());
    let zone: Tz = tz_name
        .trim()
        .parse()
        .map_err(|_| ToolError::Input(format!("unknown time zone {tz_name:?}")))?;
    let busy = availability::busy_from_events(&events, zone);
    let slots = availability::free_slots(&busy, now, days, duration, &work, zone);
    Ok(json!({
        "tz": zone.name(),
        "duration_minutes": duration,
        "count": slots.len(),
        "slots": slots.iter().map(|s| json!({
            "start": rfc3339(s.start),
            "end": rfc3339(s.end),
            "day": s.day,
            "label": s.label,
            "time": s.time,
        })).collect::<Vec<_>>(),
    }))
}

async fn crm_lookup(server: &Server, a: &Args<'_>) -> ToolResult {
    let email = a.str("email")?;
    // Phase 9's lookup through the app hook (config, session and its
    // ten-minute cache live there). Without an app (tests) there is no
    // session, so the honest answer is "not connected".
    let Some(hooks) = server.hooks() else {
        return Err(ToolError::Failed(format!(
            "CRM lookup for {email} is not connected in this build"
        )));
    };
    let found = hooks.crm_lookup(email.clone()).await?;
    Ok(json!({ "email": email, "result": found }))
}

// ---- write tools (queued, never sent) ---------------------------------------------

async fn thread_op(server: &Server, a: &Args<'_>, which: &'static str) -> ToolResult {
    let thread_id = a.int("thread_id", 0)?;
    let on = a.bool("on", true)?;
    let (kind, extra): (&'static str, Value) = match which {
        "archive" => ("archive", json!({})),
        "star" => ("set_flag", json!({ "flag": "flagged", "on": on })),
        _ => ("set_flag", json!({ "flag": "seen", "on": on })),
    };
    let (count, accounts) = server
        .db
        .call(move |conn| {
            let ids: Vec<i64> = conn
                .prepare_cached("SELECT id FROM messages WHERE thread_id = ?1")?
                .query_map([thread_id], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            if ids.is_empty() {
                return Ok((0usize, Vec::new()));
            }
            let accounts = match which {
                "archive" => {
                    queue_op_local(conn, &ids, kind, &extra, bodies::remove_messages_local)?
                }
                "star" => queue_op_local(conn, &ids, kind, &extra, move |c, i| {
                    bodies::set_flag_local(c, i, "flagged", on)
                })?,
                _ => queue_op_local(conn, &ids, kind, &extra, move |c, i| {
                    bodies::set_flag_local(c, i, "seen", on)
                })?,
            };
            Ok((ids.len(), accounts))
        })
        .await?;
    if count == 0 {
        return Err(ToolError::Input(format!("thread {thread_id} not found")));
    }
    if let Some(h) = server.hooks() {
        h.mail_changed(accounts).await;
    }
    Ok(json!({
        "thread_id": thread_id,
        "action": which,
        "on": on,
        "messages": count,
        "queued": true,
    }))
}

async fn create_draft(server: &Server, a: &Args<'_>) -> ToolResult {
    let to_arg = a.opt_str("to").unwrap_or_default();
    let cc = a.opt_str("cc").unwrap_or_default();
    let subject_arg = a.opt_str("subject").unwrap_or_default();
    let body = a.str("body")?;
    let reply_to = a.opt_int("reply_to_message_id")?;
    if reply_to.is_none() && to_arg.is_empty() {
        return Err(ToolError::Input(
            "`to` is required for a new message".into(),
        ));
    }
    if reply_to.is_none() && subject_arg.is_empty() {
        return Err(ToolError::Input(
            "`subject` is required for a new message".into(),
        ));
    }
    let candidate = format!("skim-{}@skim.local", uuid::Uuid::new_v4());
    let draft = server
        .db
        .call(move |conn| {
            // Which mailbox, what mode, what the empty fields default to.
            let (account_id, mode, to, subject) = match reply_to {
                Some(mid) => {
                    let row: Option<(String, Option<String>, Option<String>)> = conn
                        .query_row(
                            "SELECT account_id, subject, from_addr FROM messages WHERE id = ?1",
                            [mid],
                            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                        )
                        .optional()?;
                    let Some((account_id, orig_subject, from_addr)) = row else {
                        return Ok(None);
                    };
                    let subject = if subject_arg.is_empty() {
                        let base = orig_subject.unwrap_or_default();
                        if base.to_lowercase().starts_with("re:") {
                            base
                        } else {
                            format!("Re: {base}")
                        }
                    } else {
                        subject_arg
                    };
                    let to = if to_arg.is_empty() {
                        from_addr.unwrap_or_default()
                    } else {
                        to_arg
                    };
                    (account_id, "reply", to, subject)
                }
                None => {
                    let Some(account) = accounts::list(conn)?.into_iter().next() else {
                        return Ok(None);
                    };
                    (account.id, "new", to_arg, subject_arg)
                }
            };
            let signature = accounts::list(conn)?
                .into_iter()
                .find(|acc| acc.id == account_id)
                .and_then(|acc| acc.signature);
            let full_body = format!("{body}{}", signature_block(signature.as_deref()));
            let mut d =
                drafts::create(conn, &account_id, mode, reply_to, &to, &subject, &full_body)?;
            if !cc.is_empty() {
                d.cc = cc;
                drafts::update(conn, &d)?;
            }
            // Same steps as `save_server_draft`: a stable server identity, then
            // the op that APPENDs it to the Drafts folder.
            drafts::ensure_imap_message_id(conn, d.id, &candidate)?;
            bodies::enqueue_op(conn, &account_id, "save_draft", &json!({ "draftId": d.id }))?;
            Ok(Some(d))
        })
        .await?;
    let Some(draft) = draft else {
        return Err(match reply_to {
            Some(mid) => ToolError::Input(format!("message {mid} not found")),
            None => ToolError::Failed("no account configured".into()),
        });
    };
    if let Some(h) = server.hooks() {
        h.drafts_changed(draft.account_id.clone()).await;
    }
    Ok(json!({
        "draft_id": draft.id,
        "account_id": draft.account_id,
        "mode": draft.mode,
        "to": draft.to,
        "cc": draft.cc,
        "subject": draft.subject,
        "saved_to_drafts": true,
        "sent": false,
    }))
}

/// The calendar an event lands on: the selected primary of the first account
/// with one, else the first selected calendar at all.
fn own_calendar(conn: &rusqlite::Connection) -> rusqlite::Result<Option<model::CalendarRow>> {
    let mut fallback = None;
    for acc in accounts::list(conn)? {
        let cals = store::list_calendars(conn, &acc.id)?;
        if let Some(c) = cals.iter().find(|c| c.selected && c.is_primary) {
            return Ok(Some(c.clone()));
        }
        if fallback.is_none() {
            fallback = cals.into_iter().find(|c| c.selected);
        }
    }
    Ok(fallback)
}

async fn create_event(server: &Server, a: &Args<'_>) -> ToolResult {
    let title = a.str("title")?;
    let start = when_arg(a, "start")?;
    let end = when_arg(a, "end")?;
    if end <= start {
        return Err(ToolError::Input("`end` must be after `start`".into()));
    }
    let description = a.opt_str("description");
    let input = gapi::EventInput {
        summary: Some(title.clone()),
        description,
        start_ts: Some(start),
        end_ts: Some(end),
        all_day: Some(false),
        // Own calendar only: no guests, no Meet, nothing that notifies anyone.
        attendees: None,
        add_meet: None,
        ..Default::default()
    };
    let body = gapi::event_body(&input, None);
    debug_assert!(
        body.get("attendees")
            .is_none_or(|v| v.as_array().is_some_and(Vec::is_empty)),
        "an MCP event must carry no attendees"
    );
    let created = server
        .db
        .call(move |conn| {
            let Some(cal) = own_calendar(conn)? else {
                return Ok(None);
            };
            let row = match calendar::commands::local_row_from_input(cal.id, &input) {
                Ok(r) => r,
                Err(e) => return Ok(Some(Err(e.to_string()))),
            };
            let id = store::insert_local_event(conn, &row)?;
            store::queue_op(
                conn,
                &cal.account_id,
                "create",
                &json!({ "event_id": id, "send_updates": "none", "body": body }),
            )?;
            Ok(store::get_event(conn, id)?.map(|e| Ok((cal, e))))
        })
        .await?;
    let (cal, event) = match created {
        None => {
            return Err(ToolError::Failed(
                "no calendar connected (Settings → Calendar)".into(),
            ))
        }
        Some(Err(m)) => return Err(ToolError::Input(m)),
        Some(Ok(pair)) => pair,
    };
    if let Some(h) = server.hooks() {
        h.calendar_changed(cal.account_id.clone()).await;
    }
    Ok(json!({
        "event_id": event.id,
        "calendar": cal.summary,
        "account_id": cal.account_id,
        "title": event.summary,
        "start": rfc3339(event.start_ts),
        "end": rfc3339(event.end_ts),
        "attendees": [],
        "queued": true,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::NewMessage;
    use crate::db::queries::insert_message;
    use crate::db::Db;
    use crate::fork::mcp::test_support::{call, spawn};

    const TOKEN: &str = "t";

    /// Two accounts, an inbox each plus a Trash on the first, four messages in
    /// three threads, bodies on two of them, a selected primary calendar with
    /// one event tomorrow. Returns `(thread ids, message ids)` in insert order.
    fn seed(db: &Db) -> (Vec<i64>, Vec<i64>) {
        db.with(|conn| {
            conn.execute_batch(
                "INSERT INTO accounts (id, email, provider, imap_host, smtp_host, created_at, signature)
                   VALUES ('a1','me@x.example','gmail','i','s',0,'Paddy'),
                          ('a2','me@y.example','custom','i','s',1,NULL);
                 INSERT INTO folders (id, account_id, imap_name, role, display_name, sort_order)
                   VALUES (1,'a1','INBOX','inbox','Inbox',0),
                          (2,'a1','[Gmail]/Trash','trash','Trash',1),
                          (3,'a2','INBOX','inbox','Inbox',0);
                 INSERT INTO fork_cal_calendars (id, account_id, google_id, summary, is_primary, selected, access_role)
                   VALUES (10,'a1','me@x.example','Patrick',1,1,'owner');",
            )?;
            let now = Utc::now().timestamp();
            conn.execute(
                "INSERT INTO fork_cal_events (calendar_id, google_id, status, summary, start_ts, end_ts, all_day, attendees_json)
                 VALUES (10, 'ev1', 'confirmed', 'Standup', ?1, ?2, 0, '[{\"email\":\"anna@nw.example\",\"responseStatus\":\"accepted\"}]'),
                        (10, 'ev2', 'cancelled', 'Gone', ?1, ?2, 0, NULL)",
                rusqlite::params![now + 86_400, now + 86_400 + 1800],
            )?;
            let mk = |folder: i64, uid: u32, subject: &str, from: &str, date: i64, read: bool| NewMessage {
                account_id: if folder == 3 { "a2".into() } else { "a1".into() },
                folder_id: folder,
                uid,
                message_id: Some(format!("<{uid}@example>")),
                subject: Some(subject.into()),
                from_name: Some(from.split('@').next().unwrap().into()),
                from_addr: Some(from.into()),
                to_addrs: vec![Address { name: None, addr: "me@x.example".into() }],
                date,
                snippet: Some(format!("snippet {uid}")),
                is_read: read,
                ..Default::default()
            };
            let mut threads = Vec::new();
            let mut ids = Vec::new();
            for m in [
                mk(1, 1, "Q3 launch checklist", "anna@nw.example", 1_700_000_100, false),
                mk(1, 2, "Contract redline", "marcus@acme.example", 1_700_000_200, true),
                mk(3, 3, "Invoice 42", "billing@vendor.example", 1_700_000_300, false),
            ] {
                let (id, tid) = insert_message(conn, &m)?.expect("inserted");
                ids.push(id);
                threads.push(tid);
            }
            // A reply from me in the first thread.
            let mut reply = mk(1, 4, "Re: Q3 launch checklist", "me@x.example", 1_700_000_400, true);
            reply.in_reply_to = Some("<1@example>".into());
            reply.references = vec!["<1@example>".into()];
            let (id, tid) = insert_message(conn, &reply)?.expect("inserted");
            assert_eq!(tid, threads[0], "reply threads onto the original");
            ids.push(id);
            bodies::set_body(
                conn,
                ids[0],
                None,
                Some("Can we ship Friday?\n\nAnna"),
                "Can we ship Friday?",
                &[],
            )?;
            bodies::set_body(
                conn,
                ids[3],
                Some("<p>Yes, Friday works.</p>"),
                Some("Yes, Friday works.\n\nOn Mon, Anna wrote:\n> Can we ship Friday?"),
                "Yes, Friday works.",
                &[],
            )?;
            Ok((threads, ids))
        })
        .unwrap()
    }

    fn pending_ops(db: &Db) -> Vec<(String, String, Value)> {
        db.with(|conn| {
            conn.prepare("SELECT account_id, kind, payload FROM pending_ops ORDER BY id")?
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        serde_json::from_str(&r.get::<_, String>(2)?).unwrap(),
                    ))
                })?
                .collect()
        })
        .unwrap()
    }

    #[tokio::test]
    async fn search_mail_uses_the_operators_and_groups_by_thread() {
        let db = Db::open_in_memory().unwrap();
        let (threads, _) = seed(&db);
        let (url, _stop) = spawn(db, TOKEN).await;
        let r = call(&url, TOKEN, "search_mail", json!({ "query": "launch" })).await;
        assert_eq!(r["isError"], false);
        let s = &r["structuredContent"];
        assert_eq!(s["count"], 1, "{s}");
        assert_eq!(s["threads"][0]["thread_id"], threads[0]);
        assert_eq!(s["threads"][0]["message_count"], 2);
        assert_eq!(s["threads"][0]["from"]["email"], "me@x.example");
        // An operator with no free text still answers.
        let r = call(
            &url,
            TOKEN,
            "search_mail",
            json!({ "query": "from:marcus" }),
        )
        .await;
        assert_eq!(
            r["structuredContent"]["threads"][0]["subject"],
            "Contract redline"
        );
        // The text content mirrors the structured one.
        let text: Value = serde_json::from_str(r["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(text, r["structuredContent"]);
        // Missing query is a tool error, not a crash.
        let r = call(&url, TOKEN, "search_mail", json!({})).await;
        assert_eq!(r["isError"], true);
    }

    #[tokio::test]
    async fn get_thread_returns_stripped_bodies_and_null_when_not_fetched() {
        let db = Db::open_in_memory().unwrap();
        let (threads, ids) = seed(&db);
        let (url, _stop) = spawn(db, TOKEN).await;
        let r = call(
            &url,
            TOKEN,
            "get_thread",
            json!({ "thread_id": threads[0] }),
        )
        .await;
        let s = &r["structuredContent"];
        assert_eq!(s["messages"].as_array().unwrap().len(), 2);
        assert_eq!(s["messages"][0]["message_id"], ids[0]);
        assert_eq!(s["messages"][0]["body"], "Can we ship Friday?\n\nAnna");
        assert_eq!(s["messages"][1]["body"], "Yes, Friday works.");
        let r = call(
            &url,
            TOKEN,
            "get_thread",
            json!({ "thread_id": threads[0], "strip_quotes": false }),
        )
        .await;
        assert!(r["structuredContent"]["messages"][1]["body"]
            .as_str()
            .unwrap()
            .contains("> Can we ship Friday?"));
        let r = call(
            &url,
            TOKEN,
            "get_thread",
            json!({ "thread_id": threads[1] }),
        )
        .await;
        assert!(r["structuredContent"]["messages"][0]["body"].is_null());
        let r = call(&url, TOKEN, "get_thread", json!({ "thread_id": 999_999 })).await;
        assert_eq!(r["isError"], true);
    }

    #[tokio::test]
    async fn list_unread_defaults_to_the_inbox_and_takes_a_folder() {
        let db = Db::open_in_memory().unwrap();
        let (threads, _) = seed(&db);
        let (url, _stop) = spawn(db, TOKEN).await;
        let r = call(&url, TOKEN, "list_unread", json!({})).await;
        let s = &r["structuredContent"];
        // Thread 0 has one unread message (the original) across both accounts' inboxes.
        let got: Vec<i64> = s["threads"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["thread_id"].as_i64().unwrap())
            .collect();
        assert_eq!(got, vec![threads[2], threads[0]], "{s}");
        assert_eq!(s["threads"][1]["message_count"], 2);
        let r = call(&url, TOKEN, "list_unread", json!({ "folder": "Trash" })).await;
        assert_eq!(r["structuredContent"]["count"], 0);
        let r = call(
            &url,
            TOKEN,
            "list_unread",
            json!({ "folder": "inbox", "limit": 1 }),
        )
        .await;
        assert_eq!(r["structuredContent"]["count"], 1);
    }

    #[tokio::test]
    async fn list_court_reads_the_fork_court_table() {
        let db = Db::open_in_memory().unwrap();
        let (threads, ids) = seed(&db);
        db.with(|conn| {
            conn.execute(
                "INSERT INTO fork_court (thread_id, account_id, state, since, last_message_id, reason)
                 VALUES (?1, 'a2', 'on_me', 1_700_000_300, ?2, NULL),
                        (?3, 'a1', 'waiting', 1_700_000_400, ?4, NULL)",
                rusqlite::params![threads[2], ids[2], threads[0], ids[3]],
            )
            .map(|_| ())
        })
        .unwrap();
        let (url, _stop) = spawn(db, TOKEN).await;
        let r = call(&url, TOKEN, "list_court", json!({})).await;
        let s = &r["structuredContent"];
        assert_eq!(s["count"], 1, "{s}");
        assert_eq!(s["threads"][0]["thread_id"], threads[2]);
        assert!(s["threads"][0]["age_days"].as_i64().unwrap() > 0);
        let r = call(&url, TOKEN, "list_court", json!({ "state": "waiting" })).await;
        assert_eq!(
            r["structuredContent"]["threads"][0]["thread_id"],
            threads[0]
        );
        let r = call(&url, TOKEN, "list_court", json!({ "state": "none" })).await;
        assert_eq!(r["isError"], true);
    }

    #[tokio::test]
    async fn calendar_reads_skip_cancelled_and_slots_avoid_the_event() {
        let db = Db::open_in_memory().unwrap();
        seed(&db);
        let (url, _stop) = spawn(db, TOKEN).await;
        let now = Utc::now().timestamp();
        let r = call(
            &url,
            TOKEN,
            "get_calendar",
            json!({ "from": rfc3339(now), "to": rfc3339(now + 3 * 86_400) }),
        )
        .await;
        let s = &r["structuredContent"];
        assert_eq!(s["count"], 1, "{s}");
        assert_eq!(s["events"][0]["title"], "Standup");
        assert_eq!(s["events"][0]["attendees"][0]["email"], "anna@nw.example");
        let r = call(
            &url,
            TOKEN,
            "get_calendar",
            json!({ "from": "2026-01-01", "to": "2025-01-01" }),
        )
        .await;
        assert_eq!(r["isError"], true);

        let r = call(
            &url,
            TOKEN,
            "find_free_slots",
            json!({ "duration": 30, "days": 3, "tz": "UTC" }),
        )
        .await;
        let s = &r["structuredContent"];
        assert_eq!(s["tz"], "UTC");
        let standup = (now + 86_400, now + 86_400 + 1800);
        for slot in s["slots"].as_array().unwrap() {
            let start = model::parse_rfc3339(slot["start"].as_str().unwrap()).unwrap();
            let end = model::parse_rfc3339(slot["end"].as_str().unwrap()).unwrap();
            assert!(
                end <= standup.0 || start >= standup.1,
                "slot overlaps the event"
            );
            assert!(start > now);
        }
        let r = call(
            &url,
            TOKEN,
            "find_free_slots",
            json!({ "tz": "Mars/Olympus" }),
        )
        .await;
        assert_eq!(r["isError"], true);
    }

    #[tokio::test]
    async fn archive_star_and_read_queue_the_uis_ops() {
        let db = Db::open_in_memory().unwrap();
        let (threads, _) = seed(&db);
        let (url, _stop) = spawn(db.clone(), TOKEN).await;
        let r = call(&url, TOKEN, "mark_read", json!({ "thread_id": threads[0] })).await;
        assert_eq!(r["structuredContent"]["messages"], 2);
        let r = call(
            &url,
            TOKEN,
            "star",
            json!({ "thread_id": threads[1], "on": true }),
        )
        .await;
        assert_eq!(r["structuredContent"]["queued"], true);
        let r = call(&url, TOKEN, "archive", json!({ "thread_id": threads[2] })).await;
        assert_eq!(r["structuredContent"]["messages"], 1);
        let ops = pending_ops(&db);
        assert_eq!(ops.len(), 3, "{ops:?}");
        assert_eq!(ops[0].1, "set_flag");
        assert_eq!(ops[0].2["flag"], "seen");
        assert_eq!(ops[0].2["on"], true);
        assert_eq!(ops[0].2["uids"].as_array().unwrap().len(), 2);
        assert_eq!(ops[1].2["flag"], "flagged");
        assert_eq!(ops[2].1, "archive");
        assert_eq!(ops[2].0, "a2");
        // Optimistic local state moved, like a keypress.
        db.with(|conn| {
            let unread: i64 = conn.query_row(
                "SELECT count(*) FROM messages WHERE thread_id = ?1 AND is_read = 0",
                [threads[0]],
                |r| r.get(0),
            )?;
            assert_eq!(unread, 0);
            let starred: i64 = conn.query_row(
                "SELECT count(*) FROM messages WHERE thread_id = ?1 AND is_starred = 1",
                [threads[1]],
                |r| r.get(0),
            )?;
            assert_eq!(starred, 1);
            let left: i64 = conn.query_row(
                "SELECT count(*) FROM messages WHERE thread_id = ?1",
                [threads[2]],
                |r| r.get(0),
            )?;
            assert_eq!(left, 0, "archived rows are gone locally");
            Ok(())
        })
        .unwrap();
        let r = call(&url, TOKEN, "archive", json!({ "thread_id": threads[2] })).await;
        assert_eq!(r["isError"], true, "a second archive finds nothing");
    }

    #[tokio::test]
    async fn create_draft_saves_to_drafts_and_never_sends() {
        let db = Db::open_in_memory().unwrap();
        let (_, ids) = seed(&db);
        let (url, _stop) = spawn(db.clone(), TOKEN).await;
        let r = call(
            &url,
            TOKEN,
            "create_draft",
            json!({ "to": "anna@nw.example", "cc": "priya@bw.example",
                    "subject": "Numbers", "body": "Attached below." }),
        )
        .await;
        let s = &r["structuredContent"];
        assert_eq!(r["isError"], false, "{r}");
        assert_eq!(s["sent"], false);
        assert_eq!(s["account_id"], "a1");
        let draft_id = s["draft_id"].as_i64().unwrap();
        let d = db
            .with(|conn| drafts::get(conn, draft_id))
            .unwrap()
            .unwrap();
        assert_eq!(d.to, "anna@nw.example");
        assert_eq!(d.cc, "priya@bw.example");
        assert_eq!(d.body, "Attached below.\n\n-- \nPaddy");
        assert_eq!(d.mode, "new");
        let ops = pending_ops(&db);
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].1, "save_draft");
        assert_eq!(ops[0].2["draftId"], draft_id);
        assert!(!ops.iter().any(|o| o.1 == "send"), "nothing queued a send");

        // A reply: recipient and subject default from the original.
        let r = call(
            &url,
            TOKEN,
            "create_draft",
            json!({ "to": "", "subject": "", "body": "Friday it is.",
                    "reply_to_message_id": ids[0] }),
        )
        .await;
        let s = &r["structuredContent"];
        assert_eq!(s["mode"], "reply", "{r}");
        assert_eq!(s["to"], "anna@nw.example");
        assert_eq!(s["subject"], "Re: Q3 launch checklist");
        let d = db
            .with(|conn| drafts::get(conn, s["draft_id"].as_i64().unwrap()))
            .unwrap()
            .unwrap();
        assert_eq!(d.reply_to_message_id, Some(ids[0]));

        let r = call(
            &url,
            TOKEN,
            "create_draft",
            json!({ "to": "", "subject": "x", "body": "y" }),
        )
        .await;
        assert_eq!(r["isError"], true, "a new message needs a recipient");
        let r = call(
            &url,
            TOKEN,
            "create_draft",
            json!({ "to": "a@b", "subject": "x", "body": "y", "reply_to_message_id": 424242 }),
        )
        .await;
        assert_eq!(r["isError"], true);
    }

    #[tokio::test]
    async fn create_event_lands_on_the_primary_calendar_with_no_attendees() {
        let db = Db::open_in_memory().unwrap();
        seed(&db);
        let (url, _stop) = spawn(db.clone(), TOKEN).await;
        let r = call(
            &url,
            TOKEN,
            "create_event",
            json!({ "title": "Deep work", "start": "2026-10-01T09:00:00+02:00",
                    "end": "2026-10-01T11:00:00+02:00", "description": "no calls" }),
        )
        .await;
        let s = &r["structuredContent"];
        assert_eq!(r["isError"], false, "{r}");
        assert_eq!(s["calendar"], "Patrick");
        assert_eq!(s["start"], "2026-10-01T07:00:00Z");
        assert_eq!(s["attendees"], json!([]));
        let (kind, payload): (String, Value) = db
            .with(|conn| {
                conn.query_row(
                    "SELECT kind, payload FROM fork_cal_ops ORDER BY id DESC LIMIT 1",
                    [],
                    |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            serde_json::from_str(&r.get::<_, String>(1)?).unwrap(),
                        ))
                    },
                )
            })
            .unwrap();
        assert_eq!(kind, "create");
        assert_eq!(payload["send_updates"], "none");
        assert!(payload["body"].get("attendees").is_none(), "{payload}");
        assert_eq!(payload["body"]["summary"], "Deep work");
        let row = db
            .with(|conn| store::get_event(conn, s["event_id"].as_i64().unwrap()))
            .unwrap()
            .unwrap();
        assert!(row.local_only);
        assert!(row.attendees_json.is_none());

        let r = call(
            &url,
            TOKEN,
            "create_event",
            json!({ "title": "x", "start": "2026-10-01T11:00:00Z", "end": "2026-10-01T09:00:00Z" }),
        )
        .await;
        assert_eq!(r["isError"], true);
    }

    #[tokio::test]
    async fn create_event_without_a_calendar_says_so() {
        let db = Db::open_in_memory().unwrap();
        let (url, _stop) = spawn(db, TOKEN).await;
        let r = call(
            &url,
            TOKEN,
            "create_event",
            json!({ "title": "x", "start": "2026-10-01T09:00:00Z", "end": "2026-10-01T10:00:00Z" }),
        )
        .await;
        assert_eq!(r["isError"], true);
        assert!(r["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("no calendar"));
    }

    #[tokio::test]
    async fn crm_lookup_reports_not_connected() {
        let (url, _stop) = spawn(Db::open_in_memory().unwrap(), TOKEN).await;
        let r = call(&url, TOKEN, "crm_lookup", json!({ "email": "a@b.example" })).await;
        assert_eq!(r["isError"], true);
        assert!(r["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("not connected"));
    }

    #[test]
    fn parse_when_accepts_the_three_shapes() {
        assert_eq!(parse_when("2026-10-01T09:00:00+02:00"), Some(1_790_838_000));
        assert_eq!(parse_when("2026-10-01"), Some(1_790_812_800));
        assert!(parse_when("2026-10-01T09:00").is_some());
        assert!(parse_when("2026-10-01 09:00").is_some());
        assert_eq!(parse_when("tomorrow"), None);
        assert_eq!(parse_when(""), None);
    }

    #[test]
    fn body_text_prefers_text_falls_back_to_html_and_keeps_bottom_posts() {
        assert_eq!(
            body_text(Some("<p>Hi</p>"), Some("Hi there\n\n> old"), true).as_deref(),
            Some("Hi there")
        );
        assert_eq!(
            body_text(Some("<p>Hi <b>you</b></p>"), None, true).as_deref(),
            Some("Hi you")
        );
        assert_eq!(body_text(None, Some("   "), true), None);
        // Bottom-posted: nothing above the attribution, so the cut would be
        // empty and the whole text is kept instead.
        let bottom = "On Tue, Aug 4, 2026 at 10:00, Ann <ann@example.com> wrote:\n\
                      > the question\n\nMy answer is down here.";
        assert_eq!(body_text(None, Some(bottom), true).as_deref(), Some(bottom));
        let long = "x".repeat(MAX_BODY_CHARS + 10);
        assert!(body_text(None, Some(&long), false)
            .unwrap()
            .ends_with("[truncated]"));
    }
}
