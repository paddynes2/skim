//! Fork (v1.1.35, D82): a local draft whose server copy is gone goes too.
//!
//! Opening a server draft makes a local `drafts` row that mirrors it
//! (`origin_message_id` + `imap_message_id`). When that draft was deleted or
//! sent elsewhere (Gmail web, the phone, an agent), sync removed the Drafts
//! message but nothing removed the mirror. A reply mirror kept its thread
//! "on me" with the draft reason in the court, and a reopen could show stale
//! text. The owner's cache held 73 such rows on 27 Oct.
//!
//! A mirror is dropped only when all of these hold:
//! - it is server-backed (`origin_message_id` and `imap_message_id` set; a
//!   local-only draft is never touched);
//! - no message with its Message-ID is left in a Drafts folder (a save in
//!   Skim APPENDs a new copy before it expunges the old one, and the new copy
//!   is fetched before flags are reconciled). The origin row is not the test:
//!   SQLite reuses a freed `messages.id`, so a dead mirror's origin can point
//!   at an unrelated message (seen on the owner's cache);
//! - no op of any state names it (a queued or failed save or send holds
//!   edits the server has not got).
//!
//! Nothing is lost: with no op pending, the server copy held the content.

use rusqlite::Connection;

const ORPHANS: &str = "
    SELECT d.id FROM drafts d
    WHERE d.origin_message_id IS NOT NULL
      AND d.imap_message_id IS NOT NULL
      AND NOT EXISTS (
            SELECT 1 FROM messages m JOIN folders f ON f.id = m.folder_id
            WHERE m.account_id = d.account_id
              AND m.message_id = d.imap_message_id
              AND f.role = 'drafts')
      AND NOT EXISTS (
            SELECT 1 FROM pending_ops p
            WHERE p.account_id = d.account_id
              AND json_extract(p.payload, '$.draftId') = d.id)";

/// Delete every orphaned draft mirror. Returns how many went.
pub fn drop_orphaned(conn: &Connection) -> rusqlite::Result<usize> {
    let n = conn.execute(&format!("DELETE FROM drafts WHERE id IN ({ORPHANS})"), [])?;
    if n > 0 {
        tracing::info!(dropped = n, "draft mirrors whose server copy is gone");
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::drop_orphaned;
    use crate::db::drafts;
    use crate::db::models::NewMessage;
    use crate::db::queries::insert_message;
    use crate::db::Db;
    use rusqlite::{params, Connection};

    fn seed(conn: &Connection) -> rusqlite::Result<(i64, i64)> {
        conn.execute_batch(
            "INSERT INTO accounts (id, email, provider, imap_host, smtp_host, created_at)
             VALUES ('a1', 'work@example.com', 'custom', 'imap.example.com', 'smtp.example.com', 0);
             INSERT INTO folders (account_id, imap_name, role, display_name, unread_count, sort_order)
             VALUES ('a1', 'Sent', 'sent', 'Sent', 0, 10), ('a1', 'Drafts', 'drafts', 'Drafts', 0, 11);",
        )?;
        let id = |name: &str| -> rusqlite::Result<i64> {
            conn.query_row(
                "SELECT id FROM folders WHERE account_id = 'a1' AND imap_name = ?1",
                params![name],
                |r| r.get(0),
            )
        };
        Ok((id("Sent")?, id("Drafts")?))
    }

    fn add(conn: &mut Connection, folder: i64, uid: u32, msgid: &str) -> i64 {
        insert_message(
            conn,
            &NewMessage {
                account_id: "a1".into(),
                folder_id: folder,
                uid,
                message_id: Some(msgid.into()),
                subject: Some("Re: Agreement".into()),
                date: 100,
                ..Default::default()
            },
        )
        .unwrap()
        .unwrap()
        .0
    }

    fn mirror(conn: &Connection, origin: i64, msgid: &str) -> i64 {
        drafts::create_server_draft(
            conn,
            "a1",
            "reply",
            None,
            "x@y",
            "",
            "",
            "Re: Agreement",
            "words",
            origin,
            msgid,
        )
        .unwrap()
        .id
    }

    fn exists(conn: &Connection, draft: i64) -> bool {
        drafts::get(conn, draft).unwrap().is_some()
    }

    fn remove(conn: &Connection, message: i64) {
        conn.execute("DELETE FROM messages WHERE id = ?1", params![message])
            .unwrap();
    }

    #[test]
    fn a_mirror_whose_server_copy_was_deleted_goes() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            let (_, dr) = seed(conn)?;
            let m = add(conn, dr, 1, "d1@x");
            let d = mirror(conn, m, "d1@x");
            assert_eq!(drop_orphaned(conn)?, 0, "server copy still there");
            remove(conn, m);
            assert_eq!(drop_orphaned(conn)?, 1);
            assert!(!exists(conn, d));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn a_mirror_sent_elsewhere_goes_though_sent_keeps_its_message_id() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            let (sent, dr) = seed(conn)?;
            let m = add(conn, dr, 1, "d1@x");
            let d = mirror(conn, m, "d1@x");
            remove(conn, m);
            add(conn, sent, 1, "d1@x");
            assert_eq!(drop_orphaned(conn)?, 1);
            assert!(!exists(conn, d));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn a_save_in_skim_replaces_the_copy_and_keeps_the_mirror() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            let (_, dr) = seed(conn)?;
            let m = add(conn, dr, 1, "d1@x");
            let d = mirror(conn, m, "d1@x");
            // APPEND of the new copy (same Message-ID, new UID), then expunge.
            add(conn, dr, 2, "d1@x");
            remove(conn, m);
            assert_eq!(drop_orphaned(conn)?, 0);
            assert!(exists(conn, d));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn a_queued_or_failed_op_keeps_the_mirror() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            let (_, dr) = seed(conn)?;
            for (uid, state) in [(1u32, "pending"), (2, "failed")] {
                let msgid = format!("d{uid}@x");
                let m = add(conn, dr, uid, &msgid);
                let d = mirror(conn, m, &msgid);
                conn.execute(
                    "INSERT INTO pending_ops (account_id, kind, payload, created_at, attempts, state)
                     VALUES ('a1', 'save_draft', json_object('draftId', ?1), 0, 0, ?2)",
                    params![d, state],
                )?;
                remove(conn, m);
                assert_eq!(drop_orphaned(conn)?, 0, "{state}");
                assert!(exists(conn, d), "{state}");
            }
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn a_reused_origin_id_does_not_keep_a_dead_mirror() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            let (sent, dr) = seed(conn)?;
            let m = add(conn, dr, 1, "d1@x");
            let d = mirror(conn, m, "d1@x");
            remove(conn, m);
            // The freed id goes to the next message, an unrelated one.
            assert_eq!(add(conn, sent, 7, "other@x"), m);
            assert_eq!(drop_orphaned(conn)?, 1);
            assert!(!exists(conn, d));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn a_local_only_draft_is_never_touched() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            seed(conn)?;
            let d = drafts::create(conn, "a1", "new", None, "x@y", "Hi", "words")?;
            assert_eq!(drop_orphaned(conn)?, 0);
            assert!(exists(conn, d.id));
            Ok(())
        })
        .unwrap();
    }
}
