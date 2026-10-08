//! DORMANT since v1.1.3 (D56): Deals replaced the Snooze / Follow-ups UI after
//! zero uses, nothing sets a reminder any more and [`start`] is no longer
//! called. Fork migration f0009 emptied the table. The table, the commands
//! and the `fork::list` clauses remain and match no rows; remove them together
//! when convenient.
//!
//! Snooze and Follow-ups (v1.1.1), the replacement for the On me / Waiting
//! views, which guessed from the last sender and were not useful. Here every
//! reminder is one Patrick set on a thread; the table and its rules are in
//! `migrations/f0007_reminders.sql`.
//!
//! The lists honour them through `fork::list::apply`: a snoozed thread is
//! hidden until due, and a due reminder pins its thread to the top until it is
//! opened. [`sweep`] keeps the table honest after mail arrives: it binds a new
//! email's follow-up to its Sent copy, deletes follow-ups someone answered, and
//! brings a snoozed thread back early when someone replies. [`start`] runs the
//! sweep every minute and emits [`EVT_UPDATED`] when anything moved or fell due.

use crate::db::models::ThreadRow;
use crate::error::{Result, SkimError};
use crate::state::AppState;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

/// Emitted after a set / clear / sweep that changed something. The payload's
/// `due` lists reminders that fell due since the last emit (for the toast).
pub const EVT_UPDATED: &str = "reminders:updated";

pub const KIND_SNOOZE: &str = "snooze";
pub const KIND_FOLLOWUP: &str = "followup";

/// A follow-up set on a new email waits this long for its Sent copy.
const PENDING_TTL: i64 = 3 * 86_400;
/// A Sent copy may carry a Date a little before the moment Send was pressed.
const PENDING_SLACK: i64 = 600;

fn kind_ok(kind: &str) -> Result<()> {
    if kind == KIND_SNOOZE || kind == KIND_FOLLOWUP {
        Ok(())
    } else {
        Err(SkimError::other(
            "reminders",
            format!("unknown kind {kind:?}"),
        ))
    }
}

pub fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// SQL for "this thread is snoozed and not yet due" (`tid` = thread id expr).
pub fn hidden_pred(tid: &str) -> String {
    format!(
        "NOT EXISTS (SELECT 1 FROM fork_reminders fr WHERE fr.thread_id = {tid} \
         AND fr.kind = 'snooze' AND fr.due_ts > CAST(strftime('%s','now') AS INTEGER))"
    )
}

/// SQL for "a reminder on this thread is due and not yet opened" (the pin).
pub fn pinned_expr(tid: &str) -> String {
    format!(
        "EXISTS (SELECT 1 FROM fork_reminders fr WHERE fr.thread_id = {tid} \
         AND fr.due_ts <= CAST(strftime('%s','now') AS INTEGER) AND fr.seen_ts IS NULL)"
    )
}

/// Set (or move) a reminder on a thread. Re-setting clears seen/notified.
/// An unknown kind fails the table's CHECK.
pub fn set(
    conn: &Connection,
    thread_id: i64,
    kind: &str,
    due_ts: i64,
    now: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO fork_reminders (kind, thread_id, set_ts, due_ts)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(thread_id, kind) WHERE thread_id IS NOT NULL
         DO UPDATE SET set_ts = excluded.set_ts, due_ts = excluded.due_ts,
                       seen_ts = NULL, notified_ts = NULL",
        params![kind, thread_id, now, due_ts],
    )?;
    Ok(())
}

/// The thread a message belongs to.
pub fn thread_of(conn: &Connection, message_id: i64) -> rusqlite::Result<Option<i64>> {
    conn.query_row(
        "SELECT thread_id FROM messages WHERE id = ?1",
        [message_id],
        |r| r.get(0),
    )
    .optional()
}

