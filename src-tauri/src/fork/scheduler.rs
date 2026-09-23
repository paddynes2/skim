//! Fork (6.4): one scheduler for undo send and send later.
//!
//! A queued op can be held: a `fork_op_schedule` row keeps it out of
//! `sync.rs::drain_ops` until `not_before` (the drain's SELECT carries
//! [`DRAIN_HOLD_FILTER`]; held ops no longer block the ops behind them, D4).
//! One background task sleeps until the earliest `not_before`, wakes that
//! account's engine, and re-arms whenever a schedule changes ([`rearm`]) or the
//! app starts (the table outlives a restart, so a scheduled send survives one
//! while the app runs in the tray).
//!
//! Undo send is the same mechanism with a short hold: cancelling deletes the op
//! and its row, refused inside a 1 s margin of `not_before` because the drain
//! may already be building the message.

use crate::error::{Result, SkimError};
use crate::state::AppState;
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use serde_json::json;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::Notify;

/// The predicate `sync.rs::drain_ops` appends to its SELECT. A test below
/// asserts the upstream file carries it verbatim.
pub const DRAIN_HOLD_FILTER: &str =
    "AND id NOT IN (SELECT op_id FROM fork_op_schedule WHERE not_before > unixepoch())";

/// Cancelling this close to `not_before` is refused (seconds).
pub const CANCEL_MARGIN_SECS: i64 = 1;

/// The longest the task sleeps before re-reading the table, so a clock jump or
/// a missed wake costs at most this.
const MAX_SLEEP: Duration = Duration::from_secs(3600);

static NOTIFY: OnceLock<Arc<Notify>> = OnceLock::new();

fn notify() -> &'static Arc<Notify> {
    NOTIFY.get_or_init(|| Arc::new(Notify::new()))
}

