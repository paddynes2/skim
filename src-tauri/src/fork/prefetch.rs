//! Opening mail fast (v1.1.12, D65). Measured on the owner's machine over a
//! week: 11 opens took over 4 s; six were large bodies nobody had downloaded,
//! three were a fetch connection that had died since its last use.
//!
//! Two remedies, both on top of the sync engine's three connections:
//! - [`start`]: while the window is on screen, wake every account's fetch
//!   connection every [`KEEPALIVE`]. `warm` is free when the session is warm,
//!   and once it has been idle past the engine's probe age it spends one NOOP,
//!   so a dead session is found and replaced in the background, not under a
//!   click.
//! - [`fork_prefetch_threads`]: the list asks for the rows it shows; their
//!   newest uncached message goes to the background connection
//!   (`SyncHandle::prefetch`), never ahead of a click.

use crate::error::Result;
use crate::state::AppState;
use rusqlite::{params, Connection};
use std::collections::BTreeMap;
use std::time::Duration;
use tauri::{AppHandle, Manager, State};

/// How often a visible window wakes the fetch connections.
const KEEPALIVE: Duration = Duration::from_secs(45);
/// Rows one request may ask about.
pub const MAX_THREADS: usize = 60;

/// Keep the fetch connections warm while the main window is on screen.
pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut tick = tokio::time::interval(KEEPALIVE);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            let shown = app.get_webview_window("main").is_some_and(|w| {
                w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(true)
            });
            if !shown {
                continue;
            }
            for handle in app.state::<AppState>().engines.lock().await.values() {
                handle.warm_fetch();
            }
        }
    });
}

/// For each thread, its newest message outside Trash and Spam whose body is
/// not cached yet, grouped by account. At most [`MAX_THREADS`] threads.
pub fn uncached_newest(
    conn: &Connection,
    thread_ids: &[i64],
) -> rusqlite::Result<BTreeMap<String, Vec<i64>>> {
    let mut stmt = conn.prepare_cached(
        "SELECT m.id, m.account_id, m.body_state FROM messages m
         JOIN folders f ON f.id = m.folder_id
         WHERE m.thread_id = ?1 AND (f.role IS NULL OR f.role NOT IN ('trash', 'junk'))
         ORDER BY m.date DESC, m.id DESC LIMIT 1",
    )?;
    let mut out: BTreeMap<String, Vec<i64>> = BTreeMap::new();
    for t in thread_ids.iter().take(MAX_THREADS) {
        let row = stmt
            .query_map(params![t], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })?
            .next()
            .transpose()?;
        if let Some((id, account, 0)) = row {
            out.entry(account).or_default().push(id);
        }
    }
    Ok(out)
}

/// Download the shown rows' bodies ahead. Returns how many were queued.
#[tauri::command]
pub async fn fork_prefetch_threads(
    state: State<'_, AppState>,
    thread_ids: Vec<i64>,
) -> Result<usize> {
    let wanted = state
        .db
        .read("fork_prefetch_threads", move |conn| {
            uncached_newest(conn, &thread_ids)
        })
        .await?;
    let engines = state.engines.lock().await;
    let mut queued = 0;
    for (account, pks) in wanted {
        if let Some(handle) = engines.get(&account) {
            queued += pks.len();
            handle.prefetch(pks);
        }
    }
    Ok(queued)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::NewMessage;
    use crate::db::queries::insert_message;

    #[test]
    fn only_the_newest_uncached_message_of_each_thread_is_asked_for() {
        let db = crate::db::Db::open_in_memory().unwrap();
        db.with(|conn| {
            conn.execute_batch(
                "INSERT INTO accounts (id, email, provider, imap_host, smtp_host, created_at)
                   VALUES ('a1','me@example.com','custom','imap.x','smtp.x',0);
                 INSERT INTO folders (id, account_id, imap_name, role, display_name, sort_order)
                   VALUES (1,'a1','INBOX','inbox','Inbox',0), (2,'a1','Trash','trash','Trash',1);",
            )?;
            let add = |conn: &mut Connection, folder, uid, id: &str, refs: Vec<String>, date| {
                insert_message(
                    conn,
                    &NewMessage {
                        account_id: "a1".into(),
                        folder_id: folder,
                        uid,
                        message_id: Some(format!("<{id}>")),
                        references: refs,
                        subject: Some(format!("s-{id}")),
                        from_addr: Some("jo@x.example".into()),
                        date,
                        ..Default::default()
                    },
                )
                .unwrap()
                .expect("inserted")
            };
            let (old, t1) = add(conn, 1, 1, "a1@x", vec![], 100);
            let (newest, _) = add(conn, 1, 2, "a2@x", vec!["<a1@x>".into()], 200);
            let (cached, t2) = add(conn, 1, 3, "b1@x", vec![], 300);
            conn.execute(
                "UPDATE messages SET body_state = 1 WHERE id = ?1",
                params![cached],
            )?;
            let (_, t3) = add(conn, 2, 4, "c1@x", vec![], 400);
            let got = uncached_newest(conn, &[t1, t2, t3, 999])?;
            assert_eq!(got.get("a1"), Some(&vec![newest]));
            assert_ne!(newest, old);
            Ok(())
        })
        .unwrap();
    }
}
