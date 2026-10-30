use super::draft_sync::*;
use crate::db::{bodies, draft_attachments, drafts, Db};

#[test]
fn successful_retry_clears_only_matching_failed_draft_saves() {
    let db = Db::open_in_memory().unwrap();
    db.with(|conn| {
        conn.execute("INSERT INTO accounts (id,email,provider,imap_host,smtp_host,created_at) VALUES ('a','one@example.com','gmail','imap.gmail.com','smtp',0),('b','two@example.com','custom','imap.example.com','smtp',0)", [])?;
        let draft = drafts::create(conn, "a", "new", None, "", "Subject", "Words")?;
        let other = drafts::create(conn, "a", "new", None, "", "Other", "Words")?;
        for (account, id, kind, state) in [
            ("a", draft.id, "save_draft", "failed"),
            ("a", draft.id, "save_draft", "pending"),
            ("a", draft.id, "send", "failed"),
            ("a", other.id, "save_draft", "failed"),
            ("b", draft.id, "save_draft", "failed"),
        ] {
            bodies::enqueue_op(conn, account, kind, &serde_json::json!({"draftId":id}))?;
            conn.execute("UPDATE pending_ops SET state=?1 WHERE id=last_insert_rowid()", [state])?;
        }
        let current = snapshot(conn, draft.id)?.unwrap();
        confirm(conn, draft.id, &current.digest)?;
        assert_eq!(conn.query_row("SELECT count(*) FROM pending_ops", [], |r| r.get::<_,i64>(0))?, 4);
        assert_eq!(conn.query_row("SELECT count(*) FROM pending_ops WHERE kind='save_draft' AND state='failed' AND account_id='a' AND json_extract(payload,'$.draftId')=?1", [draft.id], |r| r.get::<_,i64>(0))?, 0);
        assert_eq!(status(conn, draft.id)?.state, "synced");
        Ok(())
    }).unwrap();
}

#[test]
fn receipt_only_confirms_exact_content_including_same_size_replaced_files() {
    let db = Db::open_in_memory().unwrap();
    db.with(|conn| {
        conn.execute("INSERT INTO accounts (id,email,provider,imap_host,smtp_host,created_at) VALUES ('a','me@example.com','gmail','imap.gmail.com','smtp',0)",[])?;
        let mut draft=drafts::create(conn,"a","new",None,"reader@example.com","Subject","Words")?;
        drafts::ensure_imap_message_id(conn,draft.id,"first@example.com")?;
        assert_eq!(status(conn,draft.id)?.state,"local");
        assert!(status(conn,draft.id)?.gmail);
        bodies::enqueue_op(conn,"a","save_draft",&serde_json::json!({"draftId":draft.id}))?;
        assert_eq!(status(conn,draft.id)?.state,"pending");
        conn.execute("UPDATE pending_ops SET state='failed'",[])?;
        assert_eq!(status(conn,draft.id)?.state,"failed");
        let before=snapshot(conn,draft.id)?.unwrap();
        confirm(conn,draft.id,&before.digest)?;
        assert_eq!(status(conn,draft.id)?.state,"synced");
        conn.execute("DELETE FROM pending_ops",[])?;
        draft.body="Newer words".into();drafts::update(conn,&draft)?;
        confirm(conn,draft.id,&before.digest)?;
        assert_eq!(status(conn,draft.id)?.state,"local","an older request cannot certify newer edits");
        let current=snapshot(conn,draft.id)?.unwrap();
        assert_eq!(current.draft.body,"Newer words");
        confirm(conn,draft.id,&current.digest)?;
        super::compose::set_words_html(conn,draft.id,Some("<p><b>Newer words</b></p>"))?;
        assert_eq!(status(conn,draft.id)?.state,"local");
        let attachment=draft_attachments::add(conn,draft.id,"file.txt","text/plain",b"first")?;
        let current=snapshot(conn,draft.id)?.unwrap();
        assert!(current.html.unwrap().contains("<b>Newer words</b>"));
        confirm(conn,draft.id,&current.digest)?;
        draft_attachments::remove(conn,attachment.id)?;
        draft_attachments::add(conn,draft.id,"file.txt","text/plain",b"other")?;
        assert_eq!(status(conn,draft.id)?.state,"local","equal metadata does not prove equal bytes");
        let current=snapshot(conn,draft.id)?.unwrap();
        confirm(conn,draft.id,&current.digest)?;
        assert_eq!(status(conn,draft.id)?.state,"synced");
        drafts::delete(conn,draft.id)?;
        confirm(conn,draft.id,&current.digest)?;
        let replacement=drafts::create(conn,"a","new",None,"reader@example.com","Subject","Newer words")?;
        assert_eq!(status(conn,replacement.id)?.state,"local");
        Ok(())
    }).unwrap();
}
