//! Priority (v1.1.11, D64): the unread Inbox conversations to read first sort
//! to the top of the Inbox under a "Priority" header.
//!
//! A conversation is in Priority while it has an unread Inbox message and one
//! of these holds:
//! - it is a Deals conversation (`fork::deals`);
//! - it is starred;
//! - one of its unread Inbox messages scored at or above the threshold
//!   (`fork_priority_threshold`, default [`THRESHOLD_DEFAULT`]) on one TypeSafe
//!   Jev yes/no question ([`question`]).
//!
//! What Jev sees: the sender, the subject and the stored preview with links
//! and 4 to 8 digit codes removed ([`redact`]); never the body. The preview is
//! `messages.snippet`, which `mail::parse::make_snippet` caps at 140
//! characters and which is empty until the body is fetched, so a message waits
//! for its body (or [`BODY_WAIT_SECS`]) before it is scored. Each message is
//! scored once, keyed by account, sender and Message-ID.
//!
//! Never scored: the user's own mail, bulk mail (List-Unsubscribe), calendar
//! invitations and replies, sign-in codes and password resets
//! ([`is_sensitive`]), mail that fails or lacks sender authentication
//! ([`authenticated`]: a forged "payment problem" must not reach the top),
//! mail older than [`SCORE_WINDOW_DAYS`], and senders on the local "never in
//! Priority" list (`fork_priority_exclude`, one domain or address per line).
//! Deals and starred conversations ignore that list: the user put them there.
//! The first sentence of the question (who the recipient is) is the local
//! setting `fork_priority_about`; the repository is public, so nothing
//! personal is compiled in.
//!
//! Fail-open. No key, a refused key or an outage means no score, so the
//! message keeps its date place; nothing is ever hidden. Account-wide failures
//! (key, payment, rate, outage) pause scoring for [`FAIL_BACKOFF`]. A failure
//! that belongs to one message (a 4xx about the request, an answer in a shape
//! this code cannot read) is stored as a NULL score and counted against
//! [`DAILY_CAP`], like `court::ai_pass` does, so it is never billed twice.
//!
//! The pass writes its answer to `fork_priority` (`migrations/f0012_priority.sql`).
//! The Inbox list query orders by it ahead of everything else (`fork::list`)
//! and checks unread itself, so a conversation leaves Priority the moment it
//! is read. The webview keeps the open conversation in place until the user
//! moves to another one (`src/fork/priority/sticky.ts`).

use crate::db::{queries, Db};
use crate::error::{Result, SkimError};
use crate::fork::list::ListOpts;
use crate::state::AppState;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

/// Emitted when the Priority set changed, so the Inbox reloads its order.
pub const EVT_UPDATED: &str = "priority:updated";
/// Credential Manager key holding the TypeSafe API key.
pub const SECRET_KEY: &str = "fork:typesafe";
/// Settings keys (also in `fork::settings::ALLOWED`).
pub const SETTING_ON: &str = "fork_priority"; // on | off, default on
pub const SETTING_THRESHOLD: &str = "fork_priority_threshold";
pub const SETTING_EXCLUDE: &str = "fork_priority_exclude";
pub const SETTING_ABOUT: &str = "fork_priority_about";
/// Internal counter row: `"YYYY-MM-DD:N"`, messages charged so far today.
const USED_KEY: &str = "fork_priority_used";

/// Fitted on a real mailbox before the build (D64): conversations the owner
/// replied to scored 0.24 to 0.8; newsletters, form receipts and alerts under
/// 0.3.
pub const THRESHOLD_DEFAULT: f64 = 0.35;
/// Messages Jev may be asked about per local day.
pub const DAILY_CAP: i64 = 500;
/// Messages scored per pass; the next pass picks up the rest.
const PASS_LIMIT: i64 = 60;
/// Parallel requests (TypeSafe rate-limits at about eight per key).
const WORKERS: usize = 4;
/// Only unread mail this recent is scored.
pub const SCORE_WINDOW_DAYS: i64 = 14;
/// A message whose body has not arrived is scored without a preview once it
/// is this old.
pub const BODY_WAIT_SECS: i64 = 900;
/// When messages are waiting for their bodies, look again this much later.
const BODY_RECHECK: Duration = Duration::from_secs(45);
/// Scores older than this are pruned; their mail is outside the window.
const SCORE_KEEP_DAYS: i64 = 60;
/// A guard only: snippets are at most 140 characters already.
const PREVIEW_CHARS: usize = 400;
/// After an account-wide failure, no scoring for this long.
const FAIL_BACKOFF: Duration = Duration::from_secs(600);
const HTTP_TIMEOUT: Duration = Duration::from_secs(20);

const ENDPOINT: &str = "https://api.typesafe.ai/v1/systemone";
/// Pinned: the threshold was fitted on this model. Move it only after
/// re-measuring.
pub const MODEL: &str = "jev-1.13.0";

/// The default first sentence of the question. `fork_priority_about` replaces
/// it locally with a line about the inbox's owner.
pub const ABOUT_DEFAULT: &str = "The recipient is the owner of this inbox.";
/// The rest of the question, with its criteria. Edit together with the
/// threshold.
pub const QUESTION_REST: &str = "This email arrived in their inbox. Should it be in a short list \
at the top of their inbox of emails they should read personally today?";
pub const QUESTION_TRUE: &str = "A real person wrote to them directly: a client, prospect, \
partner, investor, colleague or someone they know, or a time-sensitive notice they must act on \
(a payment problem, a security alert, a legal or tax deadline, a meeting change).";
pub const QUESTION_FALSE: &str = "Newsletters, marketing, promotions, product updates, automated \
notifications, receipts, social media alerts, cold sales pitches from strangers, or anything \
they can safely read later or never.";

pub const REASON_DEAL: &str = "deal";
pub const REASON_STARRED: &str = "starred";
pub const REASON_JEV: &str = "jev";

/// Calendar invitations and replies: Skim shows these as invite cards, and
/// they read as personal mail to Jev (D64 measurement). Matched on the
/// lowercased subject start.
const CALENDAR_PREFIXES: &[&str] = &[
    "invitation:",
    "updated invitation:",
    "updated invitation with note:",
    "accepted:",
    "declined:",
    "tentative:",
    "tentatively accepted:",
    "proposed new time:",
    "new time proposed:",
    "canceled event:",
    "cancelled event:",
    "canceled:",
    "cancelled:",
    "invitation from google calendar",
];
const CALENDAR_SENDER: &str = "calendar-notification@google.com";

