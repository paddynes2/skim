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
    let mut stmt=conn.prepare("SELECT p.id,p.kind,p.payload,a.email,p.created_at FROM pending_ops p JOIN accounts a ON a.id=p.account_id WHERE p.state='failed' AND (?1 IS NULL OR p.account_id=?1) ORDER BY p.created_at DESC,p.id DESC LIMIT 30")?;
    let failures = stmt
        .query_map([account_id], |r| {
            let kind: String = r.get(1)?;
            let payload: String = r.get(2)?;
            let action = flag_action(&kind, &payload);
            Ok(Failure {
                id: r.get(0)?,
                action: action.unwrap_or(&kind).to_string(),
                retryable: action.is_some(),
                kind,
                account_email: r.get(3)?,
                created_at: r.get(4)?,
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
