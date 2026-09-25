//! One fixed SSH operation; message content and user direction travel on stdin.
use crate::error::{Result, SkimError};
use crate::state::AppState;
use rusqlite::OptionalExtension;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tauri::State;

const ACCOUNT: &str = "patrick@autospark.ai";
const REMOTE: &str = "sudo -n -u cc-nesbitt -H /opt/os-google-mcp/venv/bin/python /home/cc-nesbitt/.local/share/skim/estate_reply.py";
const SSH_TIMEOUT: Duration = Duration::from_secs(45);

fn error(message: impl Into<String>) -> SkimError {
    SkimError::other("estate_reply", message)
}

fn valid_message_id(value: &str) -> bool {
    value.starts_with('<')
        && value.ends_with('>')
        && value.contains('@')
        && value.len() <= 1000
        && !value.chars().any(|c| c.is_whitespace() || c.is_control())
        && !value[1..value.len() - 1].contains(['<', '>'])
}

fn wire_message_id(stored: &str) -> Option<String> {
    // Skim's query writer stores normalized IDs without RFC angle brackets.
    let wire = if stored.starts_with('<') {
        stored.to_string()
    } else {
        format!("<{stored}>")
    };
    valid_message_id(&wire).then_some(wire)
}

fn ssh(request: Value) -> Result<Value> {
    let mut command = Command::new("ssh");
    command.args([
        "-T",
        "-o",
        "BatchMode=yes",
        "-o",
        "ConnectTimeout=10",
        "-o",
        "StrictHostKeyChecking=yes",
        "-o",
        "ServerAliveInterval=10",
        "-o",
        "ServerAliveCountMax=2",
        "clouddev-admin",
        REMOTE,
    ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // no console window on each status read
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| error("Could not start SSH. Check that Windows OpenSSH is installed."))?;
    let input = serde_json::to_vec(&request).map_err(|_| error("Invalid reply request"))?;
    let written = child
        .stdin
        .take()
        .ok_or_else(|| error("SSH input unavailable"))?
        .write_all(&input);
    if written.is_err() {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error(
            "Could not reach the estate. Check your SSH connection.",
        ));
    }
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| error("SSH output unavailable"))?;
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.take(65_537).read_to_end(&mut bytes).map(|_| bytes)
    });
    let deadline = Instant::now() + SSH_TIMEOUT;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error(
                "The estate connection timed out. Check progress to reconnect to the same request.",
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let bytes = reader
        .join()
        .map_err(|_| error("Could not read the estate response"))??;
    if !status.success() {
        return Err(error(
            "The estate is unavailable. Check SSH access and the installed reply adapter.",
        ));
    }
    if bytes.len() > 65_536 {
        return Err(error("Estate response exceeded its limit"));
    }
    serde_json::from_slice(&bytes).map_err(|_| error("The estate returned an invalid response"))
}

#[tauri::command]
pub async fn fork_estate_reply(
    state: State<'_, AppState>,
    message_id: i64,
    action: String,
    instruction: Option<String>,
) -> Result<Value> {
    if !matches!(action.as_str(), "start" | "status") {
        return Err(error("Unknown reply action"));
    }
    let direction = instruction.unwrap_or_default();
    if direction.chars().count() > 2000 || direction.contains('\0') {
        return Err(error("Keep the reply direction under 2,000 characters."));
    }
    let target = state
        .db
        .call(move |conn| {
            conn.query_row(
                "SELECT a.email, a.id, m.message_id, f.role,
             EXISTS(SELECT 1 FROM drafts d WHERE d.account_id = a.id AND
                (d.reply_to_message_id = m.id OR d.reply_to_message_id IN
                    (SELECT id FROM messages WHERE thread_id = m.thread_id)))
             FROM messages m JOIN folders f ON f.id = m.folder_id
             JOIN accounts a ON a.id = f.account_id WHERE m.id = ?1",
                [message_id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, Option<String>>(2)?,
                        r.get::<_, Option<String>>(3)?,
                        r.get::<_, bool>(4)?,
                    ))
                },
            )
            .optional()
        })
        .await?
        .ok_or_else(|| error("The selected email no longer exists."))?;
    if !target.0.eq_ignore_ascii_case(ACCOUNT) {
        return Err(error(
            "Estate replies are connected to patrick@autospark.ai only.",
        ));
    }
    if matches!(
        target.3.as_deref(),
        Some("drafts" | "sent" | "trash" | "junk")
    ) {
        return Err(error("Select an incoming email to draft a reply."));
    }
    let rfc = target
        .2
        .and_then(|s| wire_message_id(&s))
        .ok_or_else(|| error("This email has no usable Message-ID."))?;
    if action == "start" && target.4 {
        return Err(error(
            "You already have a local reply draft. Continue editing it before requesting another.",
        ));
    }
    let request =
        json!({"action":action,"account":ACCOUNT,"messageId":rfc,"instruction":direction});
    let mut response = tokio::task::spawn_blocking(move || ssh(request)).await??;
    if response["status"] == "ready" {
        let saved_rfc = response["rfcMessageId"]
            .as_str()
            .filter(|s| valid_message_id(s))
            .ok_or_else(|| error("The saved draft has no verified Message-ID."))?
            .trim_start_matches('<')
            .trim_end_matches('>')
            .to_string();
        let account = target.1.clone();
        let local = state
            .db
            .call(move |conn| {
                conn.query_row(
                    "SELECT m.id FROM messages m JOIN folders f ON f.id = m.folder_id
                WHERE f.account_id = ?1 AND f.role = 'drafts' AND m.message_id = ?2 LIMIT 1",
                    rusqlite::params![account, saved_rfc],
                    |r| r.get::<_, i64>(0),
                )
                .optional()
            })
            .await?;
        response["localMessageId"] = json!(local);
        if local.is_none() {
            let account = target.1.clone();
            let ids = state
                .db
                .call(move |conn| {
                    super::freshness::folder_ids_by_role(conn, Some(&account), "drafts")
                })
                .await?;
            if let Some(engine) = state.engines.lock().await.get(&target.1) {
                for id in ids {
                    engine.sync_folder(id);
                }
            }
        }
    }
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires Patrick's configured SSH route; reads only an absent request receipt"]
    fn live_ssh_adapter_status() {
        let response = ssh(json!({"action":"status","account":ACCOUNT,
            "messageId":"<skim-rust-transport-probe@invalid.example>"}))
        .unwrap();
        assert_eq!(response["status"], "idle");
    }

    #[test]
    fn accepts_rfc_identity_but_not_headers_or_shell_text() {
        assert!(valid_message_id("<reply.123@example.com>"));
        for bad in [
            "",
            "abc",
            "<a@b>\r\nBcc: bad@bad",
            "<a @b>",
            "<<a@b>>",
            "<a@b>;cmd",
        ] {
            assert!(!valid_message_id(bad), "{bad}");
        }
    }

    #[test]
    fn maps_skims_normalized_identity_to_gateway_and_back() {
        let stored = crate::mail::threading::normalize_msgid("<reply.123@example.com>").unwrap();
        assert_eq!(stored, "reply.123@example.com");
        let wire = wire_message_id(&stored).unwrap();
        assert_eq!(wire, "<reply.123@example.com>");
        assert_eq!(
            crate::mail::threading::normalize_msgid(&wire).unwrap(),
            stored
        );
        assert!(wire_message_id("bad\r\nheader@example.com").is_none());
    }
}