/// Sign-in codes, password resets and account confirmations never leave the
/// machine. Matched on the lowercased subject and preview.
const SENSITIVE: &[&str] = &[
    "verification code",
    "verify your",
    "one-time",
    "one time password",
    "passcode",
    "security code",
    "login code",
    "log-in code",
    "sign-in code",
    "sign in code",
    "authentication code",
    "two-factor",
    "2fa",
    "password reset",
    "reset your password",
    "reset password",
    "magic link",
    "confirm your email",
    "confirm your account",
];

// ---- pure rules ------------------------------------------------------------

/// The "never in Priority" list: one domain or address per line (commas also
/// split), `#` starts a comment line, a leading `@` is ignored. Lowercased.
pub fn parse_exclude(text: &str) -> Vec<String> {
    text.lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .flat_map(|l| l.split(','))
        .map(|e| e.trim().trim_start_matches('@').to_ascii_lowercase())
        .filter(|e| !e.is_empty())
        .collect()
}

/// An address entry matches only itself; a domain entry matches its
/// addresses and any subdomain's.
pub fn excluded(addr: &str, list: &[String]) -> bool {
    let a = addr.trim().to_ascii_lowercase();
    let domain = a.rsplit_once('@').map(|(_, d)| d).unwrap_or("");
    list.iter().any(|e| {
        if e.contains('@') {
            *e == a
        } else {
            !domain.is_empty() && (domain == e || domain.ends_with(&format!(".{e}")))
        }
    })
}

pub fn is_calendar(subject: &str, from_addr: &str) -> bool {
    if from_addr.trim().eq_ignore_ascii_case(CALENDAR_SENDER) {
        return true;
    }
    let s = subject.trim_start().to_lowercase();
    CALENDAR_PREFIXES.iter().any(|p| s.starts_with(p))
}

pub fn is_sensitive(subject: &str, preview: &str) -> bool {
    let text = format!("{subject} {preview}").to_lowercase();
    SENSITIVE.iter().any(|p| text.contains(p))
}

/// The sender is authenticated: DMARC did not fail, and SPF, DKIM or DMARC
/// passed. Missing results count as not authenticated.
pub fn authenticated(spf: &str, dkim: &str, dmarc: &str) -> bool {
    if dmarc.trim().eq_ignore_ascii_case("fail") {
        return false;
    }
    [spf, dkim, dmarc]
        .iter()
        .any(|v| v.trim().eq_ignore_ascii_case("pass"))
}

/// The preview Jev sees: links and standalone 4 to 8 digit numbers (codes,
/// account and reference numbers) removed, whitespace collapsed.
pub fn redact(preview: &str) -> String {
    preview
        .split_whitespace()
        .filter(|w| {
            let lower = w.to_ascii_lowercase();
            !(lower.contains("://") || lower.starts_with("www."))
        })
        .map(|w| {
            let core = w.trim_matches(|c: char| !c.is_ascii_alphanumeric());
            if (4..=8).contains(&core.len()) && core.chars().all(|c| c.is_ascii_digit()) {
                "#"
            } else {
                w
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(PREVIEW_CHARS)
        .collect()
}

/// The threshold setting, or the default when unset or not in (0, 1].
pub fn parse_threshold(raw: Option<&str>) -> f64 {
    raw.and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|v| *v > 0.0 && *v <= 1.0)
        .unwrap_or(THRESHOLD_DEFAULT)
}

/// The question: the local first sentence (or the default), then the rest.
pub fn question(about: Option<&str>) -> String {
    let about = about
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(ABOUT_DEFAULT);
    format!("{about} {QUESTION_REST}")
}

/// One unread Inbox message as the decision needs it.
#[derive(Debug, Clone)]
pub struct Unread {
    pub thread_id: i64,
    pub from_addr: String,
    pub subject: String,
    pub starred: bool,
    pub score: Option<f64>,
}

fn rank(reason: &str) -> u8 {
    match reason {
        REASON_DEAL => 3,
        REASON_STARRED => 2,
        _ => 1,
    }
}

/// Which conversations are in Priority, and why. Pure: the tests drive it.
/// A deal or a star wins outright; a Jev score counts only at or above the
/// threshold and only for a sender that is not excluded (the list may have
/// grown since the message was scored).
pub fn decide(
    rows: &[Unread],
    deals: &HashSet<i64>,
    threshold: f64,
    exclude: &[String],
) -> BTreeMap<i64, &'static str> {
    let mut out: BTreeMap<i64, &'static str> = BTreeMap::new();
    for r in rows {
        let reason = if deals.contains(&r.thread_id) {
            REASON_DEAL
        } else if r.starred {
            REASON_STARRED
        } else if r.score.is_some_and(|s| s >= threshold)
            && !excluded(&r.from_addr, exclude)
            && !is_calendar(&r.subject, &r.from_addr)
        {
            REASON_JEV
        } else {
            continue;
        };
        let slot = out.entry(r.thread_id).or_insert(reason);
        if rank(reason) > rank(slot) {
            *slot = reason;
        }
    }
    out
}

/// One message Jev has not scored yet.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub key: String,
    pub from_name: String,
    pub from_addr: String,
    pub subject: String,
    pub preview: String,
}

/// The request for one message.
pub fn request_body(c: &Candidate, question: &str) -> Value {
    let from = if c.from_name.trim().is_empty() {
        c.from_addr.clone()
    } else {
        format!("{} <{}>", c.from_name.trim(), c.from_addr)
    };
    json!({
        "state": {
            "from": from,
            "subject": c.subject,
            "preview": redact(&c.preview),
            "has_unsubscribe_link": false,
        },
        "model": MODEL,
        "questions": {
            "read": {
                "type": "noul",
                "instructions": question,
                "criteria": { "true": QUESTION_TRUE, "false": QUESTION_FALSE },
            }
        }
    })
}

/// The yes-probability and the model that answered, or `None` for any other
/// shape: a malformed answer is no score, never a "no".
pub fn parse_noul(v: &Value) -> Option<(f64, String)> {
    let p = v.pointer("/answers/read/noul")?.as_f64()?;
    if !(0.0..=1.0).contains(&p) {
        return None;
    }
    let model = v.get("model").and_then(Value::as_str).unwrap_or(MODEL);
    Some((p, model.to_string()))
}

