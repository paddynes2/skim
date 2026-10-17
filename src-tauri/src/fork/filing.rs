//! Filing (v1.1.24, D72): TypeSafe Jev moves automated mail out of the Inbox
//! into named folders (Gmail labels).
//!
//! The rules are the local setting `fork_filing`, one per line:
//! `Folder: what belongs there, in plain words`. For each conversation with an
//! Inbox message inside the look-back window ([`SWEEP_DAYS`]), Jev is asked
//! one yes/no question per rule about its newest Inbox message ([`ask_for`]).
//! At or above the threshold (`fork_filing_threshold`, default
//! [`THRESHOLD_DEFAULT`]) the conversation's Inbox messages move to that
//! rule's folder; with two rules over the bar the higher score wins.
//!
//! Never filed, whatever Jev says ([`Guard`]):
//! - a Deals conversation, a starred one, or one the user wrote in;
//! - mail from inside the company (`replies::Company`: own addresses, own
//!   domain, the Colleagues list);
//! - a conversation whose Inbox mail comes from more than one sender domain
//!   (a person may have written into an automated thread);
//! - senders on the Never-file list (`fork_filing_never`) or the Priority
//!   Always list;
//! - calendar mail, sign-in codes and resets, unauthenticated mail;
//! - mail Priority or To reply scored over its own bar.
//!
//! Moved back means never again: a filed message that shows up in the Inbox
//! again (the Undo bar in the reading pane, or the user moving it) puts its
//! sender on `fork_filing_never` for that rule ([`note_moved_back`]).
//!
//! `fork_filing_mode` is `off` (the default), `preview` (score, list what
//! would move in Settings, move nothing) or `on`. Fail-open like Priority: no
//! key or an outage scores nothing and so moves nothing. Scoring runs inside
//! `priority::schedule`, after To reply and under the same lock. The rules
//! and every list are local: the fork's repository is public.

use crate::commands::mail::{queue_op_local, wire_folder_name};
use crate::db::{bodies, queries, Db};
use crate::error::Result;
use crate::fork::deals;
use crate::fork::priority::{
    self, authenticated, body_preview, excluded, is_calendar, is_sensitive, msg_key, parse_exclude,
    Answer, Ask, Candidate, BODY_WAIT_SECS,
};
use crate::fork::replies::{self, Company};
use crate::state::AppState;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::json;
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap, HashSet};
use tauri::{AppHandle, Emitter, Manager, State};

pub const SETTING_RULES: &str = "fork_filing";
pub const SETTING_MODE: &str = "fork_filing_mode";
pub const SETTING_NEVER: &str = "fork_filing_never";
pub const SETTING_THRESHOLD: &str = "fork_filing_threshold";
/// Internal counter row: `"YYYY-MM-DD:N"`, questions charged so far today.
const USED_KEY: &str = "fork_filing_used";

pub const MODE_OFF: &str = "off";
pub const MODE_PREVIEW: &str = "preview";
pub const MODE_ON: &str = "on";

/// A move hides mail, so the bar is well above Priority's 0.35.
pub const THRESHOLD_DEFAULT: f64 = 0.7;
/// The first pass also files what arrived in this many days.
pub const SWEEP_DAYS: i64 = 30;
/// Questions per local day, every rule together. The first sweep is about
/// two per Inbox conversation in [`SWEEP_DAYS`].
pub const DAILY_CAP: i64 = 2000;
/// Questions per pass; a full pass asks `priority::schedule` to run again.
pub const PASS_LIMIT: i64 = 40;
/// Moves per pass.
const MOVE_LIMIT: usize = 200;
/// More rules than this are ignored: each costs one question per message.
pub const MAX_RULES: usize = 6;
/// A logged message back in the Inbox sooner than this may be the server
/// catching up with the move, not the user undoing it.
const SETTLE_SECS: i64 = 600;
const KEEP_DAYS: i64 = 120;

const QUESTION_NO: &str = "Anything else, above all a real person writing to them \
personally (a client, prospect, partner, investor, colleague, broker or seller), or a notice \
they must act on (a payment problem, a security alert, a legal or tax deadline, a meeting change).";

// ---- rules ---------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    /// The folder name, lowercased: the rule's identity in the log and the
    /// never list.
    pub key: String,
    /// What Jev's answers are stored under: the key plus a hash of the
    /// description, so editing a description asks Jev again.
    pub score_key: String,
    pub folder: String,
    pub description: String,
}

fn score_key(key: &str, description: &str) -> String {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(description.as_bytes());
    let hex: String = hash.iter().take(6).map(|b| format!("{b:02x}")).collect();
    format!("{key}\u{1f}{hex}")
}

/// System folder names a rule may not take: the move would delete, train
/// the spam filter or hide mail. `folder_for` also refuses any folder with a
/// role, whatever its name.
const RESERVED: &[&str] = &[
    "inbox",
    "spam",
    "junk",
    "trash",
    "bin",
    "deleted",
    "deleted items",
    "sent",
    "sent mail",
    "drafts",
    "archive",
    "all mail",
    "starred",
    "important",
];

/// `Folder: description` per line. Blank lines, `#` comments, lines without
/// a colon or with an empty side, and a second rule for the same folder are
/// skipped; at most [`MAX_RULES`].
pub fn parse_rules(text: &str) -> Vec<Rule> {
    let mut out: Vec<Rule> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((folder, description)) = line.split_once(':') else {
            continue;
        };
        let (folder, description) = (folder.trim(), description.trim());
        if folder.is_empty() || description.is_empty() || folder.len() > 60 {
            continue;
        }
        let key = folder.to_lowercase();
        if RESERVED.contains(&key.as_str())
            || key.starts_with("[gmail]")
            || out.iter().any(|r| r.key == key)
        {
            continue;
        }
        out.push(Rule {
            score_key: score_key(&key, description),
            key,
            folder: folder.to_string(),
            description: description.to_string(),
        });
        if out.len() >= MAX_RULES {
            break;
        }
    }
    out
}

/// The question for one rule. Its first sentence is the local
/// `fork_priority_about`, as Priority's is.
pub fn ask_for(rule: &Rule, about: Option<&str>) -> Ask {
    let about = about
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(priority::ABOUT_DEFAULT);
    Ask {
        instructions: format!(
            "{about} This email arrived in their inbox. Does it belong in their \"{}\" folder \
instead, so they never need to see it in the inbox?",
            rule.folder
        ),
        yes: Cow::Owned(rule.description.clone()),
        no: Cow::Borrowed(QUESTION_NO),
    }
}

pub fn parse_threshold(raw: Option<&str>) -> f64 {
    raw.and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|v| *v >= 0.5 && *v <= 1.0)
        .unwrap_or(THRESHOLD_DEFAULT)
}

