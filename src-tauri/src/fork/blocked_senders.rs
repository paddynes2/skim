//! Exact sender blocks, scoped to one mailbox. Rules are local to Skim.
use crate::commands::mail::queue_op_local;
use crate::db::{bodies, queries};
use crate::error::{Result, SkimError};
use crate::state::AppState;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::json;
use std::collections::BTreeMap;
use tauri::{AppHandle, Emitter, Manager, State};

const SETTING: &str = "fork_blocked_senders";
const BATCH_LIMIT: i64 = 200;
type Rules = BTreeMap<String, Vec<String>>;

fn rules(conn: &Connection) -> rusqlite::Result<Rules> {
    let raw = queries::get_setting(conn, SETTING)?.unwrap_or_else(|| "{}".into());
    serde_json::from_str(&raw).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })
}

fn write_rules(conn: &Connection, value: &Rules) -> rusqlite::Result<()> {
    queries::set_setting(conn, SETTING, &json!(value).to_string())
}

fn block_message(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    let (account, sender, own): (String, String, bool) = conn.query_row(
        "SELECT m.account_id, lower(trim(coalesce(m.from_addr,''))),
         EXISTS(SELECT 1 FROM accounts a WHERE lower(a.email)=lower(trim(m.from_addr)))
         FROM messages m JOIN folders f ON f.id=m.folder_id
         WHERE m.id=?1 AND coalesce(f.role,'') NOT IN ('sent','drafts')",
        params![id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    if own || sender.is_empty() || !sender.contains('@') {
        return Err(rusqlite::Error::InvalidParameterName(
            "Cannot block this sender".into(),
        ));
    }
    let mut value = rules(conn)?;
    let senders = value.entry(account).or_default();
    if !senders.contains(&sender) {
        senders.push(sender);
    }
    write_rules(conn, &value)
}

/// Use the existing Spam queue. Other senders in the same thread stay put.
fn sweep(conn: &mut Connection) -> rusqlite::Result<Vec<String>> {
    let value = rules(conn)?;
    if value.is_empty() {
        return Ok(Vec::new());
    }
    let ids = {
        let mut stmt = conn.prepare(
            "SELECT m.id FROM messages m JOIN folders f ON f.id=m.folder_id
             JOIN json_each(?1) account ON account.key=m.account_id
             JOIN json_each(account.value) sender ON sender.value=lower(trim(m.from_addr))
             WHERE f.role='inbox'
             AND NOT EXISTS(SELECT 1 FROM accounts a WHERE lower(a.email)=lower(trim(m.from_addr)))
             AND NOT EXISTS(SELECT 1 FROM pending_ops p, json_each(p.payload,'$.uids') u
                 WHERE p.account_id=m.account_id AND p.kind='junk'
                 AND json_extract(p.payload,'$.folderId')=m.folder_id AND u.value=m.uid)
             ORDER BY m.id LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![json!(value).to_string(), BATCH_LIMIT], |r| r.get(0))?;
        rows.collect::<rusqlite::Result<Vec<i64>>>()?
    };
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    queue_op_local(
        conn,
        &ids,
        "junk",
        &json!({"forkBlocked": true}),
        // A failed automatic move must leave the mail visible. Remove the
        // cached copy only after the server accepts the Spam operation.
        |_, _| Ok(()),
    )
}

pub fn complete(
    conn: &mut Connection,
    account: &str,
    payload: &serde_json::Value,
) -> rusqlite::Result<()> {
    if payload["forkBlocked"] != true {
        return Ok(());
    }
    let ids = {
        let mut stmt = conn.prepare("SELECT id FROM messages WHERE account_id=?1 AND folder_id=?2 AND uid IN (SELECT value FROM json_each(?3,'$.uids'))")?;
        let rows = stmt.query_map(
            params![account, payload["folderId"].as_i64(), payload.to_string()],
            |r| r.get(0),
        )?;
        rows.collect::<rusqlite::Result<Vec<i64>>>()?
    };
    bodies::remove_messages_local(conn, &ids)
}

pub fn schedule(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        match state.db.call(sweep).await {
            Ok(accounts) if !accounts.is_empty() => {
                let engines = state.engines.lock().await;
                for id in accounts {
                    if let Some(engine) = engines.get(&id) {
                        engine.run_ops();
                    }
                }
                drop(engines);
                // This event also schedules the next bounded batch.
                let _ = app.emit("mail:updated", json!({}));
            }
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %e, "sender block could not move mail to Spam"),
        }
    });
}

