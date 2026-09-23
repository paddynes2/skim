//! Meeting prep (PLAN.md Phase 11): who is in the room, what was last said
//! to them, and a short AI brief over it.
//!
//! Three pure pieces, each unit-tested on its own, and three commands:
//! - [`external_guests`]: the event's attendees minus Patrick's own addresses.
//! - [`guest_threads`]: the guest's last threads through the Phase 4 filter
//!   builder (`from:` OR `to:` that address, last [`THREAD_DAYS`] days).
//! - [`upcoming_with_guests`]: events starting inside the reminder window
//!   that have at least one external guest.
//! - `fork_prep(event_id)`, `fork_prep_upcoming()`, `fork_prep_brief(...)`.
//!
//! The brief streams over a `Channel<AiEvent>` exactly like the one-shot
//! features in `commands/ai.rs` (`spawn_stream`, which is private there, so
//! the same shape is reproduced here rather than reached across).
//! Read-only: nothing here sends mail, writes the calendar or the CRM.

use crate::commands::ai::{ai_context, AiEvent};
use crate::db::models::ThreadRow;
use crate::error::{Result, SkimError};
use crate::fork::calendar::model::EventRow;
use crate::fork::calendar::store;
use crate::fork::court::own_addresses;
use crate::fork::search_query::{filter_sql, Filters, Term};
use crate::state::AppState;
use rusqlite::types::Value as SqlValue;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use tauri::ipc::Channel;
use tauri::State;

/// How far back a guest's threads are searched.
pub const THREAD_DAYS: i64 = 90;
/// How many threads per guest the panel shows.
pub const THREAD_LIMIT: i64 = 5;
/// The reminder window: events starting within this many seconds are "up".
pub const REMINDER_SECS: i64 = 10 * 60;
/// Output ceiling for the brief; it is meant to be short.
const BRIEF_MAX_TOKENS: u32 = 1024;

// ---- guests --------------------------------------------------------------------

/// One entry of Google's `attendees` array, the fields prep cares about.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Attendee {
    pub email: String,
    pub name: Option<String>,
    /// Google marks the calendar owner's own entry `self: true`.
    pub is_self: bool,
    /// Rooms and equipment; never a person to prep for.
    pub resource: bool,
}

/// A guest who is not Patrick. `email` is lowercased.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Guest {
    pub email: String,
    pub name: Option<String>,
}

/// Parse the verbatim `attendees_json` an event row carries. Entries without
/// an email are skipped; nothing else is validated.
pub fn parse_attendees(json: Option<&str>) -> Vec<Attendee> {
    let Some(json) = json else {
        return Vec::new();
    };
    let Ok(Value::Array(items)) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|a| {
            let email = a.get("email")?.as_str()?.trim().to_string();
            if email.is_empty() {
                return None;
            }
            Some(Attendee {
                email,
                name: a
                    .get("displayName")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string),
                is_self: a.get("self").and_then(Value::as_bool).unwrap_or(false),
                resource: a.get("resource").and_then(Value::as_bool).unwrap_or(false),
            })
        })
        .collect()
}

/// Everyone on the event who is not Patrick: own addresses (any case) and the
/// `self` entry are dropped, rooms are dropped, duplicates collapse onto the
/// first occurrence, keeping the first display name seen for that address.
pub fn external_guests(attendees: &[Attendee], own: &HashSet<String>) -> Vec<Guest> {
    let mut out: Vec<Guest> = Vec::new();
    for a in attendees {
        let email = a.email.trim().to_ascii_lowercase();
        if a.is_self || a.resource || own.contains(&email) {
            continue;
        }
        match out.iter_mut().find(|g| g.email == email) {
            Some(g) => {
                if g.name.is_none() {
                    g.name = a.name.clone();
                }
            }
            None => out.push(Guest {
                email,
                name: a.name.clone(),
            }),
        }
    }
    out
}

// ---- threads -------------------------------------------------------------------

/// A thread a guest was on: the list row shape plus where its newest hit
/// lives, so the UI can open it (`mail.openLocation(folderId, id, hitMessageId)`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepThread {
    #[serde(flatten)]
    pub row: ThreadRow,
    pub folder_id: i64,
    pub hit_message_id: i64,
}

