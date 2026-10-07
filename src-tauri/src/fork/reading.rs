//! Batched recipient labels for the visible outgoing message rows.
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::{
    db::queries::addresses_from_json,
    error::{Result, SkimError},
    state::AppState,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecipientRequest {
    key: String,
    thread_id: i64,
    message_id: Option<i64>,
    from_addr: String,
    date: i64,
}

#[derive(Serialize)]
pub struct RecipientLabel {
    key: String,
    label: String,
}

#[tauri::command]
pub async fn fork_reading_recipients(
    state: State<'_, AppState>,
    rows: Vec<RecipientRequest>,
) -> Result<Vec<RecipientLabel>> {
    if rows.len() > 100 {
        return Err(SkimError::other(
            "invalid_input",
            "At most 100 message rows are allowed.",
        ));
    }
    state.db.read("fork_reading_recipients", move |conn| {
        let mut statement = conn.prepare("SELECT to_addrs FROM messages WHERE (?1 IS NOT NULL AND id = ?1) OR (?1 IS NULL AND thread_id = ?2 AND lower(from_addr) = lower(?3) AND date = ?4) ORDER BY id DESC LIMIT 1")?;
        let mut output = Vec::new();
        for row in rows {
            let json: Option<Option<String>> = statement.query_row(params![row.message_id, row.thread_id, row.from_addr, row.date], |r| r.get(0)).optional()?;
            let addresses = addresses_from_json(json.flatten().as_deref());
            let names: Vec<String> = addresses.into_iter().take(3).map(|a| a.name.filter(|name| !name.trim().is_empty()).unwrap_or(a.addr)).collect();
            output.push(RecipientLabel {key: row.key, label: names.join(", ")});
        }
        Ok(output)
    }).await
}
