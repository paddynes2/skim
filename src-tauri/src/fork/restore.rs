//! Undo after the grace window (PLAN.md 3.2, decision D3).
//!
//! Upstream deletes the local rows the moment a message is archived, so a
//! local undo is impossible once the op has run. Instead the webview keeps a
//! snapshot (account, source folder, RFC 822 Message-IDs) and, on undo, asks
//! for a `fork_restore` op: look the messages up by Message-ID where they
//! went, and put them back where they were.
//!
//! - Gmail archive: SELECT `[Gmail]/All Mail`, `UID SEARCH HEADER Message-ID`,
//!   `UID COPY` to the source folder (Gmail adds the label back).
//! - Everything else (archive folder, Trash, Spam, a move): SELECT the
//!   destination, search, `UID MOVE` back to the source.
//!
//! The IMAP sequence is written against a small trait so it can be driven by
//! a scripted server in tests without an `Engine`.

use crate::error::{Result, SkimError};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncWrite};

/// Op kind in `pending_ops` for a restore.
pub const OP_KIND: &str = "fork_restore";

/// What the webview keeps for every removal, taken before the rows go.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RemovalSnapshot {
    pub account_id: String,
    pub folder_id: i64,
    pub folder_imap_name: String,
    /// RFC 822 Message-IDs, angle brackets included.
    pub message_ids: Vec<String>,
}

/// Where the messages went, and how to bring them back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The messages still exist in `search_in` (Gmail All Mail): copy them
    /// back so the source label is re-added.
    Copy,
    /// The messages were moved to `search_in`: move them back.
    Move,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestorePlan {
    pub source: String,
    pub search_in: String,
    pub message_ids: Vec<String>,
    pub mode: Mode,
}

/// The four IMAP verbs a restore needs.
pub(crate) trait RestoreImap {
    async fn select_mailbox(&mut self, mailbox: &str) -> Result<()>;
    async fn search_message_id(&mut self, message_id: &str) -> Result<Vec<u32>>;
    async fn uid_copy_to(&mut self, uid_set: &str, dest: &str) -> Result<()>;
    async fn uid_move_to(&mut self, uid_set: &str, dest: &str) -> Result<()>;
}

fn imap_err(e: async_imap::error::Error) -> SkimError {
    SkimError::other("imap", e.to_string())
}

/// Same quoting rule as `sync::quote_mailbox`: async-imap quotes for MOVE but
/// interpolates raw for COPY.
fn quote_mailbox(name: &str) -> String {
    format!("\"{}\"", name.replace('\\', "\\\\").replace('"', "\\\""))
}

impl<T> RestoreImap for async_imap::Session<T>
where
    T: AsyncRead + AsyncWrite + Unpin + Send + std::fmt::Debug,
{
    async fn select_mailbox(&mut self, mailbox: &str) -> Result<()> {
        self.select(mailbox)
            .await
            .map(|_| ())
            .map_err(|e| SkimError::other("folder", format!("cannot open {mailbox}: {e}")))
    }

    async fn search_message_id(&mut self, message_id: &str) -> Result<Vec<u32>> {
        // The header value is searched as a quoted string; a Message-ID never
        // contains a double quote or a backslash, but strip them defensively.
        let clean: String = message_id
            .chars()
            .filter(|c| !matches!(c, '"' | '\\' | '\r' | '\n'))
            .collect();
        let found = self
            .uid_search(format!("HEADER Message-ID \"{clean}\""))
            .await
            .map_err(imap_err)?;
        let mut uids: Vec<u32> = found.into_iter().collect();
        uids.sort_unstable();
        Ok(uids)
    }

    async fn uid_copy_to(&mut self, uid_set: &str, dest: &str) -> Result<()> {
        self.uid_copy(uid_set, quote_mailbox(dest))
            .await
            .map_err(imap_err)
    }

    async fn uid_move_to(&mut self, uid_set: &str, dest: &str) -> Result<()> {
        self.uid_mv(uid_set, dest).await.map_err(imap_err)
    }
}