pub fn mode(conn: &Connection) -> rusqlite::Result<&'static str> {
    Ok(
        match queries::get_setting(conn, SETTING_MODE)?
            .as_deref()
            .map(str::trim)
        {
            Some(MODE_ON) => MODE_ON,
            Some(MODE_PREVIEW) => MODE_PREVIEW,
            _ => MODE_OFF,
        },
    )
}

/// Pick the rule a message files under: the highest score at or above the
/// bar, ties to the earlier rule. Unscored rules (`None`) never win.
pub fn best<'a>(
    rules: &'a [Rule],
    scores: &HashMap<String, Option<f64>>,
    bar: f64,
) -> Option<(&'a Rule, f64)> {
    let mut pick: Option<(&Rule, f64)> = None;
    for r in rules {
        if let Some(Some(s)) = scores.get(&r.score_key) {
            if *s >= bar && pick.is_none_or(|(_, b)| *s > b) {
                pick = Some((r, *s));
            }
        }
    }
    pick
}

// ---- the conversations ---------------------------------------------------------

/// The newest Inbox message of one conversation.
#[derive(Debug, Clone, Default)]
pub struct Newest {
    pub thread_id: i64,
    pub account_id: String,
    pub key: String,
    pub from_name: String,
    pub from_addr: String,
    pub subject: String,
    pub preview: String,
    pub body_ready: bool,
    pub date: i64,
    pub spf: String,
    pub dkim: String,
    pub dmarc: String,
}

fn newest(conn: &Connection, floor: i64) -> rusqlite::Result<Vec<Newest>> {
    let key = msg_key("m");
    let mut stmt = conn.prepare(&format!(
        "SELECT m.thread_id, m.account_id, {key}, coalesce(m.from_name, ''),
                coalesce(m.from_addr, ''), coalesce(m.subject, ''), coalesce(m.snippet, ''),
                m.body_state IN (1, 2), m.date,
                coalesce((SELECT b.body_text FROM message_bodies b WHERE b.message_id = m.id), ''),
                coalesce(m.auth_spf, ''), coalesce(m.auth_dkim, ''), coalesce(m.auth_dmarc, '')
         FROM messages m JOIN folders f ON f.id = m.folder_id
         WHERE f.role = 'inbox' AND m.date >= ?1
           AND m.id = (SELECT m2.id FROM messages m2 JOIN folders f2 ON f2.id = m2.folder_id
                       WHERE f2.role = 'inbox' AND m2.thread_id = m.thread_id
                       ORDER BY m2.date DESC, m2.id DESC LIMIT 1)
         ORDER BY m.date DESC, m.id DESC"
    ))?;
    let rows = stmt.query_map(params![floor], |r| {
        let snippet: String = r.get(6)?;
        let body: String = r.get(9)?;
        let from_body = body_preview(&body);
        Ok(Newest {
            thread_id: r.get(0)?,
            account_id: r.get(1)?,
            key: r.get(2)?,
            from_name: r.get(3)?,
            from_addr: r.get(4)?,
            subject: r.get(5)?,
            preview: if from_body.is_empty() {
                snippet
            } else {
                from_body
            },
            body_ready: r.get(7)?,
            date: r.get(8)?,
            spf: r.get(10)?,
            dkim: r.get(11)?,
            dmarc: r.get(12)?,
        })
    })?;
    rows.collect()
}

/// What rules out filing a conversation, loaded once per pass.
pub struct Guard {
    company: Company,
    deals: HashSet<i64>,
    never: Vec<String>,
    never_rule: HashSet<(String, String)>,
    priority_bar: f64,
    reply_bar: f64,
}

impl Guard {
    pub fn load(conn: &mut Connection, deals: &deals::Cache) -> rusqlite::Result<Self> {
        let deals: HashSet<i64> = deals.membership(conn)?.1.keys().copied().collect();
        let mut never =
            parse_exclude(&queries::get_setting(conn, SETTING_NEVER)?.unwrap_or_default());
        never.extend(parse_exclude(
            &queries::get_setting(conn, priority::SETTING_ALWAYS)?.unwrap_or_default(),
        ));
        let mut stmt = conn.prepare("SELECT rule, from_addr FROM fork_filing_never")?;
        let never_rule = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<HashSet<_>>>()?;
        drop(stmt);
        Ok(Guard {
            company: Company::load(conn)?,
            deals,
            never,
            never_rule,
            priority_bar: priority::parse_threshold(
                queries::get_setting(conn, priority::SETTING_THRESHOLD)?.as_deref(),
            ),
            reply_bar: replies::parse_threshold(
                queries::get_setting(conn, replies::SETTING_THRESHOLD)?.as_deref(),
            ),
        })
    }

    /// Why this conversation is never filed, or `None` when it may be. Rule
    /// by rule refusals are [`Guard::refused_by`].
    pub fn kept(&self, conn: &Connection, n: &Newest) -> rusqlite::Result<Option<&'static str>> {
        if self.deals.contains(&n.thread_id) {
            return Ok(Some("deal"));
        }
        if self.company.inside(&n.from_addr) {
            return Ok(Some("company"));
        }
        if excluded(&n.from_addr, &self.never) {
            return Ok(Some("never"));
        }
        if is_calendar(&n.subject, &n.from_addr) || is_sensitive(&n.subject, &n.preview) {
            return Ok(Some("notice"));
        }
        if !authenticated(&n.spf, &n.dkim, &n.dmarc) {
            return Ok(Some("unauthenticated"));
        }
        // Every message of the conversation, not only the newest: all of its
        // Inbox copies move together, so each must pass on its own.
        let key = msg_key("m");
        let mut stmt = conn.prepare_cached(&format!(
            "SELECT m.is_starred, coalesce(m.from_addr, ''), coalesce(f.role, ''),
                    coalesce(m.subject, ''), coalesce(m.snippet, ''),
                    coalesce(m.auth_spf, ''), coalesce(m.auth_dkim, ''), coalesce(m.auth_dmarc, ''),
                    coalesce(m.message_id, ''), {key}
             FROM messages m JOIN folders f ON f.id = m.folder_id
             WHERE m.thread_id = ?1"
        ))?;
        let rows = stmt.query_map(params![n.thread_id], |r| {
            Ok((
                r.get::<_, bool>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                [
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, String>(7)?,
                    r.get::<_, String>(8)?,
                    r.get::<_, String>(9)?,
                ],
            ))
        })?;
        let sender = n.from_addr.trim().to_ascii_lowercase();
        for row in rows.collect::<rusqlite::Result<Vec<_>>>()? {
            let (starred, from, role, [subject, snippet, spf, dkim, dmarc, mid, key]) = row;
            if starred {
                return Ok(Some("starred"));
            }
            if self.company.own.contains(&from.trim().to_ascii_lowercase()) || role == "sent" {
                return Ok(Some("own"));
            }
            if role != "inbox" {
                continue;
            }
            // One sender, by address: a platform relays real people from
            // its own domain, so a domain match is not enough.
            if from.trim().to_ascii_lowercase() != sender {
                return Ok(Some("mixed"));
            }
            // The filing log and the moved-back check need a Message-ID.
            if mid.trim().is_empty() {
                return Ok(Some("no-id"));
            }
            if is_calendar(&subject, &from) || is_sensitive(&subject, &snippet) {
                return Ok(Some("notice"));
            }
            if !authenticated(&spf, &dkim, &dmarc) {
                return Ok(Some("unauthenticated"));
            }
            if self.wanted(conn, &key)? {
                return Ok(Some("wanted"));
            }
        }
        Ok(None)
    }

    /// Priority or To reply rated this message worth reading: it stays.
    /// Only a scored message can be rated: Priority never scores bulk mail.
    fn wanted(&self, conn: &Connection, key: &str) -> rusqlite::Result<bool> {
        let p: Option<f64> = conn
            .query_row(
                "SELECT score FROM fork_priority_score WHERE msg_key = ?1",
                params![key],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        let r: Option<f64> = conn
            .query_row(
                "SELECT score FROM fork_reply_score WHERE msg_key = ?1",
                params![key],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        Ok(p.is_some_and(|s| s >= self.priority_bar) || r.is_some_and(|s| s >= self.reply_bar))
    }

    pub fn refused_by(&self, rule: &Rule, from_addr: &str) -> bool {
        self.never_rule
            .contains(&(rule.key.clone(), from_addr.trim().to_ascii_lowercase()))
    }
}

fn scores_for(conn: &Connection, key: &str) -> rusqlite::Result<HashMap<String, Option<f64>>> {
    let mut stmt =
        conn.prepare_cached("SELECT rule, score FROM fork_filing_score WHERE msg_key = ?1")?;
    let rows = stmt.query_map(params![key], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, Option<f64>>(1)?))
    })?;
    rows.collect()
}

