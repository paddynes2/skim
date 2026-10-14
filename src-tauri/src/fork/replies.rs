//! To reply (v1.1.19, D70): the conversations that wait on a reply from the
//! user, as one list in the sidebar.
//!
//! The court (`fork::court`) already knows which conversations have someone
//! else's message last (`on_me`). On its own that was 351 conversations in 30
//! days on the owner's mailbox, which is why its "On me" view went unused
//! (D54). This module adds one more TypeSafe Jev question about that last
//! message: does the sender expect a personal reply? Measured before the
//! build on 126 of the owner's own messages from 90 days (replied to within 14
//! days versus not): AUC 0.86, against 0.73 for the Priority score; at
//! [`THRESHOLD_DEFAULT`] it picks 49 of the 56 he replied to.
//!
//! A conversation is in To reply when ([`decide`]):
//! - the user marked it `needs` (a reply is owed), until it is trashed or
//!   archived out of the Inbox; or
//! - it is `on_me`, someone else wrote its last message, that message is
//!   inside the court's look-back window (`fork_court_window_days`, default
//!   30), it is not marked `done`, and
//!   - a local reply draft was saved on it at or after that message (an older
//!     draft is a leftover: the reply may have gone out from Gmail or the
//!     phone, which never deletes Skim's draft row); or
//!   - it is not a colleague's answer to someone outside the company (below),
//!     calendar mail or a sign-in or reset notice, the sender is
//!     authenticated and not on the exclude list, and Jev scored the last
//!     message at or above the threshold (`fork_reply_threshold`), or at or
//!     above [`DEAL_FLOOR`] in a Deals conversation, or Jev was
//!     asked and gave no usable answer (listed as "not checked": a missed
//!     reply costs more than a wrong row).
//!
//! Only Inbox mail: the court puts a conversation on the user only while it
//! has an Inbox copy, so mail archived unanswered drops out. "The user" is
//! every account address (`court::own_addresses`); a reply from a send-as
//! alias would read as someone else's (the owner sends from one address).
//!
//! A colleague's answer counts as a reply (v1.1.21): a last message from
//! someone else at the mailbox's own domain (`autospark.ai`, never a
//! personal-mail one such as gmail.com) that went to at least one address
//! outside it. On October 14 that was a colleague answering IQ-EQ and XL
//! Sigma in 4 listed conversations. Mail a colleague sent only inside the
//! company is still asked about: it may be waiting on the user. A `needs`
//! mark ends with the colleague's answer, as it does with the user's own
//! reply (a mark lasts until the next message); one set on the colleague's
//! message lists it, and so does a reply draft saved after it.
//!
//! A mark lasts until the next message in the conversation, from anyone
//! (`fork_reply_mark.msg_key`). So a reply sent from Skim, from Gmail on the
//! web or from the phone takes the conversation out (the court sees the Sent
//! copy and moves it to `waiting`), and a new message from them brings it back
//! for Jev to judge again.
//!
//! What Jev sees is what Priority already sends (`fork::priority`): sender,
//! subject and the start of the body with links and codes removed. The same
//! mail is never asked about: bulk and automated senders (the court drops
//! them), calendar mail, sign-in codes, unauthenticated mail, the exclude list,
//! the user's own mail, a colleague's answer. Fail-open: no key or an outage leaves a conversation
//! unscored, so it is simply not listed by the Jev rule; nothing else changes.
//! Scoring runs inside `priority::schedule`, after Priority's own pass and
//! under the same lock, so the two never compete for TypeSafe's rate limit.

use crate::db::models::ThreadRow;
use crate::db::queries::addresses_from_json;
use crate::db::{queries, Db};
use crate::error::{Result, SkimError};
use crate::fork::court::{self, CourtRow, REASON_DRAFT, STATE_ON_ME};
use crate::fork::deals;
use crate::fork::priority::{
    self, authenticated, body_preview, excluded, is_calendar, is_sensitive, msg_key, parse_exclude,
    Answer, Ask, Candidate, BODY_WAIT_SECS,
};
use crate::state::AppState;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::json;
use std::collections::{BTreeMap, HashSet};
use tauri::{AppHandle, Emitter, State};

/// Emitted when scores or marks change what the list holds.
pub const EVT_UPDATED: &str = "replies:updated";
/// Virtual folder id of the view. Mirrors `src/fork/court/api.ts`.
pub const VF_TO_REPLY: i64 = -925;

pub const SETTING_THRESHOLD: &str = "fork_reply_threshold";
/// Internal counter row: `"YYYY-MM-DD:N"`, messages charged so far today.
const USED_KEY: &str = "fork_reply_used";

/// Fitted on the owner's mail (module comment): 0.5 would drop 13 of the 49.
pub const THRESHOLD_DEFAULT: f64 = 0.4;
/// The bar for a Deals conversation: lower, not none. On the owner's mailbox
/// on October 14 (1.1.19 live check) "always" listed 32 Deals rows, 8 of them
/// notices (a signature receipt at 0.06, a portal login at 0.07, an
/// auto-acknowledgement at 0.31); at 0.2 the people's mail stays (0.23 to
/// 0.36) and the receipts go.
pub const DEAL_FLOOR: f64 = 0.2;
/// Messages asked about per local day. The first fill is about 160.
pub const DAILY_CAP: i64 = 500;
/// Messages per pass; a full pass asks `priority::schedule` to run again.
pub const PASS_LIMIT: i64 = 60;
/// Scores and marks older than this are pruned.
const KEEP_DAYS: i64 = 400;

pub const MARK_DONE: &str = "done";
pub const MARK_NEEDS: &str = "needs";
/// Row tags (the court row badge shows them next to the age).
pub const TAG_MARKED: &str = "marked";
pub const TAG_DEAL: &str = "deal";
/// Jev was asked and gave no usable answer.
pub const TAG_UNCHECKED: &str = "unchecked";

/// The question. Its first sentence is the local `fork_priority_about`, as
/// Priority's is.
pub const QUESTION_REST: &str =
    "This email arrived in their inbox. Does the sender expect a personal reply from them?";
pub const QUESTION_TRUE: &str = "A real person wrote to them directly and is waiting on them: \
a question, a request, a proposal, a decision, a document, a time to meet, or an introduction \
or conversation they need to answer.";
pub const QUESTION_FALSE: &str = "Newsletters, daily or weekly briefs and digests, marketing, \
receipts, automated notifications, meeting reminders and 'meeting now' notices, booking, \
invitation or signature confirmations, shared-file notices, auto-replies, cold sales pitches \
from strangers, FYI updates, and messages that close the conversation such as thanks or noted.";

