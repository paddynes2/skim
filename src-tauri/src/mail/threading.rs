//! Gmail-style conversation threading.
//!
//! A message joins a thread when any of its `References` / `In-Reply-To` /
//! `Message-ID` identifiers match a message already in the database. When the
//! matches span several threads, those threads are merged into the oldest one.
//! Messages with no reference data fall back to a normalized-subject match,
//! guarded by account, a 30-day window and at least one shared participant so
//! that newsletters with identical subjects don't collapse into mega-threads.

use crate::db::models::NewMessage;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::BTreeSet;

const SUBJECT_FALLBACK_WINDOW_SECS: i64 = 30 * 24 * 3600;

/// Strip reply/forward prefixes and collapse whitespace; empty subjects
/// return `None` so they never participate in subject-fallback matching.
pub fn normalize_subject(subject: &str) -> Option<String> {
    let mut s = subject.trim();
    loop {
        let lower = s.to_lowercase();
        let mut stripped = false;
        for prefix in [
            "re:", "fwd:", "fw:", "aw:", "sv:", "vs:", "odp:", "antw:", "回复:", "回覆:", "转发:",
            "答复:", "res:", "rif:",
        ] {
            if lower.starts_with(prefix) {
                s = s[prefix.len()..].trim_start();
                stripped = true;
                break;
            }
        }
        // "Re[2]:" style
        if !stripped && (s.starts_with("Re[") || s.starts_with("re[") || s.starts_with("RE[")) {
            if let Some(end) = s.find("]:") {
                if s[3..end].chars().all(|c| c.is_ascii_digit()) {
                    s = s[end + 2..].trim_start();
                    stripped = true;
                }
            }
        }
        if !stripped {
            break;
        }
    }
    let norm = s
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    if norm.is_empty() {
        None
    } else {
        Some(norm)
    }
}