/// Every Message-ID of this conversation's Inbox copies, with the row ids.
fn inbox_messages(conn: &Connection, thread_id: i64) -> rusqlite::Result<Vec<(i64, String)>> {
    let mut stmt = conn.prepare_cached(
        "SELECT m.id, coalesce(nullif(m.message_id, ''), 'id:' || m.id)
         FROM messages m JOIN folders f ON f.id = m.folder_id
         WHERE f.role = 'inbox' AND m.thread_id = ?1",
    )?;
    let rows = stmt.query_map(params![thread_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    rows.collect()
}

fn logged(conn: &Connection, account_id: &str, message_id: &str) -> rusqlite::Result<bool> {
    conn.query_row(
        "SELECT 1 FROM fork_filing_log WHERE account_id = ?1 AND message_id = ?2",
        params![account_id, message_id],
        |_| Ok(()),
    )
    .optional()
    .map(|o| o.is_some())
}

/// One conversation that files.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Filing {
    pub thread_id: i64,
    #[serde(skip)]
    pub account_id: String,
    #[serde(skip)]
    pub messages: Vec<(i64, String)>,
    pub from_name: String,
    pub from_addr: String,
    pub subject: String,
    pub date: i64,
    #[serde(skip)]
    pub rule: String,
    pub folder: String,
    pub score: f64,
}

/// What files now, from the stored scores, under every guard.
pub fn decide(
    conn: &mut Connection,
    deals: &deals::Cache,
    now: i64,
) -> rusqlite::Result<Vec<Filing>> {
    let rules = parse_rules(&queries::get_setting(conn, SETTING_RULES)?.unwrap_or_default());
    if rules.is_empty() {
        return Ok(Vec::new());
    }
    let bar = parse_threshold(queries::get_setting(conn, SETTING_THRESHOLD)?.as_deref());
    let guard = Guard::load(conn, deals)?;
    let mut out = Vec::new();
    for n in newest(conn, now - SWEEP_DAYS * 86_400)? {
        let scores = scores_for(conn, &n.key)?;
        let open: Vec<Rule> = rules
            .iter()
            .filter(|r| !guard.refused_by(r, &n.from_addr))
            .cloned()
            .collect();
        let Some((rule, score)) = best(&open, &scores, bar) else {
            continue;
        };
        if guard.kept(conn, &n)?.is_some() {
            continue;
        }
        let messages = inbox_messages(conn, n.thread_id)?;
        // Filed once already: it is back because the user moved it back
        // ([`note_moved_back`]) or because the server has not caught up.
        let mut seen = false;
        for (_, mid) in &messages {
            seen |= logged(conn, &n.account_id, mid)?;
        }
        if seen || messages.is_empty() {
            continue;
        }
        out.push(Filing {
            thread_id: n.thread_id,
            account_id: n.account_id.clone(),
            messages,
            from_name: n.from_name.clone(),
            from_addr: n.from_addr.clone(),
            subject: n.subject.clone(),
            date: n.date,
            rule: rule.key.clone(),
            folder: rule.folder.clone(),
            score,
        });
    }
    Ok(out)
}

/// A filed message the user moved back to the Inbox: its sender is never
/// filed by that rule again. Returns how many were noted.
pub fn note_moved_back(conn: &mut Connection, now: i64) -> rusqlite::Result<usize> {
    let tx = conn.transaction()?;
    let back: Vec<(i64, String, String)> = {
        let mut stmt = tx.prepare(
            "SELECT l.id, l.rule, lower(l.from_addr) FROM fork_filing_log l
             WHERE l.undone_at IS NULL AND l.filed_at < ?1
               AND NOT EXISTS (SELECT 1 FROM pending_ops p WHERE p.account_id = l.account_id
                               AND p.kind = 'move' AND p.state = 'pending')
               AND EXISTS (SELECT 1 FROM messages m JOIN folders f ON f.id = m.folder_id
                           WHERE f.role = 'inbox' AND m.account_id = l.account_id
                             AND coalesce(nullif(m.message_id, ''), 'id:' || m.id) = l.message_id)",
        )?;
        let rows = stmt.query_map(params![now - SETTLE_SECS], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    for (id, rule, from) in &back {
        tx.execute(
            "UPDATE fork_filing_log SET undone_at = ?2 WHERE id = ?1",
            params![id, now],
        )?;
        tx.execute(
            "INSERT OR IGNORE INTO fork_filing_never (rule, from_addr, added_at) VALUES (?1, ?2, ?3)",
            params![rule, from, now],
        )?;
    }
    tx.commit()?;
    Ok(back.len())
}

// ---- scoring -------------------------------------------------------------------

fn used_today(conn: &Connection, today: &str) -> rusqlite::Result<i64> {
    let raw = queries::get_setting(conn, USED_KEY)?.unwrap_or_default();
    Ok(match raw.split_once(':') {
        Some((day, n)) if day == today => n.parse().unwrap_or(0),
        _ => 0,
    })
}

/// Per rule, the messages to ask Jev about.
type Questions = Vec<(Rule, Vec<Candidate>)>;

/// The questions still to ask: per rule, the newest Inbox messages without a
/// score for it, under every guard that does not need a score, at most
/// `limit` in all. Messages waiting on their body are skipped for
/// [`BODY_WAIT_SECS`], as in Priority.
fn questions(
    conn: &mut Connection,
    deals: &deals::Cache,
    rules: &[Rule],
    now: i64,
    limit: usize,
) -> rusqlite::Result<(Questions, bool)> {
    let guard = Guard::load(conn, deals)?;
    let mut per: Vec<(Rule, Vec<Candidate>)> =
        rules.iter().map(|r| (r.clone(), Vec::new())).collect();
    let mut total = 0;
    let mut waiting = false;
    if limit == 0 {
        return Ok((per, waiting));
    }
    'outer: for n in newest(conn, now - SWEEP_DAYS * 86_400)? {
        let scores = scores_for(conn, &n.key)?;
        let missing: Vec<usize> = rules
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                !scores.contains_key(&r.score_key) && !guard.refused_by(r, &n.from_addr)
            })
            .map(|(i, _)| i)
            .collect();
        if missing.is_empty() || guard.kept(conn, &n)?.is_some() {
            continue;
        }
        if !n.body_ready && n.date > now - BODY_WAIT_SECS {
            waiting = true;
            continue;
        }
        let c = Candidate {
            key: n.key.clone(),
            from_name: n.from_name.clone(),
            from_addr: n.from_addr.clone(),
            subject: n.subject.clone(),
            preview: n.preview.clone(),
        };
        for i in missing {
            per[i].1.push(c.clone());
            total += 1;
            if total >= limit {
                break 'outer;
            }
        }
    }
    Ok((per, waiting))
}