/// How an HTTP status ends one request.
#[derive(Debug, PartialEq, Eq)]
pub enum StatusClass {
    Ok,
    /// Try again shortly, then pause scoring.
    Retry,
    /// The key or the account: pause scoring now.
    Account,
    /// Something about this one message: record it and move on.
    Message,
}

pub fn classify_status(code: u16) -> StatusClass {
    match code {
        200..=299 => StatusClass::Ok,
        401..=403 => StatusClass::Account,
        408 | 429 | 500..=599 => StatusClass::Retry,
        _ => StatusClass::Message,
    }
}

/// `col` (a thread id) is in the stored Priority set. The list query adds its
/// own folder-scoped unread check.
pub fn member_pred(col: &str) -> String {
    format!("{col} IN (SELECT thread_id FROM fork_priority)")
}

// ---- database ----------------------------------------------------------------

/// The message's score key: account, sender and normalized Message-ID (or its
/// row id), so a Message-ID reused by another sender never inherits a score.
const MSG_KEY: &str =
    "m.account_id || char(31) || lower(coalesce(m.from_addr, '')) || char(31) || \
coalesce(nullif(m.message_id, ''), 'id:' || m.id)";

pub fn enabled(conn: &Connection) -> rusqlite::Result<bool> {
    Ok(queries::get_setting(conn, SETTING_ON)?.as_deref() != Some("off"))
}

/// List options for a folder: Priority order only in an Inbox.
pub fn opts_for_folder(
    conn: &Connection,
    folder_id: i64,
    mut opts: ListOpts,
) -> rusqlite::Result<ListOpts> {
    let role: Option<String> = conn
        .query_row(
            "SELECT role FROM folders WHERE id = ?1",
            params![folder_id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    opts.priority = role.as_deref() == Some("inbox") && enabled(conn)?;
    Ok(opts)
}

/// List options for a virtual (all accounts) folder addressed by role.
pub fn opts_for_role(
    conn: &Connection,
    role: Option<&str>,
    mut opts: ListOpts,
) -> rusqlite::Result<ListOpts> {
    opts.priority = role == Some("inbox") && enabled(conn)?;
    Ok(opts)
}

fn unread_rows(conn: &Connection) -> rusqlite::Result<Vec<Unread>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT m.thread_id, coalesce(m.from_addr, ''), coalesce(m.subject, ''), t.starred, s.score
         FROM messages m
         JOIN folders f ON f.id = m.folder_id
         JOIN threads t ON t.id = m.thread_id
         LEFT JOIN fork_priority_score s ON s.msg_key = {MSG_KEY}
         WHERE f.role = 'inbox' AND m.is_read = 0"
    ))?;
    let rows = stmt.query_map([], |r| {
        Ok(Unread {
            thread_id: r.get(0)?,
            from_addr: r.get(1)?,
            subject: r.get(2)?,
            starred: r.get::<_, i64>(3)? != 0,
            score: r.get(4)?,
        })
    })?;
    rows.collect()
}

/// Recompute the Priority set and store it. `true` when it changed.
pub fn recompute(conn: &mut Connection, deals: &HashSet<i64>) -> rusqlite::Result<bool> {
    let want = if enabled(conn)? {
        let threshold = parse_threshold(queries::get_setting(conn, SETTING_THRESHOLD)?.as_deref());
        let exclude =
            parse_exclude(&queries::get_setting(conn, SETTING_EXCLUDE)?.unwrap_or_default());
        decide(&unread_rows(conn)?, deals, threshold, &exclude)
    } else {
        BTreeMap::new()
    };
    let have: BTreeMap<i64, String> = {
        let mut stmt = conn.prepare("SELECT thread_id, reason FROM fork_priority")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let same = have.len() == want.len()
        && want
            .iter()
            .all(|(t, r)| have.get(t).map(String::as_str) == Some(*r));
    if same {
        return Ok(false);
    }
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM fork_priority", [])?;
    {
        let mut ins =
            tx.prepare_cached("INSERT INTO fork_priority (thread_id, reason) VALUES (?1, ?2)")?;
        for (t, r) in &want {
            ins.execute(params![t, r])?;
        }
    }
    tx.commit()?;
    Ok(true)
}

/// What a candidate scan found.
#[derive(Debug, Default)]
pub struct Scan {
    pub candidates: Vec<Candidate>,
    /// Some messages are waiting for their bodies: look again shortly.
    pub waiting: bool,
}

/// Unscored unread Inbox mail since `floor`, newest first, at most `limit`,
/// one per score key, under every rule in the module comment.
pub fn candidates(conn: &Connection, floor: i64, now: i64, limit: usize) -> rusqlite::Result<Scan> {
    let mut scan = Scan::default();
    if limit == 0 {
        return Ok(scan);
    }
    let own = crate::fork::court::own_addresses(conn)?;
    let exclude = parse_exclude(&queries::get_setting(conn, SETTING_EXCLUDE)?.unwrap_or_default());
    let mut stmt = conn.prepare(&format!(
        "SELECT {MSG_KEY}, coalesce(m.from_name, ''), coalesce(m.from_addr, ''),
                coalesce(m.subject, ''), coalesce(m.snippet, ''),
                m.body_state IN (1, 2), m.date,
                coalesce(m.auth_spf, ''), coalesce(m.auth_dkim, ''), coalesce(m.auth_dmarc, '')
         FROM messages m JOIN folders f ON f.id = m.folder_id
         WHERE f.role = 'inbox' AND m.is_read = 0 AND m.date >= ?1
           AND coalesce(m.list_unsubscribe, '') = ''
           AND NOT EXISTS (SELECT 1 FROM fork_priority_score s WHERE s.msg_key = {MSG_KEY})
         ORDER BY m.date DESC, m.id DESC"
    ))?;
    let mut rows = stmt.query(params![floor])?;
    let mut seen = HashSet::new();
    while let Some(r) = rows.next()? {
        let c = Candidate {
            key: r.get(0)?,
            from_name: r.get(1)?,
            from_addr: r.get(2)?,
            subject: r.get(3)?,
            preview: r.get(4)?,
        };
        let body_ready: bool = r.get(5)?;
        let date: i64 = r.get(6)?;
        let (spf, dkim, dmarc): (String, String, String) = (r.get(7)?, r.get(8)?, r.get(9)?);
        if own.contains(&c.from_addr.trim().to_ascii_lowercase())
            || excluded(&c.from_addr, &exclude)
            || is_calendar(&c.subject, &c.from_addr)
            || is_sensitive(&c.subject, &c.preview)
            || !authenticated(&spf, &dkim, &dmarc)
            || !seen.insert(c.key.clone())
        {
            continue;
        }
        if !body_ready && date > now - BODY_WAIT_SECS {
            scan.waiting = true;
            continue;
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

/// One answer to store: a score, or `None` for a message Jev could not judge
/// (stored so it is never asked about again).
pub type Answer = (String, Option<f64>, String);

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
            "INSERT OR REPLACE INTO fork_priority_score (msg_key, score, model, scored_at)
             VALUES (?1, ?2, ?3, ?4)",
        )?;
        for (key, score, model) in answers {
            ins.execute(params![key, score, model, now])?;
        }
    }
    tx.execute(
        "DELETE FROM fork_priority_score WHERE scored_at < ?1",
        params![now - SCORE_KEEP_DAYS * 86_400],
    )?;
    queries::set_setting(&tx, USED_KEY, &format!("{today}:{used}"))?;
    tx.commit()
}

/// Conversations in Priority that are still unread in an Inbox: the rows the
/// Inbox list puts first.
pub fn ids(conn: &Connection) -> rusqlite::Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        "SELECT p.thread_id FROM fork_priority p
         WHERE EXISTS (SELECT 1 FROM messages m JOIN folders f ON f.id = m.folder_id
                       WHERE m.thread_id = p.thread_id AND f.role = 'inbox' AND m.is_read = 0)",
    )?;
    let rows = stmt.query_map([], |r| r.get(0))?;
    rows.collect()
}