#[tauri::command]
pub async fn fork_block_sender(
    app: AppHandle,
    state: State<'_, AppState>,
    message_id: i64,
) -> Result<()> {
    state
        .db
        .call(move |conn| block_message(conn, message_id))
        .await?;
    schedule(app);
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockedSender {
    account_id: String,
    account_email: String,
    sender: String,
}

#[tauri::command]
pub async fn fork_blocked_senders(state: State<'_, AppState>) -> Result<Vec<BlockedSender>> {
    state
        .db
        .call(|conn| {
            let mut rows = Vec::new();
            for (account_id, senders) in rules(conn)? {
                let email: String = conn
                    .query_row(
                        "SELECT email FROM accounts WHERE id=?1",
                        params![account_id],
                        |r| r.get(0),
                    )
                    .optional()?
                    .unwrap_or_else(|| account_id.clone());
                for sender in senders {
                    rows.push(BlockedSender {
                        account_id: account_id.clone(),
                        account_email: email.clone(),
                        sender,
                    });
                }
            }
            Ok(rows)
        })
        .await
}

#[tauri::command]
pub async fn fork_unblock_sender(
    state: State<'_, AppState>,
    account_id: String,
    sender: String,
) -> Result<()> {
    if account_id.is_empty() {
        return Err(SkimError::other("block", "The mailbox is missing."));
    }
    state
        .db
        .call(move |conn| {
            let mut value = rules(conn)?;
            if let Some(senders) = value.get_mut(&account_id) {
                senders.retain(|s| s != &sender);
            }
            value.retain(|_, senders| !senders.is_empty());
            write_rules(conn, &value)
        })
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    fn seed(conn: &Connection) {
        conn.execute_batch("INSERT INTO accounts (id,email,provider,imap_host,smtp_host,created_at) VALUES ('a','me@example.com','custom','i','s',0),('b','other@example.com','custom','i','s',0);
            INSERT INTO folders (id,account_id,imap_name,display_name,role) VALUES (1,'a','INBOX','Inbox','inbox'),(2,'a','Sent','Sent','sent'),(3,'b','INBOX','Inbox','inbox'),(4,'a','Archive','Archive','archive');
            INSERT INTO messages (id,account_id,folder_id,uid,date,from_addr) VALUES
            (1,'a',1,1,0,'NEWS@list.example'),(2,'a',1,2,0,'someone@list.example'),(3,'b',3,3,0,'news@list.example'),(4,'a',2,4,0,'news@list.example'),(5,'a',4,5,0,'news@list.example'),(6,'a',1,6,0,'other@example.com');").unwrap();
    }

    #[test]
    fn exact_sender_account_and_inbox_only_and_no_duplicate_ops() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            seed(conn);
            block_message(conn, 1)?;
            block_message(conn, 1)?;
            assert_eq!(rules(conn)?["a"], vec!["news@list.example"]);
            assert_eq!(sweep(conn)?, vec!["a"]);
            assert!(sweep(conn)?.is_empty());
            let remaining: i64 =
                conn.query_row("SELECT count(*) FROM messages", [], |r| r.get(0))?;
            assert_eq!(remaining, 6);
            let (kind, payload): (String, String) =
                conn.query_row("SELECT kind,payload FROM pending_ops", [], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })?;
            assert_eq!(kind, "junk");
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&payload).unwrap()["uids"],
                json!([1])
            );
            assert!(block_message(conn, 6).is_err());
            assert!(block_message(conn, 4).is_err());
            conn.execute("UPDATE pending_ops SET state='failed'", [])?;
            assert!(sweep(conn)?.is_empty());
            assert_eq!(
                conn.query_row("SELECT count(*) FROM messages WHERE id=1", [], |r| r
                    .get::<_, i64>(0))?,
                1
            );
            complete(conn, "b", &serde_json::from_str(&payload).unwrap())?;
            assert_eq!(
                conn.query_row("SELECT count(*) FROM messages WHERE id=1", [], |r| r
                    .get::<_, i64>(0))?,
                1
            );
            complete(conn, "a", &serde_json::from_str(&payload).unwrap())?;
            assert_eq!(
                conn.query_row("SELECT count(*) FROM messages", [], |r| r.get::<_, i64>(0))?,
                5
            );
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn future_mail_is_queued_until_unblocked_and_invalid_rules_preserve_mail() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            seed(conn); block_message(conn, 1)?; sweep(conn)?;
            conn.execute("INSERT INTO messages (id,account_id,folder_id,uid,date,from_addr) VALUES (7,'a',1,7,0,'news@list.example')", [])?;
            assert_eq!(sweep(conn)?, vec!["a"]);
            write_rules(conn, &Rules::new())?;
            conn.execute("INSERT INTO messages (id,account_id,folder_id,uid,date,from_addr) VALUES (8,'a',1,8,0,'news@list.example')", [])?;
            assert!(sweep(conn)?.is_empty());
            queries::set_setting(conn, SETTING, "broken")?;
            assert!(sweep(conn).is_err());
            assert_eq!(conn.query_row("SELECT count(*) FROM messages WHERE id=8", [], |r| r.get::<_, i64>(0))?, 1);
            Ok(())
        }).unwrap();
    }
}