fn store_scores(
    conn: &mut Connection,
    rule: &str,
    answers: &[Answer],
    today: &str,
    now: i64,
) -> rusqlite::Result<()> {
    let used = used_today(conn, today)? + answers.len() as i64;
    let tx = conn.transaction()?;
    {
        let mut ins = tx.prepare_cached(
            "INSERT OR REPLACE INTO fork_filing_score (msg_key, rule, score, model, scored_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for (key, score, model) in answers {
            ins.execute(params![key, rule, score, model, now])?;
        }
    }
    tx.execute(
        "DELETE FROM fork_filing_score WHERE scored_at < ?1",
        params![now - KEEP_DAYS * 86_400],
    )?;
    queries::set_setting(&tx, USED_KEY, &format!("{today}:{used}"))?;
    tx.commit()
}

/// Ask Jev what is new, when Filing is not off, keyed, under the cap and not
/// paused. Called by `priority::schedule` while it holds the scoring lock.
/// Returns how many answers were stored and whether messages wait on bodies.
pub async fn score_new(app: &AppHandle) -> (usize, bool) {
    if priority::paused() {
        return (0, false);
    }
    let Some(api_key) = priority::api_key() else {
        return (0, false);
    };
    let state = app.state::<AppState>();
    let db: Db = state.db.clone();
    let cache = state.fork.deals.clone();
    let now = now_ts();
    let day = today();
    let found = db
        .call({
            let day = day.clone();
            move |conn| {
                if mode(conn)? == MODE_OFF {
                    return Ok(None);
                }
                let rules =
                    parse_rules(&queries::get_setting(conn, SETTING_RULES)?.unwrap_or_default());
                if rules.is_empty() {
                    return Ok(None);
                }
                let left = (DAILY_CAP - used_today(conn, &day)?).clamp(0, PASS_LIMIT);
                let about = queries::get_setting(conn, priority::SETTING_ABOUT)?;
                let (per, waiting) = questions(conn, &cache, &rules, now, left as usize)?;
                Ok(Some((per, waiting, about)))
            }
        })
        .await;
    let (per, waiting, about) = match found {
        Ok(Some(v)) => v,
        Ok(None) => return (0, false),
        Err(e) => {
            tracing::warn!(error = %e, "filing: candidate scan failed");
            return (0, false);
        }
    };
    let mut stored = 0;
    for (rule, cands) in per {
        if cands.is_empty() {
            continue;
        }
        let ask = ask_for(&rule, about.as_deref());
        let (answers, fatal) = priority::score_all(&api_key, cands, &ask).await;
        priority::note_health(fatal.as_deref());
        let n = answers.len();
        if n > 0 {
            let day = day.clone();
            let key = rule.score_key.clone();
            if let Err(e) = db
                .call(move |conn| store_scores(conn, &key, &answers, &day, now))
                .await
            {
                tracing::warn!(error = %e, "filing: storing scores failed");
                return (stored, waiting);
            }
            stored += n;
        }
        if fatal.is_some() {
            break;
        }
    }
    (stored, waiting)
}

// ---- moving --------------------------------------------------------------------