// ---- Jev ---------------------------------------------------------------------

#[derive(Debug)]
enum ScoreErr {
    /// Pause scoring: the key, the account or the service.
    Account(String),
    /// This message only: store it unscored and charge it.
    Message(String),
}

fn http() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .build()
            .unwrap_or_default()
    })
}

async fn score_one(
    api_key: &str,
    c: &Candidate,
    question: &str,
) -> std::result::Result<(f64, String), ScoreErr> {
    let body = request_body(c, question);
    let mut last = String::new();
    for attempt in 0..2 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        let resp = match http()
            .post(ENDPOINT)
            .bearer_auth(api_key)
            .json(&body)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                last = format!("network: {e}");
                continue;
            }
        };
        let status = resp.status();
        match classify_status(status.as_u16()) {
            StatusClass::Ok => {
                let v: Value = resp
                    .json()
                    .await
                    .map_err(|e| ScoreErr::Message(format!("unreadable answer: {e}")))?;
                return parse_noul(&v)
                    .ok_or_else(|| ScoreErr::Message("unexpected answer shape".into()));
            }
            StatusClass::Account => {
                return Err(ScoreErr::Account(format!("key refused (http {status})")))
            }
            StatusClass::Message => return Err(ScoreErr::Message(format!("http {status}"))),
            StatusClass::Retry => last = format!("http {status}"),
        }
    }
    Err(ScoreErr::Account(last))
}

/// Ask about every candidate, [`WORKERS`] at a time. Stops at the first
/// account-wide failure and returns it with the answers already in hand.
async fn score_all(
    api_key: &str,
    cands: Vec<Candidate>,
    question: &str,
) -> (Vec<Answer>, Option<String>) {
    use futures::stream::{self, StreamExt};
    let mut out = Vec::new();
    let mut stream = stream::iter(cands.into_iter().map(|c| {
        let key = api_key.to_string();
        let question = question.to_string();
        async move {
            let r = score_one(&key, &c, &question).await;
            (c.key, r)
        }
    }))
    .buffer_unordered(WORKERS);
    while let Some((key, r)) = stream.next().await {
        if let Some(e) = absorb(&mut out, key, r) {
            return (out, Some(e));
        }
    }
    (out, None)
}

/// Fold one outcome into the answers. A message Jev could not judge is kept
/// as an unscored answer, so it is charged and never asked about again;
/// `Some(error)` is account-wide and ends the pass.
fn absorb(
    out: &mut Vec<Answer>,
    key: String,
    r: std::result::Result<(f64, String), ScoreErr>,
) -> Option<String> {
    match r {
        Ok((score, model)) => out.push((key, Some(score), model)),
        Err(ScoreErr::Message(e)) => {
            tracing::warn!(error = %e, "priority: message left unscored");
            out.push((key, None, MODEL.to_string()));
        }
        Err(ScoreErr::Account(e)) => return Some(e),
    }
    None
}

// ---- runtime -------------------------------------------------------------------

#[derive(Default)]
struct Health {
    paused_until: Option<Instant>,
    last_error: Option<String>,
}

fn health() -> &'static Mutex<Health> {
    static H: OnceLock<Mutex<Health>> = OnceLock::new();
    H.get_or_init(Mutex::default)
}

/// Serializes recomputes (short). Scoring has its own lock, so a star or a
/// new deal reaches the list without waiting for a scoring pass.
fn recompute_lock() -> &'static tokio::sync::Mutex<()> {
    static L: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    L.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn scoring_lock() -> &'static tokio::sync::Mutex<()> {
    static L: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    L.get_or_init(|| tokio::sync::Mutex::new(()))
}

/// A run is queued and has not started yet: later events ride on it.
static QUEUED: AtomicBool = AtomicBool::new(false);
/// A delayed look for messages waiting on their bodies is armed.
static RECHECK_ARMED: AtomicBool = AtomicBool::new(false);

fn now_ts() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// `mail:updated` and startup hook. A burst of events while a run is queued
/// rides on that run, which sees all of them.
pub fn schedule(app: AppHandle) {
    if QUEUED.swap(true, Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        {
            let _guard = recompute_lock().lock().await;
            QUEUED.store(false, Ordering::SeqCst);
            if recompute_with(&app).await {
                let _ = app.emit(EVT_UPDATED, json!({}));
            }
        }
        // One scoring pass at a time; a busy one will see this event's mail
        // on its own next look, or the next event's.
        let Ok(_scoring) = scoring_lock().try_lock() else {
            return;
        };
        let db = app.state::<AppState>().db.clone();
        let (scored, waiting) = score_new(&db).await;
        if scored > 0 {
            let _guard = recompute_lock().lock().await;
            if recompute_with(&app).await {
                let _ = app.emit(EVT_UPDATED, json!({}));
            }
        }
        if waiting && !RECHECK_ARMED.swap(true, Ordering::SeqCst) {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(BODY_RECHECK).await;
                RECHECK_ARMED.store(false, Ordering::SeqCst);
                schedule(app);
            });
        }
    });
}