/// The `hit` CTE for one guest: messages `from:` OR `to:` the address since
/// `since` (unix seconds), each half built by the Phase 4 filter builder so
/// the LIKE shapes and the Trash/Spam exclusion are the search's own.
pub fn thread_query(email: &str, since: i64) -> (String, Vec<SqlValue>) {
    let from = Filters {
        from: vec![Term::plain(email)],
        after: Some(since),
        ..Filters::default()
    };
    let to = Filters {
        to: vec![Term::plain(email)],
        after: Some(since),
        ..Filters::default()
    };
    let (from_sql, mut params) = filter_sql(&from);
    let (to_sql, to_params) = filter_sql(&to);
    params.extend(to_params);
    let sql = format!(
        "SELECT m.id, m.thread_id, m.date, m.folder_id FROM messages m \
         WHERE m.thread_id IS NOT NULL AND ((1=1{from_sql}) OR (1=1{to_sql}))"
    );
    (sql, params)
}

/// The guest's newest `limit` threads since `since`, newest hit first,
/// shaped like `search_query::search_threads`.
pub fn guest_threads(
    conn: &Connection,
    email: &str,
    since: i64,
    limit: i64,
) -> rusqlite::Result<Vec<PrepThread>> {
    let (hit, mut params) = thread_query(email, since);
    params.push(SqlValue::Integer(limit.clamp(0, 50)));
    let sql = format!(
        "WITH hit AS ({hit})
         SELECT t.id, m.from_name, m.from_addr, m.subject, m.snippet, m.date,
                (NOT EXISTS (SELECT 1 FROM messages m3
                             WHERE m3.thread_id = t.id AND m3.is_read = 0)),
                t.starred, max(m2.has_attachments), t.message_count, t.account_id,
                m.folder_id, m.id
         FROM threads t
         JOIN hit h ON h.thread_id = t.id
         JOIN messages m ON m.id = h.id
         JOIN messages m2 ON m2.thread_id = t.id
         WHERE h.date = (SELECT max(h2.date) FROM hit h2 WHERE h2.thread_id = t.id)
         GROUP BY t.id
         ORDER BY m.date DESC, t.id DESC
         LIMIT ?"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(params.iter()), |r| {
            let from_name: Option<String> = r.get(1)?;
            let from_addr: Option<String> = r.get(2)?;
            Ok(PrepThread {
                row: ThreadRow {
                    id: r.get(0)?,
                    message_id: None,
                    account_id: r.get(10)?,
                    from_name: from_name
                        .filter(|s| !s.is_empty())
                        .or_else(|| from_addr.clone())
                        .unwrap_or_default(),
                    from_addr: from_addr.unwrap_or_default(),
                    subject: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                    snippet: r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                    date: r.get(5)?,
                    is_read: r.get(6)?,
                    is_starred: r.get(7)?,
                    has_attachments: r.get::<_, i64>(8)? != 0,
                    message_count: r.get(9)?,
                },
                folder_id: r.get(11)?,
                hit_message_id: r.get(12)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

// ---- payload -------------------------------------------------------------------

/// One guest's prep: who, their last threads, and their CRM card if any.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepGuest {
    pub email: String,
    pub name: Option<String>,
    pub threads: Vec<PrepThread>,
    /// The Rebound card (Phase 9 lookup response, verbatim) or `None` when
    /// the guest is not in the CRM or the CRM is not connected.
    pub crm: Option<Value>,
}

/// What `fork_prep` answers with.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepPayload {
    pub event_id: i64,
    pub summary: String,
    pub start_ts: i64,
    pub end_ts: i64,
    pub guests: Vec<PrepGuest>,
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Guests and threads for one event, without the CRM half (pure over the
/// connection; the tests drive it directly).
pub fn prep_guests(
    conn: &Connection,
    event: &EventRow,
    now: i64,
) -> rusqlite::Result<Vec<PrepGuest>> {
    let own = own_addresses(conn)?;
    let since = now - THREAD_DAYS * 86_400;
    let guests = external_guests(&parse_attendees(event.attendees_json.as_deref()), &own);
    guests
        .into_iter()
        .map(|g| {
            Ok(PrepGuest {
                threads: guest_threads(conn, &g.email, since, THREAD_LIMIT)?,
                email: g.email,
                name: g.name,
                crm: None,
            })
        })
        .collect()
}

/// Seam for Phase 9: the Rebound card for one address. `fork::crm` has no
/// lookup yet (its file is a stub), so this answers `None`; when
/// `fork::crm::lookup(&state, email) -> Result<Option<Value>>` lands, this
/// body becomes that call (a miss or a CRM error stays `None`: the panel
/// shows "Not in Rebound" either way, never blocks on the CRM).
async fn crm_card(_state: &AppState, _email: &str) -> Option<Value> {
    None
}

async fn load_event(state: &AppState, event_id: i64) -> Result<EventRow> {
    state
        .db
        .read("fork_prep_event", move |conn| {
            store::get_event(conn, event_id)
        })
        .await?
        .ok_or_else(|| SkimError::other("prep", "event not found"))
}

async fn load_payload(state: &AppState, event_id: i64) -> Result<PrepPayload> {
    let event = load_event(state, event_id).await?;
    let now = now_unix();
    let ev = event.clone();
    let mut guests = state
        .db
        .read("fork_prep_guests", move |conn| prep_guests(conn, &ev, now))
        .await?;
    for g in &mut guests {
        g.crm = crm_card(state, &g.email).await;
    }
    Ok(PrepPayload {
        event_id: event.id,
        summary: event.summary,
        start_ts: event.start_ts,
        end_ts: event.end_ts,
        guests,
    })
}

/// The prep panel's data for one event.
#[tauri::command]
pub async fn fork_prep(state: State<'_, AppState>, event_id: i64) -> Result<PrepPayload> {
    load_payload(&state, event_id).await
}

// ---- reminder ------------------------------------------------------------------

/// An event the reminder should name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpcomingEvent {
    pub id: i64,
    pub summary: String,
    pub start_ts: i64,
    pub guests: Vec<Guest>,
}

/// Timed events starting in `[now, now + REMINDER_SECS]`, not cancelled, not
/// declined, with at least one external guest. Earliest start first.
pub fn upcoming_with_guests(
    events: &[EventRow],
    own: &HashSet<String>,
    now: i64,
) -> Vec<UpcomingEvent> {
    let mut out: Vec<UpcomingEvent> = events
        .iter()
        .filter(|e| !e.all_day && e.status != "cancelled")
        .filter(|e| e.self_response.as_deref() != Some("declined"))
        .filter(|e| e.start_ts >= now && e.start_ts <= now + REMINDER_SECS)
        .filter_map(|e| {
            let guests = external_guests(&parse_attendees(e.attendees_json.as_deref()), own);
            (!guests.is_empty()).then(|| UpcomingEvent {
                id: e.id,
                summary: e.summary.clone(),
                start_ts: e.start_ts,
                guests,
            })
        })
        .collect();
    out.sort_by_key(|e| (e.start_ts, e.id));
    out
}

/// Events starting within the next ten minutes that have external guests.
/// The UI polls this once a minute and toasts each event once.
#[tauri::command]
pub async fn fork_prep_upcoming(state: State<'_, AppState>) -> Result<Vec<UpcomingEvent>> {
    let now = now_unix();
    state
        .db
        .read("fork_prep_upcoming", move |conn| {
            let own = own_addresses(conn)?;
            let events = store::events_between(conn, None, now, now + REMINDER_SECS + 1)?;
            Ok(upcoming_with_guests(&events, &own, now))
        })
        .await
}

// ---- brief ---------------------------------------------------------------------

const BRIEF_SYSTEM: &str =
    "You write a short meeting brief for the reader, who is about to meet the \
guests listed. Use only the material given: the guests' recent email threads (subjects and \
snippets) and any CRM card. Output in Markdown, at most 150 words: one line per guest (who they \
are, the open thread with them, anything owed either way), then a single line of suggested \
talking points. No preamble, no headings, no invented facts; if the material is thin, say so \
in one line.";

fn fmt_ts(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|d| {
            d.with_timezone(&chrono::Local)
                .format("%a %d %b %H:%M")
                .to_string()
        })
        .unwrap_or_default()
}

/// The user turn of the brief request: event, then per guest the CRM card
/// and the threads (newest first). Pure, tested for shape.
pub fn brief_prompt(payload: &PrepPayload, now_line: &str) -> String {
    let mut s = String::new();
    s.push_str(&format!("Now: {now_line}\n"));
    s.push_str(&format!(
        "Meeting: {} at {}\n",
        if payload.summary.trim().is_empty() {
            "(untitled)"
        } else {
            payload.summary.trim()
        },
        fmt_ts(payload.start_ts)
    ));
    for g in &payload.guests {
        s.push_str("\n## Guest: ");
        match &g.name {
            Some(n) => s.push_str(&format!("{n} <{}>\n", g.email)),
            None => s.push_str(&format!("{}\n", g.email)),
        }
        match &g.crm {
            Some(card) => s.push_str(&format!(
                "CRM card: {}\n",
                serde_json::to_string(card).unwrap_or_default()
            )),
            None => s.push_str("CRM card: not in the CRM\n"),
        }
        if g.threads.is_empty() {
            s.push_str("Threads: none in the last 90 days\n");
        } else {
            s.push_str("Threads (newest first):\n");
            for t in &g.threads {
                s.push_str(&format!(
                    "- {} | {} | from {} | {}\n",
                    fmt_ts(t.row.date),
                    t.row.subject.trim(),
                    t.row.from_addr,
                    t.row.snippet.trim()
                ));
            }
        }
    }
    s
}

/// Stream a brief for the event over `channel` (`delta` / `reasoning` /
/// `done` / `error`, the shapes `aiStream` in the UI already handles). The
/// task registers under `request_id` so `ai_cancel` can abort it.
#[tauri::command]
pub async fn fork_prep_brief(
    state: State<'_, AppState>,
    request_id: String,
    event_id: i64,
    channel: Channel<AiEvent>,
) -> Result<()> {
    let ctx = ai_context(&state.db).await?;
    let payload = load_payload(&state, event_id).await?;
    if payload.guests.is_empty() {
        return Err(SkimError::other("prep", "no external guests on this event"));
    }
    let user = brief_prompt(&payload, &ctx.now);
    let task = tokio::spawn(async move {
        let mut reported = false;
        let mut on_reasoning = || {
            if !reported {
                reported = true;
                let _ = channel.send(AiEvent::Reasoning);
            }
        };
        let mut on_delta = |d: &str| {
            let _ = channel.send(AiEvent::Delta {
                text: d.to_string(),
            });
        };
        let messages = vec![crate::ai::ChatMessage {
            role: "user",
            content: user,
        }];
        let result = match &ctx.endpoint {
            None => {
                let request = crate::ai::anthropic::Request {
                    model: ctx.model.clone(),
                    system: BRIEF_SYSTEM.to_string(),
                    messages,
                    media: Vec::new(),
                    max_tokens: BRIEF_MAX_TOKENS,
                };
                crate::ai::anthropic::stream(&ctx.key, &request, &mut on_delta, &mut on_reasoning)
                    .await
            }
            Some(ep) => {
                let request = crate::ai::openai_compat::Request {
                    model: ctx.model.clone(),
                    system: BRIEF_SYSTEM.to_string(),
                    messages,
                    max_tokens: BRIEF_MAX_TOKENS,
                };
                crate::ai::openai_compat::stream(
                    ep,
                    &ctx.key,
                    &request,
                    &mut on_delta,
                    &mut on_reasoning,
                )
                .await
            }
        };
        match result {
            Ok(_) => {
                let _ = channel.send(AiEvent::Done {
                    citations: Vec::new(),
                });
            }
            Err(e) => {
                let _ = channel.send(AiEvent::Error {
                    code: e.code().to_string(),
                    message: e.to_string(),
                });
            }
        }
    });
    if let Ok(mut tasks) = state.ai_tasks.lock() {
        tasks.retain(|_, h| !h.is_finished());
        tasks.insert(request_id, task.abort_handle());
    }
    Ok(())
}

// ---- tests ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::{Address, NewMessage};
    use crate::db::{queries::insert_message, Db};

    const ME: &str = "me@example.com";
    const ANNA: &str = "anna@northwind.example";

    fn own() -> HashSet<String> {
        [ME.to_string(), "paddy@autospark.ai".to_string()]
            .into_iter()
            .collect()
    }

    fn att(email: &str, name: Option<&str>) -> Attendee {
        Attendee {
            email: email.into(),
            name: name.map(str::to_string),
            ..Attendee::default()
        }
    }

    // ---- external_guests ----

    #[test]
    fn own_addresses_are_excluded_case_insensitively() {
        let list = vec![
            att("Me@Example.COM", Some("Patrick")),
            att("PADDY@autospark.ai", None),
            att("Anna@Northwind.example", Some("Anna Weber")),
        ];
        let got = external_guests(&list, &own());
        assert_eq!(
            got,
            vec![Guest {
                email: ANNA.into(),
                name: Some("Anna Weber".into())
            }]
        );
    }

    #[test]
    fn self_and_resource_entries_are_excluded_even_when_not_in_own_list() {
        let list = vec![
            Attendee {
                email: "other-mailbox@example.com".into(),
                is_self: true,
                ..Attendee::default()
            },
            Attendee {
                email: "room-4@resource.calendar.google.com".into(),
                name: Some("Room 4".into()),
                resource: true,
                ..Attendee::default()
            },
            att("bob@acme.example", Some("Bob")),
        ];
        let got = external_guests(&list, &own());
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].email, "bob@acme.example");
    }

    #[test]
    fn duplicates_collapse_and_keep_the_first_display_name() {
        let list = vec![
            att("anna@northwind.example", None),
            att("ANNA@northwind.example", Some("Anna Weber")),
            att("anna@northwind.example", Some("A. Weber")),
            att("bob@acme.example", Some("Bob")),
        ];
        let got = external_guests(&list, &own());
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].email, ANNA);
        // The first entry had no name; the first name seen fills it.
        assert_eq!(got[0].name.as_deref(), Some("Anna Weber"));
        assert_eq!(got[1].email, "bob@acme.example");
    }

    #[test]
    fn parse_attendees_reads_googles_shape_and_skips_entries_without_email() {
        let json = r#"[
          {"email":"me@example.com","self":true,"responseStatus":"accepted"},
          {"email":"anna@northwind.example","displayName":"Anna Weber","responseStatus":"needsAction"},
          {"displayName":"nobody"},
          {"email":"room@resource.calendar.google.com","resource":true}
        ]"#;
        let got = parse_attendees(Some(json));
        assert_eq!(got.len(), 3);
        assert!(got[0].is_self);
        assert_eq!(got[1].name.as_deref(), Some("Anna Weber"));
        assert!(got[2].resource);
        assert!(parse_attendees(None).is_empty());
        assert!(parse_attendees(Some("not json")).is_empty());
        assert!(parse_attendees(Some("{}")).is_empty());
    }

    // ---- thread query ----

    #[test]
    fn thread_query_is_from_or_to_since_the_cutoff() {
        let (sql, params) = thread_query(ANNA, 1_000);
        assert!(sql.contains("m.from_addr LIKE ?"), "{sql}");
        assert!(sql.contains("COALESCE(m.to_addrs,'') LIKE ?"), "{sql}");
        assert!(sql.contains(") OR (1=1"), "the two halves are ORed: {sql}");
        assert_eq!(sql.matches("m.date >= ?").count(), 2, "{sql}");
        assert_eq!(
            sql.matches("role IN ('trash','junk')").count(),
            2,
            "each half keeps the search's Trash/Spam exclusion: {sql}"
        );
        // after, from-name, from-addr, after, to
        assert_eq!(params.len(), 5);
        assert_eq!(params[0], SqlValue::Integer(1_000));
        assert_eq!(params[3], SqlValue::Integer(1_000));
        assert_eq!(params[4], SqlValue::Text(format!("%{ANNA}%")));
        assert_eq!(sql.matches('?').count(), params.len());
    }

    // ---- DB-backed ----

    const NOW: i64 = 1_800_000_000;
    const DAY: i64 = 86_400;

    fn seed(conn: &Connection) {
        conn.execute_batch(&format!(
            "INSERT INTO accounts (id, email, provider, imap_host, smtp_host, created_at)
               VALUES ('a1','{ME}','custom','imap.x','smtp.x',0);
             INSERT INTO folders (id, account_id, imap_name, role, display_name, sort_order)
               VALUES (1,'a1','INBOX','inbox','Inbox',0),
                      (2,'a1','Sent','sent','Sent',1),
                      (5,'a1','Trash','trash','Trash',4);
             INSERT INTO fork_cal_calendars (id, account_id, google_id, summary, selected)
               VALUES (1,'a1','{ME}','Mine',1);"
        ))
        .unwrap();
    }

    struct M<'a> {
        folder: i64,
        uid: u32,
        subject: &'a str,
        from: &'a str,
        to: &'a str,
        date: i64,
    }

    fn add(conn: &mut Connection, m: M) -> (i64, i64) {
        insert_message(
            conn,
            &NewMessage {
                account_id: "a1".into(),
                folder_id: m.folder,
                uid: m.uid,
                message_id: Some(format!("<{}-{}@x>", m.folder, m.uid)),
                subject: Some(m.subject.into()),
                from_name: Some(if m.from == ME { "Me" } else { "Anna Weber" }.into()),
                from_addr: Some(m.from.into()),
                to_addrs: vec![Address {
                    name: None,
                    addr: m.to.into(),
                }],
                date: m.date,
                snippet: Some(format!("snippet of {}", m.subject)),
                ..Default::default()
            },
        )
        .unwrap()
        .expect("inserted")
    }

    fn event(id: i64, start: i64, attendees: &str) -> EventRow {
        EventRow {
            id,
            calendar_id: 1,
            google_id: format!("g{id}"),
            status: "confirmed".into(),
            summary: format!("Event {id}"),
            start_ts: start,
            end_ts: start + 1800,
            attendees_json: Some(attendees.into()),
            ..EventRow::default()
        }
    }

    fn insert_event(conn: &Connection, e: &EventRow) {
        conn.execute(
            "INSERT INTO fork_cal_events (id, calendar_id, google_id, status, summary, start_ts, end_ts,
                                          all_day, attendees_json, self_response)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![
                e.id,
                e.calendar_id,
                e.google_id,
                e.status,
                e.summary,
                e.start_ts,
                e.end_ts,
                e.all_day as i64,
                e.attendees_json,
                e.self_response
            ],
        )
        .unwrap();
    }

    const ATT_ANNA: &str = r#"[{"email":"me@example.com","self":true},{"email":"Anna@northwind.example","displayName":"Anna Weber"}]"#;

    #[test]
    fn guest_threads_finds_mail_from_and_to_the_guest_inside_the_window() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            seed(conn);
            // Inbound from Anna, 1 day ago.
            let (_, t_in) = add(
                conn,
                M {
                    folder: 1,
                    uid: 1,
                    subject: "Q3 launch",
                    from: ANNA,
                    to: ME,
                    date: NOW - DAY,
                },
            );
            // Outbound to Anna, 2 days ago (Sent).
            let (_, t_out) = add(
                conn,
                M {
                    folder: 2,
                    uid: 1,
                    subject: "Contract redline",
                    from: ME,
                    to: ANNA,
                    date: NOW - 2 * DAY,
                },
            );
            // Someone else: never Anna's.
            add(
                conn,
                M {
                    folder: 1,
                    uid: 2,
                    subject: "Invoice 42",
                    from: "bob@acme.example",
                    to: ME,
                    date: NOW - DAY,
                },
            );
            // Anna, but 200 days ago: outside the window.
            add(
                conn,
                M {
                    folder: 1,
                    uid: 3,
                    subject: "Old kickoff",
                    from: ANNA,
                    to: ME,
                    date: NOW - 200 * DAY,
                },
            );
            // Anna, in Trash: the search's exclusion applies.
            add(
                conn,
                M {
                    folder: 5,
                    uid: 1,
                    subject: "Binned",
                    from: ANNA,
                    to: ME,
                    date: NOW - DAY,
                },
            );

            let since = NOW - THREAD_DAYS * DAY;
            let rows = guest_threads(conn, ANNA, since, THREAD_LIMIT)?;
            let ids: Vec<i64> = rows.iter().map(|r| r.row.id).collect();
            assert_eq!(ids, vec![t_in, t_out], "newest hit first");
            assert_eq!(rows[0].row.subject, "Q3 launch");
            assert_eq!(rows[0].folder_id, 1);
            assert_eq!(rows[1].row.subject, "Contract redline");
            assert_eq!(rows[1].folder_id, 2);
            assert_eq!(rows[1].row.from_addr, ME);
            assert!(rows.iter().all(|r| r.hit_message_id > 0));

            // The limit is honoured.
            assert_eq!(guest_threads(conn, ANNA, since, 1)?.len(), 1);
            // Case does not matter for the address.
            assert_eq!(
                guest_threads(conn, "ANNA@Northwind.example", since, 5)?.len(),
                2
            );
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn prep_guests_reads_the_event_and_drops_patrick() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            seed(conn);
            add(
                conn,
                M {
                    folder: 1,
                    uid: 1,
                    subject: "Q3 launch",
                    from: ANNA,
                    to: ME,
                    date: NOW - DAY,
                },
            );
            let e = event(7, NOW + 300, ATT_ANNA);
            insert_event(conn, &e);
            let loaded = store::get_event(conn, 7)?.expect("event row");
            let guests = prep_guests(conn, &loaded, NOW)?;
            assert_eq!(guests.len(), 1);
            assert_eq!(guests[0].email, ANNA);
            assert_eq!(guests[0].name.as_deref(), Some("Anna Weber"));
            assert_eq!(guests[0].threads.len(), 1);
            assert!(guests[0].crm.is_none(), "no CRM lookup wired yet");
            Ok(())
        })
        .unwrap();
    }

    // ---- reminder ----

    #[test]
    fn upcoming_with_guests_keeps_only_timed_live_events_in_the_window_with_externals() {
        let own = own();
        let only_me = r#"[{"email":"me@example.com","self":true}]"#;
        let mut declined = event(4, NOW + 120, ATT_ANNA);
        declined.self_response = Some("declined".into());
        let mut cancelled = event(5, NOW + 120, ATT_ANNA);
        cancelled.status = "cancelled".into();
        let mut all_day = event(6, NOW + 120, ATT_ANNA);
        all_day.all_day = true;
        let events = vec![
            event(1, NOW + 540, ATT_ANNA),               // in: 9 min
            event(2, NOW + 60, ATT_ANNA),                // in: 1 min (sorts first)
            event(3, NOW + REMINDER_SECS + 1, ATT_ANNA), // out: 10m01s
            event(8, NOW - 1, ATT_ANNA),                 // out: already started
            event(9, NOW + 120, only_me),                // out: no external guest
            declined,
            cancelled,
            all_day,
            event(10, NOW + REMINDER_SECS, ATT_ANNA), // in: exactly 10 min
        ];
        let got = upcoming_with_guests(&events, &own, NOW);
        let ids: Vec<i64> = got.iter().map(|e| e.id).collect();
        assert_eq!(ids, vec![2, 1, 10]);
        assert_eq!(got[0].guests[0].email, ANNA);
    }

    // ---- brief prompt ----

    #[test]
    fn brief_prompt_carries_guests_cards_and_threads() {
        let payload = PrepPayload {
            event_id: 1,
            summary: "Call with Anna".into(),
            start_ts: NOW,
            end_ts: NOW + 1800,
            guests: vec![
                PrepGuest {
                    email: ANNA.into(),
                    name: Some("Anna Weber".into()),
                    threads: vec![PrepThread {
                        row: ThreadRow {
                            id: 1,
                            message_id: None,
                            account_id: "a1".into(),
                            from_name: "Anna Weber".into(),
                            from_addr: ANNA.into(),
                            subject: "Q3 launch".into(),
                            snippet: "Three things still need an owner".into(),
                            date: NOW - DAY,
                            is_read: true,
                            is_starred: false,
                            has_attachments: false,
                            message_count: 5,
                        },
                        folder_id: 1,
                        hit_message_id: 11,
                    }],
                    crm: Some(serde_json::json!({"company": "Northwind", "deal": "Pilot"})),
                },
                PrepGuest {
                    email: "bob@acme.example".into(),
                    name: None,
                    threads: Vec::new(),
                    crm: None,
                },
            ],
        };
        let p = brief_prompt(&payload, "Monday");
        assert!(p.starts_with("Now: Monday\nMeeting: Call with Anna at "));
        assert!(p.contains("## Guest: Anna Weber <anna@northwind.example>"));
        assert!(p.contains(r#"CRM card: {"company":"Northwind","deal":"Pilot"}"#));
        assert!(p.contains(
            "| Q3 launch | from anna@northwind.example | Three things still need an owner"
        ));
        assert!(p.contains("## Guest: bob@acme.example\nCRM card: not in the CRM\nThreads: none"));
    }
}