pub fn ask(about: Option<&str>) -> Ask {
    let about = about
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(priority::ABOUT_DEFAULT);
    Ask {
        instructions: format!("{about} {QUESTION_REST}"),
        yes: QUESTION_TRUE,
        no: QUESTION_FALSE,
    }
}

/// The threshold setting, or the default when unset or not in (0, 1].
pub fn parse_threshold(raw: Option<&str>) -> f64 {
    raw.and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|v| *v > 0.0 && *v <= 1.0)
        .unwrap_or(THRESHOLD_DEFAULT)
}

// ---- the rule ------------------------------------------------------------------

/// One court row as the rule needs it, with its last message.
#[derive(Debug, Clone, Default)]
pub struct Row {
    pub thread_id: i64,
    pub state: String,
    pub since: i64,
    /// The court's reason (`filed`, `draft started`, ...).
    pub reason: Option<String>,
    /// The last message: sender, subject and sender authentication.
    pub from_addr: String,
    pub subject: String,
    pub spf: String,
    pub dkim: String,
    pub dmarc: String,
    /// The last message is the user's own. The court still says `on_me` when
    /// a local draft is left over after the reply went out from Gmail or the
    /// phone; that conversation is not waiting on him.
    pub from_me: bool,
    /// A colleague answered: the last message is from someone else at the
    /// mailbox's own domain and went outside it ([`colleague_answered`]).
    pub colleague: bool,
    /// A local reply draft on the conversation saved at or after the last
    /// message. An older one is a leftover, not a reply in progress.
    pub fresh_draft: bool,
    /// Jev was asked about the last message (`score` stays `None` when it
    /// could not judge it).
    pub asked: bool,
    pub score: Option<f64>,
    /// The user's mark, only when it was set on this same last message.
    pub mark: Option<String>,
}

/// Which conversations are in To reply, with the row tag (`None` = Jev).
/// Pure: the tests drive it. A mark is the user's own call and wins. Then
/// only a conversation waiting on the user, inside the window: a reply draft
/// in progress; else never one a colleague answered, calendar mail, an
/// unauthenticated sender or one on the exclude list; else a Deals
/// conversation, a Jev score at the threshold,
/// or a message Jev was asked about and could not judge (listed, since a
/// missed reply costs more than a wrong row).
pub fn decide(
    rows: &[Row],
    deals: &HashSet<i64>,
    threshold: f64,
    floor: i64,
    exclude: &[String],
) -> BTreeMap<i64, Option<&'static str>> {
    let mut out = BTreeMap::new();
    for r in rows {
        let tag = match r.mark.as_deref() {
            Some(MARK_DONE) => continue,
            // Trashed, or archived out of the Inbox: dealt with.
            Some(MARK_NEEDS) if r.reason.as_deref() == Some(court::REASON_FILED) => continue,
            Some(MARK_NEEDS) => Some(TAG_MARKED),
            _ => {
                if r.state != STATE_ON_ME || r.since < floor || r.from_me {
                    continue;
                }
                if r.fresh_draft {
                    Some(REASON_DRAFT)
                } else if r.colleague
                    || excluded(&r.from_addr, exclude)
                    || is_calendar(&r.subject, &r.from_addr)
                    || is_sensitive(&r.subject, "")
                    || !authenticated(&r.spf, &r.dkim, &r.dmarc)
                {
                    continue;
                } else if deals.contains(&r.thread_id) && r.score.is_some_and(|s| s >= DEAL_FLOOR) {
                    Some(TAG_DEAL)
                } else if r.score.is_some_and(|s| s >= threshold) {
                    None
                } else if r.asked && r.score.is_none() {
                    Some(TAG_UNCHECKED)
                } else {
                    continue;
                }
            }
        };
        out.insert(r.thread_id, tag);
    }
    out
}

fn domain_of(addr: &str) -> Option<String> {
    addr.trim()
        .rsplit_once('@')
        .map(|(_, d)| d.to_ascii_lowercase())
}

/// A colleague answered: `from` is not the user's own address, it is at one
/// of the mailbox's own domains (`deals::own_domains`: never a personal-mail
/// one, so a gmail.com mailbox makes no colleagues; a subdomain counts, as in
/// Deals), and one of the message's To or Cc addresses (JSON, as stored) is
/// outside them. A note sent only inside the company is not an answer to
/// anyone outside it. Outside means any other domain, a personal one too.
pub fn colleague_answered(
    from: &str,
    to_json: Option<&str>,
    cc_json: Option<&str>,
    own: &HashSet<String>,
    domains: &HashSet<String>,
) -> bool {
    if own.contains(&from.trim().to_ascii_lowercase())
        || !domain_of(from).is_some_and(|d| deals::is_own(&d, domains))
    {
        return false;
    }
    addresses_from_json(to_json)
        .into_iter()
        .chain(addresses_from_json(cc_json))
        .any(|a| domain_of(&a.addr).is_some_and(|d| !deals::is_own(&d, domains)))
}

fn rows(conn: &Connection, floor: i64) -> rusqlite::Result<Vec<Row>> {
    let own = court::own_addresses(conn)?;
    let domains = deals::own_domains(conn)?;
    let key = msg_key("m");
    let mut stmt = conn.prepare(&format!(
        "SELECT c.thread_id, c.state, coalesce(c.since, 0), c.reason,
                coalesce(m.from_addr, ''), coalesce(m.subject, ''),
                coalesce(m.auth_spf, ''), coalesce(m.auth_dkim, ''), coalesce(m.auth_dmarc, ''),
                EXISTS (SELECT 1 FROM drafts d
                        JOIN messages dm ON dm.id = d.reply_to_message_id
                                         OR dm.id = d.origin_message_id
                        WHERE dm.thread_id = c.thread_id AND d.updated_at >= m.date),
                s.msg_key IS NOT NULL, s.score, k.mark, m.to_addrs, m.cc_addrs
         FROM fork_court c
         JOIN messages m ON m.id = c.last_message_id
         LEFT JOIN fork_reply_score s ON s.msg_key = {key}
         LEFT JOIN fork_reply_mark k ON k.thread_id = c.thread_id AND k.msg_key = {key}
         WHERE (c.state = 'on_me' AND c.since >= ?1)
            OR c.thread_id IN (SELECT thread_id FROM fork_reply_mark)"
    ))?;
    let rows = stmt.query_map(params![floor], |r| {
        let from_addr: String = r.get(4)?;
        let (to, cc): (Option<String>, Option<String>) = (r.get(13)?, r.get(14)?);
        Ok(Row {
            thread_id: r.get(0)?,
            state: r.get(1)?,
            since: r.get(2)?,
            reason: r.get(3)?,
            from_me: own.contains(&from_addr.trim().to_ascii_lowercase()),
            colleague: colleague_answered(&from_addr, to.as_deref(), cc.as_deref(), &own, &domains),
            from_addr,
            subject: r.get(5)?,
            spf: r.get(6)?,
            dkim: r.get(7)?,
            dmarc: r.get(8)?,
            fresh_draft: r.get(9)?,
            asked: r.get(10)?,
            score: r.get(11)?,
            mark: r.get(12)?,
        })
    })?;
    rows.collect()
}