/// Normalize a Message-ID-like token: strip angle brackets and whitespace.
pub fn normalize_msgid(raw: &str) -> Option<String> {
    let s = raw
        .trim()
        .trim_start_matches('<')
        .trim_end_matches('>')
        .trim();
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

/// Resolve (or create) the thread for `msg`. Returns the thread id.
/// Runs inside the caller's transaction.
pub fn resolve_thread(conn: &Connection, msg: &NewMessage) -> rusqlite::Result<i64> {
    // 1. Reference-based match.
    let mut ref_ids: BTreeSet<String> = BTreeSet::new();
    for r in &msg.references {
        if let Some(id) = normalize_msgid(r) {
            ref_ids.insert(id);
        }
    }
    if let Some(irt) = msg.in_reply_to.as_deref().and_then(normalize_msgid_opt) {
        ref_ids.insert(irt);
    }
    if let Some(own) = msg.message_id.as_deref().and_then(normalize_msgid_opt) {
        // Another copy of this message (e.g. same mail in another Gmail label
        // folder) may already be threaded.
        ref_ids.insert(own);
    }

    let mut thread_ids: BTreeSet<i64> = BTreeSet::new();
    if !ref_ids.is_empty() {
        // Forward: messages whose Message-ID appears among our references.
        let mut forward = conn.prepare_cached(
            "SELECT DISTINCT thread_id FROM messages
             WHERE account_id = ?1 AND message_id = ?2 AND thread_id IS NOT NULL",
        )?;
        // Via references: messages that share any of these identifiers in
        // their own References/In-Reply-To. Covers both the parent arriving
        // after its replies (they reference us) and siblings that reference
        // a common ancestor we also reference — the usual cases when sync
        // walks newest-first.
        let mut via_refs = conn.prepare_cached(
            "SELECT DISTINCT m.thread_id FROM message_refs r
             JOIN messages m ON m.id = r.message_id
             WHERE m.account_id = ?1 AND r.ref = ?2 AND m.thread_id IS NOT NULL",
        )?;
        for id in &ref_ids {
            let found: Vec<i64> = forward
                .query_map(params![msg.account_id, id], |r| r.get(0))?
                .collect::<Result<_, _>>()?;
            thread_ids.extend(found);
            let found: Vec<i64> = via_refs
                .query_map(params![msg.account_id, id], |r| r.get(0))?
                .collect::<Result<_, _>>()?;
            thread_ids.extend(found);
        }
    }

    if !thread_ids.is_empty() {
        let mut iter = thread_ids.into_iter();
        let target = iter.next().expect("nonempty");
        // Merge any additional threads into the oldest (lowest id).
        for other in iter {
            merge_threads(conn, target, other)?;
        }
        return Ok(target);
    }

    // 2. Subject fallback — when the message carries no reference data, or
    // (fork, D71) when it is a reply whose references match nothing we hold.
    // A Gmail API draft can point at an ancestor the cache never saw; without
    // this its reply, and every answer to that reply, opened a new thread.
    let no_refs = msg.references.is_empty() && msg.in_reply_to.is_none();
    let subject_norm = msg.subject.as_deref().and_then(normalize_subject);
    let orphan_reply = !no_refs
        && msg.subject.as_deref().is_some_and(has_reply_prefix)
        && is_own_sender(conn, msg.from_addr.as_deref())?;
    if no_refs || orphan_reply {
        if let Some(ref norm) = subject_norm {
            let found = if no_refs {
                subject_fallback(conn, msg, norm)?
            } else {
                subject_fallback_before(conn, msg, norm, -1)?
            };
            if let Some(id) = found {
                return Ok(id);
            }
        }
    }

    // 3. New thread.
    conn.execute(
        "INSERT INTO threads (account_id, subject_norm, last_date, message_count, unread_count, starred, snippet)
         VALUES (?1, ?2, ?3, 0, 0, 0, NULL)",
        params![msg.account_id, subject_norm, msg.date],
    )?;
    Ok(conn.last_insert_rowid())
}

fn normalize_msgid_opt(raw: &str) -> Option<String> {
    normalize_msgid(raw)
}

/// Fork (D71): whether the message was sent by the user. The orphan rule is
/// only for the user's own replies: received mail keeps upstream threading.
fn is_own_sender(conn: &Connection, from: Option<&str>) -> rusqlite::Result<bool> {
    let Some(from) = from else { return Ok(false) };
    Ok(crate::fork::court::own_addresses(conn)?.contains(&from.trim().to_ascii_lowercase()))
}

/// Fork (D71): whether a subject carries a reply or forward prefix, so it
/// names a conversation that already exists.
pub fn has_reply_prefix(subject: &str) -> bool {
    match normalize_subject(subject) {
        Some(norm) => {
            norm != subject
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase()
        }
        None => false,
    }
}

/// thread_id, account_id, subject, from_addr, to_addrs, cc_addrs, date.
type FirstRow = (
    i64,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    i64,
);

/// Fork (D71): one-time repair for conversations split before the orphan
/// reply rule. A thread whose oldest message is a reply, whose references
/// point at nothing outside it, joins the conversation the subject fallback
/// finds for that message. Returns how many threads were merged.
pub fn rejoin_split_replies(conn: &Connection) -> rusqlite::Result<usize> {
    let firsts: Vec<i64> = conn
        .prepare(
            "SELECT m.id FROM messages m
             WHERE m.id = (SELECT m2.id FROM messages m2 WHERE m2.thread_id = m.thread_id
                           ORDER BY m2.date, m2.id LIMIT 1)
             ORDER BY m.date",
        )?
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    let mut merged = 0;
    for id in firsts {
        let row: Option<FirstRow> = conn
            .query_row(
                "SELECT thread_id, account_id, subject, from_addr, to_addrs, cc_addrs, date
                 FROM messages WHERE id = ?1",
                params![id],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                    ))
                },
            )
            .optional()?;
        let Some((thread_id, account_id, subject, from, to_json, cc_json, date)) = row else {
            continue; // merged away earlier in this pass
        };
        let Some(subject) = subject else { continue };
        if !has_reply_prefix(&subject) || !is_own_sender(conn, from.as_deref())? {
            continue;
        }
        let Some(norm) = normalize_subject(&subject) else {
            continue;
        };
        // References that resolve outside this thread mean it is joined already
        // or deliberately apart; leave it.
        let outside: bool = conn
            .prepare_cached(
                "SELECT 1 FROM message_refs r
                 JOIN messages p ON p.account_id = ?2 AND p.message_id = r.ref
                 WHERE r.message_id IN (SELECT id FROM messages WHERE thread_id = ?1)
                   AND p.thread_id <> ?1 LIMIT 1",
            )?
            .exists(params![thread_id, account_id])?;
        if outside {
            continue;
        }
        let msg = NewMessage {
            account_id: account_id.clone(),
            subject: Some(subject.clone()),
            from_addr: from,
            to_addrs: crate::db::queries::addresses_from_json(to_json.as_deref()),
            cc_addrs: crate::db::queries::addresses_from_json(cc_json.as_deref()),
            date,
            ..Default::default()
        };
        if let Some(target) = subject_fallback_before(conn, &msg, &norm, thread_id)? {
            merge_threads(conn, target, thread_id)?;
            merged += 1;
        }
    }
    Ok(merged)
}

