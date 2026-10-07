//! Read and starred state for the whole folder, not only its newest window.
//!
//! Upstream's `reconcile_flags` diffs server flags against the newest 500
//! cached rows of a folder. A message read in Gmail on the web or the phone
//! after it had aged out of that window stayed unread here for good: on
//! October 7 the Inbox showed 38 unread, 28 of them in two threads from
//! July to September. `UID SEARCH UNSEEN` and
//! `UID SEARCH FLAGGED` return the server's whole unread and starred sets in
//! two short replies, so every cached row is brought into line on each folder
//! sync.
//!
//! Gmail keeps one read flag and one star per message and shows them under
//! every label the message carries, so a local read or star also applies to
//! every cached copy of that message in a Gmail account. Without this, opening
//! a thread marked the one copy the reading pane kept (it dedups by
//! Message-ID) and left the Inbox copy unread until the next sync.

use std::collections::HashSet;

use rusqlite::{params, Connection};

use crate::db::bodies;
use crate::error::{Result, SkimError};
use crate::mail::sync::Engine;

fn imap_err(e: async_imap::error::Error) -> SkimError {
    SkimError::other("imap", e.to_string())
}

/// Bring every cached row of the selected folder into line with the server's
/// unread and starred sets. The folder must already be selected (the sync pass
/// that calls this has just selected it). Returns whether any row changed.
pub(crate) async fn reconcile_whole_folder(engine: &mut Engine, folder_id: i64) -> Result<bool> {
    let session = engine.session().await?;
    let unseen: HashSet<u32> = session
        .uid_search("UNSEEN")
        .await
        .map_err(imap_err)?
        .into_iter()
        .collect();
    let flagged: HashSet<u32> = session
        .uid_search("FLAGGED")
        .await
        .map_err(imap_err)?
        .into_iter()
        .collect();
    engine
        .db
        .call(move |conn| apply_server_sets(conn, folder_id, &unseen, &flagged))
        .await
}