async fn recompute_with(app: &AppHandle) -> bool {
    let state = app.state::<AppState>();
    let cache = state.fork.deals.clone();
    let out = state
        .db
        .call(move |conn| {
            let deals: HashSet<i64> = cache.membership(conn)?.1.keys().copied().collect();
            recompute(conn, &deals)
        })
        .await;
    match out {
        Ok(changed) => changed,
        Err(e) => {
            tracing::warn!(error = %e, "priority pass failed");
            false
        }
    }
}

/// Score what is new, when on, keyed, under the cap and not paused. Returns
/// how many answers were stored and whether messages wait on their bodies.
async fn score_new(db: &Db) -> (usize, bool) {
    if health()
        .lock()
        .ok()
        .and_then(|h| h.paused_until)
        .is_some_and(|until| Instant::now() < until)
    {
        return (0, false);
    }
    let api_key = match crate::secrets::get(SECRET_KEY) {
        Ok(Some(k)) if !k.trim().is_empty() => k.trim().to_string(),
        _ => return (0, false),
    };
    let now = now_ts();
    let day = today();
    let found = db
        .call({
            let day = day.clone();
            move |conn| {
                if !enabled(conn)? {
                    return Ok((Scan::default(), String::new()));
                }
                let left = (DAILY_CAP - used_today(conn, &day)?).clamp(0, PASS_LIMIT);
                let scan = candidates(conn, now - SCORE_WINDOW_DAYS * 86_400, now, left as usize)?;
                let q = question(queries::get_setting(conn, SETTING_ABOUT)?.as_deref());
                Ok((scan, q))
            }
        })
        .await;
    let (scan, question) = match found {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "priority: candidate scan failed");
            return (0, false);
        }
    };
    let waiting = scan.waiting;
    if scan.candidates.is_empty() {
        return (0, waiting);
    }
    let (answers, fatal) = score_all(&api_key, scan.candidates, &question).await;
    if let Ok(mut h) = health().lock() {
        match &fatal {
            Some(e) => {
                tracing::warn!(error = %e, "priority: scoring paused");
                h.paused_until = Some(Instant::now() + FAIL_BACKOFF);
                h.last_error = Some(e.clone());
            }
            None => {
                h.paused_until = None;
                h.last_error = None;
            }
        }
    }
    let n = answers.len();
    if n > 0 {
        if let Err(e) = db
            .call(move |conn| store_scores(conn, &answers, &day, now))
            .await
        {
            tracing::warn!(error = %e, "priority: storing scores failed");
            return (0, waiting);
        }
    }
    (n, waiting)
}

// ---- commands --------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub enabled: bool,
    pub key_present: bool,
    pub threshold: f64,
    pub exclude: String,
    pub about: String,
    pub scored_today: i64,
    pub cap: i64,
    pub count: usize,
    pub last_error: Option<String>,
}

#[tauri::command]
pub async fn fork_priority_status(state: State<'_, AppState>) -> Result<Status> {
    let key_present =
        matches!(crate::secrets::get(SECRET_KEY), Ok(Some(k)) if !k.trim().is_empty());
    let last_error = health().lock().ok().and_then(|h| h.last_error.clone());
    let day = today();
    state
        .db
        .read("fork_priority_status", move |conn| {
            Ok(Status {
                enabled: enabled(conn)?,
                key_present,
                threshold: parse_threshold(
                    queries::get_setting(conn, SETTING_THRESHOLD)?.as_deref(),
                ),
                exclude: queries::get_setting(conn, SETTING_EXCLUDE)?.unwrap_or_default(),
                about: queries::get_setting(conn, SETTING_ABOUT)?.unwrap_or_default(),
                scored_today: used_today(conn, &day)?,
                cap: DAILY_CAP,
                count: ids(conn)?.len(),
                last_error,
            })
        })
        .await
}

#[tauri::command]
pub async fn fork_priority_ids(state: State<'_, AppState>) -> Result<Vec<i64>> {
    state.db.read("fork_priority_ids", |conn| ids(conn)).await
}

/// Save (or, when empty, remove) the TypeSafe key. A key is checked with one
/// harmless request first, so a mistyped key fails here, not silently later.
#[tauri::command]
pub async fn fork_priority_set_key(app: AppHandle, key: String) -> Result<()> {
    let key = key.trim().to_string();
    if key.is_empty() {
        crate::secrets::delete(SECRET_KEY)?;
    } else {
        let probe = Candidate {
            key: String::new(),
            from_name: String::new(),
            from_addr: "check@example.com".into(),
            subject: "Key check".into(),
            preview: String::new(),
        };
        if let Err(ScoreErr::Account(e) | ScoreErr::Message(e)) =
            score_one(&key, &probe, &question(None)).await
        {
            return Err(SkimError::other(
                "priority",
                format!("TypeSafe check failed: {e}"),
            ));
        }
        crate::secrets::set(SECRET_KEY, &key)?;
    }
    if let Ok(mut h) = health().lock() {
        *h = Health::default();
    }
    schedule(app);
    Ok(())
}

