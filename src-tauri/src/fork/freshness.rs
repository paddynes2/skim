//! Fork (3.4): Sent and Drafts update in real time.
//!
//! IDLE watches INBOX only; every other folder waits for the five-minute
//! poll. This module closes the two gaps a user actually sees: a message just
//! sent (Gmail files the copy itself) and a draft just saved. Two hooks:
//!
//! - `folders_after_op`: the sync engine asks which role folders an op of a
//!   given kind changed on the server, and resyncs them in the same drain.
//! - `fork_sync_folder`: the UI asks for one targeted sync when Sent or Drafts
//!   is opened or the window regains focus, debounced per folder.

use crate::error::Result;
use crate::state::AppState;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::State;

/// A folder is synced on request at most once per this window.
pub const DEBOUNCE: Duration = Duration::from_secs(20);

/// Which role folders an op of `kind` leaves changed on the server once it has
/// landed. Sending removes the server draft too, so Drafts follows Sent.
pub fn roles_after_op(kind: &str) -> &'static [&'static str] {
    match kind {
        "send" => &["sent", "drafts"],
        "rsvp" => &["sent"],
        "save_draft" => &["drafts"],
        _ => &[],
    }
}

/// Ids of every folder with `role`, in one account or (`None`) all of them.
pub fn folder_ids_by_role(
    conn: &rusqlite::Connection,
    account_id: Option<&str>,
    role: &str,
) -> rusqlite::Result<Vec<i64>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id FROM folders WHERE role = ?1 AND (?2 IS NULL OR account_id = ?2) ORDER BY id",
    )?;
    let rows = stmt.query_map(rusqlite::params![role, account_id], |r| r.get(0))?;
    rows.collect()
}

/// The folder ids the engine should resync after an op of `kind` succeeded
/// for `account_id`. Pure over the connection so it is testable in memory.
pub fn folders_after_op(
    conn: &rusqlite::Connection,
    account_id: &str,
    kind: &str,
) -> rusqlite::Result<Vec<i64>> {
    let mut out = Vec::new();
    for role in roles_after_op(kind) {
        out.extend(folder_ids_by_role(conn, Some(account_id), role)?);
    }
    Ok(out)
}

/// Engine-side wrapper: a DB error here only costs a resync, never the op.
pub async fn folders_after_op_db(db: &crate::db::Db, account_id: &str, kind: &str) -> Vec<i64> {
    let account_id = account_id.to_string();
    let kind = kind.to_string();
    db.call(move |conn| folders_after_op(conn, &account_id, &kind))
        .await
        .unwrap_or_default()
}