/// Wake the scheduler: something in `fork_op_schedule` changed.
pub fn rearm() {
    notify().notify_one();
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Hold `op_id` until `not_before`. `None` is a no-op, so the send path can
/// call this unconditionally.
pub fn hold(
    conn: &Connection,
    op_id: i64,
    not_before: Option<i64>,
    kind: &str,
    label: Option<&str>,
) -> rusqlite::Result<()> {
    if let Some(t) = not_before {
        conn.execute(
            "INSERT INTO fork_op_schedule (op_id, not_before, kind, label) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(op_id) DO UPDATE SET not_before = excluded.not_before,
                                              kind = excluded.kind, label = excluded.label",
            rusqlite::params![op_id, t, kind, label],
        )?;
    }
    Ok(())
}

/// Every live hold: `(not_before, account_id)` for ops still pending.
pub fn holds(conn: &Connection) -> rusqlite::Result<Vec<(i64, String)>> {
    let mut stmt = conn.prepare_cached(
        "SELECT s.not_before, o.account_id FROM fork_op_schedule s
         JOIN pending_ops o ON o.id = s.op_id
         WHERE o.state = 'pending'
         ORDER BY s.not_before",
    )?;
    let rows = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// What the task does with the holds at `now`: which accounts have a due op
/// (wake their engines) and when the next future one falls due (sleep until
/// then, or wait for a change when there is none). Pure, so it is testable
/// without a clock.
pub fn plan(holds: &[(i64, String)], now: i64) -> (Vec<String>, Option<i64>) {
    let mut due: Vec<String> = Vec::new();
    let mut next: Option<i64> = None;
    for (t, account) in holds {
        if *t <= now {
            if !due.contains(account) {
                due.push(account.clone());
            }
        } else {
            next = Some(next.map_or(*t, |n| n.min(*t)));
        }
    }
    (due, next)
}

/// How long to sleep for a plan: until `next`, capped, or `None` = wait for a
/// change only.
pub fn sleep_for(next: Option<i64>, now: i64) -> Option<Duration> {
    next.map(|t| Duration::from_secs((t - now).max(0) as u64).min(MAX_SLEEP))
}

#[derive(Debug, PartialEq, Eq)]
pub enum Cancel {
    /// The op and its hold are gone; the draft is back, by id.
    Cancelled(i64),
    /// Inside the margin: the drain may already have it.
    TooLate,
    NotFound,
}

/// Cancel a held op: delete it (the hold goes with it) and return the draft it
/// carried, unless `not_before` is within [`CANCEL_MARGIN_SECS`] of `now`.
pub fn cancel(conn: &Connection, op_id: i64, now: i64) -> rusqlite::Result<Cancel> {
    let row: Option<(i64, String, String)> = conn
        .query_row(
            "SELECT s.not_before, o.payload, o.state FROM fork_op_schedule s
             JOIN pending_ops o ON o.id = s.op_id WHERE s.op_id = ?1",
            rusqlite::params![op_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let Some((not_before, payload, state)) = row else {
        return Ok(Cancel::NotFound);
    };
    if state != "pending" || not_before - now < CANCEL_MARGIN_SECS {
        return Ok(Cancel::TooLate);
    }
    let draft_id = serde_json::from_str::<serde_json::Value>(&payload)
        .ok()
        .and_then(|v| v["draftId"].as_i64())
        .unwrap_or(0);
    // Explicit on both tables: the cascade needs foreign keys on, and a
    // connection without them must not leave a hold pointing at nothing.
    conn.execute(
        "DELETE FROM fork_op_schedule WHERE op_id = ?1",
        rusqlite::params![op_id],
    )?;
    conn.execute(
        "DELETE FROM pending_ops WHERE id = ?1",
        rusqlite::params![op_id],
    )?;
    Ok(Cancel::Cancelled(draft_id))
}

/// Bring a held op forward to `now` (the drain picks it up on the next wake).
pub fn send_now(conn: &Connection, op_id: i64, now: i64) -> rusqlite::Result<bool> {
    let n = conn.execute(
        "UPDATE fork_op_schedule SET not_before = ?2, label = NULL WHERE op_id = ?1",
        rusqlite::params![op_id, now],
    )?;
    Ok(n > 0)
}

/// One pending scheduled send, for the Scheduled list.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledSend {
    pub op_id: i64,
    pub draft_id: i64,
    pub account_id: String,
    pub to: String,
    pub subject: String,
    pub snippet: String,
    pub not_before: i64,
    pub label: Option<String>,
}

/// Pending scheduled sends joined to their drafts, soonest first. A hold whose
/// draft is gone (sent by another path, discarded) is not listed.
pub fn list(conn: &Connection) -> rusqlite::Result<Vec<ScheduledSend>> {
    let mut stmt = conn.prepare_cached(
        "SELECT s.op_id, d.id, d.account_id, COALESCE(d.to_addrs, ''), COALESCE(d.subject, ''),
                COALESCE(d.body_text, ''), s.not_before, s.label
         FROM fork_op_schedule s
         JOIN pending_ops o ON o.id = s.op_id AND o.state = 'pending' AND o.kind = 'send'
         JOIN drafts d ON d.id = o.payload->>'draftId'
         ORDER BY s.not_before, s.op_id",
    )?;
    let rows = stmt
        .query_map([], |r| {
            let body: String = r.get(5)?;
            Ok(ScheduledSend {
                op_id: r.get(0)?,
                draft_id: r.get(1)?,
                account_id: r.get(2)?,
                to: r.get(3)?,
                subject: r.get(4)?,
                snippet: snippet(&body),
                not_before: r.get(6)?,
                label: r.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// The first line of the user's own words, trimmed to a list-row length.
fn snippet(body: &str) -> String {
    let words = body
        .split("\n\n-- \n")
        .next()
        .unwrap_or(body)
        .split("\n\nOn ")
        .next()
        .unwrap_or(body);
    let flat: String = words.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out: String = flat.chars().take(120).collect();
    if flat.chars().count() > 120 {
        out.push('…');
    }
    out
}

/// Tell every window a send is held (the main window shows "Sending… Undo")
/// and wake the scheduler.
pub fn announce(app: &AppHandle, op_id: i64, draft_id: i64, not_before: i64, label: Option<&str>) {
    let _ = app.emit(
        "fork:send-held",
        json!({ "opId": op_id, "draftId": draft_id, "notBefore": not_before, "label": label }),
    );
    let _ = app.emit("fork:scheduled", json!({}));
    rearm();
}

#[tauri::command]
pub async fn fork_cancel_scheduled(
    app: AppHandle,
    state: State<'_, AppState>,
    op_id: i64,
) -> Result<i64> {
    let outcome = state
        .db
        .call(move |conn| cancel(conn, op_id, now()))
        .await?;
    match outcome {
        Cancel::Cancelled(draft_id) => {
            rearm();
            let _ = app.emit("fork:scheduled", json!({}));
            let _ = app.emit("drafts:updated", json!({}));
            Ok(draft_id)
        }
        Cancel::TooLate => Err(SkimError::other("schedule", "too late to undo")),
        Cancel::NotFound => Err(SkimError::other("schedule", "not scheduled")),
    }
}

#[tauri::command]
pub async fn fork_scheduled_list(state: State<'_, AppState>) -> Result<Vec<ScheduledSend>> {
    state
        .db
        .read("fork_scheduled_list", |conn| list(conn))
        .await
}

#[tauri::command]
pub async fn fork_send_now(app: AppHandle, state: State<'_, AppState>, op_id: i64) -> Result<()> {
    let found = state
        .db
        .call(move |conn| send_now(conn, op_id, now()))
        .await?;
    if !found {
        return Err(SkimError::other("schedule", "not scheduled"));
    }
    rearm();
    let _ = app.emit("fork:scheduled", json!({}));
    Ok(())
}

/// Start the scheduler task. Called once from `fork::start`.
pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let state = app.state::<AppState>();
            let rows = state.db.call(|conn| holds(conn)).await.unwrap_or_default();
            let (due, next) = plan(&rows, now());
            if !due.is_empty() {
                let engines = state.engines.lock().await;
                for account in &due {
                    if let Some(handle) = engines.get(account) {
                        handle.run_ops();
                    }
                }
                drop(engines);
                let _ = app.emit("fork:scheduled", json!({}));
            }
            match sleep_for(next, now()) {
                Some(d) => {
                    tokio::select! {
                        _ = tokio::time::sleep(d) => {}
                        _ = notify().notified() => {}
                    }
                }
                None => notify().notified().await,
            }
        }
    });
}

/// Drop every queued `send` / `save_draft` op for `draft_id` (and, by the
/// cascade, its hold). Called when the draft is deleted: `drafts.id` is not
/// AUTOINCREMENT, so a later draft can reuse the id, and an orphaned op would
/// then ship that draft's body without anyone pressing Send.
pub fn drop_ops_for_draft(conn: &Connection, draft_id: i64) -> rusqlite::Result<usize> {
    conn.execute(
        "DELETE FROM pending_ops WHERE kind IN ('send', 'save_draft') \
         AND json_extract(payload, '$.draftId') = ?1",
        rusqlite::params![draft_id],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    fn seed(conn: &Connection) -> rusqlite::Result<()> {
        conn.execute_batch(
            "INSERT INTO accounts (id, email, provider, imap_host, smtp_host, created_at)
               VALUES ('a1','me@x','gmail','imap.gmail.com','smtp.gmail.com',0),
                      ('a2','you@x','custom','imap.y','smtp.y',0);",
        )
    }

    fn enqueue(conn: &Connection, account: &str, draft_id: i64) -> i64 {
        crate::db::bodies::enqueue_op(conn, account, "send", &json!({ "draftId": draft_id }))
            .unwrap();
        conn.last_insert_rowid()
    }

    /// The exact query `drain_ops` runs, with the fork's predicate.
    fn drain_pick(conn: &Connection, account: &str) -> Option<i64> {
        conn.query_row(
            &format!(
                "SELECT id FROM pending_ops WHERE account_id = ?1 AND state = 'pending' {DRAIN_HOLD_FILTER} ORDER BY id LIMIT 1"
            ),
            rusqlite::params![account],
            |r| r.get(0),
        )
        .optional()
        .unwrap()
    }

    #[test]
    fn sync_rs_carries_the_drain_predicate_verbatim() {
        let sync = include_str!("../mail/sync.rs");
        assert!(
            sync.contains(DRAIN_HOLD_FILTER),
            "drain_ops must exclude held ops with the exact predicate"
        );
    }

    #[test]
    fn drain_skips_held_ops_and_takes_the_ones_behind() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            seed(conn)?;
            let held = enqueue(conn, "a1", 1);
            let behind = enqueue(conn, "a1", 2);
            let past = enqueue(conn, "a1", 3);
            hold(conn, held, Some(now() + 600), "send", Some("Tomorrow"))?;
            hold(conn, past, Some(now() - 5), "send", None)?;
            hold(conn, behind, None, "send", None)?; // a no-op hold
            assert_eq!(
                drain_pick(conn, "a1"),
                Some(behind),
                "FIFO, minus the held one"
            );
            conn.execute("DELETE FROM pending_ops WHERE id = ?1", [behind])?;
            assert_eq!(
                drain_pick(conn, "a1"),
                Some(past),
                "a hold in the past holds nothing"
            );
            conn.execute("DELETE FROM pending_ops WHERE id = ?1", [past])?;
            assert_eq!(drain_pick(conn, "a1"), None, "only the held op remains");
            // Deleting the op takes the hold with it (foreign keys are on).
            conn.execute("DELETE FROM pending_ops WHERE id = ?1", [held])?;
            let left: i64 =
                conn.query_row("SELECT count(*) FROM fork_op_schedule", [], |r| r.get(0))?;
            assert_eq!(left, 0);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn deleting_a_draft_drops_its_held_send_so_a_reused_id_cannot_inherit_it() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            seed(conn)?;
            let mine = enqueue(conn, "a1", 7);
            let other = enqueue(conn, "a1", 8);
            hold(conn, mine, Some(now() + 3600), "send", Some("Tomorrow"))?;
            assert_eq!(drop_ops_for_draft(conn, 7)?, 1);
            let ids: Vec<i64> = conn
                .prepare("SELECT id FROM pending_ops ORDER BY id")?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            assert_eq!(ids, vec![other], "only draft 7's op is gone");
            let holds: i64 =
                conn.query_row("SELECT count(*) FROM fork_op_schedule", [], |r| r.get(0))?;
            assert_eq!(holds, 0, "its hold went with it");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn cancel_is_refused_inside_the_margin() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            seed(conn)?;
            let op = enqueue(conn, "a1", 42);
            hold(conn, op, Some(1000), "send", None)?;
            assert_eq!(cancel(conn, op, 1000)?, Cancel::TooLate);
            assert_eq!(cancel(conn, op, 1001)?, Cancel::TooLate, "already due");
            assert_eq!(cancel(conn, 999_999, 0)?, Cancel::NotFound);
            // A whole margin away: cancelled, the draft id comes back, both rows go.
            assert_eq!(
                cancel(conn, op, 1000 - CANCEL_MARGIN_SECS)?,
                Cancel::Cancelled(42)
            );
            let ops: i64 = conn.query_row("SELECT count(*) FROM pending_ops", [], |r| r.get(0))?;
            let holds: i64 =
                conn.query_row("SELECT count(*) FROM fork_op_schedule", [], |r| r.get(0))?;
            assert_eq!((ops, holds), (0, 0));
            assert_eq!(cancel(conn, op, 0)?, Cancel::NotFound);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn plan_wakes_due_accounts_once_and_sleeps_to_the_next_future_hold() {
        let rows = vec![
            (100, "a1".to_string()),
            (150, "a1".to_string()),
            (120, "a2".to_string()),
            (500, "a2".to_string()),
            (300, "a1".to_string()),
        ];
        let (due, next) = plan(&rows, 200);
        assert_eq!(due, vec!["a1".to_string(), "a2".to_string()]);
        assert_eq!(next, Some(300));
        let (due, next) = plan(&rows, 50);
        assert!(due.is_empty());
        assert_eq!(next, Some(100));
        let (due, next) = plan(&rows, 1000);
        assert_eq!(due.len(), 2);
        assert_eq!(next, None, "nothing left in the future: wait for a change");
        assert_eq!(plan(&[], 0), (vec![], None));
        // Re-arm after a restart is the same function over the table.
        assert_eq!(sleep_for(Some(260), 200), Some(Duration::from_secs(60)));
        assert_eq!(sleep_for(Some(100), 200), Some(Duration::ZERO));
        assert_eq!(sleep_for(None, 200), None);
        assert_eq!(sleep_for(Some(200 + 7 * 86400), 200), Some(MAX_SLEEP));
    }

    #[test]
    fn holds_and_list_follow_the_op_state_and_the_draft() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            seed(conn)?;
            let d = crate::db::drafts::create(
                conn,
                "a1",
                "new",
                None,
                "bob@x",
                "Later",
                "See you\nthen\n\n-- \nPatrick",
            )?;
            let op = enqueue(conn, "a1", d.id);
            hold(conn, op, Some(5000), "send", Some("Tomorrow 08:00"))?;
            let orphan = enqueue(conn, "a2", 999);
            hold(conn, orphan, Some(4000), "send", None)?;
            assert_eq!(
                holds(conn)?,
                vec![(4000, "a2".to_string()), (5000, "a1".to_string())]
            );
            let rows = list(conn)?;
            assert_eq!(rows.len(), 1, "a hold without a draft is not listed");
            let row = &rows[0];
            assert_eq!((row.op_id, row.draft_id, row.not_before), (op, d.id, 5000));
            assert_eq!(row.label.as_deref(), Some("Tomorrow 08:00"));
            assert_eq!(row.snippet, "See you then");
            assert_eq!(row.subject, "Later");
            assert!(send_now(conn, op, 4200)?);
            assert!(!send_now(conn, 999_999, 4200)?);
            let rows = list(conn)?;
            assert_eq!(rows[0].not_before, 4200);
            assert_eq!(rows[0].label, None);
            // A failed op drops out of both views.
            conn.execute(
                "UPDATE pending_ops SET state = 'failed' WHERE id = ?1",
                [op],
            )?;
            assert!(list(conn)?.is_empty());
            assert_eq!(holds(conn)?, vec![(4000, "a2".to_string())]);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn snippet_takes_the_words_only() {
        assert_eq!(snippet("Hi\n\n-- \nSig\n\nOn x wrote:\n> q"), "Hi");
        assert_eq!(snippet("Hi there\n\nOn Mon, Bob wrote:\n> q"), "Hi there");
        let long = "w ".repeat(100);
        assert_eq!(snippet(&long).chars().count(), 121);
    }
}