/// A follow-up on an email not sent yet: bound to its thread by [`sweep`].
/// The caller checks the subject is not empty.
pub fn set_pending(
    conn: &Connection,
    subject: &str,
    due_ts: i64,
    now: i64,
) -> rusqlite::Result<()> {
    let subject = subject.trim();
    conn.execute(
        "INSERT INTO fork_reminders (kind, thread_id, subject, set_ts, due_ts)
         VALUES ('followup', NULL, ?1, ?2, ?3)",
        params![subject, now, due_ts],
    )?;
    Ok(())
}

pub fn clear(conn: &Connection, thread_id: i64, kind: &str) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM fork_reminders WHERE thread_id = ?1 AND kind = ?2",
        params![thread_id, kind],
    )?;
    Ok(())
}

/// Opening a thread un-pins its due reminders. A due snooze is then done; a
/// due follow-up stays in the Follow-ups view until answered or cleared.
pub fn seen(conn: &Connection, thread_id: i64, now: i64) -> rusqlite::Result<bool> {
    let n = conn.execute(
        "UPDATE fork_reminders SET seen_ts = ?2
         WHERE thread_id = ?1 AND due_ts <= ?2 AND seen_ts IS NULL",
        params![thread_id, now],
    )?;
    conn.execute(
        "DELETE FROM fork_reminders
         WHERE thread_id = ?1 AND kind = 'snooze' AND seen_ts IS NOT NULL",
        [thread_id],
    )?;
    Ok(n > 0)
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Reminder {
    pub kind: String,
    pub set_ts: i64,
    pub due_ts: i64,
}

pub fn get(conn: &Connection, thread_id: i64) -> rusqlite::Result<Vec<Reminder>> {
    let mut stmt = conn.prepare_cached(
        "SELECT kind, set_ts, due_ts FROM fork_reminders WHERE thread_id = ?1 ORDER BY kind",
    )?;
    let rows = stmt
        .query_map([thread_id], |r| {
            Ok(Reminder {
                kind: r.get(0)?,
                set_ts: r.get(1)?,
                due_ts: r.get(2)?,
            })
        })?
        .collect();
    rows
}

/// True when someone other than Patrick wrote in the thread after `after`.
fn replied_since(
    conn: &Connection,
    thread_id: i64,
    after: i64,
    own: &std::collections::HashSet<String>,
) -> rusqlite::Result<bool> {
    let mut stmt =
        conn.prepare_cached("SELECT from_addr FROM messages WHERE thread_id = ?1 AND date > ?2")?;
    let addrs = stmt
        .query_map(params![thread_id, after], |r| r.get::<_, Option<String>>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(addrs.into_iter().flatten().any(|a| {
        let a = a.trim().to_ascii_lowercase();
        !a.is_empty() && !own.contains(&a)
    }))
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SweepStats {
    pub bound: usize,
    pub answered: usize,
    pub woken: usize,
    pub dropped: usize,
}

impl SweepStats {
    pub fn changed(&self) -> bool {
        self.bound + self.answered + self.woken + self.dropped > 0
    }
}

/// Keep reminders true to the mailbox. Cheap: the table holds a handful of rows.
pub fn sweep(conn: &mut Connection, now: i64) -> rusqlite::Result<SweepStats> {
    let own = crate::fork::court::own_addresses(conn)?;
    let mut st = SweepStats::default();
    let tx = conn.transaction()?;

    // Rows whose thread is gone (deleted mail).
    st.dropped += tx.execute(
        "DELETE FROM fork_reminders WHERE thread_id IS NOT NULL
           AND NOT EXISTS (SELECT 1 FROM threads t WHERE t.id = fork_reminders.thread_id)",
        [],
    )?;

    // 1. Pending follow-ups: find the Sent copy by subject, sent by him.
    let pending: Vec<(i64, String, i64)> = {
        let mut s =
            tx.prepare("SELECT id, subject, set_ts FROM fork_reminders WHERE thread_id IS NULL")?;
        let rows = s
            .query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    r.get(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    for (id, subject, set_ts) in pending {
        let mut s = tx.prepare_cached(
            "SELECT thread_id, from_addr FROM messages
             WHERE trim(subject) = ?1 AND date >= ?2 ORDER BY date ASC",
        )?;
        let candidates = s
            .query_map(params![subject, set_ts - PENDING_SLACK], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let thread = candidates.into_iter().find_map(|(t, from)| {
            let from = from.unwrap_or_default().trim().to_ascii_lowercase();
            own.contains(&from).then_some(t)
        });
        match thread {
            Some(t) => {
                // A reminder of that kind already on the thread wins.
                let taken: bool = tx
                    .query_row(
                        "SELECT 1 FROM fork_reminders WHERE thread_id = ?1 AND kind = 'followup'",
                        [t],
                        |_| Ok(true),
                    )
                    .optional()?
                    .unwrap_or(false);
                if taken {
                    tx.execute("DELETE FROM fork_reminders WHERE id = ?1", [id])?;
                } else {
                    tx.execute(
                        "UPDATE fork_reminders SET thread_id = ?2 WHERE id = ?1",
                        params![id, t],
                    )?;
                    st.bound += 1;
                }
            }
            None if now - set_ts > PENDING_TTL => {
                tx.execute("DELETE FROM fork_reminders WHERE id = ?1", [id])?;
                st.dropped += 1;
            }
            None => {}
        }
    }

    // 2. Follow-ups someone answered, and 3. snoozes someone replied to.
    let live: Vec<(i64, String, i64, i64, i64)> = {
        let mut s = tx.prepare(
            "SELECT id, kind, thread_id, set_ts, due_ts FROM fork_reminders
             WHERE thread_id IS NOT NULL",
        )?;
        let rows = s
            .query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    for (id, kind, thread, set_ts, due_ts) in live {
        if !replied_since(&tx, thread, set_ts, &own)? {
            continue;
        }
        if kind == KIND_FOLLOWUP {
            tx.execute("DELETE FROM fork_reminders WHERE id = ?1", [id])?;
            st.answered += 1;
        } else if due_ts > now {
            tx.execute(
                "UPDATE fork_reminders SET due_ts = ?2 WHERE id = ?1",
                params![id, now],
            )?;
            st.woken += 1;
        }
    }
    tx.commit()?;
    Ok(st)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DueNote {
    pub kind: String,
    pub subject: String,
}

/// Reminders that fell due and have not been announced; marks them announced.
pub fn take_due(conn: &Connection, now: i64) -> rusqlite::Result<Vec<DueNote>> {
    let mut s = conn.prepare_cached(
        "SELECT r.id, r.kind,
                (SELECT m.subject FROM messages m WHERE m.thread_id = r.thread_id
                 ORDER BY m.date DESC LIMIT 1)
         FROM fork_reminders r
         WHERE r.thread_id IS NOT NULL AND r.due_ts <= ?1
           AND r.notified_ts IS NULL AND r.seen_ts IS NULL",
    )?;
    let rows = s
        .query_map([now], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut out = Vec::new();
    for (id, kind, subject) in rows {
        conn.execute(
            "UPDATE fork_reminders SET notified_ts = ?2 WHERE id = ?1",
            params![id, now],
        )?;
        out.push(DueNote {
            kind,
            subject: subject.unwrap_or_default(),
        });
    }
    Ok(out)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReminderRow {
    #[serde(flatten)]
    pub row: ThreadRow,
    pub kind: String,
    pub set_ts: i64,
    pub due_ts: i64,
}

/// One view's rows, shaped like the ordinary thread list (latest message).
/// Snoozed: soonest first. Follow-ups: most overdue first.
pub fn list(
    conn: &Connection,
    kind: &str,
    offset: i64,
    limit: i64,
) -> rusqlite::Result<Vec<ReminderRow>> {
    let mut stmt = conn.prepare_cached(
        "SELECT t.id,
                m.from_name, m.from_addr, m.subject, m.snippet, t.last_date,
                (NOT EXISTS (SELECT 1 FROM messages m3
                             WHERE m3.thread_id = t.id AND m3.is_read = 0)),
                t.starred,
                max(m.has_attachments), t.message_count, t.account_id,
                r.kind, r.set_ts, r.due_ts
         FROM fork_reminders r
         JOIN threads t ON t.id = r.thread_id
         JOIN messages m ON m.thread_id = t.id
         WHERE r.kind = ?3
           AND m.date = t.last_date
         GROUP BY t.id
         ORDER BY r.due_ts ASC, t.id ASC
         LIMIT ?1 OFFSET ?2",
    )?;
    let rows = stmt
        .query_map(params![limit, offset, kind], |r| {
            let from_name: Option<String> = r.get(1)?;
            let from_addr: Option<String> = r.get(2)?;
            Ok(ReminderRow {
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
                kind: r.get(11)?,
                set_ts: r.get(12)?,
                due_ts: r.get(13)?,
            })
        })?
        .collect();
    rows
}

#[derive(Debug, Clone, Copy, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReminderCounts {
    /// Snoozed threads not yet due.
    pub snoozed: i64,
    /// Follow-ups waiting on a reply (due or not).
    pub followups: i64,
    /// Of those, the ones past their date.
    pub followups_due: i64,
}

pub fn counts(conn: &Connection, now: i64) -> rusqlite::Result<ReminderCounts> {
    conn.query_row(
        "SELECT
           sum(kind = 'snooze' AND due_ts > ?1),
           sum(kind = 'followup' AND thread_id IS NOT NULL),
           sum(kind = 'followup' AND thread_id IS NOT NULL AND due_ts <= ?1)
         FROM fork_reminders",
        [now],
        |r| {
            Ok(ReminderCounts {
                snoozed: r.get::<_, Option<i64>>(0)?.unwrap_or(0),
                followups: r.get::<_, Option<i64>>(1)?.unwrap_or(0),
                followups_due: r.get::<_, Option<i64>>(2)?.unwrap_or(0),
            })
        },
    )
}

fn emit(app: &AppHandle, due: &[DueNote]) {
    let _ = app.emit(EVT_UPDATED, serde_json::json!({ "due": due }));
}

/// Sweep + announce, then emit when anything moved. Shared by the timer, the
/// mail listener and the commands.
async fn tick(app: &AppHandle) {
    let db = app.state::<AppState>().db.clone();
    let res = db
        .call(|conn| {
            let t = now();
            let st = sweep(conn, t)?;
            let due = take_due(conn, t)?;
            Ok((st, due))
        })
        .await;
    match res {
        Ok((st, due)) if st.changed() || !due.is_empty() => emit(app, &due),
        Ok(_) => {}
        Err(e) => tracing::warn!(error = %e, "reminder sweep failed"),
    }
}

/// Every minute, and after every `mail:updated`.
pub fn start(app: AppHandle) {
    use tauri::Listener;
    let app2 = app.clone();
    app.listen("mail:updated", move |_| {
        let app3 = app2.clone();
        tauri::async_runtime::spawn(async move { tick(&app3).await });
    });
    tauri::async_runtime::spawn(async move {
        loop {
            tick(&app).await;
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
    });
}

#[tauri::command]
pub async fn fork_reminder_set(
    app: AppHandle,
    state: State<'_, AppState>,
    thread_id: i64,
    kind: String,
    due_ts: i64,
) -> Result<()> {
    kind_ok(&kind)?;
    state
        .db
        .call(move |conn| set(conn, thread_id, &kind, due_ts, now()))
        .await?;
    emit(&app, &[]);
    Ok(())
}

/// A follow-up from the composer. A reply names its parent message (the thread
/// is known); a new email names its subject and binds once the Sent copy syncs.
#[tauri::command]
pub async fn fork_reminder_followup_on_send(
    app: AppHandle,
    state: State<'_, AppState>,
    reply_to_message_id: Option<i64>,
    subject: String,
    due_ts: i64,
) -> Result<()> {
    if reply_to_message_id.is_none() && subject.trim().is_empty() {
        return Err(SkimError::other("reminders", "a follow-up needs a subject"));
    }
    state
        .db
        .call(move |conn| {
            let t = now();
            let thread = match reply_to_message_id {
                Some(m) => thread_of(conn, m)?,
                None => None,
            };
            match thread {
                Some(th) => set(conn, th, KIND_FOLLOWUP, due_ts, t),
                None => set_pending(conn, &subject, due_ts, t),
            }
        })
        .await?;
    emit(&app, &[]);
    Ok(())
}

#[tauri::command]
pub async fn fork_reminder_clear(
    app: AppHandle,
    state: State<'_, AppState>,
    thread_id: i64,
    kind: String,
) -> Result<()> {
    kind_ok(&kind)?;
    state
        .db
        .call(move |conn| clear(conn, thread_id, &kind))
        .await?;
    emit(&app, &[]);
    Ok(())
}

#[tauri::command]
pub async fn fork_reminder_seen(
    app: AppHandle,
    state: State<'_, AppState>,
    thread_id: i64,
) -> Result<()> {
    let changed = state
        .db
        .call(move |conn| seen(conn, thread_id, now()))
        .await?;
    if changed {
        emit(&app, &[]);
    }
    Ok(())
}

#[tauri::command]
pub async fn fork_reminder_get(
    state: State<'_, AppState>,
    thread_id: i64,
) -> Result<Vec<Reminder>> {
    state
        .db
        .read("fork_reminder_get", move |conn| get(conn, thread_id))
        .await
}

#[tauri::command]
pub async fn fork_reminder_list(
    state: State<'_, AppState>,
    kind: String,
    offset: i64,
    limit: i64,
) -> Result<Vec<ReminderRow>> {
    kind_ok(&kind)?;
    state
        .db
        .read("fork_reminder_list", move |conn| {
            list(conn, &kind, offset, limit)
        })
        .await
}

#[tauri::command]
pub async fn fork_reminder_counts(state: State<'_, AppState>) -> Result<ReminderCounts> {
    state
        .db
        .read("fork_reminder_counts", |conn| counts(conn, now()))
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::{Address, NewMessage};
    use crate::db::{queries::insert_message, Db};

    const ME: &str = "me@example.com";

    fn seed(conn: &Connection) {
        conn.execute_batch(&format!(
            "INSERT INTO accounts (id, email, provider, imap_host, smtp_host, created_at)
               VALUES ('a1','{ME}','custom','imap.x','smtp.x',0);
             INSERT INTO folders (id, account_id, imap_name, role, display_name, sort_order)
               VALUES (1,'a1','INBOX','inbox','Inbox',0),
                      (2,'a1','Sent','sent','Sent',1);"
        ))
        .unwrap();
    }

    #[allow(clippy::too_many_arguments)]
    fn add(
        conn: &mut Connection,
        folder: i64,
        uid: u32,
        msgid: &str,
        refs: &[&str],
        from: &str,
        subject: &str,
        date: i64,
    ) -> (i64, i64) {
        insert_message(
            conn,
            &NewMessage {
                account_id: "a1".into(),
                folder_id: folder,
                uid,
                message_id: Some(format!("<{msgid}>")),
                references: refs.iter().map(|r| format!("<{r}>")).collect(),
                subject: Some(subject.into()),
                from_name: Some("X".into()),
                from_addr: Some(from.into()),
                to_addrs: vec![Address {
                    name: None,
                    addr: ME.into(),
                }],
                date,
                snippet: Some("hi".into()),
                ..Default::default()
            },
        )
        .unwrap()
        .expect("inserted")
    }

    fn inbox_ids(conn: &Connection) -> Vec<i64> {
        crate::db::queries::list_threads(conn, 1, 0, 50)
            .unwrap()
            .into_iter()
            .map(|t| t.id)
            .collect()
    }

    #[test]
    fn snooze_hides_until_due_then_pins_until_opened() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            seed(conn);
            let (_, old) = add(conn, 1, 1, "a1", &[], "anna@x.com", "Old", 100);
            let (_, new) = add(conn, 1, 2, "b1", &[], "bob@x.com", "New", 200);
            let t = now();
            assert_eq!(inbox_ids(conn), vec![new, old]);

            set(conn, old, KIND_SNOOZE, t + 3600, t).unwrap();
            assert_eq!(inbox_ids(conn), vec![new], "hidden while snoozed");
            assert_eq!(counts(conn, t).unwrap().snoozed, 1);

            conn.execute("UPDATE fork_reminders SET due_ts = ?1", [t - 1])
                .unwrap();
            assert_eq!(inbox_ids(conn), vec![old, new], "back and pinned on top");
            assert_eq!(take_due(conn, t).unwrap().len(), 1);
            assert!(take_due(conn, t).unwrap().is_empty(), "announced once");

            assert!(seen(conn, old, t).unwrap());
            assert_eq!(
                inbox_ids(conn),
                vec![new, old],
                "opened: back in date order"
            );
            assert!(get(conn, old).unwrap().is_empty(), "a seen snooze is done");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn a_reply_answers_a_followup_and_wakes_a_snooze() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            seed(conn);
            let t = now();
            let (_, th) = add(conn, 2, 1, "s1", &[], ME, "Pricing", t - 100);
            set(conn, th, KIND_FOLLOWUP, t + 86_400, t - 50).unwrap();
            set(conn, th, KIND_SNOOZE, t + 86_400, t - 50).unwrap();

            // His own second nudge answers nothing.
            add(conn, 2, 2, "s2", &["s1"], ME, "Re: Pricing", t - 10);
            let st = sweep(conn, t).unwrap();
            assert_eq!((st.answered, st.woken), (0, 0));

            add(
                conn,
                1,
                3,
                "r1",
                &["s1"],
                "anna@x.com",
                "Re: Pricing",
                t - 5,
            );
            let st = sweep(conn, t).unwrap();
            assert_eq!((st.answered, st.woken), (1, 1));
            let left = get(conn, th).unwrap();
            assert_eq!(left.len(), 1);
            assert_eq!((left[0].kind.as_str(), left[0].due_ts), (KIND_SNOOZE, t));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn a_new_emails_followup_binds_to_its_sent_copy() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            seed(conn);
            let t = now();
            set_pending(conn, "Intro to Wes", t + 86_400, t).unwrap();
            assert_eq!(sweep(conn, t + 2).unwrap().bound, 0, "not synced yet");
            let (_, th) = add(conn, 2, 2, "y1", &[], ME, "Intro to Wes", t + 3);
            assert_eq!(sweep(conn, t + 4).unwrap().bound, 1);
            assert_eq!(get(conn, th).unwrap()[0].kind, KIND_FOLLOWUP);
            assert_eq!(counts(conn, t + 4).unwrap().followups, 1);

            set_pending(conn, "Never sent", t, t - PENDING_TTL - 1).unwrap();
            assert_eq!(sweep(conn, t).unwrap().dropped, 1, "stale pending goes");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn list_orders_by_due_and_rejects_unknown_kinds() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            seed(conn);
            let t = now();
            let (_, a) = add(conn, 2, 1, "a1", &[], ME, "A", 100);
            let (_, b) = add(conn, 2, 2, "b1", &[], ME, "B", 200);
            set(conn, a, KIND_FOLLOWUP, t + 500, t).unwrap();
            set(conn, b, KIND_FOLLOWUP, t - 500, t).unwrap();
            let rows = list(conn, KIND_FOLLOWUP, 0, 10).unwrap();
            assert_eq!(
                rows.iter().map(|r| r.row.id).collect::<Vec<_>>(),
                vec![b, a]
            );
            assert_eq!(counts(conn, t).unwrap().followups_due, 1);
            assert!(set(conn, a, "bogus", t, t).is_err());
            Ok(())
        })
        .unwrap();
    }
}
