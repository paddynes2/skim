//! Receipts for the exact MIME inputs accepted by the mail server.
use crate::{
    db::{
        draft_attachments,
        drafts::{self, Draft},
    },
    error::Result,
    state::AppState,
};
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::State;

pub struct Snapshot {
    pub draft: Draft,
    pub message_id: Option<String>,
    pub attachments: Vec<(String, String, Vec<u8>)>,
    pub html: Option<String>,
    pub digest: String,
}

pub fn snapshot(conn: &Connection, id: i64) -> rusqlite::Result<Option<Snapshot>> {
    let Some(draft) = drafts::get(conn, id)? else {
        return Ok(None);
    };
    let message_id = drafts::origin_coords(conn, id)?.and_then(|(_, mid)| mid);
    let html = super::compose::outgoing_html_sync(conn, &draft)?;
    let attachments = draft_attachments::load_for_send(conn, id)?;
    let mut hash = Sha256::new();
    // Length prefixes prevent different field boundaries from sharing a digest.
    let mut field = |bytes: &[u8]| {
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    };
    for value in [
        &draft.account_id,
        &draft.mode,
        &draft.to,
        &draft.cc,
        &draft.bcc,
        &draft.subject,
        &draft.body,
    ] {
        field(value.as_bytes());
    }
    field(&draft.reply_to_message_id.unwrap_or_default().to_le_bytes());
    field(message_id.as_deref().unwrap_or_default().as_bytes());
    field(html.as_deref().unwrap_or_default().as_bytes());
    for (name, mime, data) in &attachments {
        field(name.as_bytes());
        field(mime.as_bytes());
        field(data);
    }
    let digest = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
    Ok(Some(Snapshot {
        draft,
        message_id,
        attachments,
        html,
        digest,
    }))
}

pub fn confirm(conn: &Connection, id: i64, digest: &str) -> rusqlite::Result<()> {
    conn.execute("INSERT INTO settings (key, value) SELECT ?1, ?2 WHERE EXISTS (SELECT 1 FROM drafts WHERE id = ?3)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![format!("fork_draft_receipt:{id}"), digest, id])?;
    Ok(())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub state: &'static str,
    pub gmail: bool,
}

pub fn status(conn: &Connection, id: i64) -> rusqlite::Result<Status> {
    let Some(current) = snapshot(conn, id)? else {
        return Ok(Status {
            state: "local",
            gmail: false,
        });
    };
    let gmail = conn.query_row("SELECT provider = 'gmail' OR lower(trim(imap_host)) = 'imap.gmail.com' FROM accounts WHERE id = ?1", [&current.draft.account_id], |r| r.get(0))?;
    let receipt: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            [format!("fork_draft_receipt:{id}")],
            |r| r.get(0),
        )
        .optional()?;
    if receipt.as_deref() == Some(current.digest.as_str()) {
        return Ok(Status {
            state: "synced",
            gmail,
        });
    }
    let pending: Option<String> = conn.query_row("SELECT state FROM pending_ops WHERE kind = 'save_draft' AND account_id = ?1
        AND json_valid(payload) AND json_extract(payload, '$.draftId') = ?2 ORDER BY id DESC LIMIT 1",
        rusqlite::params![current.draft.account_id, id], |r| r.get(0)).optional()?;
    Ok(Status {
        state: match pending.as_deref() {
            Some("failed") => "failed",
            Some(_) => "pending",
            None => "local",
        },
        gmail,
    })
}

#[tauri::command]
pub async fn fork_draft_sync_status(state: State<'_, AppState>, draft_id: i64) -> Result<Status> {
    state
        .db
        .read("fork_draft_sync_status", move |conn| status(conn, draft_id))
        .await
}
