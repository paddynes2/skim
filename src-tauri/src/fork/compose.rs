//! Fork (6.3): rich text for the composer.
//!
//! The data model keeps every upstream text path intact (D5): `drafts.body_text`
//! stays the full plain-text body (the user's words, the signature, the quoted
//! original) and is what the AI co-author, the Drafts write-back and the
//! text/plain part all read. Only the user's own words are held as HTML, in
//! `fork_draft_html`; the signature and the quote are rendered here at send
//! time from what the database already holds, so the two parts of the
//! `multipart/alternative` can never disagree about what was said.

use crate::db::drafts::Draft;
use crate::db::{bodies, Db};
use crate::error::Result;
use crate::state::AppState;
use rusqlite::{Connection, OptionalExtension};
use tauri::State;

/// The stored HTML of the user's words, if the rich editor wrote one.
pub fn get_words_html(conn: &Connection, draft_id: i64) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT words_html FROM fork_draft_html WHERE draft_id = ?1",
        rusqlite::params![draft_id],
        |r| r.get(0),
    )
    .optional()
}

/// Store (or, with `None`, drop) the HTML of the user's words. Dropping the
/// row puts the draft back on the text/plain path.
pub fn set_words_html(
    conn: &Connection,
    draft_id: i64,
    words_html: Option<&str>,
) -> rusqlite::Result<()> {
    match words_html {
        Some(html) => {
            conn.execute(
                "INSERT INTO fork_draft_html (draft_id, words_html, updated_at)
                 VALUES (?1, ?2, unixepoch())
                 ON CONFLICT(draft_id) DO UPDATE SET words_html = excluded.words_html,
                                                    updated_at = excluded.updated_at",
                rusqlite::params![draft_id, html],
            )?;
        }
        None => {
            conn.execute(
                "DELETE FROM fork_draft_html WHERE draft_id = ?1",
                rusqlite::params![draft_id],
            )?;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn fork_draft_html_get(
    state: State<'_, AppState>,
    draft_id: i64,
) -> Result<Option<String>> {
    state
        .db
        .read("fork_draft_html_get", move |conn| {
            get_words_html(conn, draft_id)
        })
        .await
}

#[tauri::command]
pub async fn fork_draft_html_set(
    state: State<'_, AppState>,
    draft_id: i64,
    words_html: Option<String>,
) -> Result<()> {
    state
        .db
        .call(move |conn| set_words_html(conn, draft_id, words_html.as_deref()))
        .await
}

/// Minimal HTML escaping for text we place inside our own markup.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The attribution line the plain body carries above the quoted original
/// (`On {date}, {who} wrote:`), so the HTML quote says exactly the same thing.
/// The composer's `splitTail` finds the quote by this anchor too.
pub fn attribution_line(body_text: &str) -> Option<&str> {
    let start = body_text.find("\n\nOn ")? + 2;
    let rest = &body_text[start..];
    let end = rest.find('\n').unwrap_or(rest.len());
    let line = &rest[..end];
    line.ends_with(" wrote:").then_some(line)
}

/// Inline images of the quoted original point at the app's own `cid` protocol
/// after sanitising; they mean nothing to a recipient, so they are dropped.
fn strip_cid_images(html: &str) -> String {
    let marker = "http://skim-cid.localhost/";
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(at) = rest.find("<img") {
        let (before, tag_on) = rest.split_at(at);
        out.push_str(before);
        let Some(close) = tag_on.find('>') else {
            out.push_str(tag_on);
            return out;
        };
        let tag = &tag_on[..=close];
        if !tag.contains(marker) {
            out.push_str(tag);
        }
        rest = &tag_on[close + 1..];
    }
    out.push_str(rest);
    out
}

/// The quoted original as safe HTML: the stored HTML sanitised with the same
/// allowlist the viewer uses (remote images kept: they were the sender's), or
/// the plain text escaped into a wrapping block.
fn original_html(conn: &Connection, message_id: i64) -> rusqlite::Result<Option<String>> {
    let Some((html, text)) = bodies::get_body(conn, message_id)? else {
        return Ok(None);
    };
    Ok(match (html, text) {
        (Some(h), _) if !h.trim().is_empty() => Some(strip_cid_images(
            &crate::mail::sanitize::sanitize_email_html(&h, message_id, true).html,
        )),
        (_, Some(t)) if !t.is_empty() => Some(format!(
            "<div style=\"white-space:pre-wrap\">{}</div>",
            escape(&t)
        )),
        _ => None,
    })
}

/// Assemble the text/html part: the user's words (sanitised with the viewer's
/// allowlist), then the signature, then the quoted original under its
/// attribution line, in the markup every client folds (`gmail_quote` +
/// `blockquote type="cite"`).
pub fn html_part(words_html: &str, signature: Option<&str>, quote: Option<(&str, &str)>) -> String {
    let words = crate::mail::sanitize::sanitize_email_html(words_html, 0, true).html;
    let mut out = format!("<div class=\"skim-words\">{words}</div>");
    if let Some(sig) = signature.map(str::trim).filter(|s| !s.is_empty()) {
        let lines: Vec<String> = sig.lines().map(escape).collect();
        out.push_str(&format!(
            "<div class=\"skim-sig\">-- <br>{}</div>",
            lines.join("<br>")
        ));
    }
    if let Some((attribution, original)) = quote {
        out.push_str(&format!(
            "<div class=\"gmail_quote\">{}<blockquote type=\"cite\" style=\"margin:0 0 0 .8ex;border-left:1px solid #ccc;padding-left:1ex\">{original}</blockquote></div>",
            escape(attribution)
        ));
    }
    out
}

/// The HTML rendition of a draft for sending, or `None` when the rich editor
/// never wrote one (the message then goes out text/plain, as before).
pub fn outgoing_html_sync(conn: &Connection, draft: &Draft) -> rusqlite::Result<Option<String>> {
    let Some(words) = get_words_html(conn, draft.id)? else {
        return Ok(None);
    };
    let signature: Option<String> = conn
        .query_row(
            "SELECT signature FROM accounts WHERE id = ?1",
            rusqlite::params![draft.account_id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    // The signature is in the HTML only when it is in the text: a user who
    // deleted the sign-off from the body must not get it back in HTML.
    let signature = signature.filter(|s| {
        let block = crate::mail::smtp::signature_block(Some(s));
        !block.is_empty() && draft.body.contains(&block)
    });
    let quote = match (attribution_line(&draft.body), draft.reply_to_message_id) {
        (Some(attribution), Some(message_id)) => {
            original_html(conn, message_id)?.map(|html| (attribution.to_string(), html))
        }
        _ => None,
    };
    Ok(Some(html_part(
        &words,
        signature.as_deref(),
        quote.as_ref().map(|(a, h)| (a.as_str(), h.as_str())),
    )))
}

/// Async wrapper for the send / save-draft paths in `mail/sync.rs`: one call,
/// one `Option<String>` to hand to `smtp::build_message_with_html`.
pub async fn outgoing_html(db: &Db, draft_id: i64) -> Option<String> {
    db.call(move |conn| {
        let Some(draft) = crate::db::drafts::get(conn, draft_id)? else {
            return Ok(None);
        };
        outgoing_html_sync(conn, &draft)
    })
    .await
    .ok()
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::Account;
    use crate::mail::smtp::{build_message_with_html, OutgoingRefs};
    use mail_parser::{MessageParser, MimeHeaders};

    fn account() -> Account {
        Account {
            id: "a1".into(),
            email: "me@example.com".into(),
            imap_user: None,
            display_name: Some("Me".into()),
            provider: "gmail".into(),
            imap_host: "imap.gmail.com".into(),
            imap_port: 993,
            smtp_host: "smtp.gmail.com".into(),
            smtp_port: 587,
            smtp_security: "tls".into(),
            auth_kind: "password".into(),
            signature: Some("Patrick".into()),
        }
    }

    fn draft(body: &str) -> Draft {
        Draft {
            id: 1,
            account_id: "a1".into(),
            reply_to_message_id: None,
            mode: "new".into(),
            to: "you@example.com".into(),
            cc: String::new(),
            bcc: String::new(),
            subject: "Hi".into(),
            body: body.into(),
            origin_message_id: None,
        }
    }

    fn no_refs() -> OutgoingRefs {
        OutgoingRefs {
            in_reply_to: None,
            references: Vec::new(),
        }
    }

    #[test]
    fn attribution_is_the_plain_bodys_own_line() {
        let body = "Thanks\n\n-- \nPatrick\n\nOn 2026-09-23 10:00, Bob <bob@x.com> wrote:\n> hi\n";
        assert_eq!(
            attribution_line(body),
            Some("On 2026-09-23 10:00, Bob <bob@x.com> wrote:")
        );
        assert_eq!(attribution_line("Thanks\n\nOn my way home\n"), None);
        assert_eq!(attribution_line("no quote"), None);
    }

    #[test]
    fn html_part_has_words_signature_and_a_cite_quote() {
        let html = html_part(
            "<p>Hello <b>Bob</b></p>",
            Some("Patrick\nCA(SA)"),
            Some(("On Mon, Bob <bob@x.com> wrote:", "<p>hi</p>")),
        );
        assert!(html.starts_with("<div class=\"skim-words\"><p>Hello <b>Bob</b></p></div>"));
        assert!(html.contains("<div class=\"skim-sig\">-- <br>Patrick<br>CA(SA)</div>"));
        assert!(html.contains(
            "<div class=\"gmail_quote\">On Mon, Bob &lt;bob@x.com&gt; wrote:<blockquote type=\"cite\""
        ));
        assert!(html.ends_with("<p>hi</p></blockquote></div>"));
    }

    #[test]
    fn html_part_sanitises_the_words() {
        let html = html_part(
            "<p onclick=\"x()\">Hi<script>alert(1)</script></p><a href=\"javascript:1\">l</a>",
            None,
            None,
        );
        assert!(!html.contains("script"));
        assert!(!html.contains("onclick"));
        assert!(!html.contains("javascript:"));
        assert!(html.contains("<p>Hi</p>"));
    }

    #[test]
    fn cid_images_are_dropped_and_others_kept() {
        let html =
            "a<img src=\"http://skim-cid.localhost/3/x\" alt=\"\">b<img src=\"https://x/y.png\">c";
        assert_eq!(strip_cid_images(html), "ab<img src=\"https://x/y.png\">c");
        assert_eq!(strip_cid_images("<img"), "<img");
    }

    #[test]
    fn mime_is_alternative_with_the_text_part_equal_to_body_text() {
        let d = draft("Hello Bob\n\n-- \nPatrick");
        let html = html_part("<p>Hello <b>Bob</b></p>", Some("Patrick"), None);
        let raw =
            build_message_with_html(&account(), &d, &no_refs(), &[], None, false, Some(&html))
                .unwrap();
        let parsed = MessageParser::default().parse(&raw).unwrap();
        assert!(parsed
            .content_type()
            .is_some_and(|ct| ct.subtype() == Some("alternative")));
        // lettre puts the wire's CRLF on the text; the words are body_text exactly.
        assert_eq!(
            parsed.body_text(0).map(|t| t.replace("\r\n", "\n")),
            Some(d.body.clone())
        );
        let got_html = parsed.body_html(0).unwrap();
        assert!(got_html.contains("<b>Bob</b>"));
        assert!(got_html.contains("skim-sig"));
    }

    #[test]
    fn mime_with_attachments_is_mixed_over_alternative() {
        let d = draft("Body");
        let att = vec![(
            "a.txt".to_string(),
            "text/plain".to_string(),
            b"xyz".to_vec(),
        )];
        let raw = build_message_with_html(
            &account(),
            &d,
            &no_refs(),
            &att,
            None,
            false,
            Some("<p>Body</p>"),
        )
        .unwrap();
        let parsed = MessageParser::default().parse(&raw).unwrap();
        assert!(parsed
            .content_type()
            .is_some_and(|ct| ct.subtype() == Some("mixed")));
        assert_eq!(parsed.body_text(0).as_deref(), Some("Body"));
        assert_eq!(parsed.body_html(0).as_deref(), Some("<p>Body</p>"));
        let att = parsed.attachment(0).expect("the file rides along");
        assert_eq!(att.attachment_name(), Some("a.txt"));
        assert_eq!(att.contents(), b"xyz");
    }

    #[test]
    fn no_html_is_text_plain_as_before() {
        let d = draft("Body");
        let raw =
            build_message_with_html(&account(), &d, &no_refs(), &[], None, false, None).unwrap();
        let parsed = MessageParser::default().parse(&raw).unwrap();
        // (mail-parser synthesises an HTML view of a text part, so ask the wire.)
        assert!(
            !String::from_utf8_lossy(&raw).contains("text/html"),
            "no HTML part was made up"
        );
        assert_eq!(parsed.body_text(0).as_deref(), Some("Body"));
        // The two entry points agree byte for byte.
        let legacy =
            crate::mail::smtp::build_message(&account(), &d, &no_refs(), &[], None, false).unwrap();
        let strip_ids = |raw: &[u8]| -> String {
            String::from_utf8_lossy(raw)
                .lines()
                .filter(|l| !l.starts_with("Message-ID") && !l.starts_with("Date"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        assert_eq!(strip_ids(&raw), strip_ids(&legacy));
    }

    #[test]
    fn outgoing_html_follows_the_row_and_the_quoted_original() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            conn.execute_batch(
                "INSERT INTO accounts (id, email, provider, imap_host, smtp_host, signature, created_at)
                   VALUES ('a1','me@x','gmail','imap.gmail.com','smtp.gmail.com','Patrick',0);
                 INSERT INTO folders (id, account_id, imap_name, role, display_name, sort_order)
                   VALUES (1,'a1','INBOX','inbox','Inbox',0);
                 INSERT INTO messages (id, account_id, folder_id, uid, date, is_read, is_starred, from_name, from_addr)
                   VALUES (7,'a1',1,1,10,1,0,'Bob','bob@x');
                 INSERT INTO message_bodies (message_id, body_html, body_text)
                   VALUES (7,'<p>hi <script>x</script>there</p>','hi there');",
            )?;
            let body = "Thanks\n\n-- \nPatrick\n\nOn 2026-09-23 10:00, Bob <bob@x> wrote:\n> hi there\n";
            let d = crate::db::drafts::create(conn, "a1", "reply", Some(7), "bob@x", "Re: x", body)?;
            assert_eq!(outgoing_html_sync(conn, &d)?, None, "no row: text/plain");
            set_words_html(conn, d.id, Some("<p>Thanks</p>"))?;
            let html = outgoing_html_sync(conn, &d)?.expect("row: html");
            assert!(html.contains("<p>Thanks</p>"));
            assert!(html.contains("skim-sig\">-- <br>Patrick"));
            assert!(html.contains("On 2026-09-23 10:00, Bob &lt;bob@x&gt; wrote:"));
            assert!(html.contains("<p>hi there</p>"), "{html}");
            assert!(!html.contains("script"));
            // The user deleted the sign-off: it must not come back in HTML.
            let mut bare = d.clone();
            bare.body = "Thanks\n\nOn 2026-09-23 10:00, Bob <bob@x> wrote:\n> hi there\n".into();
            let html = outgoing_html_sync(conn, &bare)?.unwrap();
            assert!(!html.contains("skim-sig"));
            // Dropping the row goes back to text/plain, and deleting the draft
            // takes the row with it.
            set_words_html(conn, d.id, None)?;
            assert_eq!(get_words_html(conn, d.id)?, None);
            set_words_html(conn, d.id, Some("<p>x</p>"))?;
            crate::db::drafts::delete(conn, d.id)?;
            assert_eq!(get_words_html(conn, d.id)?, None, "cascade");
            Ok(())
        })
        .unwrap();
    }
}
