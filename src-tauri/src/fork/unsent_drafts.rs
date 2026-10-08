//! Fork (v1.1.9): an unsent draft never reads as sent mail. Covers the three
//! places that rule lives: the thread summary (`threading::recompute_thread`),
//! the grouped folder lists (`fork::list::ROW_DATE`) and the conversation
//! (`bodies::get_thread`). A message is a draft only when every copy of it is
//! in a Drafts folder.
#![cfg(test)]

use crate::db::models::NewMessage;
use crate::db::queries::{insert_message, list_threads, list_unified_threads};
use crate::db::Db;
use rusqlite::{params, Connection};

/// One account with an inbox, plus its Sent and Drafts folders.
fn seed(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "INSERT INTO accounts (id, email, provider, imap_host, smtp_host, created_at)
         VALUES ('a1', 'work@example.com', 'custom', 'imap.example.com', 'smtp.example.com', 0);
         INSERT INTO folders (account_id, imap_name, role, display_name, unread_count, sort_order)
         VALUES ('a1', 'INBOX', 'inbox', 'Inbox', 0, 0);",
    )
}

fn folder_id(conn: &Connection, account: &str, imap_name: &str) -> i64 {
    conn.query_row(
        "SELECT id FROM folders WHERE account_id = ?1 AND imap_name = ?2",
        params![account, imap_name],
        |r| r.get(0),
    )
    .unwrap()
}

/// Fork (v1.1.9): file a message for account a1, optionally as a reply, so
/// a reply draft joins its parent's thread.
fn add_reply(
    conn: &mut Connection,
    folder: i64,
    uid: u32,
    msgid: &str,
    parent: Option<&str>,
    subject: &str,
    date: i64,
) {
    insert_message(
        conn,
        &NewMessage {
            account_id: "a1".into(),
            folder_id: folder,
            uid,
            message_id: Some(msgid.into()),
            in_reply_to: parent.map(Into::into),
            references: parent.map(|p| vec![p.to_string()]).unwrap_or_default(),
            subject: Some(subject.into()),
            snippet: Some(format!("{subject} body")),
            from_addr: Some("work@example.com".into()),
            date,
            is_read: true,
            ..Default::default()
        },
    )
    .unwrap();
}

fn sent_and_drafts(conn: &Connection) -> rusqlite::Result<(i64, i64)> {
    conn.execute_batch(
        "INSERT INTO folders (account_id, imap_name, role, display_name, unread_count, sort_order)
         VALUES ('a1', 'Sent', 'sent', 'Sent', 0, 10), ('a1', 'Drafts', 'drafts', 'Drafts', 0, 11);",
    )?;
    Ok((
        folder_id(conn, "a1", "Sent"),
        folder_id(conn, "a1", "Drafts"),
    ))
}

#[test]
fn a_reply_draft_neither_moves_nor_redates_its_conversation() {
    let db = Db::open_in_memory().unwrap();
    db.with(|conn| {
        seed(conn)?;
        let (sent, drafts) = sent_and_drafts(conn)?;
        add_reply(conn, sent, 1, "<s1@x>", None, "Agreement", 100);
        add_reply(conn, sent, 2, "<s2@x>", None, "Other", 200);
        // The unsent reply is the newest message of "Agreement".
        add_reply(
            conn,
            drafts,
            1,
            "<d1@x>",
            Some("<s1@x>"),
            "Re: Agreement",
            300,
        );

        let rows = list_threads(conn, sent, 0, 10)?;
        let shape: Vec<_> = rows.iter().map(|r| (r.subject.as_str(), r.date)).collect();
        assert_eq!(shape, [("Other", 200), ("Agreement", 100)]);
        let unified = list_unified_threads(conn, Some("sent"), None, 0, 10)?;
        let shape: Vec<_> = unified
            .iter()
            .map(|r| (r.subject.as_str(), r.date))
            .collect();
        assert_eq!(shape, [("Other", 200), ("Agreement", 100)]);

        // The thread's own summary ignores the draft too…
        let summary: (i64, String) = conn.query_row(
            "SELECT t.last_date, t.snippet FROM threads t
             JOIN messages m ON m.thread_id = t.id WHERE m.message_id = 'd1@x'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        assert_eq!(summary, (100, "Agreement body".to_string()));

        // …while Drafts keeps the draft's own time.
        let rows = list_threads(conn, drafts, 0, 10)?;
        let shape: Vec<_> = rows.iter().map(|r| (r.subject.as_str(), r.date)).collect();
        assert_eq!(shape, [("Re: Agreement", 300)]);
        let unified = list_unified_threads(conn, Some("drafts"), None, 0, 10)?;
        assert_eq!(unified[0].date, 300);
        Ok(())
    })
    .unwrap();
}

#[test]
fn a_thread_of_drafts_only_dates_by_its_draft() {
    let db = Db::open_in_memory().unwrap();
    db.with(|conn| {
        seed(conn)?;
        let (_, drafts) = sent_and_drafts(conn)?;
        add_reply(conn, drafts, 1, "<d1@x>", None, "New idea", 500);
        let summary: (i64, String) =
            conn.query_row("SELECT last_date, snippet FROM threads", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?;
        assert_eq!(summary, (500, "New idea body".to_string()));
        Ok(())
    })
    .unwrap();
}

#[test]
fn get_thread_marks_only_unsent_drafts() {
    let db = Db::open_in_memory().unwrap();
    db.with(|conn| {
        seed(conn)?;
        let (sent, drafts) = sent_and_drafts(conn)?;
        add_reply(conn, sent, 1, "<s1@x>", None, "Agreement", 100);
        add_reply(
            conn,
            drafts,
            1,
            "<d1@x>",
            Some("<s1@x>"),
            "Re: Agreement",
            300,
        );
        // Just sent: the Drafts copy waits for the next sync while the Sent
        // copy already carries the same Message-ID. That is sent mail.
        add_reply(
            conn,
            drafts,
            2,
            "<r2@x>",
            Some("<s1@x>"),
            "Re: Agreement",
            400,
        );
        add_reply(
            conn,
            sent,
            2,
            "<r2@x>",
            Some("<s1@x>"),
            "Re: Agreement",
            410,
        );
        let thread: i64 = conn.query_row(
            "SELECT thread_id FROM messages WHERE folder_id = ?1 AND uid = 1",
            params![sent],
            |r| r.get(0),
        )?;
        let detail = crate::db::bodies::get_thread(conn, thread)?.unwrap();
        let shape: Vec<_> = detail
            .messages
            .iter()
            .map(|m| (m.date, m.is_draft))
            .collect();
        assert_eq!(shape, [(100, false), (300, true), (400, false)]);
        Ok(())
    })
    .unwrap();
}