/// The folder row for `name` on this account, created locally (and on the
/// server by the move, `createDest`) when missing. `None` when the name is a
/// system folder (any folder with a role).
fn folder_for(
    conn: &Connection,
    account_id: &str,
    name: &str,
) -> rusqlite::Result<Option<(i64, String, bool)>> {
    let found: Option<(i64, String, Option<String>)> = conn
        .query_row(
            "SELECT id, imap_name, role FROM folders
             WHERE account_id = ?1 AND (lower(imap_name) = lower(?2) OR lower(display_name) = lower(?2))
             ORDER BY role IS NOT NULL DESC, id LIMIT 1",
            params![account_id, name],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    if let Some((id, imap, role)) = found {
        return Ok(role.is_none().then_some((id, imap, false)));
    }
    let (imap_name, display_name) = wire_folder_name(conn, account_id, name)?;
    conn.execute(
        "INSERT OR IGNORE INTO folders (account_id, imap_name, role, display_name, sort_order)
         VALUES (?1, ?2, NULL, ?3, 100)",
        params![account_id, imap_name, display_name],
    )?;
    let id: i64 = conn.query_row(
        "SELECT id FROM folders WHERE account_id = ?1 AND imap_name = ?2",
        params![account_id, imap_name],
        |r| r.get(0),
    )?;
    Ok(Some((id, imap_name, true)))
}

/// Internal: ids of failed Filing moves already recovered, comma-separated.
const RECOVERED_KEY: &str = "fork_filing_recovered";
/// Internal: why Filing paused itself, shown in Settings until the user saves.
pub const PAUSED_KEY: &str = "fork_filing_paused";

/// A Filing move the server refused for good. `remove_messages_local` took
/// its rows out of the Inbox at once, and the forward-only sync would bring
/// back only mail newer than what is still held, so the rest would stay
/// hidden. Lower the folder's `last_seen_uid` below the op's lowest UID so
/// the next sync fetches them again (`insert_message` skips rows it holds),
/// close their log rows without a never-file entry, and drop Filing back to
/// preview with a reason. Returns how many ops were recovered.
pub fn recover_failed(conn: &mut Connection, now: i64) -> rusqlite::Result<usize> {
    let done: HashSet<i64> = queries::get_setting(conn, RECOVERED_KEY)?
        .unwrap_or_default()
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    let failed: Vec<(i64, String, i64)> = {
        let mut stmt = conn.prepare(
            "SELECT id, payload, created_at FROM pending_ops
             WHERE kind = 'move' AND state = 'failed'
               AND json_extract(payload, '$.forkFiling') = 1",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let fresh: Vec<_> = failed
        .into_iter()
        .filter(|(id, ..)| !done.contains(id))
        .collect();
    if fresh.is_empty() {
        return Ok(0);
    }
    let tx = conn.transaction()?;
    let mut ids: Vec<i64> = done.into_iter().collect();
    for (id, payload, created) in &fresh {
        let p: serde_json::Value = serde_json::from_str(payload).unwrap_or_default();
        let folder = p["folderId"].as_i64();
        let low = p["uids"]
            .as_array()
            .and_then(|u| u.iter().filter_map(|v| v.as_i64()).min());
        if let (Some(folder), Some(low)) = (folder, low) {
            tx.execute(
                "UPDATE folders SET last_seen_uid = min(last_seen_uid, ?2) WHERE id = ?1",
                params![folder, (low - 1).max(0)],
            )?;
        }
        tx.execute(
            "UPDATE fork_filing_log SET undone_at = ?3
             WHERE undone_at IS NULL AND lower(folder) = lower(?1) AND filed_at BETWEEN ?2 - 120 AND ?2 + 120",
            params![p["destImapName"].as_str().unwrap_or_default(), created, now],
        )?;
        ids.push(*id);
    }
    ids.sort();
    let list: Vec<String> = ids.iter().map(i64::to_string).collect();
    queries::set_setting(&tx, RECOVERED_KEY, &list.join(","))?;
    queries::set_setting(&tx, SETTING_MODE, MODE_PREVIEW)?;
    queries::set_setting(
        &tx,
        PAUSED_KEY,
        "The mail server refused a move, so Filing went back to preview. The mail is being fetched back into the Inbox.",
    )?;
    tx.commit()?;
    Ok(fresh.len())
}

/// Queue the moves for `filings` and log them. Returns the accounts to poke,
/// how many moved and whether a folder was created. Not one transaction:
/// `remove_messages_local` opens its own. The log row follows its move, so a
/// failure in between leaves mail filed without an Undo bar, never an Undo
/// bar on mail that did not move.
fn queue_moves(
    conn: &mut Connection,
    filings: &[Filing],
    now: i64,
) -> rusqlite::Result<(Vec<String>, usize, bool)> {
    let mut accounts = Vec::new();
    let mut created = false;
    let mut moved = 0;
    for f in filings.iter().take(MOVE_LIMIT) {
        // A rule named after a system folder (Spam, Trash, Archive) never
        // moves mail: Gmail purges two of them and learns from Spam.
        let Some((dest_id, dest_imap, create)) = folder_for(conn, &f.account_id, &f.folder)? else {
            continue;
        };
        created |= create;
        let ids: Vec<i64> = f.messages.iter().map(|(id, _)| *id).collect();
        // `forkFiling` marks the op for [`recover_failed`]; the executor
        // ignores keys it does not read.
        let extra = json!({
            "destFolderId": dest_id,
            "destImapName": dest_imap,
            "createDest": create,
            "forkFiling": true,
        });
        accounts.extend(queue_op_local(
            conn,
            &ids,
            "move",
            &extra,
            bodies::remove_messages_local,
        )?);
        for (_, mid) in &f.messages {
            conn.execute(
                "INSERT INTO fork_filing_log
                   (account_id, message_id, from_addr, subject, rule, folder, score, filed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    f.account_id,
                    mid,
                    f.from_addr.to_ascii_lowercase(),
                    f.subject,
                    f.rule,
                    f.folder,
                    f.score,
                    now
                ],
            )?;
        }
        moved += 1;
    }
    accounts.sort();
    accounts.dedup();
    Ok((accounts, moved, created))
}

async fn poke(app: &AppHandle, accounts: Vec<String>, folders: bool) {
    let state = app.state::<AppState>();
    let engines = state.engines.lock().await;
    for id in accounts {
        if let Some(h) = engines.get(&id) {
            h.run_ops();
        }
    }
    drop(engines);
    if folders {
        let _ = app.emit("folders:updated", json!({}));
    }
    let _ = app.emit("mail:updated", json!({}));
    crate::badge::refresh(app).await;
}

/// Note moved-back mail, then, with Filing on, move what files. Returns how
/// many conversations moved.
pub async fn apply(app: &AppHandle) -> usize {
    let state = app.state::<AppState>();
    let cache = state.fork.deals.clone();
    let now = now_ts();
    let out = state
        .db
        .call(move |conn| {
            if recover_failed(conn, now)? > 0 {
                tracing::warn!("filing: a move failed; back to preview, mail refetched");
            }
            note_moved_back(conn, now)?;
            if mode(conn)? != MODE_ON {
                return Ok((Vec::new(), 0, false));
            }
            let filings = decide(conn, &cache, now)?;
            if filings.is_empty() {
                return Ok((Vec::new(), 0, false));
            }
            queue_moves(conn, &filings, now)
        })
        .await;
    match out {
        Ok((accounts, moved, created)) => {
            if moved > 0 {
                tracing::info!(moved, "filing: conversations filed");
                poke(app, accounts, created).await;
            }
            moved
        }
        Err(e) => {
            tracing::warn!(error = %e, "filing: apply failed");
            0
        }
    }
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

// ---- commands ------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilingStatus {
    pub mode: String,
    pub rules: String,
    pub never: String,
    pub threshold: f64,
    pub scored_today: i64,
    pub cap: i64,
    pub filed_today: i64,
    pub ignored: Vec<String>,
    /// Why Filing went back to preview on its own ([`recover_failed`]).
    pub paused: Option<String>,
}

#[tauri::command]
pub async fn fork_filing_status(state: State<'_, AppState>) -> Result<FilingStatus> {
    let now = now_ts();
    let day = today();
    state
        .db
        .read("fork_filing_status", move |conn| {
            let rules = queries::get_setting(conn, SETTING_RULES)?.unwrap_or_default();
            let parsed = parse_rules(&rules);
            let ignored = rules
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .filter(|l| {
                    !l.split_once(':').is_some_and(|(f, d)| {
                        parsed
                            .iter()
                            .any(|r| r.folder == f.trim() && r.description == d.trim())
                    })
                })
                .map(str::to_string)
                .collect();
            let filed_today: i64 = conn.query_row(
                "SELECT count(*) FROM fork_filing_log WHERE filed_at >= ?1",
                params![now - 86_400],
                |r| r.get(0),
            )?;
            Ok(FilingStatus {
                mode: mode(conn)?.to_string(),
                rules,
                never: queries::get_setting(conn, SETTING_NEVER)?.unwrap_or_default(),
                threshold: parse_threshold(
                    queries::get_setting(conn, SETTING_THRESHOLD)?.as_deref(),
                ),
                scored_today: used_today(conn, &day)?,
                cap: DAILY_CAP,
                filed_today,
                ignored,
                paused: queries::get_setting(conn, PAUSED_KEY)?.filter(|s| !s.trim().is_empty()),
            })
        })
        .await
}

/// What would move now, in any mode: the Settings preview.
#[tauri::command]
pub async fn fork_filing_preview(state: State<'_, AppState>) -> Result<Vec<Filing>> {
    let cache = state.fork.deals.clone();
    let now = now_ts();
    state.db.call(move |conn| decide(conn, &cache, now)).await
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FiledNote {
    pub folder: String,
    pub score: f64,
}

/// The open conversation was filed by Jev and is still where it was filed.
#[tauri::command]
pub async fn fork_filing_for_thread(
    state: State<'_, AppState>,
    thread_id: i64,
) -> Result<Option<FiledNote>> {
    state
        .db
        .read("fork_filing_for_thread", move |conn| {
            conn.query_row(
                "SELECT l.folder, l.score FROM fork_filing_log l
                 JOIN messages m ON m.account_id = l.account_id
                   AND coalesce(nullif(m.message_id, ''), 'id:' || m.id) = l.message_id
                 JOIN folders f ON f.id = m.folder_id
                 WHERE m.thread_id = ?1 AND l.undone_at IS NULL
                   AND (lower(f.imap_name) = lower(l.folder) OR lower(f.display_name) = lower(l.folder))
                 ORDER BY l.filed_at DESC LIMIT 1",
                params![thread_id],
                |r| Ok(FiledNote { folder: r.get(0)?, score: r.get(1)? }),
            )
            .optional()
        })
        .await
}

/// Undo: the conversation's copies in its filed folder go back to the Inbox,
/// and the sender is never filed by that rule again. Returns how many
/// messages moved; 0 means nothing was found to move and nothing changed.
#[tauri::command]
pub async fn fork_filing_undo(
    app: AppHandle,
    state: State<'_, AppState>,
    thread_id: i64,
) -> Result<usize> {
    let now = now_ts();
    let (accounts, moved) = state
        .db
        .call(move |conn| {
            let rows: Vec<(i64, String, i64, String, String)> = {
                let mut stmt = conn.prepare(
                    "SELECT m.id, m.account_id, l.id, l.rule, lower(l.from_addr) FROM fork_filing_log l
                     JOIN messages m ON m.account_id = l.account_id
                       AND coalesce(nullif(m.message_id, ''), 'id:' || m.id) = l.message_id
                     JOIN folders f ON f.id = m.folder_id
                     WHERE m.thread_id = ?1 AND l.undone_at IS NULL
                       AND (lower(f.imap_name) = lower(l.folder) OR lower(f.display_name) = lower(l.folder))",
                )?;
                let r = stmt.query_map(params![thread_id], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
                })?;
                r.collect::<rusqlite::Result<_>>()?
            };
            let mut by_account: BTreeMap<String, Vec<i64>> = BTreeMap::new();
            for (mid, account, ..) in &rows {
                by_account.entry(account.clone()).or_default().push(*mid);
            }
            let mut accounts = Vec::new();
            for (account, ids) in by_account {
                let (inbox_id, inbox_imap): (i64, String) = conn.query_row(
                    "SELECT id, imap_name FROM folders WHERE account_id = ?1 AND role = 'inbox'",
                    params![account],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?;
                let mut unique = ids;
                unique.sort();
                unique.dedup();
                let extra = json!({ "destFolderId": inbox_id, "destImapName": inbox_imap, "createDest": false });
                accounts.extend(queue_op_local(conn, &unique, "move", &extra, bodies::remove_messages_local)?);
            }
            let tx = conn.transaction()?;
            for (_, _, log_id, rule, from) in &rows {
                tx.execute("UPDATE fork_filing_log SET undone_at = ?2 WHERE id = ?1", params![log_id, now])?;
                tx.execute(
                    "INSERT OR IGNORE INTO fork_filing_never (rule, from_addr, added_at) VALUES (?1, ?2, ?3)",
                    params![rule, from, now],
                )?;
            }
            tx.commit()?;
            Ok((accounts, rows.len()))
        })
        .await?;
    if moved > 0 {
        poke(&app, accounts, false).await;
    }
    Ok(moved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_parse_one_per_folder() {
        let r = parse_rules(
            "# comment\nDeal Flow: listing alerts\n\nnot a rule\nInbox: no\nDeal flow: second\n: empty\nNotifications:  digests \n",
        );
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].folder, "Deal Flow");
        assert_eq!(r[0].key, "deal flow");
        assert_eq!(r[1].description, "digests");
    }

    #[test]
    fn rules_are_capped() {
        let text: String = (0..10).map(|i| format!("F{i}: d\n")).collect();
        assert_eq!(parse_rules(&text).len(), MAX_RULES);
    }

    #[test]
    fn best_needs_the_bar_and_takes_the_higher() {
        let rules = parse_rules("A: a\nB: b\n");
        let (a, b) = (rules[0].score_key.clone(), rules[1].score_key.clone());
        let mut s = HashMap::new();
        s.insert(a.clone(), Some(0.69));
        s.insert(b.clone(), None);
        assert!(best(&rules, &s, 0.7).is_none());
        s.insert("a".to_string(), Some(0.95));
        assert!(
            best(&rules, &s, 0.7).is_none(),
            "scored under another description"
        );
        s.insert(a.clone(), Some(0.8));
        s.insert(b.clone(), Some(0.9));
        assert_eq!(
            best(&rules, &s, 0.7).map(|(r, _)| r.key.as_str()),
            Some("b")
        );
        s.insert(b.clone(), Some(0.8));
        assert_eq!(
            best(&rules, &s, 0.7).map(|(r, _)| r.key.as_str()),
            Some("a")
        );
    }

    #[test]
    fn threshold_never_below_half() {
        assert_eq!(parse_threshold(Some("0.3")), THRESHOLD_DEFAULT);
        assert_eq!(parse_threshold(Some("0.8")), 0.8);
        assert_eq!(parse_threshold(None), THRESHOLD_DEFAULT);
    }

    // ---- against the real schema -----------------------------------------------

    use crate::db::models::NewMessage;
    use crate::db::queries::insert_message;

    const ME: &str = "me@example.com";

    /// One account: Inbox (1), Sent (2); one rule.
    fn seed(conn: &Connection) {
        conn.execute_batch(&format!(
            "INSERT INTO accounts (id, email, provider, imap_host, smtp_host, created_at)
               VALUES ('a1','{ME}','custom','imap.x','smtp.x',0);
             INSERT INTO folders (id, account_id, imap_name, role, display_name, sort_order)
               VALUES (1,'a1','INBOX','inbox','Inbox',0),
                      (2,'a1','Sent','sent','Sent',1);"
        ))
        .unwrap();
        queries::set_setting(
            conn,
            SETTING_RULES,
            "Deal Flow: business-for-sale listing alerts",
        )
        .unwrap();
    }

    fn add(
        conn: &mut Connection,
        folder: i64,
        uid: u32,
        msgid: &str,
        refs: &[&str],
        from: &str,
        date: i64,
    ) -> i64 {
        let (pk, thread) = insert_message(
            conn,
            &NewMessage {
                account_id: "a1".into(),
                folder_id: folder,
                uid,
                message_id: Some(format!("<{msgid}>")),
                references: refs.iter().map(|r| format!("<{r}>")).collect(),
                subject: Some(format!("Listing {msgid}")),
                from_name: Some("Sender".into()),
                from_addr: Some(from.into()),
                date,
                snippet: Some("New SaaS listing".into()),
                auth_dmarc: Some("pass".into()),
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
        thread
    }

    fn key_of(conn: &Connection, thread: i64) -> String {
        newest(conn, 0)
            .unwrap()
            .into_iter()
            .find(|n| n.thread_id == thread)
            .expect("in the inbox")
            .key
    }

    /// Jev's answer for the newest Inbox message of a thread.
    fn score(conn: &Connection, thread: i64, s: f64) {
        conn.execute(
            "INSERT OR REPLACE INTO fork_filing_score (msg_key, rule, score, model, scored_at)
             VALUES (?1, ?2, ?3, 'm', 0)",
            params![key_of(conn, thread), rule_key(conn), s],
        )
        .unwrap();
    }

    const BOT: &str = "notifications@listings.com";

    /// The score key of the one seeded rule.
    fn rule_key(conn: &Connection) -> String {
        let text = queries::get_setting(conn, SETTING_RULES).unwrap().unwrap();
        parse_rules(&text)[0].score_key.clone()
    }

    #[test]
    fn editing_a_description_asks_again() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            seed(conn);
            let now = 10_000_000;
            let cache = deals::Cache::default();
            let t = add(conn, 1, 1, "plain", &[], BOT, now - 10);
            score(conn, t, 0.9);
            let rules = parse_rules(&queries::get_setting(conn, SETTING_RULES)?.unwrap());
            assert!(questions(conn, &cache, &rules, now, 5)?.0[0].1.is_empty());
            assert_eq!(decide(conn, &cache, now)?.len(), 1);
            queries::set_setting(conn, SETTING_RULES, "Deal Flow: something else")?;
            let rules = parse_rules(&queries::get_setting(conn, SETTING_RULES)?.unwrap());
            assert_eq!(questions(conn, &cache, &rules, now, 5)?.0[0].1.len(), 1);
            assert!(
                decide(conn, &cache, now)?.is_empty(),
                "the old answer no longer counts"
            );
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn only_a_scored_automated_conversation_under_every_guard_files() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            seed(conn);
            let now = 10_000_000;
            let cache = deals::Cache::default();
            let plain = add(conn, 1, 1, "plain", &[], BOT, now - 10);
            let low = add(conn, 1, 2, "low", &[], BOT, now - 20);
            let starred = add(conn, 1, 3, "starred", &[], BOT, now - 30);
            conn.execute("UPDATE messages SET is_starred = 1 WHERE thread_id = ?1", params![starred])?;
            // A broker's listing he answered: his reply sits in Sent.
            let answered = add(conn, 1, 4, "broker", &[], "carson@broker.com", now - 40);
            add(conn, 2, 1, "reply", &["broker"], ME, now - 35);
            let colleague = add(conn, 1, 5, "colleague", &[], "wes@example.com", now - 50);
            // A person wrote into an automated thread.
            let mixed = add(conn, 1, 6, "mixed1", &[], "alerts@listings.com", now - 70);
            add(conn, 1, 7, "mixed2", &["mixed1"], "anna@person.com", now - 60);
            let rated = add(conn, 1, 8, "rated", &[], BOT, now - 80);
            for t in [plain, starred, answered, colleague, mixed, rated] {
                score(conn, t, 0.9);
            }
            score(conn, low, 0.69);
            // Priority rated this one worth reading.
            conn.execute(
                "INSERT INTO fork_priority_score (msg_key, score, model, scored_at) VALUES (?1, 0.8, 'm', 0)",
                params![key_of(conn, rated)],
            )?;
            // Outside the look-back window.
            let old = add(conn, 1, 9, "old", &[], BOT, now - (SWEEP_DAYS + 1) * 86_400);
            score(conn, old, 0.9);

            let out = decide(conn, &cache, now)?;
            assert_eq!(out.iter().map(|f| f.thread_id).collect::<Vec<_>>(), vec![plain]);
            assert_eq!(out[0].folder, "Deal Flow");

            queries::set_setting(conn, SETTING_NEVER, "listings.com")?;
            assert!(decide(conn, &cache, now)?.is_empty(), "Never-file list");
            queries::set_setting(conn, SETTING_NEVER, "")?;
            queries::set_setting(conn, SETTING_THRESHOLD, "0.95")?;
            assert!(decide(conn, &cache, now)?.is_empty(), "threshold setting");
            queries::set_setting(conn, SETTING_THRESHOLD, "")?;

            let guard = Guard::load(conn, &cache)?;
            let n = newest(conn, 0)?;
            let why = |t: i64| {
                guard
                    .kept(conn, n.iter().find(|x| x.thread_id == t).unwrap())
                    .unwrap()
            };
            assert_eq!(why(plain), None);
            assert_eq!(why(starred), Some("starred"));
            assert_eq!(why(answered), Some("own"));
            assert_eq!(why(colleague), Some("company"));
            assert_eq!(why(mixed), Some("mixed"));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn a_filed_conversation_moved_back_is_never_filed_again() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            seed(conn);
            let now = 10_000_000;
            let cache = deals::Cache::default();
            let t = add(conn, 1, 1, "plain", &[], BOT, now - 10);
            score(conn, t, 0.9);
            let filings = decide(conn, &cache, now)?;
            let (accounts, moved, created) = queue_moves(conn, &filings, now)?;
            assert_eq!(
                (accounts, moved, created),
                (vec!["a1".to_string()], 1, true)
            );
            let (kind, payload): (String, String) =
                conn.query_row("SELECT kind, payload FROM pending_ops", [], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })?;
            assert_eq!(kind, "move");
            assert!(payload.contains("Deal Flow"), "{payload}");
            assert!(newest(conn, 0)?.is_empty(), "out of the Inbox at once");
            assert!(decide(conn, &cache, now)?.is_empty());

            // The server applied the move; the user moves it back: the same
            // Message-ID in the Inbox again.
            conn.execute("DELETE FROM pending_ops", [])?;
            add(conn, 1, 2, "plain", &[], BOT, now - 10);
            assert_eq!(
                note_moved_back(conn, now + 60)?,
                0,
                "too soon: the server may be catching up"
            );
            assert!(
                decide(conn, &cache, now + 60)?.is_empty(),
                "logged: never filed twice"
            );
            assert_eq!(note_moved_back(conn, now + SETTLE_SECS + 1)?, 1);
            // A new listing from that sender stays in the Inbox too.
            let next = add(conn, 1, 3, "next", &[], BOT, now);
            score(conn, next, 0.95);
            assert!(decide(conn, &cache, now + SETTLE_SECS + 2)?.is_empty());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn a_system_folder_is_never_a_rule_or_a_destination() {
        assert!(parse_rules("Spam: cold pitches\nTrash: x\n[Gmail]/Bin: y\nArchive: z").is_empty());
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            seed(conn);
            conn.execute(
                "INSERT INTO folders (id, account_id, imap_name, role, display_name, sort_order)
                 VALUES (3, 'a1', '[Gmail]/Spam', 'junk', 'Pitches', 2)",
                [],
            )?;
            assert!(
                folder_for(conn, "a1", "Pitches")?.is_none(),
                "matched by display name, has a role"
            );
            assert!(folder_for(conn, "a1", "Deal Flow")?.is_some_and(|(_, _, created)| created));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn every_inbox_message_must_pass_and_one_without_an_id_never_moves() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            seed(conn);
            let now = 10_000_000;
            let cache = deals::Cache::default();
            // A buyer's reply relayed from the platform's own domain.
            let relayed = add(conn, 1, 1, "r1", &[], BOT, now - 60);
            add(conn, 1, 2, "r2", &["r1"], "buyer-reply@listings.com", now - 50);
            // An older message in the thread Priority rated.
            let rated = add(conn, 1, 3, "p1", &[], BOT, now - 40);
            let older_key: String = newest(conn, 0)?
                .into_iter()
                .find(|n| n.thread_id == rated)
                .unwrap()
                .key;
            add(conn, 1, 4, "p2", &["p1"], BOT, now - 30);
            conn.execute(
                "INSERT INTO fork_priority_score (msg_key, score, model, scored_at) VALUES (?1, 0.9, 'm', 0)",
                params![older_key],
            )?;
            let bare = add(conn, 1, 5, "bare", &[], BOT, now - 20);
            conn.execute("UPDATE messages SET message_id = '' WHERE thread_id = ?1", params![bare])?;
            for t in [relayed, rated, bare] {
                score(conn, t, 0.9);
            }
            assert!(decide(conn, &cache, now)?.is_empty());
            let guard = Guard::load(conn, &cache)?;
            let n = newest(conn, 0)?;
            let why = |t: i64| guard.kept(conn, n.iter().find(|x| x.thread_id == t).unwrap()).unwrap();
            assert_eq!(why(relayed), Some("mixed"));
            assert_eq!(why(rated), Some("wanted"));
            assert_eq!(why(bare), Some("no-id"));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn no_question_is_asked_once_the_cap_is_spent() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            seed(conn);
            let now = 10_000_000;
            add(conn, 1, 1, "plain", &[], BOT, now - 10);
            let rules = parse_rules("Deal Flow: alerts");
            let cache = deals::Cache::default();
            let (per, _) = questions(conn, &cache, &rules, now, 0)?;
            assert!(per.iter().all(|(_, c)| c.is_empty()));
            let (per, _) = questions(conn, &cache, &rules, now, 5)?;
            assert_eq!(per[0].1.len(), 1);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn a_refused_move_brings_the_mail_back_and_pauses_filing() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            seed(conn);
            let now = 10_000_000;
            let cache = deals::Cache::default();
            queries::set_setting(conn, SETTING_MODE, MODE_ON)?;
            let t = add(conn, 1, 7, "plain", &[], BOT, now - 10);
            add(conn, 1, 9, "newer", &[], "anna@person.com", now - 5);
            conn.execute("UPDATE folders SET last_seen_uid = 9 WHERE id = 1", [])?;
            score(conn, t, 0.9);
            let filings = decide(conn, &cache, now)?;
            queue_moves(conn, &filings, now)?;
            conn.execute(
                "UPDATE pending_ops SET state = 'failed', created_at = ?1",
                params![now],
            )?;
            assert_eq!(recover_failed(conn, now + 30)?, 1);
            let last: i64 =
                conn.query_row("SELECT last_seen_uid FROM folders WHERE id = 1", [], |r| {
                    r.get(0)
                })?;
            assert_eq!(last, 6, "the next sync fetches UID 7 again");
            assert_eq!(mode(conn)?, MODE_PREVIEW);
            assert!(queries::get_setting(conn, PAUSED_KEY)?.is_some());
            assert_eq!(recover_failed(conn, now + 60)?, 0, "once per op");
            // The refetched copy is not read as the user moving it back.
            add(conn, 1, 7, "plain", &[], BOT, now - 10);
            assert_eq!(note_moved_back(conn, now + SETTLE_SECS + 60)?, 0);
            let never: i64 =
                conn.query_row("SELECT count(*) FROM fork_filing_never", [], |r| r.get(0))?;
            assert_eq!(never, 0);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn a_pending_move_is_not_read_as_moved_back() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            seed(conn);
            let now = 10_000_000;
            let cache = deals::Cache::default();
            let t = add(conn, 1, 1, "plain", &[], BOT, now - 10);
            score(conn, t, 0.9);
            let filings = decide(conn, &cache, now)?;
            queue_moves(conn, &filings, now)?;
            // A retry poll brought the row back while the op still waits.
            add(conn, 1, 1, "plain", &[], BOT, now - 10);
            assert_eq!(note_moved_back(conn, now + SETTLE_SECS + 1)?, 0);
            conn.execute("DELETE FROM pending_ops", [])?;
            assert_eq!(note_moved_back(conn, now + SETTLE_SECS + 1)?, 1);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn question_names_the_folder_and_its_description() {
        let r = &parse_rules("Deal Flow: business-for-sale alerts")[0];
        let a = ask_for(r, Some("They run a holding company."));
        assert!(a.instructions.starts_with("They run a holding company."));
        assert!(a.instructions.contains("\"Deal Flow\""));
        assert_eq!(a.yes, "business-for-sale alerts");
    }
}