/// Fork (D71): the stricter fallback for an orphan reply and for the repair.
/// The candidate must have started before `msg`, must not be `exclude`, and
/// must share a participant OTHER than the user's own addresses (every
/// conversation in the mailbox includes the user, so that match proves
/// nothing).
fn subject_fallback_before(
    conn: &Connection,
    msg: &NewMessage,
    subject_norm: &str,
    exclude: i64,
) -> rusqlite::Result<Option<i64>> {
    let own = crate::fork::court::own_addresses(conn)?;
    let mut participants: BTreeSet<String> = BTreeSet::new();
    if let Some(from) = &msg.from_addr {
        participants.insert(from.to_lowercase());
    }
    for a in msg.to_addrs.iter().chain(msg.cc_addrs.iter()) {
        participants.insert(a.addr.to_lowercase());
    }
    participants.retain(|p| !own.contains(p));
    if participants.is_empty() {
        return Ok(None);
    }
    let candidates: Vec<i64> = conn
        .prepare_cached(
            "SELECT t.id FROM threads t
             WHERE t.account_id = ?1 AND t.subject_norm = ?2 AND t.id <> ?5
               AND abs(t.last_date - ?3) <= ?4
               AND (SELECT min(m.date) FROM messages m WHERE m.thread_id = t.id) <= ?3
             ORDER BY t.last_date DESC LIMIT 8",
        )?
        .query_map(
            params![
                msg.account_id,
                subject_norm,
                msg.date,
                SUBJECT_FALLBACK_WINDOW_SECS,
                exclude
            ],
            |r| r.get(0),
        )?
        .collect::<Result<_, _>>()?;
    for thread_id in candidates {
        let rows: Vec<(Option<String>, Option<String>, Option<String>)> = conn
            .prepare_cached(
                "SELECT from_addr, to_addrs, cc_addrs FROM messages WHERE thread_id = ?1",
            )?
            .query_map(params![thread_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?
            .collect::<Result<_, _>>()?;
        for (from, to_json, cc_json) in rows {
            if from.is_some_and(|f| participants.contains(&f.to_lowercase())) {
                return Ok(Some(thread_id));
            }
            for json in [to_json, cc_json].into_iter().flatten() {
                if let Ok(addrs) = serde_json::from_str::<Vec<crate::db::models::Address>>(&json) {
                    if addrs
                        .iter()
                        .any(|a| participants.contains(&a.addr.to_lowercase()))
                    {
                        return Ok(Some(thread_id));
                    }
                }
            }
        }
    }
    Ok(None)
}

fn subject_fallback(
    conn: &Connection,
    msg: &NewMessage,
    subject_norm: &str,
) -> rusqlite::Result<Option<i64>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id FROM threads
         WHERE account_id = ?1 AND subject_norm = ?2
           AND abs(last_date - ?3) <= ?4
         ORDER BY last_date DESC LIMIT 8",
    )?;
    let candidates: Vec<i64> = stmt
        .query_map(
            params![
                msg.account_id,
                subject_norm,
                msg.date,
                SUBJECT_FALLBACK_WINDOW_SECS
            ],
            |r| r.get(0),
        )?
        .collect::<Result<_, _>>()?;

    // Participants of the incoming message.
    let mut participants: BTreeSet<String> = BTreeSet::new();
    if let Some(from) = &msg.from_addr {
        participants.insert(from.to_lowercase());
    }
    for a in msg.to_addrs.iter().chain(msg.cc_addrs.iter()) {
        participants.insert(a.addr.to_lowercase());
    }
    // Fork (D78): the user is on every message in the mailbox, so sharing them
    // proves nothing. Without this, every new email the user sent under one
    // subject (the same outreach to many people) joined one conversation.
    // Mail between the user's own addresses only keeps the old match.
    let own = crate::fork::court::own_addresses(conn)?;
    let others: BTreeSet<String> = participants
        .iter()
        .filter(|p| !own.contains(*p))
        .cloned()
        .collect();
    if !others.is_empty() {
        participants = others;
    }

    for thread_id in candidates {
        let mut stmt = conn.prepare_cached(
            "SELECT from_addr, to_addrs, cc_addrs FROM messages WHERE thread_id = ?1",
        )?;
        let rows: Vec<(Option<String>, Option<String>, Option<String>)> = stmt
            .query_map(params![thread_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?
            .collect::<Result<_, _>>()?;
        for (from, to_json, cc_json) in rows {
            if let Some(f) = from {
                if participants.contains(&f.to_lowercase()) {
                    return Ok(Some(thread_id));
                }
            }
            for json in [to_json, cc_json].into_iter().flatten() {
                if let Ok(addrs) = serde_json::from_str::<Vec<crate::db::models::Address>>(&json) {
                    if addrs
                        .iter()
                        .any(|a| participants.contains(&a.addr.to_lowercase()))
                    {
                        return Ok(Some(thread_id));
                    }
                }
            }
        }
    }
    Ok(None)
}

/// One message as the D78 repair sees it.
struct SplitRow {
    id: i64,
    msgid: Option<String>,
    refs: BTreeSet<String>,
    subject: Option<String>,
    people: BTreeSet<String>,
    date: i64,
}

/// Fork (D78): one-time repair for conversations the old subject fallback
/// joined only because the user was on both messages. Each thread is split
/// into the groups its own messages support: the same Message-ID, a
/// reference between them or to a shared ancestor, or the same subject
/// within the fallback window with a participant other than the user. The
/// group holding the oldest message keeps the thread id. Returns how many
/// threads were created.
pub fn split_subject_merges(conn: &Connection) -> rusqlite::Result<usize> {
    let own = crate::fork::court::own_addresses(conn)?;
    let threads: Vec<i64> = conn
        .prepare("SELECT thread_id FROM messages WHERE thread_id IS NOT NULL GROUP BY thread_id HAVING count(*) > 1")?
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    let mut created = 0;
    for thread_id in threads {
        let rows = split_rows(conn, thread_id, &own)?;
        let groups = split_groups(&rows);
        if groups.len() < 2 {
            continue;
        }
        let account_id: String = conn.query_row(
            "SELECT account_id FROM threads WHERE id = ?1",
            params![thread_id],
            |r| r.get(0),
        )?;
        // `groups` is ordered by each group's oldest message; the first stays.
        for group in groups.iter().skip(1) {
            let first = &rows[group[0]];
            let norm = first.subject.as_deref().and_then(normalize_subject);
            conn.execute(
                "INSERT INTO threads (account_id, subject_norm, last_date, message_count, unread_count, starred, snippet)
                 VALUES (?1, ?2, ?3, 0, 0, 0, NULL)",
                params![account_id, norm, first.date],
            )?;
            let new_id = conn.last_insert_rowid();
            let mut update =
                conn.prepare_cached("UPDATE messages SET thread_id = ?1 WHERE id = ?2")?;
            for &i in group {
                update.execute(params![new_id, rows[i].id])?;
            }
            recompute_thread(conn, new_id)?;
            created += 1;
        }
        recompute_thread(conn, thread_id)?;
    }
    Ok(created)
}

fn split_rows(
    conn: &Connection,
    thread_id: i64,
    own: &std::collections::HashSet<String>,
) -> rusqlite::Result<Vec<SplitRow>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, message_id, subject, from_addr, to_addrs, cc_addrs, date
         FROM messages WHERE thread_id = ?1 ORDER BY date, id",
    )?;
    let mut rows: Vec<SplitRow> = stmt
        .query_map(params![thread_id], |r| {
            let mut people = BTreeSet::new();
            if let Some(f) = r.get::<_, Option<String>>(3)? {
                people.insert(f.trim().to_lowercase());
            }
            for col in [4, 5] {
                if let Some(json) = r.get::<_, Option<String>>(col)? {
                    if let Ok(addrs) =
                        serde_json::from_str::<Vec<crate::db::models::Address>>(&json)
                    {
                        people.extend(addrs.into_iter().map(|a| a.addr.trim().to_lowercase()));
                    }
                }
            }
            // Mail between the user's own addresses keeps matching on them.
            if people.iter().any(|p| !own.contains(p)) {
                people.retain(|p| !own.contains(p));
            }
            Ok(SplitRow {
                id: r.get(0)?,
                msgid: r
                    .get::<_, Option<String>>(1)?
                    .as_deref()
                    .and_then(normalize_msgid),
                refs: BTreeSet::new(),
                subject: r.get(2)?,
                people,
                date: r.get(6)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    let mut refs = conn.prepare_cached("SELECT ref FROM message_refs WHERE message_id = ?1")?;
    for row in &mut rows {
        let found: Vec<String> = refs
            .query_map(params![row.id], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        row.refs = found.iter().filter_map(|s| normalize_msgid(s)).collect();
    }
    Ok(rows)
}

/// Connected groups of `rows` (index lists), ordered by their oldest member.
fn split_groups(rows: &[SplitRow]) -> Vec<Vec<usize>> {
    let mut parent: Vec<usize> = (0..rows.len()).collect();
    fn find(p: &mut [usize], i: usize) -> usize {
        let mut i = i;
        while p[i] != i {
            p[i] = p[p[i]];
            i = p[i];
        }
        i
    }
    let norms: Vec<Option<String>> = rows
        .iter()
        .map(|r| r.subject.as_deref().and_then(normalize_subject))
        .collect();
    let ids: Vec<BTreeSet<String>> = rows
        .iter()
        .map(|r| {
            let mut s = r.refs.clone();
            s.extend(r.msgid.clone());
            s
        })
        .collect();
    for a in 0..rows.len() {
        for b in (a + 1)..rows.len() {
            let (x, y) = (&rows[a], &rows[b]);
            let linked = !ids[a].is_disjoint(&ids[b]);
            let by_subject = norms[a].is_some()
                && norms[a] == norms[b]
                && (x.date - y.date).abs() <= SUBJECT_FALLBACK_WINDOW_SECS
                && !x.people.is_disjoint(&y.people);
            if linked || by_subject {
                let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
                if ra != rb {
                    parent[ra.max(rb)] = ra.min(rb);
                }
            }
        }
    }
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut root_at: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for i in 0..rows.len() {
        let root = find(&mut parent, i);
        let g = *root_at.entry(root).or_insert_with(|| {
            groups.push(Vec::new());
            groups.len() - 1
        });
        groups[g].push(i);
    }
    groups
}

fn merge_threads(conn: &Connection, target: i64, other: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE messages SET thread_id = ?1 WHERE thread_id = ?2",
        params![target, other],
    )?;
    conn.execute("DELETE FROM threads WHERE id = ?1", params![other])?;
    recompute_thread(conn, target)?;
    Ok(())
}

/// Recompute a thread's aggregate columns from its messages. Deletes the
/// thread row when no messages remain; returns whether the thread survives.
///
/// Fork (v1.1.9): `last_date` and `snippet` come from the newest message
/// outside a Drafts folder, so an unsent reply never moves or re-dates its
/// conversation in Inbox, Sent or Deals. A thread of drafts only falls back to
/// its newest draft.
///
/// Fork (v1.1.10, D63): `message_count` counts distinct Message-IDs, the way
/// `bodies::get_thread` shows the conversation, so a message Gmail files
/// under Inbox and Important counts once.
pub fn recompute_thread(conn: &Connection, thread_id: i64) -> rusqlite::Result<bool> {
    let row: Option<(i64, i64, i64, i64)> = conn
        .query_row(
            "SELECT count(DISTINCT COALESCE(m.message_id, 'pk:' || m.id)),
                    COALESCE(max(CASE WHEN f.role IS 'drafts' THEN NULL ELSE m.date END),
                             max(m.date)),
                    sum(m.is_read = 0), max(m.is_starred)
             FROM messages m LEFT JOIN folders f ON f.id = m.folder_id
             WHERE m.thread_id = ?1",
            params![thread_id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get::<_, Option<i64>>(1)?.unwrap_or(0),
                    r.get::<_, Option<i64>>(2)?.unwrap_or(0),
                    r.get::<_, Option<i64>>(3)?.unwrap_or(0),
                ))
            },
        )
        .optional()?;

    let (count, last_date, unread, starred) = row.unwrap_or((0, 0, 0, 0));
    if count == 0 {
        conn.execute("DELETE FROM threads WHERE id = ?1", params![thread_id])?;
        return Ok(false);
    }
    let snippet: Option<String> = conn
        .query_row(
            "SELECT m.snippet FROM messages m LEFT JOIN folders f ON f.id = m.folder_id
             WHERE m.thread_id = ?1
             ORDER BY f.role IS 'drafts', m.date DESC LIMIT 1",
            params![thread_id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    conn.execute(
        "UPDATE threads SET message_count = ?2, last_date = ?3, unread_count = ?4,
                            starred = ?5, snippet = ?6
         WHERE id = ?1",
        params![thread_id, count, last_date, unread, starred, snippet],
    )?;
    Ok(true)
}

#[cfg(test)]
mod d71_tests {
    use super::*;
    use crate::db::models::Address;
    use crate::db::{queries, Db};

    const ME: &str = "me@example.com";

    fn seed(conn: &Connection) -> rusqlite::Result<i64> {
        conn.execute(
            "INSERT INTO accounts (id, email, provider, imap_host, smtp_host, created_at)
             VALUES ('a1', ?1, 'custom', 'imap.example.com', 'smtp.example.com', 0)",
            params![ME],
        )?;
        conn.execute(
            "INSERT INTO folders (account_id, imap_name, role, display_name, unread_count, sort_order)
             VALUES ('a1', 'INBOX', 'inbox', 'Inbox', 0, 0)",
            [],
        )?;
        Ok(conn.last_insert_rowid())
    }

    #[allow(clippy::too_many_arguments)]
    fn msg(
        folder: i64,
        uid: u32,
        id: &str,
        irt: Option<&str>,
        subject: &str,
        from: &str,
        to: &str,
        date: i64,
    ) -> NewMessage {
        NewMessage {
            account_id: "a1".into(),
            folder_id: folder,
            uid,
            message_id: Some(format!("<{id}>")),
            in_reply_to: irt.map(|r| format!("<{r}>")),
            references: irt.map(|r| vec![format!("<{r}>")]).unwrap_or_default(),
            subject: Some(subject.into()),
            from_addr: Some(from.into()),
            to_addrs: vec![Address {
                name: None,
                addr: to.into(),
            }],
            date,
            ..Default::default()
        }
    }

    #[test]
    fn reply_prefix_detection() {
        assert!(has_reply_prefix("Re: Intro"));
        assert!(has_reply_prefix("RE:  FW: Intro"));
        assert!(!has_reply_prefix("Intro"));
        assert!(!has_reply_prefix("Regarding the intro"));
    }

    #[test]
    fn orphan_reply_joins_the_conversation_with_that_person() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            let f = seed(conn)?;
            let (_, t1) = queries::insert_message(
                conn,
                &msg(
                    f,
                    1,
                    "p1",
                    None,
                    "Intro | A & B",
                    "raees@ciba.test",
                    ME,
                    100,
                ),
            )?
            .unwrap();
            // Our reply points at an ancestor the cache never saw.
            let (_, t2) = queries::insert_message(
                conn,
                &msg(
                    f,
                    2,
                    "r1",
                    Some("unknown"),
                    "Re: Intro | A & B",
                    ME,
                    "raees@ciba.test",
                    200,
                ),
            )?
            .unwrap();
            assert_eq!(t1, t2);
            // A same-subject reply to someone else stays apart: only I am shared.
            let (_, t3) = queries::insert_message(
                conn,
                &msg(
                    f,
                    3,
                    "r2",
                    Some("unknown2"),
                    "Re: Intro | A & B",
                    ME,
                    "other@x.test",
                    300,
                ),
            )?
            .unwrap();
            assert_ne!(t1, t3);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn repair_rejoins_a_split_conversation_and_its_answers() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            let f = seed(conn)?;
            let (_, t1) = queries::insert_message(conn, &msg(f, 1, "p1", None, "Handover", "jess@cb.test", ME, 100))?.unwrap();
            // Simulate the old behaviour: our reply and her answer in a thread of their own.
            conn.execute(
                "INSERT INTO threads (account_id, subject_norm, last_date, message_count, unread_count, starred)
                 VALUES ('a1', 'handover', 300, 0, 0, 0)",
                [],
            )?;
            let split = conn.last_insert_rowid();
            for (uid, id, irt, from, to, date) in [(2, "r1", "ghost", ME, "jess@cb.test", 200), (3, "a1", "r1", "jess@cb.test", ME, 300)] {
                let (pk, t) = queries::insert_message(conn, &msg(f, uid, id, Some(irt), "Re: Handover", from, to, date))?.unwrap();
                conn.execute("UPDATE messages SET thread_id = ?1 WHERE id = ?2", params![split, pk])?;
                if t != split && t != t1 {
                    conn.execute("DELETE FROM threads WHERE id = ?1", params![t])?;
                }
            }
            recompute_thread(conn, t1)?;
            recompute_thread(conn, split)?;
            assert_eq!(rejoin_split_replies(conn)?, 1);
            let n: i64 = conn.query_row("SELECT count(*) FROM messages WHERE thread_id = ?1", params![t1], |r| r.get(0))?;
            assert_eq!(n, 3);
            assert_eq!(rejoin_split_replies(conn)?, 0, "second run finds nothing");
            Ok(())
        })
        .unwrap();
    }
}