/// The database half of [`reconcile_whole_folder`]: a cached row is read
/// unless its UID is in `unseen`, and starred exactly when it is in `flagged`.
///
/// Skipped while a read or star for this account still waits to reach the
/// server: the server's answer predates it and would undo it. The next sync,
/// after the queue drains, catches up.
pub fn apply_server_sets(
    conn: &mut Connection,
    folder_id: i64,
    unseen: &HashSet<u32>,
    flagged: &HashSet<u32>,
) -> rusqlite::Result<bool> {
    let pending: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM pending_ops p JOIN folders f ON f.account_id = p.account_id
                        WHERE f.id = ?1 AND p.state = 'pending' AND p.kind = 'set_flag')",
        params![folder_id],
        |r| r.get(0),
    )?;
    if pending {
        return Ok(false);
    }

    let rows: Vec<(i64, u32, bool, bool)> = {
        let mut stmt = conn.prepare_cached(
            "SELECT id, uid, is_read, is_starred FROM messages WHERE folder_id = ?1",
        )?;
        let rows = stmt
            .query_map(params![folder_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };

    let (mut read_on, mut read_off, mut star_on, mut star_off) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for (pk, uid, is_read, is_starred) in rows {
        let server_read = !unseen.contains(&uid);
        if server_read != is_read {
            if server_read {
                read_on.push(pk);
            } else {
                read_off.push(pk);
            }
        }
        let server_star = flagged.contains(&uid);
        if server_star != is_starred {
            if server_star {
                star_on.push(pk);
            } else {
                star_off.push(pk);
            }
        }
    }

    let changed =
        !(read_on.is_empty() && read_off.is_empty() && star_on.is_empty() && star_off.is_empty());
    for (ids, flag, on) in [
        (read_on, "seen", true),
        (read_off, "seen", false),
        (star_on, "flagged", true),
        (star_off, "flagged", false),
    ] {
        if !ids.is_empty() {
            bodies::set_flag_local(conn, &ids, flag, on)?;
        }
    }
    Ok(changed)
}

/// `ids` plus every cached copy that shares their read and starred state on
/// the server: for a Gmail account, the same message (same Message-ID) under
/// each of its labels. Any other account keeps one flag per mailbox copy, so
/// its rows come back unchanged.
pub fn with_shared_copies(conn: &Connection, ids: &[i64]) -> rusqlite::Result<Vec<i64>> {
    let mut stmt = conn.prepare_cached(
        "SELECT c.id FROM messages m
           JOIN accounts a ON a.id = m.account_id
           JOIN messages c ON c.account_id = m.account_id AND c.message_id = m.message_id
          WHERE m.id = ?1 AND m.message_id IS NOT NULL
            AND (a.provider = 'gmail' OR lower(trim(a.imap_host)) = ?2)",
    )?;
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for id in ids {
        if seen.insert(*id) {
            out.push(*id);
        }
        let copies = stmt
            .query_map(params![id, crate::fork::gmail::GMAIL_IMAP_HOST], |r| {
                r.get::<_, i64>(0)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for c in copies {
            if seen.insert(c) {
                out.push(c);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::NewMessage;
    use crate::db::queries::insert_message;
    use crate::db::Db;

    /// One account with an Inbox and an Important label. `provider` decides
    /// whether copies share flags.
    fn seed(conn: &mut Connection, provider: &str, host: &str) -> (i64, i64) {
        conn.execute(
            "INSERT INTO accounts (id, email, provider, imap_host, smtp_host, created_at)
             VALUES ('a1', 'p@example.com', ?1, ?2, 'smtp.example.com', 0)",
            params![provider, host],
        )
        .unwrap();
        for (name, role, sort) in [("INBOX", "inbox", 0), ("[Gmail]/Important", "important", 1)] {
            conn.execute(
                "INSERT INTO folders (account_id, imap_name, role, display_name, unread_count, sort_order)
                 VALUES ('a1', ?1, ?2, ?1, 0, ?3)",
                params![name, role, sort],
            )
            .unwrap();
        }
        let id = |conn: &Connection, name: &str| -> i64 {
            conn.query_row(
                "SELECT id FROM folders WHERE imap_name = ?1",
                params![name],
                |r| r.get(0),
            )
            .unwrap()
        };
        (id(conn, "INBOX"), id(conn, "[Gmail]/Important"))
    }

    fn put(
        conn: &mut Connection,
        folder: i64,
        uid: u32,
        msgid: &str,
        subject: &str,
        read: bool,
    ) -> i64 {
        insert_message(
            conn,
            &NewMessage {
                account_id: "a1".into(),
                folder_id: folder,
                uid,
                message_id: Some(msgid.into()),
                subject: Some(subject.into()),
                from_addr: Some("sender@example.com".into()),
                date: i64::from(uid) * 100,
                is_read: read,
                ..Default::default()
            },
        )
        .unwrap()
        .unwrap()
        .0
    }

    fn flags(conn: &Connection, pk: i64) -> (bool, bool) {
        conn.query_row(
            "SELECT is_read, is_starred FROM messages WHERE id = ?1",
            params![pk],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap()
    }

    fn unread_count(conn: &Connection, folder: i64) -> i64 {
        conn.query_row(
            "SELECT unread_count FROM folders WHERE id = ?1",
            params![folder],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn whole_folder_follows_the_server_sets() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            let (inbox, _) = seed(conn, "gmail", "imap.gmail.com");
            // Old unread that Gmail has since read, a still-unread one, and a
            // read one the server says is unread again and starred.
            let stale = put(conn, inbox, 1, "<1@x>", "stale", false);
            let still = put(conn, inbox, 2, "<2@x>", "still", false);
            let reopened = put(conn, inbox, 3, "<3@x>", "reopened", true);
            let unseen: HashSet<u32> = [2, 3].into();
            let flagged: HashSet<u32> = [3].into();
            assert!(apply_server_sets(conn, inbox, &unseen, &flagged)?);
            assert_eq!(flags(conn, stale), (true, false));
            assert_eq!(flags(conn, still), (false, false));
            assert_eq!(flags(conn, reopened), (false, true));
            assert_eq!(unread_count(conn, inbox), 2);
            // A second pass with the same sets changes nothing.
            assert!(!apply_server_sets(conn, inbox, &unseen, &flagged)?);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn a_pending_flag_op_holds_the_pass() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            let (inbox, _) = seed(conn, "gmail", "imap.gmail.com");
            let pk = put(
                conn,
                inbox,
                1,
                "<1@x>",
                "read here, not yet on the server",
                true,
            );
            conn.execute(
                "INSERT INTO pending_ops (account_id, kind, payload, created_at)
                 VALUES ('a1', 'set_flag', '{}', 0)",
                [],
            )?;
            let unseen: HashSet<u32> = [1].into();
            assert!(!apply_server_sets(conn, inbox, &unseen, &HashSet::new())?);
            assert_eq!(flags(conn, pk), (true, false));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn gmail_copies_share_flags_other_accounts_do_not() {
        for (provider, host, shared) in [
            ("gmail", "imap.gmail.com", true),
            ("custom", "IMAP.GMAIL.COM", true),
            ("custom", "imap.fastmail.com", false),
        ] {
            let db = Db::open_in_memory().unwrap();
            db.with(|conn| {
                let (inbox, important) = seed(conn, provider, host);
                let a = put(conn, inbox, 1, "<1@x>", "one", false);
                let b = put(conn, important, 7, "<1@x>", "one", false);
                let other = put(conn, inbox, 2, "<2@x>", "two", false);
                let got = with_shared_copies(conn, &[a, other])?;
                if shared {
                    assert_eq!(got, vec![a, b, other], "{provider} {host}");
                } else {
                    assert_eq!(got, vec![a, other], "{provider} {host}");
                }
                Ok(())
            })
            .unwrap();
        }
    }
}