/// Run one restore. Returns how many Message-IDs were found and put back; a
/// Message-ID the server cannot find is skipped, not an error (the toast says
/// "couldn't restore" when the count is short, and the ids go to the log).
pub(crate) async fn restore<S: RestoreImap>(imap: &mut S, plan: &RestorePlan) -> Result<usize> {
    imap.select_mailbox(&plan.search_in).await?;
    let mut restored = 0usize;
    for message_id in &plan.message_ids {
        let uids = imap.search_message_id(message_id).await?;
        if uids.is_empty() {
            tracing::warn!(target: "skim_lib", "restore: Message-ID not found on the server");
            continue;
        }
        let set = uids
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        match plan.mode {
            Mode::Copy => imap.uid_copy_to(&set, &plan.source).await?,
            Mode::Move => imap.uid_move_to(&set, &plan.source).await?,
        }
        restored += 1;
    }
    Ok(restored)
}

/// Build the plan for a queued `fork_restore` op payload:
/// `{ sourceImapName, sourceFolderId, messageIds, kind, destImapName? }` plus
/// the role folders the account has (`all`, `archive`, `trash`, `junk`).
pub fn plan_from_payload(
    payload: &serde_json::Value,
    is_gmail: bool,
    role_folder: impl Fn(&str) -> Option<String>,
) -> Result<RestorePlan> {
    let source = payload["sourceImapName"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| SkimError::other("restore", "no source folder"))?
        .to_string();
    let message_ids: Vec<String> = payload["messageIds"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    if message_ids.is_empty() {
        return Err(SkimError::other("restore", "nothing to restore"));
    }
    let kind = payload["kind"].as_str().unwrap_or("archive");
    let (search_in, mode) = match kind {
        "archive" if is_gmail => (
            role_folder("all").ok_or_else(|| SkimError::other("restore", "no All Mail folder"))?,
            Mode::Copy,
        ),
        "archive" => (
            role_folder("archive")
                .ok_or_else(|| SkimError::other("restore", "no archive folder"))?,
            Mode::Move,
        ),
        "delete" => (
            role_folder("trash").ok_or_else(|| SkimError::other("restore", "no trash folder"))?,
            Mode::Move,
        ),
        "spam" => (
            role_folder("junk").ok_or_else(|| SkimError::other("restore", "no spam folder"))?,
            Mode::Move,
        ),
        "move" => (
            payload["destImapName"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| SkimError::other("restore", "no move destination"))?
                .to_string(),
            Mode::Move,
        ),
        other => {
            return Err(SkimError::other(
                "restore",
                format!("unknown removal kind: {other}"),
            ))
        }
    };
    Ok(RestorePlan {
        source,
        search_in,
        message_ids,
        mode,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    /// A minimal scripted IMAP server for one restore: greets, answers LOGIN,
    /// SELECT, UID SEARCH (by a fixed table), UID COPY / UID MOVE, LOGOUT.
    /// Records every command it saw.
    async fn scripted_server(
        known: Vec<(&'static str, Vec<u32>)>,
    ) -> (u16, tokio::task::JoinHandle<Vec<String>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (sock, _) = listener.accept().await.unwrap();
            let mut reader = BufReader::new(sock);
            reader.get_mut().write_all(b"* OK ready\r\n").await.unwrap();
            let mut seen = Vec::new();
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).await.unwrap() == 0 {
                    break;
                }
                let line = line.trim_end().to_string();
                seen.push(line.clone());
                let mut parts = line.splitn(2, ' ');
                let tag = parts.next().unwrap_or("a").to_string();
                let rest = parts.next().unwrap_or("").to_string();
                let upper = rest.to_ascii_uppercase();
                let reply = if upper.starts_with("LOGIN") {
                    format!("{tag} OK logged in\r\n")
                } else if upper.starts_with("SELECT") {
                    format!("* 3 EXISTS\r\n* OK [UIDVALIDITY 1] ok\r\n{tag} OK [READ-WRITE] selected\r\n")
                } else if upper.starts_with("UID SEARCH") {
                    let hits: Vec<String> = known
                        .iter()
                        .filter(|(id, _)| rest.contains(id))
                        .flat_map(|(_, uids)| uids.iter().map(u32::to_string))
                        .collect();
                    format!("* SEARCH {}\r\n{tag} OK done\r\n", hits.join(" "))
                } else if upper.starts_with("UID COPY") || upper.starts_with("UID MOVE") {
                    format!("{tag} OK done\r\n")
                } else if upper.starts_with("LOGOUT") {
                    format!("* BYE\r\n{tag} OK bye\r\n")
                } else {
                    format!("{tag} BAD unexpected\r\n")
                };
                reader.get_mut().write_all(reply.as_bytes()).await.unwrap();
                if upper.starts_with("LOGOUT") {
                    break;
                }
            }
            seen
        });
        (port, server)
    }

    async fn session_for(port: u16) -> async_imap::Session<tokio::net::TcpStream> {
        let tcp = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        let mut client = async_imap::Client::new(tcp);
        client.read_response().await.unwrap().unwrap();
        client.login("u", "p").await.map_err(|(e, _)| e).unwrap()
    }

    #[tokio::test]
    async fn gmail_archive_restore_copies_from_all_mail_to_inbox() {
        let (port, server) =
            scripted_server(vec![("<a@x>", vec![41]), ("<b@x>", vec![42, 43])]).await;
        let mut session = session_for(port).await;
        let plan = RestorePlan {
            source: "INBOX".into(),
            search_in: "[Gmail]/All Mail".into(),
            message_ids: vec!["<a@x>".into(), "<missing@x>".into(), "<b@x>".into()],
            mode: Mode::Copy,
        };
        let restored = restore(&mut session, &plan).await.unwrap();
        assert_eq!(restored, 2, "the missing id is skipped, not fatal");
        session.logout().await.unwrap();
        let seen = server.await.unwrap();
        assert!(
            seen.iter()
                .any(|l| l.contains("SELECT \"[Gmail]/All Mail\"")),
            "{seen:?}"
        );
        assert!(seen
            .iter()
            .any(|l| l.contains("UID SEARCH HEADER Message-ID \"<a@x>\"")));
        assert!(
            seen.iter().any(|l| l.contains("UID COPY 41 \"INBOX\"")),
            "{seen:?}"
        );
        assert!(
            seen.iter().any(|l| l.contains("UID COPY 42,43 \"INBOX\"")),
            "{seen:?}"
        );
        assert!(!seen.iter().any(|l| l.contains("UID MOVE")));
    }

    #[tokio::test]
    async fn trash_restore_moves_back_to_the_source() {
        let (port, server) = scripted_server(vec![("<t@x>", vec![7])]).await;
        let mut session = session_for(port).await;
        let plan = RestorePlan {
            source: "Clients".into(),
            search_in: "Trash".into(),
            message_ids: vec!["<t@x>".into()],
            mode: Mode::Move,
        };
        assert_eq!(restore(&mut session, &plan).await.unwrap(), 1);
        session.logout().await.unwrap();
        let seen = server.await.unwrap();
        assert!(
            seen.iter().any(|l| l.contains("SELECT \"Trash\"")),
            "{seen:?}"
        );
        assert!(
            seen.iter().any(|l| l.contains("UID MOVE 7 \"Clients\"")),
            "{seen:?}"
        );
    }

    fn roles(role: &str) -> Option<String> {
        match role {
            "all" => Some("[Gmail]/All Mail".into()),
            "archive" => Some("Archive".into()),
            "trash" => Some("[Gmail]/Trash".into()),
            "junk" => Some("[Gmail]/Spam".into()),
            _ => None,
        }
    }

    #[test]
    fn plan_picks_the_place_to_look_per_kind() {
        let base = |kind: &str| json!({ "sourceImapName": "INBOX", "messageIds": ["<a@x>"], "kind": kind });
        let p = plan_from_payload(&base("archive"), true, roles).unwrap();
        assert_eq!(
            (p.search_in.as_str(), p.mode),
            ("[Gmail]/All Mail", Mode::Copy)
        );
        let p = plan_from_payload(&base("archive"), false, roles).unwrap();
        assert_eq!((p.search_in.as_str(), p.mode), ("Archive", Mode::Move));
        let p = plan_from_payload(&base("delete"), true, roles).unwrap();
        assert_eq!(
            (p.search_in.as_str(), p.mode),
            ("[Gmail]/Trash", Mode::Move)
        );
        let p = plan_from_payload(&base("spam"), true, roles).unwrap();
        assert_eq!((p.search_in.as_str(), p.mode), ("[Gmail]/Spam", Mode::Move));
        let mut mv = base("move");
        mv["destImapName"] = json!("Clients");
        let p = plan_from_payload(&mv, true, roles).unwrap();
        assert_eq!((p.search_in.as_str(), p.mode), ("Clients", Mode::Move));
        assert_eq!(p.source, "INBOX");
    }

    #[test]
    fn plan_refuses_empty_or_unknown_input() {
        let empty = json!({ "sourceImapName": "INBOX", "messageIds": [], "kind": "archive" });
        assert!(plan_from_payload(&empty, true, roles).is_err());
        let no_source = json!({ "messageIds": ["<a@x>"], "kind": "archive" });
        assert!(plan_from_payload(&no_source, true, roles).is_err());
        let bogus =
            json!({ "sourceImapName": "INBOX", "messageIds": ["<a@x>"], "kind": "explode" });
        assert!(plan_from_payload(&bogus, true, roles).is_err());
        // No All Mail on a Gmail account: cannot restore an archive.
        let p = plan_from_payload(
            &json!({ "sourceImapName": "INBOX", "messageIds": ["<a@x>"], "kind": "archive" }),
            true,
            |_| None,
        );
        assert!(p.is_err());
    }

    #[test]
    fn snapshot_round_trips_through_json() {
        let snap = RemovalSnapshot {
            account_id: "a1".into(),
            folder_id: 3,
            folder_imap_name: "INBOX".into(),
            message_ids: vec!["<a@x>".into()],
        };
        let json = serde_json::to_value(&snap).unwrap();
        assert_eq!(json["folderImapName"], "INBOX");
        let back: RemovalSnapshot = serde_json::from_value(json).unwrap();
        assert_eq!(back, snap);
    }
}

/// The `fork_restore` op, run by the sync engine's `execute_op`. Returns the
/// source folder id so the drain resyncs it and the rows reappear.
pub(crate) async fn execute(
    engine: &mut crate::mail::sync::Engine,
    payload: &serde_json::Value,
) -> Result<Option<i64>> {
    let account_id = engine.account.id.clone();
    let roles: std::collections::HashMap<String, String> = engine
        .db
        .call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT role, imap_name FROM folders WHERE account_id = ?1 AND role IS NOT NULL",
            )?;
            let rows = stmt.query_map([account_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect()
        })
        .await?;
    let is_gmail = crate::fork::gmail::is_gmail(&engine.account);
    let plan = plan_from_payload(payload, is_gmail, |role| roles.get(role).cloned())?;
    engine.ensure_selected(&plan.search_in).await?;
    let session = engine.session().await?;
    let restored = restore(session, &plan).await?;
    if restored < plan.message_ids.len() {
        crate::append_log(
            "skim-fork.log",
            &format!(
                "restore: {restored} of {} message-ids found in {}",
                plan.message_ids.len(),
                plan.search_in
            ),
        );
    }
    Ok(payload["sourceFolderId"].as_i64())
}