/// What `fork_sync_folder` syncs: real folders as `(account_id, folder_id)`.
/// A non-negative id names one folder; a negative id is a unified virtual
/// folder, which stands for every real folder with `role` across accounts.
pub fn resolve_targets(
    conn: &rusqlite::Connection,
    folder_id: i64,
    role: Option<&str>,
) -> rusqlite::Result<Vec<(String, i64)>> {
    use rusqlite::OptionalExtension;
    if folder_id >= 0 {
        let row: Option<(String, i64)> = conn
            .query_row(
                "SELECT account_id, id FROM folders WHERE id = ?1",
                rusqlite::params![folder_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        return Ok(row.into_iter().collect());
    }
    let Some(role) = role else {
        return Ok(Vec::new());
    };
    let mut stmt =
        conn.prepare_cached("SELECT account_id, id FROM folders WHERE role = ?1 ORDER BY id")?;
    let rows = stmt.query_map(rusqlite::params![role], |r| Ok((r.get(0)?, r.get(1)?)))?;
    rows.collect()
}

/// Per-folder "last requested" clock. Admits a folder once per window; a
/// refused request is dropped, not queued, because the sync it wanted either
/// just ran or is about to.
#[derive(Default)]
pub struct Debounce {
    last: Mutex<HashMap<i64, Instant>>,
}

impl Debounce {
    pub fn admit(&self, folder_id: i64, now: Instant, window: Duration) -> bool {
        let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
        match last.get(&folder_id) {
            Some(prev) if now.duration_since(*prev) < window => false,
            _ => {
                last.insert(folder_id, now);
                true
            }
        }
    }
}

fn debounce() -> &'static Debounce {
    static DEBOUNCE_STATE: OnceLock<Debounce> = OnceLock::new();
    DEBOUNCE_STATE.get_or_init(Debounce::default)
}

/// One targeted sync of a folder, at most once per 20 s per folder. Pass a
/// negative `folder_id` with `role` for a unified virtual folder.
#[tauri::command]
pub async fn fork_sync_folder(
    state: State<'_, AppState>,
    folder_id: i64,
    role: Option<String>,
) -> Result<()> {
    let targets = state
        .db
        .read("fork_sync_folder", move |conn| {
            resolve_targets(conn, folder_id, role.as_deref())
        })
        .await?;
    let now = Instant::now();
    let engines = state.engines.lock().await;
    for (account_id, id) in targets {
        if !debounce().admit(id, now, DEBOUNCE) {
            continue;
        }
        if let Some(handle) = engines.get(&account_id) {
            handle.sync_folder(id);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    fn seed(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
        conn.execute_batch(
            "INSERT INTO accounts (id, email, provider, imap_host, smtp_host, created_at)
               VALUES ('a1','a@x','gmail','imap.gmail.com','smtp.gmail.com',0),
                      ('a2','b@x','custom','imap.y','smtp.y',0);
             INSERT INTO folders (id, account_id, imap_name, role, display_name, sort_order)
               VALUES (1,'a1','INBOX','inbox','Inbox',0),
                      (2,'a1','[Gmail]/Sent Mail','sent','Sent',1),
                      (3,'a1','[Gmail]/Drafts','drafts','Drafts',2),
                      (4,'a2','INBOX','inbox','Inbox',0),
                      (5,'a2','Sent','sent','Sent',1),
                      (6,'a2','Drafts','drafts','Drafts',2),
                      (7,'a2','Clients',NULL,'Clients',3);",
        )
    }

    #[test]
    fn roles_follow_the_op_kind() {
        assert_eq!(roles_after_op("send"), &["sent", "drafts"]);
        assert_eq!(roles_after_op("save_draft"), &["drafts"]);
        assert_eq!(roles_after_op("rsvp"), &["sent"]);
        assert!(roles_after_op("archive").is_empty());
        assert!(roles_after_op("").is_empty());
    }

    #[test]
    fn affected_folders_after_send_and_save_draft() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            seed(conn)?;
            // Gmail account: sent is included even though nothing was appended.
            assert_eq!(folders_after_op(conn, "a1", "send")?, vec![2, 3]);
            assert_eq!(folders_after_op(conn, "a2", "send")?, vec![5, 6]);
            assert_eq!(folders_after_op(conn, "a1", "save_draft")?, vec![3]);
            assert_eq!(folders_after_op(conn, "a2", "rsvp")?, vec![5]);
            assert!(folders_after_op(conn, "a1", "archive")?.is_empty());
            // An account with no such folders yields nothing, not an error.
            assert!(folders_after_op(conn, "nope", "send")?.is_empty());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn targets_one_real_folder_or_every_folder_of_a_role() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            seed(conn)?;
            assert_eq!(resolve_targets(conn, 5, None)?, vec![("a2".into(), 5)]);
            // A role passed with a real id is ignored: the id wins.
            assert_eq!(
                resolve_targets(conn, 3, Some("sent"))?,
                vec![("a1".into(), 3)]
            );
            assert!(resolve_targets(conn, 99, None)?.is_empty());
            assert_eq!(
                resolve_targets(conn, -3, Some("sent"))?,
                vec![("a1".into(), 2), ("a2".into(), 5)]
            );
            assert_eq!(
                resolve_targets(conn, -4, Some("drafts"))?,
                vec![("a1".into(), 3), ("a2".into(), 6)]
            );
            assert!(resolve_targets(conn, -1, None)?.is_empty());
            assert!(resolve_targets(conn, -1, Some("nothing"))?.is_empty());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn debounce_admits_once_per_window_per_folder() {
        let d = Debounce::default();
        let t0 = Instant::now();
        assert!(d.admit(2, t0, DEBOUNCE));
        assert!(!d.admit(2, t0 + Duration::from_secs(5), DEBOUNCE));
        assert!(!d.admit(2, t0 + Duration::from_secs(19), DEBOUNCE));
        // Another folder is its own clock.
        assert!(d.admit(3, t0 + Duration::from_secs(5), DEBOUNCE));
        // The window is measured from the last ADMITTED request, so a refused
        // burst never pushes the next admission further out.
        assert!(d.admit(2, t0 + Duration::from_secs(20), DEBOUNCE));
        assert!(!d.admit(2, t0 + Duration::from_secs(30), DEBOUNCE));
        assert!(d.admit(3, t0 + Duration::from_secs(25), DEBOUNCE));
    }
}
