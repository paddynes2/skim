//! Durable mail queue status. Retrying does not create a new operation.
use crate::error::{Result, SkimError};
use crate::state::AppState;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::Value;
use tauri::State;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueStatus {
    pub pending: i64,
    pub scheduled: i64,
    pub failed: i64,
    pub failures: Vec<Failure>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Failure {
    pub id: i64,
    pub kind: String,
    pub action: String,
    pub account_email: String,
    pub created_at: i64,
    pub retryable: bool,
    pub locations: Vec<FailureLocation>,
    pub draft_id: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FailureLocation {
    pub message_id: i64,
    pub thread_id: Option<i64>,
    pub folder_id: i64,
    pub subject: String,
}

fn failure_locations(
    conn: &Connection,
    account: &str,
    payload: &str,
) -> rusqlite::Result<(Vec<FailureLocation>, Option<i64>)> {
    let Ok(value) = serde_json::from_str::<Value>(payload) else {
        return Ok((vec![], None));
    };
    let draft = if let Some(id) = value.get("draftId").and_then(Value::as_i64) {
        conn.query_row(
            "SELECT id FROM drafts WHERE id=?1 AND account_id=?2",
            params![id, account],
            |r| r.get(0),
        )
        .optional()?
    } else {
        None
    };
    let mut locations = vec![];
    if let (Some(folder), Some(uids)) = (
        value.get("imapName").and_then(Value::as_str),
        value.get("uids").and_then(Value::as_array),
    ) {
        let mut query = conn.prepare("SELECT m.id,m.thread_id,m.folder_id,COALESCE(m.subject,'') FROM messages m JOIN folders f ON f.id=m.folder_id WHERE m.account_id=?1 AND f.account_id=?1 AND f.imap_name=?2 AND m.uid=?3")?;
        for uid in uids.iter().filter_map(Value::as_i64).take(20) {
            if let Some(location) = query
                .query_row(params![account, folder, uid], |r| {
                    Ok(FailureLocation {
                        message_id: r.get(0)?,
                        thread_id: r.get(1)?,
                        folder_id: r.get(2)?,
                        subject: r.get(3)?,
                    })
                })
                .optional()?
            {
                locations.push(location);
            }
        }
    }
    Ok((locations, draft))
}

/// Only explicit flag assignments can be repeated from this surface.
/// Sends, RSVP, moves, deletions, folders, drafts and unknown operations stay failed.
fn flag_action(kind: &str, payload: &str) -> Option<&'static str> {
    if kind != "set_flag" {
        return None;
    }
    let p: Value = serde_json::from_str(payload).ok()?;
    let on = p.get("on")?.as_bool()?;
    let flag = p.get("flag")?.as_str()?;
    let uids = p.get("uids")?.as_array()?;
    if uids.is_empty()
        || !uids
            .iter()
            .all(|v| v.as_u64().is_some_and(|n| n > 0 && n <= u32::MAX as u64))
        || p.get("imapName")?.as_str()?.is_empty()
    {
        return None;
    }
    match (flag, on) {
        ("seen", true) => Some("read"),
        ("seen", false) => Some("unread"),
        ("flagged", true) => Some("star"),
        ("flagged", false) => Some("unstar"),
        _ => None,
    }
}

pub fn status(conn: &Connection, account_id: Option<&str>) -> rusqlite::Result<QueueStatus> {
    let (pending, scheduled, failed) = conn.query_row(
        "SELECT COALESCE(sum(state='pending' AND NOT EXISTS(SELECT 1 FROM fork_op_schedule s WHERE s.op_id=p.id AND s.not_before>unixepoch())),0), COALESCE(sum(state='pending' AND EXISTS(SELECT 1 FROM fork_op_schedule s WHERE s.op_id=p.id AND s.not_before>unixepoch())),0), COALESCE(sum(state='failed'),0) FROM pending_ops p WHERE (?1 IS NULL OR account_id=?1)",
        [account_id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
    let mut stmt=conn.prepare("SELECT p.id,p.kind,p.payload,a.email,p.created_at,p.account_id FROM pending_ops p JOIN accounts a ON a.id=p.account_id WHERE p.state='failed' AND (?1 IS NULL OR p.account_id=?1) ORDER BY p.created_at DESC,p.id DESC LIMIT 30")?;
    let failures = stmt
        .query_map([account_id], |r| {
            let kind: String = r.get(1)?;
            let payload: String = r.get(2)?;
            let action = flag_action(&kind, &payload);
            let (locations, draft_id) = failure_locations(conn, &r.get::<_, String>(5)?, &payload)?;
            Ok(Failure {
                id: r.get(0)?,
                action: action.unwrap_or(&kind).to_string(),
                retryable: action.is_some(),
                kind,
                account_email: r.get(3)?,
                created_at: r.get(4)?,
                locations,
                draft_id,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(QueueStatus {
        pending,
        scheduled,
        failed,
        failures,
    })
}

/// Re-check the operation inside the transaction. An unknown or changed row stays untouched.
fn retry_local(conn: &mut Connection, id: i64, account_id: &str) -> rusqlite::Result<bool> {
    let tx = conn.transaction()?;
    let row: Option<(String, String)> = tx
        .query_row(
            "SELECT kind,payload FROM pending_ops WHERE id=?1 AND account_id=?2 AND state='failed'",
            params![id, account_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((kind, payload)) = row else {
        return Ok(false);
    };
    if flag_action(&kind, &payload).is_none() {
        return Ok(false);
    }
    tx.execute(
        "UPDATE pending_ops SET state='pending',attempts=0 WHERE id=?1 AND state='failed'",
        [id],
    )?;
    tx.commit()?;
    Ok(true)
}
#[tauri::command]
pub async fn fork_mail_queue_status(
    state: State<'_, AppState>,
    account_id: Option<String>,
) -> Result<QueueStatus> {
    state
        .db
        .read("fork_mail_queue_status", move |conn| {
            status(conn, account_id.as_deref())
        })
        .await
}
#[tauri::command]
pub async fn fork_mail_retry_flag(state: State<'_, AppState>, id: i64) -> Result<()> {
    let account: Option<String> = state
        .db
        .read("fork_mail_retry_flag", move |conn| {
            conn.query_row(
                "SELECT account_id FROM pending_ops WHERE id=?1 AND state='failed'",
                [id],
                |r| r.get(0),
            )
            .optional()
        })
        .await?;
    let Some(account) = account else {
        return Err(SkimError::other(
            "queue_changed",
            "This failed action is no longer available.",
        ));
    };
    let engines = state.engines.lock().await;
    let handle = engines
        .get(&account)
        .filter(|h| !h.tx.is_closed())
        .ok_or_else(|| {
            SkimError::other(
                "offline",
                "Connect this mail account before retrying the action.",
            )
        })?;
    if !state
        .db
        .call(move |conn| retry_local(conn, id, &account))
        .await?
    {
        return Err(SkimError::other(
            "retry_unavailable",
            "This action cannot be retried from the queue.",
        ));
    }
    handle.run_ops();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failure_links_stay_in_the_operation_account_and_survive_missing_sources() {
        let db = crate::db::Db::open_in_memory().unwrap();
        db.with(|conn| {
            conn.execute_batch("INSERT INTO accounts(id,email,provider,imap_host,smtp_host,created_at) VALUES('a','a@example.com','custom','i','s',0),('b','b@example.com','custom','i','s',0);
                INSERT INTO folders(id,account_id,imap_name,display_name) VALUES(1,'a','INBOX','Inbox'),(2,'b','INBOX','Inbox');
                INSERT INTO messages(id,account_id,folder_id,uid,date,subject) VALUES(1,'a',1,7,0,'First'),(2,'b',2,7,0,'Other account');
                INSERT INTO drafts(id,account_id,updated_at) VALUES(1,'a',0);")?;
            let payload = r#"{"imapName":"INBOX","uids":[7,999],"draftId":1}"#;
            let (locations, draft) = failure_locations(conn, "a", payload)?;
            assert_eq!(locations.len(), 1);
            assert_eq!(locations[0].message_id, 1);
            assert_eq!(locations[0].subject, "First");
            assert_eq!(draft, Some(1));
            let (other, draft) = failure_locations(conn, "b", payload)?;
            assert_eq!(other[0].message_id, 2);
            assert_eq!(draft, None);
            assert!(failure_locations(conn, "missing", payload)?.0.is_empty());
            assert!(failure_locations(conn, "a", "invalid json")?.0.is_empty());
            Ok(())
        }).unwrap();
    }
    #[test]
    fn queue_retry_only_requeues_valid_failed_flag_and_keeps_send_failed() {
        let db = crate::db::Db::open_in_memory().unwrap();
        db.with(|conn| {
            conn.execute_batch("INSERT INTO accounts(id,email,provider,imap_host,smtp_host,created_at) VALUES('a','me@example.com','custom','i','s',0);")?;
            let flag=r#"{"imapName":"INBOX","uids":[1,2],"flag":"seen","on":true}"#;
            for (kind,payload,state) in [("set_flag",flag,"failed"),("send","{}","failed"),("set_flag","{}","failed"),("set_flag",flag,"pending")] {
                conn.execute("INSERT INTO pending_ops(account_id,kind,payload,state,created_at,attempts) VALUES('a',?1,?2,?3,0,5)",params![kind,payload,state])?;
            }
            conn.execute("INSERT INTO fork_op_schedule(op_id,not_before,kind) VALUES(4,unixepoch()+3600,'set_flag')",[])?;
            let before=status(conn,Some("a"))?;
            assert_eq!(before.failed,3);assert_eq!(before.scheduled,1);assert_eq!(before.pending,0);
            assert_eq!(before.failures.iter().filter(|f|f.retryable).count(),1);
            assert!(!retry_local(conn,2,"a")?);assert!(!retry_local(conn,3,"a")?);assert!(!retry_local(conn,1,"other")?);
            assert!(retry_local(conn,1,"a")?);assert!(!retry_local(conn,1,"a")?);
            let after=status(conn,None)?;
            assert_eq!(after.failed,2);assert_eq!(after.pending,1);assert_eq!(after.scheduled,1);
            assert_eq!(conn.query_row("SELECT attempts FROM pending_ops WHERE id=1",[],|r|r.get::<_,i64>(0))?,0);
            assert_eq!(status(conn,Some("missing"))?.failed,0);
            for kind in ["rsvp","unsubscribe","save_draft","move","delete","archive","rename_folder","delete_folder","unknown"] {assert!(flag_action(kind,flag).is_none());}
            assert_eq!(flag_action("set_flag",flag),Some("read"));
            Ok(())
        }).unwrap();
    }
}