/// The To reply set, from the stored court rows, scores and marks.
pub fn members(
    conn: &Connection,
    deals: &HashSet<i64>,
    now: i64,
) -> rusqlite::Result<BTreeMap<i64, Option<&'static str>>> {
    let floor = court::window_floor(conn, now)?;
    let threshold = parse_threshold(queries::get_setting(conn, SETTING_THRESHOLD)?.as_deref());
    let exclude =
        parse_exclude(&queries::get_setting(conn, priority::SETTING_EXCLUDE)?.unwrap_or_default());
    Ok(decide(
        &rows(conn, floor)?,
        deals,
        threshold,
        floor,
        &exclude,
    ))
}

/// The list rows, newest last message first, shaped like the court views'.
pub fn list(
    conn: &Connection,
    members: &BTreeMap<i64, Option<&'static str>>,
    offset: i64,
    limit: i64,
) -> rusqlite::Result<Vec<CourtRow>> {
    if members.is_empty() {
        return Ok(Vec::new());
    }
    let ids = serde_json::to_string(&members.keys().collect::<Vec<_>>()).unwrap_or_default();
    let mut stmt = conn.prepare(
        "SELECT t.id,
                m.from_name, m.from_addr, m.subject, m.snippet, t.last_date,
                (NOT EXISTS (SELECT 1 FROM messages m3
                             WHERE m3.thread_id = t.id AND m3.is_read = 0)),
                t.starred,
                max(m.has_attachments), t.message_count, t.account_id,
                coalesce(c.since, 0)
         FROM fork_court c
         JOIN threads t ON t.id = c.thread_id
         JOIN messages m ON m.thread_id = t.id
         WHERE c.thread_id IN (SELECT value FROM json_each(?3))
           AND m.date = t.last_date
         GROUP BY t.id
         ORDER BY c.since DESC, t.id DESC
         LIMIT ?1 OFFSET ?2",
    )?;
    let rows = stmt
        .query_map(params![limit, offset, ids], |r| {
            let id: i64 = r.get(0)?;
            let from_name: Option<String> = r.get(1)?;
            let from_addr: Option<String> = r.get(2)?;
            Ok(CourtRow {
                row: ThreadRow {
                    id,
                    message_id: None,
                    account_id: r.get(10)?,
                    from_name: from_name
                        .filter(|s| !s.is_empty())
                        .or_else(|| from_addr.clone())
                        .unwrap_or_default(),
                    from_addr: from_addr.unwrap_or_default(),
                    subject: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                    snippet: r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                    date: r.get(5)?,
                    is_read: r.get(6)?,
                    is_starred: r.get(7)?,
                    has_attachments: r.get::<_, i64>(8)? != 0,
                    message_count: r.get(9)?,
                },
                since: r.get(11)?,
                reason: members.get(&id).copied().flatten().map(str::to_string),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

// ---- marks -------------------------------------------------------------------------

/// Set (or, with `None`, clear) the user's mark on a conversation's current
/// last message. Returns the mark it had, so a toast's Undo can put it back.
pub fn set_mark(
    conn: &mut Connection,
    thread_id: i64,
    mark: Option<&str>,
    now: i64,
) -> rusqlite::Result<std::result::Result<Option<String>, &'static str>> {
    if mark.is_some_and(|m| m != MARK_DONE && m != MARK_NEEDS) {
        return Ok(Err("Unknown mark"));
    }
    let key = msg_key("m");
    let Some(current) = conn
        .query_row(
            &format!(
                "SELECT {key} FROM fork_court c JOIN messages m ON m.id = c.last_message_id
                 WHERE c.thread_id = ?1"
            ),
            params![thread_id],
            |r| r.get::<_, String>(0),
        )
        .optional()?
    else {
        return Ok(Err("This conversation has not been sorted yet"));
    };
    let tx = conn.transaction()?;
    let prev: Option<String> = tx
        .query_row(
            "SELECT mark FROM fork_reply_mark WHERE thread_id = ?1 AND msg_key = ?2",
            params![thread_id, current],
            |r| r.get(0),
        )
        .optional()?;
    match mark {
        Some(m) => {
            tx.execute(
                "INSERT INTO fork_reply_mark (thread_id, msg_key, mark, marked_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(thread_id) DO UPDATE SET
                   msg_key = excluded.msg_key, mark = excluded.mark,
                   marked_at = excluded.marked_at",
                params![thread_id, current, m, now],
            )?;
        }
        None => {
            tx.execute(
                "DELETE FROM fork_reply_mark WHERE thread_id = ?1",
                params![thread_id],
            )?;
        }
    }
    tx.execute(
        "DELETE FROM fork_reply_mark WHERE marked_at < ?1",
        params![now - KEEP_DAYS * 86_400],
    )?;
    tx.commit()?;
    Ok(Ok(prev))
}

// ---- scoring -----------------------------------------------------------------------

/// What a candidate scan found.
#[derive(Debug, Default)]
pub struct Scan {
    pub candidates: Vec<Candidate>,
    /// Some messages are waiting for their bodies: look again shortly.
    pub waiting: bool,
}

/// Unscored last messages of `on_me` conversations since `floor`, newest
/// first, at most `limit`, one per score key, under Priority's rules
/// (`priority::never_asked`). A conversation with a reply draft is asked
/// about too: its draft may be a leftover, and then the score decides. A body
/// is read only for a message that passed the other rules.
pub fn candidates(conn: &Connection, floor: i64, now: i64, limit: usize) -> rusqlite::Result<Scan> {
    let mut scan = Scan::default();
    if limit == 0 {
        return Ok(scan);
    }
    let own = court::own_addresses(conn)?;
    let domains = deals::own_domains(conn)?;
    let exclude =
        parse_exclude(&queries::get_setting(conn, priority::SETTING_EXCLUDE)?.unwrap_or_default());
    let key = msg_key("m");
    let mut stmt = conn.prepare(&format!(
        "SELECT {key}, coalesce(m.from_name, ''), coalesce(m.from_addr, ''),
                coalesce(m.subject, ''), coalesce(m.snippet, ''),
                m.body_state IN (1, 2), m.date,
                coalesce(m.auth_spf, ''), coalesce(m.auth_dkim, ''), coalesce(m.auth_dmarc, ''),
                m.id, m.to_addrs, m.cc_addrs
         FROM fork_court c
         JOIN messages m ON m.id = c.last_message_id
         WHERE c.state = 'on_me'
           AND coalesce(c.since, 0) >= ?1
           AND NOT EXISTS (SELECT 1 FROM fork_reply_score s WHERE s.msg_key = {key})
         ORDER BY c.since DESC, c.thread_id DESC"
    ))?;
    let mut body = conn.prepare_cached(
        "SELECT coalesce(body_text, '') FROM message_bodies WHERE message_id = ?1",
    )?;
    let mut rows = stmt.query(params![floor])?;
    let mut seen = HashSet::new();
    while let Some(r) = rows.next()? {
        let mut c = Candidate {
            key: r.get(0)?,
            from_name: r.get(1)?,
            from_addr: r.get(2)?,
            subject: r.get(3)?,
            preview: r.get(4)?,
        };
        let body_ready: bool = r.get(5)?;
        let date: i64 = r.get(6)?;
        let (spf, dkim, dmarc): (String, String, String) = (r.get(7)?, r.get(8)?, r.get(9)?);
        let (to, cc): (Option<String>, Option<String>) = (r.get(11)?, r.get(12)?);
        // A colleague's answer is never listed, so it is never paid for.
        if priority::never_asked(&c, (&spf, &dkim, &dmarc), &own, &exclude)
            || colleague_answered(&c.from_addr, to.as_deref(), cc.as_deref(), &own, &domains)
            || !seen.insert(c.key.clone())
        {
            continue;
        }
        if !body_ready && date > now - BODY_WAIT_SECS {
            scan.waiting = true;
            continue;
        }
        let text: Option<String> = body
            .query_row(params![r.get::<_, i64>(10)?], |b| b.get(0))
            .optional()?;
        let from_body = body_preview(&text.unwrap_or_default());
        if !from_body.is_empty() {
            c.preview = from_body;
            if is_sensitive(&c.subject, &c.preview) {
                continue;
            }
        }
        scan.candidates.push(c);
        if scan.candidates.len() >= limit {
            break;
        }
    }
    Ok(scan)
}

fn used_today(conn: &Connection, today: &str) -> rusqlite::Result<i64> {
    let raw = queries::get_setting(conn, USED_KEY)?.unwrap_or_default();
    Ok(match raw.split_once(':') {
        Some((day, n)) if day == today => n.parse().unwrap_or(0),
        _ => 0,
    })
}

/// Store answers and charge them to today's cap, in one transaction.
pub fn store_scores(
    conn: &mut Connection,
    answers: &[Answer],
    today: &str,
    now: i64,
) -> rusqlite::Result<()> {
    let used = used_today(conn, today)? + answers.len() as i64;
    let tx = conn.transaction()?;
    {
        let mut ins = tx.prepare_cached(
            "INSERT OR REPLACE INTO fork_reply_score (msg_key, score, model, scored_at)
             VALUES (?1, ?2, ?3, ?4)",
        )?;
        for (key, score, model) in answers {
            ins.execute(params![key, score, model, now])?;
        }
    }
    tx.execute(
        "DELETE FROM fork_reply_score WHERE scored_at < ?1",
        params![now - KEEP_DAYS * 86_400],
    )?;
    queries::set_setting(&tx, USED_KEY, &format!("{today}:{used}"))?;
    tx.commit()
}

fn now_ts() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// Ask Jev about what is new, when Priority is on, keyed, under the cap and
/// not paused. Called by `priority::schedule` while it holds the scoring lock.
/// Returns how many answers were stored and whether messages wait on bodies.
pub async fn score_new(db: &Db) -> (usize, bool) {
    if priority::paused() {
        return (0, false);
    }
    let Some(api_key) = priority::api_key() else {
        return (0, false);
    };
    let now = now_ts();
    let day = today();
    let found = db
        .call({
            let day = day.clone();
            move |conn| {
                if !priority::enabled(conn)? {
                    return Ok((Scan::default(), None));
                }
                let left = (DAILY_CAP - used_today(conn, &day)?).clamp(0, PASS_LIMIT);
                let floor = court::window_floor(conn, now)?;
                let scan = candidates(conn, floor, now, left as usize)?;
                let about = queries::get_setting(conn, priority::SETTING_ABOUT)?;
                Ok((scan, Some(ask(about.as_deref()))))
            }
        })
        .await;
    let (scan, ask) = match found {
        Ok((scan, Some(ask))) => (scan, ask),
        Ok((scan, None)) => return (0, scan.waiting),
        Err(e) => {
            tracing::warn!(error = %e, "replies: candidate scan failed");
            return (0, false);
        }
    };
    let waiting = scan.waiting;
    if scan.candidates.is_empty() {
        return (0, waiting);
    }
    let (answers, fatal) = priority::score_all(&api_key, scan.candidates, &ask).await;
    priority::note_health(fatal.as_deref());
    let n = answers.len();
    if n > 0 {
        if let Err(e) = db
            .call(move |conn| store_scores(conn, &answers, &day, now))
            .await
        {
            tracing::warn!(error = %e, "replies: storing scores failed");
            return (0, waiting);
        }
    }
    (n, waiting)
}

// ---- commands ----------------------------------------------------------------------

fn member_set(
    conn: &mut Connection,
    cache: &crate::fork::deals::Cache,
) -> rusqlite::Result<BTreeMap<i64, Option<&'static str>>> {
    let deals: HashSet<i64> = cache.membership(conn)?.1.keys().copied().collect();
    members(conn, &deals, now_ts())
}

/// The To reply conversations, for the view.
#[tauri::command]
pub async fn fork_replies_list(
    state: State<'_, AppState>,
    offset: i64,
    limit: i64,
) -> Result<Vec<CourtRow>> {
    let cache = state.fork.deals.clone();
    state
        .db
        .read("fork_replies_list", move |conn| {
            let m = member_set(conn, &cache)?;
            list(conn, &m, offset.max(0), limit.clamp(1, 200))
        })
        .await
}

/// What the sidebar count, the `D` key, the row menu and the view header read.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepliesState {
    /// Every conversation in To reply.
    pub ids: Vec<i64>,
    /// Why new mail is not being checked, when it is not: `priority_off`,
    /// `no_key` or `paused`. The list then holds only marks, drafts and
    /// Deals, and the view says so rather than looking clear.
    pub off: Option<&'static str>,
}

/// Why the reply question cannot run now, if it cannot.
pub fn scoring_off(enabled: bool, key: bool, paused: bool) -> Option<&'static str> {
    if !enabled {
        Some("priority_off")
    } else if !key {
        Some("no_key")
    } else if paused {
        Some("paused")
    } else {
        None
    }
}

#[tauri::command]
pub async fn fork_replies_state(state: State<'_, AppState>) -> Result<RepliesState> {
    let cache = state.fork.deals.clone();
    let key = priority::api_key().is_some();
    let paused = priority::paused();
    state
        .db
        .read("fork_replies_state", move |conn| {
            Ok(RepliesState {
                ids: member_set(conn, &cache)?.into_keys().collect(),
                off: scoring_off(priority::enabled(conn)?, key, paused),
            })
        })
        .await
}

/// Mark a conversation `done` (no reply needed) or `needs` (a reply is owed),
/// or clear the mark with `null`. Returns the previous mark for Undo.
#[tauri::command]
pub async fn fork_replies_mark(
    app: AppHandle,
    state: State<'_, AppState>,
    thread_id: i64,
    mark: Option<String>,
) -> Result<Option<String>> {
    let now = now_ts();
    let out = state
        .db
        .call(move |conn| set_mark(conn, thread_id, mark.as_deref(), now))
        .await?;
    let prev = out.map_err(|msg| SkimError::other("replies", msg))?;
    let _ = app.emit(EVT_UPDATED, json!({}));
    Ok(prev)
}

// ---- tests ------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::{Address, NewMessage};
    use crate::db::queries::{addresses_to_json, insert_message};
    use crate::fork::court::{pass, Scope};

    const ME: &str = "me@example.com";

    fn r(thread_id: i64, score: Option<f64>) -> Row {
        Row {
            thread_id,
            state: STATE_ON_ME.into(),
            since: 1_000,
            from_addr: "anna@client.com".into(),
            subject: "Proposal".into(),
            dmarc: "pass".into(),
            asked: score.is_some(),
            score,
            ..Row::default()
        }
    }

    fn run(rows: &[Row]) -> BTreeMap<i64, Option<&'static str>> {
        decide(rows, &[7].into(), 0.4, 500, &["alerts.com".into()])
    }

    #[test]
    fn a_score_at_the_threshold_counts_and_under_it_does_not() {
        let out = run(&[r(1, Some(0.4)), r(2, Some(0.39)), r(3, None)]);
        assert_eq!(out.get(&1), Some(&None));
        assert!(!out.contains_key(&2));
        assert!(!out.contains_key(&3), "unscored is not listed");
    }

    #[test]
    fn only_conversations_waiting_on_the_user_inside_the_window() {
        let waiting = Row {
            state: "waiting".into(),
            ..r(1, Some(0.9))
        };
        let old = Row {
            since: 499,
            ..r(2, Some(0.9))
        };
        let out = run(&[waiting, old, r(3, Some(0.9))]);
        assert_eq!(out.keys().copied().collect::<Vec<_>>(), vec![3]);
    }

    #[test]
    fn a_draft_needs_no_score_and_a_deal_needs_a_lower_one() {
        let draft = Row {
            reason: Some(REASON_DRAFT.into()),
            fresh_draft: true,
            ..r(1, None)
        };
        let out = run(&[draft]);
        assert_eq!(out.get(&1), Some(&Some(REASON_DRAFT)));
        assert_eq!(
            run(&[r(7, Some(DEAL_FLOOR))]).get(&7),
            Some(&Some(TAG_DEAL))
        );
        assert_eq!(run(&[r(7, Some(0.5))]).get(&7), Some(&Some(TAG_DEAL)));
        assert!(
            run(&[r(7, Some(0.19))]).is_empty(),
            "a receipt in a deal thread"
        );
        assert!(run(&[r(7, None)]).is_empty(), "not judged yet");
        assert!(
            run(&[r(8, Some(0.3))]).is_empty(),
            "0.3 is not enough outside Deals"
        );
    }

    #[test]
    fn a_leftover_draft_or_his_own_last_word_is_not_waiting_on_him() {
        // A local draft older than the last message: the score decides.
        let leftover = Row {
            reason: Some(REASON_DRAFT.into()),
            ..r(1, Some(0.1))
        };
        // His reply went out from the phone; the court still says on_me.
        let replied = Row {
            reason: Some(REASON_DRAFT.into()),
            fresh_draft: true,
            from_me: true,
            ..r(2, None)
        };
        assert!(run(&[leftover, replied]).is_empty());
    }

    #[test]
    fn deals_never_list_calendar_unauthenticated_or_excluded_mail() {
        let invite = Row {
            subject: "Accepted: Call with Patrick".into(),
            ..r(7, Some(0.9))
        };
        let spoof = Row {
            dmarc: "fail".into(),
            ..r(7, Some(0.9))
        };
        let excluded = Row {
            from_addr: "ops@alerts.com".into(),
            ..r(7, Some(0.9))
        };
        let reset = Row {
            subject: "[Portal] Password Reset".into(),
            ..r(7, Some(0.9))
        };
        for row in [invite, spoof, excluded, reset] {
            assert!(run(&[row]).is_empty());
        }
    }

    #[test]
    fn a_message_jev_could_not_judge_is_listed_not_dropped() {
        let unjudged = Row {
            asked: true,
            ..r(1, None)
        };
        assert_eq!(run(&[unjudged]).get(&1), Some(&Some(TAG_UNCHECKED)));
    }

    #[test]
    fn a_needs_mark_ends_when_the_conversation_is_trashed_or_archived() {
        let filed = Row {
            state: "none".into(),
            reason: Some(court::REASON_FILED.into()),
            mark: Some(MARK_NEEDS.into()),
            ..r(1, None)
        };
        assert!(run(&[filed]).is_empty());
    }

    #[test]
    fn a_colleagues_answer_is_a_reply_unless_he_is_drafting_or_marked_it() {
        let answered = Row {
            colleague: true,
            ..r(1, Some(0.9))
        };
        let deal = Row {
            colleague: true,
            ..r(7, Some(0.9))
        };
        assert!(run(&[answered, deal]).is_empty());
        let drafting = Row {
            colleague: true,
            fresh_draft: true,
            ..r(2, None)
        };
        let marked = Row {
            colleague: true,
            mark: Some(MARK_NEEDS.into()),
            ..r(3, None)
        };
        let out = run(&[drafting, marked]);
        assert_eq!(out.get(&2), Some(&Some(REASON_DRAFT)));
        assert_eq!(out.get(&3), Some(&Some(TAG_MARKED)));
    }

    #[test]
    fn a_colleague_answered_only_when_the_mail_went_outside_the_company() {
        let own: HashSet<String> = ["me@firm.ai".to_string()].into();
        let domains: HashSet<String> = ["firm.ai".to_string()].into();
        let to = |addrs: &[&str]| {
            addresses_to_json(
                &addrs
                    .iter()
                    .map(|a| Address {
                        name: None,
                        addr: a.to_string(),
                    })
                    .collect::<Vec<_>>(),
            )
        };
        let client = to(&["anna@client.com", "me@firm.ai"]);
        let inside = to(&["me@firm.ai", "ops@firm.ai"]);
        let yes = |from: &str, t: Option<&str>, c: Option<&str>| {
            colleague_answered(from, t, c, &own, &domains)
        };
        assert!(yes("Wes@Firm.AI", Some(&client), None));
        assert!(
            yes(
                "wes@firm.ai",
                Some(&inside),
                Some(&to(&["anna@client.com"]))
            ),
            "Cc counts"
        );
        assert!(
            !yes("wes@firm.ai", Some(&inside), None),
            "a note inside the company"
        );
        assert!(!yes("wes@firm.ai", None, None), "no recipients: Bcc to him");
        assert!(
            !yes("me@firm.ai", Some(&client), None),
            "his own mail is from_me"
        );
        assert!(!yes("anna@client.com", Some(&client), None));
        assert!(
            !yes("wes@firm.ai.evil.com", Some(&client), None),
            "a lookalike domain"
        );
        assert!(
            yes("wes@sub.firm.ai", Some(&client), None),
            "a subdomain is the company, as in Deals"
        );
        assert!(
            !yes("wes@firm.ai", Some(&to(&["ops@eu.firm.ai"])), None),
            "nor is a subdomain outside it"
        );
        assert!(
            yes("wes@firm.ai", Some(&to(&["jo@gmail.com"])), None),
            "a personal address is outside"
        );
        // A personal mailbox has no own domain (`deals::own_domains`), so
        // makes no colleagues.
        let gmail: HashSet<String> = ["me@gmail.com".to_string()].into();
        assert!(!colleague_answered(
            "x@gmail.com",
            Some(&client),
            None,
            &gmail,
            &HashSet::new()
        ));
    }

    #[test]
    fn scoring_off_names_the_first_missing_piece() {
        assert_eq!(scoring_off(false, false, true), Some("priority_off"));
        assert_eq!(scoring_off(true, false, true), Some("no_key"));
        assert_eq!(scoring_off(true, true, true), Some("paused"));
        assert_eq!(scoring_off(true, true, false), None);
    }

    #[test]
    fn an_excluded_sender_needs_more_than_a_score() {
        let alert = Row {
            from_addr: "ops@alerts.com".into(),
            ..r(1, Some(0.9))
        };
        assert!(run(&[alert]).is_empty());
    }

    #[test]
    fn done_beats_every_rule_and_needs_beats_state_and_window() {
        let done_deal = Row {
            mark: Some(MARK_DONE.into()),
            ..r(7, Some(0.9))
        };
        let needs_waiting_old = Row {
            state: "waiting".into(),
            since: 1,
            mark: Some(MARK_NEEDS.into()),
            ..r(2, None)
        };
        let out = run(&[done_deal, needs_waiting_old]);
        assert!(!out.contains_key(&7));
        assert_eq!(out.get(&2), Some(&Some(TAG_MARKED)));
    }

    #[test]
    fn the_threshold_setting_falls_back_to_the_default() {
        assert_eq!(parse_threshold(None), THRESHOLD_DEFAULT);
        assert_eq!(parse_threshold(Some("0.6")), 0.6);
        assert_eq!(parse_threshold(Some("0")), THRESHOLD_DEFAULT);
        assert_eq!(parse_threshold(Some("2")), THRESHOLD_DEFAULT);
        assert_eq!(parse_threshold(Some("high")), THRESHOLD_DEFAULT);
    }

    #[test]
    fn the_question_names_the_owner_and_asks_about_a_reply() {
        let a = ask(Some("The recipient runs a firm."));
        assert!(a.instructions.starts_with("The recipient runs a firm. "));
        assert!(a.instructions.contains("personal reply"));
        assert_eq!(a.yes, QUESTION_TRUE);
        assert!(ask(None).instructions.starts_with(priority::ABOUT_DEFAULT));
    }

    // ---- against the real schema -----------------------------------------------

    /// One account: Inbox (1), Sent (2).
    fn seed(conn: &Connection) {
        conn.execute_batch(&format!(
            "INSERT INTO accounts (id, email, provider, imap_host, smtp_host, created_at)
               VALUES ('a1','{ME}','custom','imap.x','smtp.x',0);
             INSERT INTO folders (id, account_id, imap_name, role, display_name, sort_order)
               VALUES (1,'a1','INBOX','inbox','Inbox',0),
                      (2,'a1','Sent','sent','Sent',1);"
        ))
        .unwrap();
    }

    struct M<'a> {
        folder: i64,
        uid: u32,
        msgid: &'a str,
        refs: Vec<&'a str>,
        from: &'a str,
        subject: &'a str,
        date: i64,
        dmarc: &'a str,
        to: Vec<&'a str>,
    }

    impl Default for M<'_> {
        fn default() -> Self {
            M {
                folder: 1,
                uid: 0,
                msgid: "",
                refs: vec![],
                from: "anna@client.com",
                subject: "Proposal",
                date: 0,
                dmarc: "pass",
                to: vec![],
            }
        }
    }

    /// Insert a message with its body marked fetched. Returns (message, thread).
    fn add(conn: &mut Connection, m: M) -> (i64, i64) {
        let (pk, thread) = insert_message(
            conn,
            &NewMessage {
                account_id: "a1".into(),
                folder_id: m.folder,
                uid: m.uid,
                message_id: Some(format!("<{}>", m.msgid)),
                references: m.refs.iter().map(|r| format!("<{r}>")).collect(),
                subject: Some(m.subject.into()),
                from_name: Some("Someone".into()),
                from_addr: Some(m.from.into()),
                date: m.date,
                snippet: Some("Can you send the deck?".into()),
                auth_dmarc: Some(m.dmarc.into()),
                to_addrs: m
                    .to
                    .iter()
                    .map(|a| Address {
                        name: None,
                        addr: a.to_string(),
                    })
                    .collect(),
                ..Default::default()
            },
        )
        .unwrap()
        .expect("inserted");
        conn.execute(
            "UPDATE messages SET body_state = 1 WHERE id = ?1",
            params![pk],
        )
        .unwrap();
        (pk, thread)
    }

    fn keys(scan: &Scan) -> Vec<String> {
        scan.candidates
            .iter()
            .map(|c| c.key.rsplit('\u{1f}').next().unwrap().to_string())
            .collect()
    }

    fn score_all_at(conn: &mut Connection, score: f64, now: i64) {
        let scan = candidates(conn, 0, now, 100).unwrap();
        let answers: Vec<Answer> = scan
            .candidates
            .iter()
            .map(|c| (c.key.clone(), Some(score), priority::MODEL.to_string()))
            .collect();
        store_scores(conn, &answers, "d", now).unwrap();
    }

    #[test]
    fn candidates_are_the_last_inbound_message_under_every_rule() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            seed(conn);
            let now = 10_000_000;
            #[rustfmt::skip]
            let msgs = [
                M { uid: 1, msgid: "keep", subject: "Proposal", date: now - 50, ..M::default() },
                M { uid: 2, msgid: "cal", subject: "Invitation: Sync", date: now - 40, ..M::default() },
                M { uid: 3, msgid: "code", subject: "Your verification code", date: now - 30, ..M::default() },
                M { uid: 4, msgid: "forged", subject: "Payment", dmarc: "fail", date: now - 20, ..M::default() },
                M { uid: 5, msgid: "bot", from: "no-reply@svc.com", subject: "Report", date: now - 10, ..M::default() },
            ];
            for m in msgs {
                add(conn, m);
            }
            pass(conn, &Scope::All, false)?;
            let scan = candidates(conn, 0, now, 10)?;
            assert_eq!(keys(&scan), vec!["keep"], "calendar, codes, forged and bots are never asked");
            score_all_at(conn, 0.5, now);
            assert!(candidates(conn, 0, now, 10)?.candidates.is_empty(), "asked once");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn a_reply_takes_a_conversation_out_and_their_answer_brings_it_back() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            seed(conn);
            let now = 10_000_000;
            let (_, t) = add(
                conn,
                M {
                    uid: 1,
                    msgid: "q1",
                    date: now - 300,
                    ..M::default()
                },
            );
            pass(conn, &Scope::All, false)?;
            score_all_at(conn, 0.8, now);
            let none = HashSet::new();
            assert!(members(conn, &none, now)?.contains_key(&t));

            // His reply lands in Sent (from Skim, the web or the phone).
            add(
                conn,
                M {
                    folder: 2,
                    uid: 1,
                    msgid: "a1",
                    refs: vec!["q1"],
                    from: ME,
                    date: now - 200,
                    ..M::default()
                },
            );
            pass(conn, &Scope::All, false)?;
            assert!(!members(conn, &none, now)?.contains_key(&t), "replied: out");

            // They answer: back in once Jev has judged the new message.
            add(
                conn,
                M {
                    uid: 2,
                    msgid: "q2",
                    refs: vec!["q1", "a1"],
                    date: now - 100,
                    ..M::default()
                },
            );
            pass(conn, &Scope::All, false)?;
            assert!(
                !members(conn, &none, now)?.contains_key(&t),
                "not judged yet"
            );
            assert_eq!(keys(&candidates(conn, 0, now, 10)?), vec!["q2"]);
            score_all_at(conn, 0.8, now);
            assert!(members(conn, &none, now)?.contains_key(&t));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn a_colleague_answering_the_client_takes_it_out_and_their_answer_brings_it_back() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            seed(conn);
            let now = 10_000_000;
            let (_, t) = add(
                conn,
                M {
                    uid: 1,
                    msgid: "q1",
                    date: now - 300,
                    ..M::default()
                },
            );
            pass(conn, &Scope::All, false)?;
            score_all_at(conn, 0.8, now);
            let none = HashSet::new();
            assert!(members(conn, &none, now)?.contains_key(&t));

            // A colleague answers the client, copying him: his Inbox copy.
            add(
                conn,
                M {
                    uid: 2,
                    msgid: "c1",
                    refs: vec!["q1"],
                    from: "wes@example.com",
                    to: vec!["anna@client.com", ME],
                    date: now - 200,
                    ..M::default()
                },
            );
            pass(conn, &Scope::All, false)?;
            assert!(
                candidates(conn, 0, now, 10)?.candidates.is_empty(),
                "never paid for"
            );
            // Scored before 1.1.21, as the four on the owner's mailbox were.
            let c1: String = conn.query_row(
                &format!(
                    "SELECT {} FROM messages m WHERE m.folder_id = 1 AND m.uid = 2",
                    msg_key("m")
                ),
                [],
                |r| r.get(0),
            )?;
            store_scores(
                conn,
                &[(c1, Some(0.9), priority::MODEL.to_string())],
                "d",
                now,
            )?;
            assert!(
                !members(conn, &none, now)?.contains_key(&t),
                "answered: out"
            );

            // A colleague's note to him alone is asked about as usual.
            let (_, t2) = add(
                conn,
                M {
                    uid: 3,
                    msgid: "n1",
                    from: "wes@example.com",
                    subject: "Can you review the IQ-EQ deck?",
                    to: vec![ME],
                    date: now - 150,
                    ..M::default()
                },
            );
            pass(conn, &Scope::All, false)?;
            assert_eq!(keys(&candidates(conn, 0, now, 10)?), vec!["n1"]);

            // The client writes again: back in once Jev has judged it.
            add(
                conn,
                M {
                    uid: 4,
                    msgid: "q2",
                    refs: vec!["q1", "c1"],
                    date: now - 100,
                    ..M::default()
                },
            );
            pass(conn, &Scope::All, false)?;
            score_all_at(conn, 0.8, now);
            let m = members(conn, &none, now)?;
            assert!(m.contains_key(&t) && m.contains_key(&t2));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn a_mark_lasts_until_the_next_message_and_returns_the_previous_one() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            seed(conn);
            let now = 10_000_000;
            let (_, t) = add(
                conn,
                M {
                    uid: 1,
                    msgid: "q1",
                    date: now - 300,
                    ..M::default()
                },
            );
            pass(conn, &Scope::All, false)?;
            score_all_at(conn, 0.8, now);
            let none = HashSet::new();

            assert_eq!(set_mark(conn, t, Some(MARK_DONE), now)?, Ok(None));
            assert!(!members(conn, &none, now)?.contains_key(&t), "done: out");
            assert_eq!(
                set_mark(conn, t, Some(MARK_NEEDS), now)?,
                Ok(Some(MARK_DONE.to_string()))
            );
            assert_eq!(members(conn, &none, now)?.get(&t), Some(&Some(TAG_MARKED)));
            assert_eq!(
                set_mark(conn, t, None, now)?,
                Ok(Some(MARK_NEEDS.to_string()))
            );
            assert_eq!(
                members(conn, &none, now)?.get(&t),
                Some(&None),
                "undo: Jev again"
            );

            set_mark(conn, t, Some(MARK_DONE), now)?.unwrap();
            add(
                conn,
                M {
                    uid: 2,
                    msgid: "q2",
                    refs: vec!["q1"],
                    date: now - 100,
                    ..M::default()
                },
            );
            pass(conn, &Scope::All, false)?;
            score_all_at(conn, 0.8, now);
            assert!(
                members(conn, &none, now)?.contains_key(&t),
                "their next message ends the done mark"
            );
            assert_eq!(
                set_mark(conn, 99_999, Some(MARK_DONE), now)?,
                Err("This conversation has not been sorted yet")
            );
            assert_eq!(set_mark(conn, t, Some("later"), now)?, Err("Unknown mark"));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn a_needs_mark_on_his_own_last_word_lists_it_until_the_next_message() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            seed(conn);
            let now = 10_000_000;
            let (_, t) = add(
                conn,
                M {
                    folder: 2,
                    uid: 1,
                    msgid: "s1",
                    from: ME,
                    date: now - 300,
                    ..M::default()
                },
            );
            pass(conn, &Scope::All, false)?;
            let none = HashSet::new();
            assert!(
                !members(conn, &none, now)?.contains_key(&t),
                "waiting on them"
            );
            set_mark(conn, t, Some(MARK_NEEDS), now)?.unwrap();
            assert_eq!(members(conn, &none, now)?.get(&t), Some(&Some(TAG_MARKED)));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn a_reply_draft_counts_until_his_reply_goes_out_from_elsewhere() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            seed(conn);
            let now = 10_000_000;
            let (q1, t) = add(
                conn,
                M {
                    uid: 1,
                    msgid: "q1",
                    date: now - 300,
                    ..M::default()
                },
            );
            // He starts a reply in Skim.
            conn.execute(
                "INSERT INTO drafts (account_id, reply_to_message_id, mode, updated_at)
                 VALUES ('a1', ?1, 'reply', ?2)",
                params![q1, now - 250],
            )?;
            pass(conn, &Scope::All, false)?;
            let none = HashSet::new();
            assert_eq!(
                members(conn, &none, now)?.get(&t),
                Some(&Some(REASON_DRAFT))
            );
            // He sends it from the phone; the local draft row stays behind.
            add(
                conn,
                M {
                    folder: 2,
                    uid: 1,
                    msgid: "a1",
                    refs: vec!["q1"],
                    from: ME,
                    date: now - 200,
                    ..M::default()
                },
            );
            pass(conn, &Scope::All, false)?;
            assert!(!members(conn, &none, now)?.contains_key(&t), "replied: out");
            // They answer "thanks": the leftover draft does not hold it in.
            add(
                conn,
                M {
                    uid: 2,
                    msgid: "q2",
                    refs: vec!["q1", "a1"],
                    date: now - 100,
                    ..M::default()
                },
            );
            pass(conn, &Scope::All, false)?;
            assert_eq!(
                keys(&candidates(conn, 0, now, 10)?),
                vec!["q2"],
                "asked despite the draft"
            );
            score_all_at(conn, 0.1, now);
            assert!(!members(conn, &none, now)?.contains_key(&t));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn the_list_is_newest_first_with_tags() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            seed(conn);
            let now = 10_000_000;
            let (_, old) = add(
                conn,
                M {
                    uid: 1,
                    msgid: "old",
                    subject: "Older ask",
                    date: now - 900,
                    ..M::default()
                },
            );
            let (_, new) = add(
                conn,
                M {
                    uid: 2,
                    msgid: "new",
                    subject: "Newer ask",
                    date: now - 100,
                    ..M::default()
                },
            );
            pass(conn, &Scope::All, false)?;
            score_all_at(conn, 0.8, now);
            set_mark(conn, old, Some(MARK_NEEDS), now)?.unwrap();
            let m = members(conn, &HashSet::new(), now)?;
            let rows = list(conn, &m, 0, 10)?;
            let got: Vec<(i64, Option<String>)> =
                rows.iter().map(|r| (r.row.id, r.reason.clone())).collect();
            assert_eq!(got, vec![(new, None), (old, Some(TAG_MARKED.to_string()))]);
            assert_eq!(rows[0].since, now - 100);
            assert_eq!(list(conn, &m, 1, 10)?.len(), 1, "pages");
            Ok(())
        })
        .unwrap();
    }
}
