//! Fork IPC commands. Every entry is appended to `generate_handler!` in
//! `lib.rs`; the frontend wrappers live in `src/fork/api.ts`.

use crate::error::Result;
use crate::state::AppState;
use tauri::State;

/// Total messages in every folder with `role` (one account, or all of them
/// when `account_id` is `None`). The sidebar shows Starred by its total, not
/// its unread count.
#[tauri::command]
pub async fn fork_role_total(
    state: State<'_, AppState>,
    role: String,
    account_id: Option<String>,
) -> Result<i64> {
    state
        .db
        .read("fork_role_total", move |conn| {
            role_total(conn, &role, account_id.as_deref())
        })
        .await
}

pub fn role_total(
    conn: &rusqlite::Connection,
    role: &str,
    account_id: Option<&str>,
) -> rusqlite::Result<i64> {
    conn.query_row(
        "SELECT count(*) FROM messages m JOIN folders f ON f.id = m.folder_id
         WHERE f.role = ?1 AND (?2 IS NULL OR f.account_id = ?2)",
        rusqlite::params![role, account_id],
        |r| r.get(0),
    )
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
               VALUES (1,'a1','[Gmail]/Starred','starred','Starred',1),
                      (2,'a2','Flagged','starred','Flagged',1),
                      (3,'a1','INBOX','inbox','Inbox',0);
             INSERT INTO messages (id, account_id, folder_id, uid, date, is_read, is_starred)
               VALUES (1,'a1',1,1,10,1,1),(2,'a1',1,2,11,1,1),(3,'a2',2,1,12,0,1),(4,'a1',3,3,13,0,0);",
        )
    }

    #[test]
    fn counts_by_role_per_account_and_overall() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            seed(conn)?;
            assert_eq!(role_total(conn, "starred", Some("a1"))?, 2);
            assert_eq!(role_total(conn, "starred", Some("a2"))?, 1);
            assert_eq!(role_total(conn, "starred", None)?, 3);
            assert_eq!(role_total(conn, "inbox", None)?, 1);
            assert_eq!(role_total(conn, "junk", None)?, 0);
            Ok(())
        })
        .unwrap();
    }
}
