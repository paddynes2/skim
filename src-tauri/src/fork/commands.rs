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

// ---- Phase 3.2: undo ----------------------------------------------------

/// What the webview keeps before a removal so it can be undone after the op
/// has run: one snapshot per (account, folder) the messages live in.
#[tauri::command]
pub async fn fork_removal_snapshot(
    state: State<'_, AppState>,
    message_ids: Vec<i64>,
) -> Result<Vec<crate::fork::restore::RemovalSnapshot>> {
    state
        .db
        .read("fork_removal_snapshot", move |conn| {
            removal_snapshot(conn, &message_ids)
        })
        .await
}

pub fn removal_snapshot(
    conn: &rusqlite::Connection,
    message_ids: &[i64],
) -> rusqlite::Result<Vec<crate::fork::restore::RemovalSnapshot>> {
    use rusqlite::OptionalExtension;
    let mut by_folder: std::collections::BTreeMap<i64, crate::fork::restore::RemovalSnapshot> =
        Default::default();
    let mut stmt = conn.prepare_cached(
        "SELECT m.folder_id, m.account_id, f.imap_name, m.message_id
         FROM messages m JOIN folders f ON f.id = m.folder_id
         WHERE m.id = ?1",
    )?;
    for id in message_ids {
        let row: Option<(i64, String, String, Option<String>)> = stmt
            .query_row(rusqlite::params![id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .optional()?;
        let Some((folder_id, account_id, imap_name, message_id)) = row else {
            continue;
        };
        let entry =
            by_folder
                .entry(folder_id)
                .or_insert_with(|| crate::fork::restore::RemovalSnapshot {
                    account_id,
                    folder_id,
                    folder_imap_name: imap_name,
                    message_ids: Vec::new(),
                });
        if let Some(mid) = message_id.filter(|m| !m.is_empty()) {
            entry.message_ids.push(mid);
        }
    }
    Ok(by_folder.into_values().collect())
}

/// Queue a `fork_restore` op for a snapshot and run the queue. `kind` is the
/// removal that happened (archive | delete | spam | move); `dest_imap_name`
/// only for a move.
#[tauri::command]
pub async fn fork_restore(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    snapshot: crate::fork::restore::RemovalSnapshot,
    kind: String,
    dest_imap_name: Option<String>,
) -> Result<()> {
    use tauri::Emitter;
    if snapshot.message_ids.is_empty() {
        return Err(crate::error::SkimError::other(
            "restore",
            "nothing to restore: no Message-IDs",
        ));
    }
    let payload = serde_json::json!({
        "sourceImapName": snapshot.folder_imap_name,
        "sourceFolderId": snapshot.folder_id,
        "messageIds": snapshot.message_ids,
        "kind": kind,
        "destImapName": dest_imap_name,
    });
    let account_id = snapshot.account_id.clone();
    let aid = account_id.clone();
    state
        .db
        .call(move |conn| {
            crate::db::bodies::enqueue_op(conn, &aid, crate::fork::restore::OP_KIND, &payload)
        })
        .await?;
    if let Some(handle) = state.engines.lock().await.get(&account_id) {
        handle.run_ops();
    }
    let _ = app.emit("mail:updated", serde_json::json!({}));
    Ok(())
}

#[cfg(test)]
mod undo_tests {
    use super::*;
    use crate::db::Db;

    #[test]
    fn snapshot_groups_by_folder_and_keeps_only_message_ids() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            conn.execute_batch(
                "INSERT INTO accounts (id, email, provider, imap_host, smtp_host, created_at)
                   VALUES ('a1','a@x','gmail','imap.gmail.com','smtp.gmail.com',0);
                 INSERT INTO folders (id, account_id, imap_name, role, display_name, sort_order)
                   VALUES (1,'a1','INBOX','inbox','Inbox',0),(2,'a1','Clients',NULL,'Clients',1);
                 INSERT INTO messages (id, account_id, folder_id, uid, date, message_id)
                   VALUES (1,'a1',1,1,10,'<a@x>'),(2,'a1',1,2,11,NULL),(3,'a1',2,1,12,'<c@x>');",
            )?;
            let snaps = removal_snapshot(conn, &[1, 2, 3, 99])?;
            assert_eq!(snaps.len(), 2);
            assert_eq!(snaps[0].folder_imap_name, "INBOX");
            assert_eq!(snaps[0].message_ids, vec!["<a@x>"]);
            assert_eq!(snaps[1].folder_imap_name, "Clients");
            assert_eq!(snaps[1].message_ids, vec!["<c@x>"]);
            Ok(())
        })
        .unwrap();
    }
}