/// After a settings change: lift any pause and run a pass now.
#[tauri::command]
pub async fn fork_priority_recompute(app: AppHandle) -> Result<()> {
    if let Ok(mut h) = health().lock() {
        h.paused_until = None;
    }
    schedule(app);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::NewMessage;
    use crate::db::queries::insert_message;
    use crate::fork::list::{apply, Shape};

    const ME: &str = "me@example.com";

    fn u(thread_id: i64, from: &str, subject: &str, starred: bool, score: Option<f64>) -> Unread {
        Unread {
            thread_id,
            from_addr: from.into(),
            subject: subject.into(),
            starred,
            score,
        }
    }

    #[test]
    fn exclude_list_parses_lines_commas_comments_and_at_signs() {
        let l = parse_exclude("# warm-up\nWarm.example, @alerts.example\n\n bob@x.example \n");
        assert_eq!(l, ["warm.example", "alerts.example", "bob@x.example"]);
    }

    #[test]
    fn domain_entries_cover_subdomains_and_address_entries_only_themselves() {
        let l = parse_exclude("warm.example\nbob@x.example");
        assert!(excluded("Anna@Warm.Example", &l));
        assert!(excluded("a@mail.warm.example", &l));
        assert!(!excluded("a@notwarm.example", &l));
        assert!(excluded("bob@x.example", &l));
        assert!(!excluded("ann@x.example", &l));
        assert!(!excluded("no-at-sign", &l));
    }

    #[test]
    fn calendar_mail_is_recognised_by_subject_or_sender() {
        assert!(is_calendar("Invitation: Catch-up @ Wed", "jo@x.example"));
        assert!(is_calendar("Accepted: Demo @ Fri", "jo@x.example"));
        assert!(is_calendar("Updated invitation: Lunch", "jo@x.example"));
        assert!(is_calendar("Anything", "calendar-notification@google.com"));
        assert!(!is_calendar("Re: Invitation to tender", "jo@x.example"));
        assert!(!is_calendar("Quick question", "jo@x.example"));
    }

    #[test]
    fn sign_in_codes_and_resets_are_sensitive() {
        assert!(is_sensitive("Your verification code", ""));
        assert!(is_sensitive("Account", "Use this one-time code to sign in"));
        assert!(is_sensitive("Reset your password", ""));
        assert!(!is_sensitive("Re: website", "Can we talk on Friday?"));
    }

    #[test]
    fn only_authenticated_senders_qualify() {
        assert!(authenticated("pass", "pass", "pass"));
        assert!(authenticated("none", "pass", ""));
        assert!(authenticated("softfail", "", "pass"));
        assert!(
            !authenticated("pass", "pass", "fail"),
            "a DMARC fail never qualifies"
        );
        assert!(!authenticated("", "", ""), "missing results do not qualify");
        assert!(!authenticated("softfail", "permerror", "none"));
    }

    #[test]
    fn redact_drops_links_and_codes_but_keeps_words_and_amounts() {
        assert_eq!(
            redact("Your code is 482913. Visit https://x.example/a?b=1 or www.x.example now"),
            "Your code is # Visit or now"
        );
        assert_eq!(redact("Invoice R12 500 due 2026"), "Invoice R12 500 due #");
        assert_eq!(redact("  Hi   there\n Anna  "), "Hi there Anna");
        assert_eq!(redact(&"x ".repeat(500)).chars().count(), PREVIEW_CHARS);
    }

    #[test]
    fn threshold_falls_back_on_junk() {
        assert_eq!(parse_threshold(None), THRESHOLD_DEFAULT);
        assert_eq!(parse_threshold(Some("0.5")), 0.5);
        assert_eq!(parse_threshold(Some("0")), THRESHOLD_DEFAULT);
        assert_eq!(parse_threshold(Some("1.5")), THRESHOLD_DEFAULT);
        assert_eq!(parse_threshold(Some("high")), THRESHOLD_DEFAULT);
    }

    #[test]
    fn the_question_takes_the_local_first_sentence() {
        assert_eq!(question(None), format!("{ABOUT_DEFAULT} {QUESTION_REST}"));
        assert_eq!(question(Some("  ")), question(None));
        assert_eq!(
            question(Some("The recipient runs a bakery.")),
            format!("The recipient runs a bakery. {QUESTION_REST}")
        );
    }

    #[test]
    fn statuses_split_into_account_message_and_retry() {
        assert_eq!(classify_status(200), StatusClass::Ok);
        for code in [401, 402, 403] {
            assert_eq!(classify_status(code), StatusClass::Account, "{code}");
        }
        for code in [408, 429, 500, 503, 529] {
            assert_eq!(classify_status(code), StatusClass::Retry, "{code}");
        }
        for code in [400, 404, 413, 422] {
            assert_eq!(classify_status(code), StatusClass::Message, "{code}");
        }
    }

    #[test]
    fn decide_ranks_deal_over_star_over_score_and_applies_the_rules() {
        let deals: HashSet<i64> = [1].into();
        let ex = parse_exclude("warm.example");
        let rows = [
            u(1, "cfo@client.example", "Memo", false, Some(0.1)),
            u(2, "jo@x.example", "Hello", true, None),
            u(3, "ivo@x.example", "Re: website", false, Some(0.38)),
            u(4, "news@x.example", "Digest", false, Some(0.2)),
            u(5, "a@warm.example", "RE: quotes", false, Some(0.9)),
            u(6, "jo@x.example", "Invitation: Catch-up", false, Some(0.7)),
            u(7, "jo@x.example", "Not scored", false, None),
            // A thread with a low and a high message is in on the high one.
            u(8, "a@x.example", "one", false, Some(0.1)),
            u(8, "b@x.example", "two", false, Some(0.6)),
            // Starred and scored: the star wins.
            u(9, "a@x.example", "both", false, Some(0.9)),
            u(9, "a@x.example", "both", true, None),
            // Excluded sender on a deal thread: the deal wins.
            u(1, "a@warm.example", "fwd", false, None),
        ];
        let got = decide(&rows, &deals, 0.35, &ex);
        let want: BTreeMap<i64, &str> = [
            (1, REASON_DEAL),
            (2, REASON_STARRED),
            (3, REASON_JEV),
            (8, REASON_JEV),
            (9, REASON_STARRED),
        ]
        .into();
        assert_eq!(got, want);
        // The threshold is inclusive.
        let at = decide(
            &[u(3, "ivo@x.example", "x", false, Some(0.35))],
            &HashSet::new(),
            0.35,
            &[],
        );
        assert_eq!(at.get(&3), Some(&REASON_JEV));
    }

    #[test]
    fn request_carries_the_question_and_the_redacted_fields_only() {
        let c = Candidate {
            key: "k".into(),
            from_name: "Ivo".into(),
            from_addr: "ivo@x.example".into(),
            subject: "Re: website".into(),
            preview: "See https://x.example code 123456".into(),
        };
        let q = question(Some("About."));
        let b = request_body(&c, &q);
        assert_eq!(b["model"], MODEL);
        assert_eq!(b["state"]["from"], "Ivo <ivo@x.example>");
        assert_eq!(b["state"]["preview"], "See code #");
        assert_eq!(b["questions"]["read"]["type"], "noul");
        assert_eq!(b["questions"]["read"]["instructions"], q);
        assert_eq!(b["questions"]["read"]["criteria"]["true"], QUESTION_TRUE);
        let state_keys: Vec<&String> = b["state"].as_object().unwrap().keys().collect();
        assert_eq!(
            state_keys.len(),
            4,
            "only sender, subject, preview and the bulk flag"
        );
        let bare = Candidate {
            from_name: " ".into(),
            ..c
        };
        assert_eq!(request_body(&bare, &q)["state"]["from"], "ivo@x.example");
    }

    #[test]
    fn outcomes_fold_into_answers_and_only_account_errors_stop() {
        let mut out = Vec::new();
        assert_eq!(absorb(&mut out, "a".into(), Ok((0.4, "m".into()))), None);
        assert_eq!(
            absorb(
                &mut out,
                "b".into(),
                Err(ScoreErr::Message("http 400".into()))
            ),
            None
        );
        assert_eq!(
            absorb(
                &mut out,
                "c".into(),
                Err(ScoreErr::Account("key refused".into()))
            ),
            Some("key refused".to_string())
        );
        let want: Vec<Answer> = vec![
            ("a".into(), Some(0.4), "m".into()),
            ("b".into(), None, MODEL.into()),
        ];
        assert_eq!(out, want);
    }

    #[test]
    fn a_malformed_answer_is_no_score() {
        let ok =
            json!({"model": "jev-1.13.0", "answers": {"read": {"type": "noul", "noul": 0.41}}});
        assert_eq!(parse_noul(&ok), Some((0.41, "jev-1.13.0".into())));
        assert_eq!(parse_noul(&json!({"answers": {}})), None);
        assert_eq!(
            parse_noul(&json!({"answers": {"read": {"noul": "high"}}})),
            None
        );
        assert_eq!(
            parse_noul(&json!({"answers": {"read": {"noul": 1.4}}})),
            None
        );
    }

    // ---- against the real schema -----------------------------------------------

    /// One account: Inbox (1), Sent (2), a label (3).
    fn seed(conn: &Connection) {
        conn.execute_batch(&format!(
            "INSERT INTO accounts (id, email, provider, imap_host, smtp_host, created_at)
               VALUES ('a1','{ME}','custom','imap.x','smtp.x',0);
             INSERT INTO folders (id, account_id, imap_name, role, display_name, sort_order)
               VALUES (1,'a1','INBOX','inbox','Inbox',0),
                      (2,'a1','Sent','sent','Sent',1),
                      (3,'a1','Clients',NULL,'Clients',2);"
        ))
        .unwrap();
    }

    struct M<'a> {
        folder: i64,
        uid: u32,
        msgid: &'a str,
        from: &'a str,
        subject: &'a str,
        date: i64,
        read: bool,
        unsub: bool,
        dmarc: &'a str,
        body: bool,
    }

    impl Default for M<'_> {
        fn default() -> Self {
            M {
                folder: 1,
                uid: 0,
                msgid: "",
                from: "jo@x.example",
                subject: "Hello",
                date: 0,
                read: false,
                unsub: false,
                dmarc: "pass",
                body: true,
            }
        }
    }

    /// Insert a message; `body` marks its body as fetched. Returns the thread.
    fn add(conn: &mut Connection, m: M) -> i64 {
        let (pk, thread) = insert_message(
            conn,
            &NewMessage {
                account_id: "a1".into(),
                folder_id: m.folder,
                uid: m.uid,
                message_id: Some(format!("<{}>", m.msgid)),
                subject: Some(m.subject.into()),
                from_name: Some("Someone".into()),
                from_addr: Some(m.from.into()),
                date: m.date,
                snippet: Some("preview text".into()),
                is_read: m.read,
                list_unsubscribe: m.unsub.then(|| "<mailto:u@x>".to_string()),
                auth_dmarc: Some(m.dmarc.into()),
                ..Default::default()
            },
        )
        .unwrap()
        .expect("inserted");
        if m.body {
            conn.execute(
                "UPDATE messages SET body_state = 1 WHERE id = ?1",
                params![pk],
            )
            .unwrap();
        }
        thread
    }

    fn keys(scan: &Scan) -> Vec<String> {
        scan.candidates
            .iter()
            .map(|c| c.key.rsplit('\u{1f}').next().unwrap().to_string())
            .collect()
    }

    #[test]
    fn candidates_apply_every_rule() {
        let db = crate::db::Db::open_in_memory().unwrap();
        db.with(|conn| {
            seed(conn);
            let now = 10_000_000;
            let floor = now - SCORE_WINDOW_DAYS * 86_400;
            #[rustfmt::skip]
            let msgs = [
                M { uid: 1, msgid: "keep@x", from: "ivo@x.example", subject: "Re: website", date: now, ..M::default() },
                M { uid: 2, msgid: "own@x", from: ME, date: now, ..M::default() },
                M { uid: 3, msgid: "bulk@x", unsub: true, date: now, ..M::default() },
                M { uid: 4, msgid: "cal@x", subject: "Invitation: Catch-up", date: now, ..M::default() },
                M { uid: 5, msgid: "warm@x", from: "a@warm.example", date: now, ..M::default() },
                M { uid: 6, msgid: "old@x", date: now - 20 * 86_400, ..M::default() },
                M { uid: 7, msgid: "read@x", read: true, date: now, ..M::default() },
                M { uid: 8, msgid: "done@x", date: now, ..M::default() },
                M { uid: 9, msgid: "otp@x", subject: "Your verification code", date: now, ..M::default() },
                M { uid: 10, msgid: "forged@x", dmarc: "fail", date: now, ..M::default() },
                // The same message filed in a label is not an Inbox candidate twice.
                M { folder: 3, uid: 11, msgid: "keep@x", from: "ivo@x.example", subject: "Re: website", date: now, ..M::default() },
            ];
            for m in msgs {
                add(conn, m);
            }
            queries::set_setting(conn, SETTING_EXCLUDE, "warm.example")?;
            let done: String = conn.query_row(
                &format!("SELECT {MSG_KEY} FROM messages m WHERE m.uid = 8"),
                [],
                |r| r.get(0),
            )?;
            store_scores(conn, &[(done, Some(0.9), MODEL.into())], "d", now)?;
            let scan = candidates(conn, floor, now, 10)?;
            assert_eq!(keys(&scan), ["keep@x"]);
            assert!(!scan.waiting);
            assert!(candidates(conn, floor, now, 0)?.candidates.is_empty());

            // A new message without its body waits; an old one goes without.
            add(conn, M { uid: 12, msgid: "fresh@x", date: now - 60, body: false, ..M::default() });
            add(conn, M { uid: 13, msgid: "stale@x", date: now - BODY_WAIT_SECS - 1, body: false, ..M::default() });
            let scan = candidates(conn, floor, now, 10)?;
            assert_eq!(keys(&scan), ["keep@x", "stale@x"]);
            assert!(scan.waiting);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn a_message_id_reused_by_another_sender_is_scored_on_its_own() {
        let db = crate::db::Db::open_in_memory().unwrap();
        db.with(|conn| {
            seed(conn);
            let now = 10_000_000;
            add(
                conn,
                M {
                    uid: 1,
                    msgid: "same@x",
                    from: "a@x.example",
                    date: now,
                    ..M::default()
                },
            );
            let first = candidates(conn, 0, now, 10)?.candidates;
            store_scores(
                conn,
                &[(first[0].key.clone(), Some(0.9), MODEL.into())],
                "d",
                now,
            )?;
            add(
                conn,
                M {
                    uid: 2,
                    msgid: "same@x",
                    from: "b@y.example",
                    date: now,
                    ..M::default()
                },
            );
            let next = candidates(conn, 0, now, 10)?.candidates;
            assert_eq!(next.len(), 1);
            assert_eq!(next[0].from_addr, "b@y.example");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn unscorable_answers_are_stored_charged_and_never_asked_again() {
        let db = crate::db::Db::open_in_memory().unwrap();
        db.with(|conn| {
            seed(conn);
            let now = 10_000_000;
            add(
                conn,
                M {
                    uid: 1,
                    msgid: "odd@x",
                    date: now,
                    ..M::default()
                },
            );
            let c = candidates(conn, 0, now, 10)?.candidates;
            store_scores(
                conn,
                &[(c[0].key.clone(), None, MODEL.into())],
                "2026-10-13",
                now,
            )?;
            assert_eq!(used_today(conn, "2026-10-13")?, 1);
            assert!(candidates(conn, 0, now, 10)?.candidates.is_empty());
            assert!(
                !recompute(conn, &HashSet::new())?,
                "no score is not Priority"
            );
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn the_daily_counter_resets_each_day_and_old_scores_are_pruned() {
        let db = crate::db::Db::open_in_memory().unwrap();
        db.with(|conn| {
            let now = 100 * 86_400;
            store_scores(
                conn,
                &[("a".into(), Some(0.5), MODEL.into())],
                "2026-10-13",
                now - 70 * 86_400,
            )?;
            store_scores(
                conn,
                &[
                    ("b".into(), Some(0.5), MODEL.into()),
                    ("c".into(), None, MODEL.into()),
                ],
                "2026-10-13",
                now,
            )?;
            assert_eq!(used_today(conn, "2026-10-13")?, 3);
            assert_eq!(used_today(conn, "2026-10-14")?, 0);
            let n: i64 =
                conn.query_row("SELECT count(*) FROM fork_priority_score", [], |r| r.get(0))?;
            assert_eq!(n, 2, "the 70-day-old score is pruned");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn the_inbox_puts_priority_first_and_reading_drops_it_back() {
        let db = crate::db::Db::open_in_memory().unwrap();
        db.with(|conn| {
            seed(conn);
            let old = add(
                conn,
                M {
                    uid: 1,
                    msgid: "old@x",
                    subject: "Re: website",
                    date: 100,
                    ..M::default()
                },
            );
            let new = add(
                conn,
                M {
                    uid: 2,
                    msgid: "new@x",
                    subject: "Newer",
                    date: 200,
                    ..M::default()
                },
            );
            add(
                conn,
                M {
                    uid: 3,
                    msgid: "newest@x",
                    subject: "Newest read",
                    date: 300,
                    read: true,
                    ..M::default()
                },
            );
            let scan = candidates(conn, 0, 1_000, 10)?.candidates;
            let key = |id: &str| {
                scan.iter()
                    .find(|c| c.key.ends_with(id))
                    .unwrap()
                    .key
                    .clone()
            };
            store_scores(
                conn,
                &[
                    (key("old@x"), Some(0.6), MODEL.into()),
                    (key("new@x"), Some(0.1), MODEL.into()),
                ],
                "d",
                1,
            )?;
            assert!(recompute(conn, &HashSet::new())?);
            assert!(
                !recompute(conn, &HashSet::new())?,
                "an unchanged set is not rewritten"
            );
            assert_eq!(ids(conn)?, [old]);

            let subjects = |conn: &mut Connection, priority: bool| -> Vec<String> {
                let opts = ListOpts {
                    priority,
                    ..ListOpts::default()
                };
                crate::db::queries::list_threads_opts(conn, 1, 0, 10, opts)
                    .unwrap()
                    .into_iter()
                    .map(|r| r.subject)
                    .collect()
            };
            assert_eq!(
                subjects(conn, true),
                ["Re: website", "Newest read", "Newer"]
            );
            assert_eq!(
                subjects(conn, false),
                ["Newest read", "Newer", "Re: website"]
            );
            let opts = opts_for_folder(conn, 1, ListOpts::default())?;
            assert!(opts.priority, "the Inbox orders by priority");
            assert!(
                !opts_for_folder(conn, 3, ListOpts::default())?.priority,
                "a label never does"
            );
            assert!(opts_for_role(conn, Some("inbox"), ListOpts::default())?.priority);
            assert!(!opts_for_role(conn, Some("sent"), ListOpts::default())?.priority);

            // Read it: it drops back to its date before any pass runs.
            conn.execute(
                "UPDATE messages SET is_read = 1 WHERE thread_id = ?1",
                params![old],
            )?;
            assert_eq!(
                subjects(conn, true),
                ["Newest read", "Newer", "Re: website"]
            );
            assert!(ids(conn)?.is_empty());

            // A deal conversation is in without a score; switching off empties it.
            assert!(recompute(conn, &[new].into())?);
            assert_eq!(ids(conn)?, [new]);
            queries::set_setting(conn, SETTING_ON, "off")?;
            assert!(!opts_for_folder(conn, 1, ListOpts::default())?.priority);
            assert!(recompute(conn, &[new].into())?);
            assert!(ids(conn)?.is_empty());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn the_priority_clause_leads_the_order() {
        let sql = apply(
            crate::db::queries::LIST_THREADS_SQL,
            ListOpts {
                priority: true,
                ..ListOpts::default()
            },
            Shape::Grouped {
                folder_pred: "m3.folder_id = ?1",
            },
        );
        let order = &sql[sql.rfind(" ORDER BY ").unwrap()..];
        assert!(
            order.starts_with(" ORDER BY (t.id IN (SELECT thread_id FROM fork_priority)"),
            "Priority must sort before reminders and the date: {order}"
        );
    }
}
