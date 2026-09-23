//! Phase 10: "Ball in my court". Which threads wait on Patrick (`on_me`) and
//! which wait on someone else (`waiting`), kept in `fork_court`
//! (`migrations/f0005_court.sql`).
//!
//! Two passes. The deterministic one ([`pass`]) reads the thread's last
//! message the way `db::bodies::get_thread` does (by date, one copy per
//! Message-ID) and decides from who sent it, whether it is bulk, where the
//! thread lives and whether a local draft is open on it. It runs over the
//! touched threads on every `mail:updated` ([`on_mail_updated`]) and over
//! everything once at startup ([`full_pass`]). A thread whose last message did
//! not change keeps its row untouched, so the AI verdict on it survives.
//!
//! The AI pass ([`ai_pass`]) is optional (`fork_court_ai`, BYOK, off by
//! default) and only ever looks at `on_me` rows the deterministic pass has
//! just (re)written, twenty threads per request, capped per day
//! (`fork_court_ai_cap`). It reads mail and writes a verdict; it never sends
//! anything.

use crate::db::models::ThreadRow;
use crate::db::{queries, Db};
use crate::error::Result;
use crate::state::AppState;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use tauri::{AppHandle, Emitter, State};

/// Emitted after every pass that wrote at least one row (and after a forced
/// recompute regardless), so the sidebar counts and the two views refresh.
pub const EVT_UPDATED: &str = "court:updated";

/// Virtual folder ids the views use (plan: `-920` On me, `-921` Waiting).
pub const VF_ON_ME: i64 = -920;
pub const VF_WAITING: i64 = -921;

pub const STATE_ON_ME: &str = "on_me";
pub const STATE_WAITING: &str = "waiting";
pub const STATE_NONE: &str = "none";

/// Reason strings (stored, shown by the row; `t()` keys are the UI's job).
pub const REASON_DRAFT: &str = "draft started";
pub const REASON_UNSUBSCRIBE: &str = "bulk: list-unsubscribe";
pub const REASON_SENDER: &str = "bulk: automated sender";
pub const REASON_FILED: &str = "filed";

/// Threads per AI request.
pub const AI_BATCH: usize = 20;
/// `fork_court_ai_cap` default: threads the AI pass may judge per local day.
pub const AI_CAP_DEFAULT: i64 = 200;
/// Internal counter row (`settings` table, not in the frontend's ALLOWED list):
/// `"YYYY-MM-DD:N"`, threads judged so far today.
const AI_USED_KEY: &str = "fork_court_ai_used";
/// Body text handed to the model per thread, in characters.
const AI_BODY_CHARS: usize = 1200;

// ---- deterministic pass -----------------------------------------------------

/// One message as the pass needs it.
#[derive(Debug, Clone)]
struct Msg {
    id: i64,
    thread_id: i64,
    account_id: String,
    /// Normalized Message-ID (no brackets), the dedup key.
    message_id: Option<String>,
    from_addr: String,
    date: i64,
    folder_role: Option<String>,
    list_unsubscribe: bool,
}

/// What the deterministic pass decided for one thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    pub state: &'static str,
    pub since: i64,
    pub last_message_id: i64,
    pub reason: Option<&'static str>,
    /// `Some(1)` when the rule itself says a reply is due (a draft is open),
    /// `None` when the AI pass may still have a say.
    pub needs_reply: Option<i64>,
}

/// Which threads a pass looks at.
#[derive(Debug, Clone)]
pub enum Scope {
    All,
    /// Threads with at least one message in one of these folders.
    Folders(Vec<i64>),
    Threads(Vec<i64>),
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PassStats {
    pub examined: usize,
    pub written: usize,
    pub deleted: usize,
}

/// The sender looks automated: `no-?reply|notifications?|mailer-daemon|bounce`
/// anywhere in the address (case-insensitive). Written by hand because the
/// crate has no regex dependency and the plan's pattern is four literals.
pub fn is_automated_sender(addr: &str) -> bool {
    let a = addr.to_ascii_lowercase();
    a.contains("no-reply")
        || a.contains("noreply")
        || a.contains("notification")
        || a.contains("mailer-daemon")
        || a.contains("bounce")
}

/// Every address the user sends from: `accounts.email` plus `imap_user` when
/// it is an address. Lowercased. All accounts together: mail from one of his
/// own mailboxes to another is still "from me".
pub fn own_addresses(conn: &Connection) -> rusqlite::Result<HashSet<String>> {
    let mut stmt = conn.prepare_cached("SELECT email, imap_user FROM accounts")?;
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
    })?;
    let mut out = HashSet::new();
    for row in rows {
        let (email, user) = row?;
        out.insert(email.trim().to_ascii_lowercase());
        if let Some(u) = user {
            if u.contains('@') {
                out.insert(u.trim().to_ascii_lowercase());
            }
        }
    }
    Ok(out)
}

/// Decide one thread from its messages (oldest first, every folder copy) and
/// whether a local draft is open on it. Pure: the unit tests drive it
/// directly. Returns `None` for a thread with no messages (its row is deleted).
fn classify(msgs: &[Msg], own: &HashSet<String>, has_draft: bool) -> Option<Verdict> {
    // Where the thread lives, over every copy. A draft copy in the Drafts
    // folder is a message in flight, not the last word, so it is left out of
    // the last-message pick below but still counts as a place the thread is.
    let roles: Vec<Option<&str>> = msgs.iter().map(|m| m.folder_role.as_deref()).collect();
    let in_inbox = roles.contains(&Some("inbox"));
    let only_junk_or_trash = roles
        .iter()
        .all(|r| matches!(*r, Some("junk") | Some("trash")));

    // Last message: by date (then id), one copy per Message-ID, the first copy
    // met wins (same walk as `get_thread`), drafts-folder copies skipped.
    let mut seen: HashSet<String> = HashSet::new();
    let mut last: Option<&Msg> = None;
    for m in msgs {
        if m.folder_role.as_deref() == Some("drafts") {
            continue;
        }
        let key = m
            .message_id
            .clone()
            .unwrap_or_else(|| format!("pk:{}", m.id));
        if seen.insert(key) {
            last = Some(m);
        }
    }
    let last = last?;
    let from_me = own.contains(&last.from_addr.trim().to_ascii_lowercase());

    if has_draft {
        return Some(Verdict {
            state: STATE_ON_ME,
            since: last.date,
            last_message_id: last.id,
            reason: Some(REASON_DRAFT),
            needs_reply: Some(1),
        });
    }
    let none = |reason: &'static str| Verdict {
        state: STATE_NONE,
        since: last.date,
        last_message_id: last.id,
        reason: Some(reason),
        needs_reply: None,
    };
    if only_junk_or_trash {
        return Some(none(REASON_FILED));
    }
    if from_me {
        return Some(Verdict {
            state: STATE_WAITING,
            since: last.date,
            last_message_id: last.id,
            reason: None,
            needs_reply: None,
        });
    }
    if last.list_unsubscribe {
        return Some(none(REASON_UNSUBSCRIBE));
    }
    if is_automated_sender(&last.from_addr) {
        return Some(none(REASON_SENDER));
    }
    if !in_inbox {
        // Only in user labels / archive / sent: he filed it (or never had it in
        // the inbox), so it is not waiting on him.
        return Some(none(REASON_FILED));
    }
    Some(Verdict {
        state: STATE_ON_ME,
        since: last.date,
        last_message_id: last.id,
        reason: None,
        needs_reply: None,
    })
}

/// Messages of the threads in scope, oldest first per thread, grouped.
fn load_messages(conn: &Connection, scope: &Scope) -> rusqlite::Result<BTreeMap<i64, Vec<Msg>>> {
    const COLS: &str = "m.id, m.thread_id, m.account_id, m.message_id, m.from_addr, m.date,
                        f.role, m.list_unsubscribe";
    let (sql, ids): (String, Vec<i64>) = match scope {
        Scope::All => (
            format!(
                "SELECT {COLS} FROM messages m JOIN folders f ON f.id = m.folder_id
                 WHERE m.thread_id IS NOT NULL ORDER BY m.thread_id, m.date, m.id"
            ),
            Vec::new(),
        ),
        Scope::Folders(folders) => (
            format!(
                "SELECT {COLS} FROM messages m JOIN folders f ON f.id = m.folder_id
                 WHERE m.thread_id IN (SELECT DISTINCT thread_id FROM messages
                                       WHERE folder_id IN ({}) AND thread_id IS NOT NULL)
                 ORDER BY m.thread_id, m.date, m.id",
                placeholders(folders.len())
            ),
            folders.clone(),
        ),
        Scope::Threads(threads) => (
            format!(
                "SELECT {COLS} FROM messages m JOIN folders f ON f.id = m.folder_id
                 WHERE m.thread_id IN ({}) ORDER BY m.thread_id, m.date, m.id",
                placeholders(threads.len())
            ),
            threads.clone(),
        ),
    };
    let mut out: BTreeMap<i64, Vec<Msg>> = BTreeMap::new();
    if matches!(scope, Scope::Folders(v) | Scope::Threads(v) if v.is_empty()) {
        return Ok(out);
    }
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(ids.iter()), |r| {
        Ok(Msg {
            id: r.get(0)?,
            thread_id: r.get(1)?,
            account_id: r.get(2)?,
            message_id: r.get::<_, Option<String>>(3)?.filter(|s| !s.is_empty()),
            from_addr: r.get::<_, Option<String>>(4)?.unwrap_or_default(),
            date: r.get(5)?,
            folder_role: r.get(6)?,
            list_unsubscribe: r.get::<_, Option<String>>(7)?.is_some(),
        })
    })?;
    for row in rows {
        let m = row?;
        out.entry(m.thread_id).or_default().push(m);
    }
    Ok(out)
}

fn placeholders(n: usize) -> String {
    (0..n).map(|_| "?").collect::<Vec<_>>().join(",")
}

/// Threads that have an unsent local draft: a `drafts` row whose
/// `reply_to_message_id` or `origin_message_id` is one of the thread's
/// messages. Rows in `drafts` are unsent by definition (sending deletes them).
fn threads_with_drafts(conn: &Connection) -> rusqlite::Result<HashSet<i64>> {
    let mut stmt = conn.prepare_cached(
        "SELECT DISTINCT m.thread_id FROM drafts d
         JOIN messages m ON m.id = d.reply_to_message_id OR m.id = d.origin_message_id
         WHERE m.thread_id IS NOT NULL",
    )?;
    let rows = stmt.query_map([], |r| r.get::<_, i64>(0))?;
    rows.collect()
}

/// The stored row, as much of it as the pass compares against.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Stored {
    state: String,
    since: Option<i64>,
    last_message_id: Option<i64>,
    reason: Option<String>,
}

fn load_stored(conn: &Connection, scope: &Scope) -> rusqlite::Result<HashMap<i64, Stored>> {
    let (sql, ids): (String, Vec<i64>) = match scope {
        Scope::All => (
            "SELECT thread_id, state, since, last_message_id, reason FROM fork_court".into(),
            Vec::new(),
        ),
        Scope::Folders(_) => (
            "SELECT thread_id, state, since, last_message_id, reason FROM fork_court".into(),
            Vec::new(),
        ),
        Scope::Threads(threads) => (
            format!(
                "SELECT thread_id, state, since, last_message_id, reason FROM fork_court
                 WHERE thread_id IN ({})",
                placeholders(threads.len())
            ),
            threads.clone(),
        ),
    };
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(ids.iter()), |r| {
        Ok((
            r.get::<_, i64>(0)?,
            Stored {
                state: r.get(1)?,
                since: r.get(2)?,
                last_message_id: r.get(3)?,
                reason: r.get(4)?,
            },
        ))
    })?;
    rows.collect()
}

/// The deterministic pass over `scope`. Writes only rows whose verdict moved;
/// a changed `last_message_id` resets the AI columns, an unchanged one keeps
/// them (unless `forced`, which rewrites every examined row). Threads in scope
/// with no messages left lose their row.
pub fn pass(conn: &mut Connection, scope: &Scope, forced: bool) -> rusqlite::Result<PassStats> {
    let own = own_addresses(conn)?;
    let by_thread = load_messages(conn, scope)?;
    let drafted = threads_with_drafts(conn)?;
    let stored = load_stored(conn, scope)?;
    let now = now_ts();
    let mut stats = PassStats::default();

    let tx = conn.transaction()?;
    {
        let mut upsert_reset = tx.prepare_cached(
            "INSERT INTO fork_court (thread_id, account_id, state, since, last_message_id,
                                     needs_reply, reason, model, computed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8)
             ON CONFLICT(thread_id) DO UPDATE SET
               account_id = excluded.account_id, state = excluded.state,
               since = excluded.since, last_message_id = excluded.last_message_id,
               needs_reply = excluded.needs_reply, reason = excluded.reason,
               model = NULL, computed_at = excluded.computed_at",
        )?;
        let mut update_keep = tx.prepare_cached(
            "UPDATE fork_court SET state = ?2, since = ?3, reason = ?4,
                    needs_reply = coalesce(?5, needs_reply), computed_at = ?6
             WHERE thread_id = ?1",
        )?;
        let mut delete = tx.prepare_cached("DELETE FROM fork_court WHERE thread_id = ?1")?;

        for (thread_id, msgs) in &by_thread {
            stats.examined += 1;
            let Some(v) = classify(msgs, &own, drafted.contains(thread_id)) else {
                continue;
            };
            let account_id = &msgs[0].account_id;
            match stored.get(thread_id) {
                Some(s) if !forced && s.last_message_id == Some(v.last_message_id) => {
                    let same = s.state == v.state
                        && s.since == Some(v.since)
                        && s.reason.as_deref() == v.reason;
                    if same {
                        continue;
                    }
                    update_keep.execute(params![
                        thread_id,
                        v.state,
                        v.since,
                        v.reason,
                        v.needs_reply,
                        now
                    ])?;
                }
                _ => {
                    upsert_reset.execute(params![
                        thread_id,
                        account_id,
                        v.state,
                        v.since,
                        v.last_message_id,
                        v.needs_reply,
                        v.reason,
                        now
                    ])?;
                }
            }
            stats.written += 1;
        }

        // Rows for threads that are gone (or, in a folder scope, threads that
        // were touched and now have no messages: the folder scope cannot name
        // them, so the full pass sweeps them).
        match scope {
            Scope::Threads(threads) => {
                for t in threads {
                    if !by_thread.contains_key(t) && stored.contains_key(t) {
                        stats.deleted += delete.execute(params![t])?;
                    }
                }
            }
            Scope::All => {
                stats.deleted += tx.execute(
                    "DELETE FROM fork_court WHERE thread_id NOT IN (SELECT id FROM threads)",
                    [],
                )?;
            }
            Scope::Folders(_) => {}
        }
    }
    tx.commit()?;
    Ok(stats)
}

fn now_ts() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ---- hooks (wired by the main session) --------------------------------------

/// `mail:updated` hook. An empty `touched_folder_ids` (the event's `{}`
/// payload) means "something changed somewhere": the whole mailbox is
/// re-examined, which is one query and a hash-map walk. Runs off the caller's
/// thread; emits [`EVT_UPDATED`] when a row moved; then the AI pass, if on.
pub fn on_mail_updated(app: AppHandle, db: Db, touched_folder_ids: Vec<i64>) {
    let scope = if touched_folder_ids.is_empty() {
        Scope::All
    } else {
        Scope::Folders(touched_folder_ids)
    };
    tauri::async_runtime::spawn(async move {
        run_pass(&app, &db, scope, false).await;
    });
}

/// Startup hook (`fork::start`): everything, once.
pub fn full_pass(app: AppHandle, db: Db) {
    tauri::async_runtime::spawn(async move {
        run_pass(&app, &db, Scope::All, false).await;
    });
}

async fn run_pass(app: &AppHandle, db: &Db, scope: Scope, forced: bool) {
    let stats = match db.call(move |conn| pass(conn, &scope, forced)).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "court pass failed");
            return;
        }
    };
    if stats.written > 0 || stats.deleted > 0 || forced {
        let _ = app.emit(EVT_UPDATED, serde_json::json!({}));
    }
    if stats.written > 0 {
        match ai_pass(db).await {
            Ok(0) => {}
            Ok(n) => {
                tracing::info!(threads = n, "court AI pass");
                let _ = app.emit(EVT_UPDATED, serde_json::json!({}));
            }
            Err(e) => tracing::warn!(error = %e, "court AI pass failed"),
        }
    }
}

// ---- AI pass ------------------------------------------------------------------

/// One thread as the model sees it.
#[derive(Debug, Serialize)]
struct AiThread {
    thread_id: i64,
    subject: String,
    from: String,
    date: String,
    snippet: String,
    body: String,
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct AiVerdict {
    pub thread_id: i64,
    pub needs_reply: bool,
    #[serde(default)]
    pub reason: Option<String>,
}

/// Strict JSON array of verdicts. Tolerates a code fence or prose around the
/// array (first `[` to last `]`); anything else is `None` and the caller keeps
/// the deterministic result.
pub fn parse_ai_reply(text: &str) -> Option<Vec<AiVerdict>> {
    let start = text.find('[')?;
    let end = text.rfind(']')?;
    if end < start {
        return None;
    }
    serde_json::from_str::<Vec<AiVerdict>>(&text[start..=end]).ok()
}

fn ai_system(now: &str) -> String {
    format!(
        "You triage one person's email. It is {now}. For each thread you are given the \
         last message someone else sent them. Decide whether the person needs to reply: \
         true when the sender is waiting for an answer, a decision, a document or an \
         action from them; false for newsletters, receipts, notifications, FYI mail, \
         automated messages, and messages that close a conversation (\"thanks\", \"noted\"). \
         Answer with ONLY a JSON array, no prose, no code fence, one object per thread: \
         [{{\"thread_id\": <number>, \"needs_reply\": <true|false>, \"reason\": \"<at most 8 words>\"}}]"
    )
}

/// Today's `(day, used)` from the counter row.
fn ai_used_today(conn: &Connection, today: &str) -> rusqlite::Result<i64> {
    let raw = queries::get_setting(conn, AI_USED_KEY)?.unwrap_or_default();
    Ok(match raw.split_once(':') {
        Some((day, n)) if day == today => n.parse().unwrap_or(0),
        _ => 0,
    })
}

fn ai_cap(conn: &Connection) -> rusqlite::Result<i64> {
    Ok(queries::get_setting(conn, "fork_court_ai_cap")?
        .and_then(|v| v.trim().parse::<i64>().ok())
        .unwrap_or(AI_CAP_DEFAULT)
        .max(0))
}

fn ai_enabled(conn: &Connection) -> rusqlite::Result<bool> {
    Ok(queries::get_setting(conn, "fork_court_ai")?.as_deref() == Some("on"))
}

/// `on_me` threads the AI has not judged at their current last message, oldest
/// first, at most `limit`, with what the model gets to read.
fn ai_candidates(conn: &Connection, limit: i64) -> rusqlite::Result<Vec<AiThread>> {
    let mut stmt = conn.prepare_cached(
        "SELECT c.thread_id, m.subject, m.from_name, m.from_addr, m.date, m.snippet, b.body_text
         FROM fork_court c
         JOIN messages m ON m.id = c.last_message_id
         LEFT JOIN message_bodies b ON b.message_id = m.id
         WHERE c.state = 'on_me' AND c.model IS NULL
         ORDER BY c.since DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit], |r| {
        let name: Option<String> = r.get(2)?;
        let addr: Option<String> = r.get(3)?;
        let from = match (name.filter(|n| !n.is_empty()), addr) {
            (Some(n), Some(a)) => format!("{n} <{a}>"),
            (Some(n), None) => n,
            (None, Some(a)) => a,
            (None, None) => String::new(),
        };
        let body: Option<String> = r.get(6)?;
        Ok(AiThread {
            thread_id: r.get(0)?,
            subject: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
            from,
            date: crate::ai::retrieval::format_date(r.get(4)?),
            snippet: r.get::<_, Option<String>>(5)?.unwrap_or_default(),
            body: body
                .map(|b| b.chars().take(AI_BODY_CHARS).collect())
                .unwrap_or_default(),
        })
    })?;
    rows.collect()
}

/// Run the AI pass once: judged threads get `needs_reply` + `reason` + `model`;
/// a batch whose reply does not parse keeps its deterministic verdict but is
/// still marked with the model (it counted against the cap and is not retried).
/// A network / provider error leaves the batch unmarked for the next pass.
/// Returns how many threads were judged. Never called from tests.
pub async fn ai_pass(db: &Db) -> Result<usize> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let t = today.clone();
    let (enabled, cap, used) = db
        .call(move |conn| Ok((ai_enabled(conn)?, ai_cap(conn)?, ai_used_today(conn, &t)?)))
        .await?;
    if !enabled || used >= cap {
        return Ok(0);
    }
    let budget = cap - used;
    let candidates = db.call(move |conn| ai_candidates(conn, budget)).await?;
    if candidates.is_empty() {
        return Ok(0);
    }
    let ctx = crate::commands::ai::ai_context(db).await?;
    let system = ai_system(&ctx.now);
    let mut judged = 0usize;
    for batch in candidates.chunks(AI_BATCH) {
        let user = serde_json::to_string_pretty(batch).unwrap_or_default();
        let text = complete(&ctx, &system, user).await?;
        let verdicts = parse_ai_reply(&text);
        let ids: Vec<i64> = batch.iter().map(|t| t.thread_id).collect();
        let model = ctx.model.clone();
        let day = today.clone();
        let n = ids.len() as i64;
        db.call(move |conn| {
            let tx = conn.transaction()?;
            match &verdicts {
                Some(vs) => {
                    let by_id: HashMap<i64, &AiVerdict> =
                        vs.iter().map(|v| (v.thread_id, v)).collect();
                    for id in &ids {
                        let (needs, reason): (Option<i64>, Option<String>) = match by_id.get(id) {
                            Some(v) => (
                                Some(i64::from(v.needs_reply)),
                                v.reason.clone().filter(|r| !r.trim().is_empty()),
                            ),
                            None => (None, None),
                        };
                        tx.execute(
                            "UPDATE fork_court SET needs_reply = ?2,
                                    reason = coalesce(?3, reason), model = ?4
                             WHERE thread_id = ?1 AND model IS NULL",
                            params![id, needs, reason, model],
                        )?;
                    }
                }
                None => {
                    tracing::warn!("court AI reply did not parse; deterministic result kept");
                    for id in &ids {
                        tx.execute(
                            "UPDATE fork_court SET model = ?2 WHERE thread_id = ?1 AND model IS NULL",
                            params![id, model],
                        )?;
                    }
                }
            }
            let used = ai_used_today(&tx, &day)? + n;
            queries::set_setting(&tx, AI_USED_KEY, &format!("{day}:{used}"))?;
            tx.commit()
        })
        .await?;
        judged += batch.len();
    }
    Ok(judged)
}

/// One-shot completion: the provider's stream, collected into a string.
async fn complete(
    ctx: &crate::commands::ai::AiContext,
    system: &str,
    user: String,
) -> Result<String> {
    let mut out = String::new();
    let mut on_delta = |d: &str| out.push_str(d);
    let mut on_reasoning = || {};
    let messages = vec![crate::ai::ChatMessage {
        role: "user",
        content: user,
    }];
    match &ctx.endpoint {
        None => {
            let request = crate::ai::anthropic::Request {
                model: ctx.model.clone(),
                system: system.to_string(),
                messages,
                media: Vec::new(),
                max_tokens: 2048,
            };
            crate::ai::anthropic::stream(&ctx.key, &request, &mut on_delta, &mut on_reasoning)
                .await?;
        }
        Some(ep) => {
            let request = crate::ai::openai_compat::Request {
                model: ctx.model.clone(),
                system: system.to_string(),
                messages,
                max_tokens: 2048,
            };
            crate::ai::openai_compat::stream(
                ep,
                &ctx.key,
                &request,
                &mut on_delta,
                &mut on_reasoning,
            )
            .await?;
        }
    }
    Ok(out)
}

// ---- commands -----------------------------------------------------------------

/// A list row: exactly `ThreadRow` plus when the ball landed and why.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CourtRow {
    #[serde(flatten)]
    pub row: ThreadRow,
    pub since: i64,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct CourtCounts {
    pub on_me: i64,
    pub waiting: i64,
}

/// Predicate for "shows in the On me view": the rule said on_me and the AI
/// (when it ran) did not say the thread needs no reply.
const VISIBLE_ON_ME: &str = "c.state = 'on_me' AND coalesce(c.needs_reply, 1) = 1";
const VISIBLE_WAITING: &str = "c.state = 'waiting'";

fn visible_pred(state: &str) -> rusqlite::Result<&'static str> {
    match state {
        STATE_ON_ME => Ok(VISIBLE_ON_ME),
        STATE_WAITING => Ok(VISIBLE_WAITING),
        other => Err(rusqlite::Error::InvalidParameterName(format!(
            "court state {other:?}"
        ))),
    }
}

/// Threads in a court state, oldest `since` first, shaped like
/// `queries::list_threads` (the thread's latest message, read = no unread
/// message in the thread) so the row renders in the same list component.
pub fn list(
    conn: &Connection,
    state: &str,
    offset: i64,
    limit: i64,
) -> rusqlite::Result<Vec<CourtRow>> {
    let pred = visible_pred(state)?;
    let sql = format!(
        "SELECT t.id,
                m.from_name, m.from_addr, m.subject, m.snippet, t.last_date,
                (NOT EXISTS (SELECT 1 FROM messages m3
                             WHERE m3.thread_id = t.id AND m3.is_read = 0)),
                t.starred,
                max(m.has_attachments), t.message_count, t.account_id,
                c.since, c.reason
         FROM fork_court c
         JOIN threads t ON t.id = c.thread_id
         JOIN messages m ON m.thread_id = t.id
         WHERE {pred}
           AND m.date = (SELECT max(m2.date) FROM messages m2 WHERE m2.thread_id = t.id)
         GROUP BY t.id
         ORDER BY c.since ASC, t.id ASC
         LIMIT ?1 OFFSET ?2"
    );
    let mut stmt = conn.prepare_cached(&sql)?;
    let rows = stmt
        .query_map(params![limit, offset], |r| {
            let from_name: Option<String> = r.get(1)?;
            let from_addr: Option<String> = r.get(2)?;
            Ok(CourtRow {
                row: ThreadRow {
                    id: r.get(0)?,
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
                since: r.get::<_, Option<i64>>(11)?.unwrap_or(0),
                reason: r.get(12)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn counts(conn: &Connection) -> rusqlite::Result<CourtCounts> {
    let sql = format!(
        "SELECT sum({VISIBLE_ON_ME}), sum({VISIBLE_WAITING})
         FROM fork_court c JOIN threads t ON t.id = c.thread_id"
    );
    conn.query_row(&sql, [], |r| {
        Ok(CourtCounts {
            on_me: r.get::<_, Option<i64>>(0)?.unwrap_or(0),
            waiting: r.get::<_, Option<i64>>(1)?.unwrap_or(0),
        })
    })
}

#[tauri::command]
pub async fn fork_court_list(
    state: State<'_, AppState>,
    court_state: String,
    offset: i64,
    limit: i64,
) -> Result<Vec<CourtRow>> {
    state
        .db
        .read("fork_court_list", move |conn| {
            list(conn, &court_state, offset, limit)
        })
        .await
}

#[tauri::command]
pub async fn fork_court_counts(state: State<'_, AppState>) -> Result<CourtCounts> {
    state
        .db
        .read("fork_court_counts", |conn| counts(conn))
        .await
}

/// Forced full pass: every row rewritten, AI columns reset. Emits
/// [`EVT_UPDATED`] when done (before the AI pass, which emits again if it
/// judges anything).
#[tauri::command]
pub async fn fork_court_recompute(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    run_pass(&app, &state.db, Scope::All, true).await;
    Ok(())
}

// ---- nudge ----------------------------------------------------------------------

/// Whole days between two unix timestamps, never negative.
pub fn age_days(since: i64, now: i64) -> i64 {
    ((now - since).max(0)) / 86_400
}

/// Text for the daily toast: `On you: N · Oldest: <name> (5d)`. `None` when
/// nothing is on him, so no toast is shown. The scheduling is the main
/// session's (`notify.rs`); this only says what.
pub fn nudge_line(conn: &Connection) -> rusqlite::Result<Option<String>> {
    nudge_line_at(conn, now_ts())
}

pub fn nudge_line_at(conn: &Connection, now: i64) -> rusqlite::Result<Option<String>> {
    let n = counts(conn)?.on_me;
    if n == 0 {
        return Ok(None);
    }
    let oldest: Option<(Option<String>, Option<String>, i64)> = conn
        .query_row(
            &format!(
                "SELECT m.from_name, m.from_addr, c.since
                 FROM fork_court c JOIN messages m ON m.id = c.last_message_id
                 WHERE {VISIBLE_ON_ME} ORDER BY c.since ASC LIMIT 1"
            ),
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    Ok(oldest.map(|(name, addr, since)| {
        let who = name
            .filter(|s| !s.trim().is_empty())
            .or(addr)
            .unwrap_or_default();
        format!(
            "On you: {n} \u{b7} Oldest: {who} ({}d)",
            age_days(since, now)
        )
    }))
}

/// Whether the daily nudge is due: `setting_hhmm` (`"HH:MM"`, or `"off"`) has
/// passed today in local time and it has not fired today yet
/// (`last_nudged_at`, unix seconds). `now` is unix seconds.
pub fn should_nudge_now(setting_hhmm: &str, last_nudged_at: Option<i64>, now: i64) -> bool {
    use chrono::TimeZone;
    let to_local = |ts: i64| chrono::Local.timestamp_opt(ts, 0).single();
    let Some(now_local) = to_local(now) else {
        return false;
    };
    should_nudge_local(
        setting_hhmm,
        last_nudged_at.and_then(to_local).map(|t| t.naive_local()),
        now_local.naive_local(),
    )
}

/// The clock-free core of [`should_nudge_now`], in one local time zone.
pub fn should_nudge_local(
    setting_hhmm: &str,
    last_nudged_at: Option<chrono::NaiveDateTime>,
    now: chrono::NaiveDateTime,
) -> bool {
    let Some(at) = parse_hhmm(setting_hhmm) else {
        return false;
    };
    if now.time() < at {
        return false;
    }
    match last_nudged_at {
        Some(last) => last.date() < now.date(),
        None => true,
    }
}

fn parse_hhmm(s: &str) -> Option<chrono::NaiveTime> {
    let s = s.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("off") {
        return None;
    }
    let (h, m) = s.split_once(':')?;
    chrono::NaiveTime::from_hms_opt(h.trim().parse().ok()?, m.trim().parse().ok()?, 0)
}

// ---- tests ------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::{Address, NewMessage};
    use crate::db::{drafts, queries::insert_message, Db};
    use chrono::NaiveDate;

    const ME: &str = "me@example.com";

    /// One account with an inbox, a Sent, a Drafts, a Junk, a Trash and a
    /// user label "Clients". Returns the folder ids in that order.
    fn seed(conn: &Connection) -> [i64; 6] {
        conn.execute_batch(&format!(
            "INSERT INTO accounts (id, email, provider, imap_host, smtp_host, created_at)
               VALUES ('a1','{ME}','custom','imap.x','smtp.x',0);
             INSERT INTO folders (id, account_id, imap_name, role, display_name, sort_order)
               VALUES (1,'a1','INBOX','inbox','Inbox',0),
                      (2,'a1','Sent','sent','Sent',1),
                      (3,'a1','Drafts','drafts','Drafts',2),
                      (4,'a1','Junk','junk','Junk',3),
                      (5,'a1','Trash','trash','Trash',4),
                      (6,'a1','Clients',NULL,'Clients',5);"
        ))
        .unwrap();
        [1, 2, 3, 4, 5, 6]
    }

    struct M<'a> {
        folder: i64,
        uid: u32,
        msgid: &'a str,
        from: &'a str,
        date: i64,
        refs: Vec<&'a str>,
        unsub: bool,
    }

    fn add(conn: &mut Connection, m: M) -> (i64, i64) {
        insert_message(
            conn,
            &NewMessage {
                account_id: "a1".into(),
                folder_id: m.folder,
                uid: m.uid,
                message_id: Some(format!("<{}>", m.msgid)),
                references: m.refs.iter().map(|r| format!("<{r}>")).collect(),
                // Subject per Message-ID family: threading's subject fallback
                // would otherwise glue every no-refs message here into one thread.
                subject: Some(format!(
                    "Re: {}",
                    m.msgid.trim_end_matches(|c: char| c.is_ascii_digit())
                )),
                from_name: Some(if m.from == ME { "Me" } else { "Anna Lee" }.into()),
                from_addr: Some(m.from.into()),
                to_addrs: vec![Address {
                    name: None,
                    addr: ME.into(),
                }],
                date: m.date,
                snippet: Some("hello".into()),
                list_unsubscribe: m.unsub.then(|| "<mailto:u@x>".to_string()),
                ..Default::default()
            },
        )
        .unwrap()
        .expect("inserted")
    }

    fn row(conn: &Connection, thread: i64) -> Option<(String, i64, i64, Option<String>)> {
        conn.query_row(
            "SELECT state, since, last_message_id, reason FROM fork_court WHERE thread_id = ?1",
            params![thread],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()
        .unwrap()
    }

    fn run(conn: &mut Connection) -> PassStats {
        pass(conn, &Scope::All, false).unwrap()
    }

    #[test]
    fn inbound_last_message_in_inbox_is_on_me_since_its_date() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            let [inbox, sent, ..] = seed(conn);
            let (_, t) = add(
                conn,
                M {
                    folder: sent,
                    uid: 1,
                    msgid: "m1",
                    from: ME,
                    date: 100,
                    refs: vec![],
                    unsub: false,
                },
            );
            let (m2, t2) = add(
                conn,
                M {
                    folder: inbox,
                    uid: 1,
                    msgid: "m2",
                    from: "anna@client.com",
                    date: 200,
                    refs: vec!["m1"],
                    unsub: false,
                },
            );
            assert_eq!(t, t2, "replies thread together");
            let stats = run(conn);
            assert_eq!(stats.examined, 1);
            assert_eq!(stats.written, 1);
            let (state, since, last, reason) = row(conn, t).unwrap();
            assert_eq!(state, "on_me");
            assert_eq!(since, 200);
            assert_eq!(last, m2);
            assert_eq!(reason, None);
            assert_eq!(
                counts(conn)?,
                CourtCounts {
                    on_me: 1,
                    waiting: 0
                }
            );
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn own_address_last_is_waiting_case_insensitive_and_imap_user_counts() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            let [inbox, sent, ..] = seed(conn);
            conn.execute("UPDATE accounts SET imap_user = 'alias@example.com'", [])?;
            add(
                conn,
                M {
                    folder: inbox,
                    uid: 1,
                    msgid: "m1",
                    from: "anna@client.com",
                    date: 100,
                    refs: vec![],
                    unsub: false,
                },
            );
            let (_, t) = add(
                conn,
                M {
                    folder: sent,
                    uid: 1,
                    msgid: "m2",
                    from: "Me@Example.COM",
                    date: 200,
                    refs: vec!["m1"],
                    unsub: false,
                },
            );
            run(conn);
            let (state, since, ..) = row(conn, t).unwrap();
            assert_eq!(state, "waiting");
            assert_eq!(since, 200);

            // A second thread answered from the IMAP login alias.
            add(
                conn,
                M {
                    folder: inbox,
                    uid: 2,
                    msgid: "n1",
                    from: "bob@client.com",
                    date: 300,
                    refs: vec![],
                    unsub: false,
                },
            );
            let (_, t2) = add(
                conn,
                M {
                    folder: sent,
                    uid: 2,
                    msgid: "n2",
                    from: "alias@example.com",
                    date: 400,
                    refs: vec!["n1"],
                    unsub: false,
                },
            );
            run(conn);
            assert_eq!(row(conn, t2).unwrap().0, "waiting");
            assert_eq!(
                counts(conn)?,
                CourtCounts {
                    on_me: 0,
                    waiting: 2
                }
            );
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn last_message_is_by_date_and_deduped_by_message_id_across_folders() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            let [inbox, _, _, _, _, label] = seed(conn);
            // Newest by date arrives first (sync walks newest-first).
            let (newest, t) = add(
                conn,
                M {
                    folder: inbox,
                    uid: 1,
                    msgid: "m2",
                    from: "anna@client.com",
                    date: 300,
                    refs: vec!["m1"],
                    unsub: false,
                },
            );
            add(
                conn,
                M {
                    folder: inbox,
                    uid: 2,
                    msgid: "m1",
                    from: ME,
                    date: 100,
                    refs: vec![],
                    unsub: false,
                },
            );
            run(conn);
            let (state, since, last, _) = row(conn, t).unwrap();
            assert_eq!((state.as_str(), since, last), ("on_me", 300, newest));

            // A label copy of the same newest message (Gmail) does not move the
            // verdict or the last_message_id: the row is left alone.
            add(
                conn,
                M {
                    folder: label,
                    uid: 1,
                    msgid: "m2",
                    from: "anna@client.com",
                    date: 300,
                    refs: vec!["m1"],
                    unsub: false,
                },
            );
            conn.execute(
                "UPDATE fork_court SET model = 'x', needs_reply = 0 WHERE thread_id = ?1",
                params![t],
            )?;
            let stats = run(conn);
            assert_eq!(stats.written, 0, "unchanged last message: no rewrite");
            let model: Option<String> = conn.query_row(
                "SELECT model FROM fork_court WHERE thread_id = ?1",
                params![t],
                |r| r.get(0),
            )?;
            assert_eq!(model.as_deref(), Some("x"), "AI verdict survives");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn a_new_message_resets_the_ai_verdict_and_forced_rewrites_everything() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            let [inbox, ..] = seed(conn);
            let (_, t) = add(
                conn,
                M {
                    folder: inbox,
                    uid: 1,
                    msgid: "m1",
                    from: "anna@client.com",
                    date: 100,
                    refs: vec![],
                    unsub: false,
                },
            );
            run(conn);
            conn.execute(
                "UPDATE fork_court SET model = 'x', needs_reply = 0 WHERE thread_id = ?1",
                params![t],
            )?;
            assert_eq!(counts(conn)?.on_me, 0, "AI said no reply: hidden");
            let (m2, _) = add(
                conn,
                M {
                    folder: inbox,
                    uid: 2,
                    msgid: "m2",
                    from: "anna@client.com",
                    date: 200,
                    refs: vec!["m1"],
                    unsub: false,
                },
            );
            run(conn);
            let (model, needs): (Option<String>, Option<i64>) = conn.query_row(
                "SELECT model, needs_reply FROM fork_court WHERE thread_id = ?1",
                params![t],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            assert_eq!((model, needs), (None, None));
            assert_eq!(row(conn, t).unwrap().2, m2);
            assert_eq!(counts(conn)?.on_me, 1);

            conn.execute("UPDATE fork_court SET model = 'y'", [])?;
            let stats = pass(conn, &Scope::All, true).unwrap();
            assert_eq!(stats.written, 1);
            let model: Option<String> =
                conn.query_row("SELECT model FROM fork_court", [], |r| r.get(0))?;
            assert_eq!(model, None, "forced pass resets the AI columns");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn bulk_list_unsubscribe_is_none() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            let [inbox, ..] = seed(conn);
            let (_, t) = add(
                conn,
                M {
                    folder: inbox,
                    uid: 1,
                    msgid: "m1",
                    from: "news@shop.com",
                    date: 100,
                    refs: vec![],
                    unsub: true,
                },
            );
            run(conn);
            let (state, _, _, reason) = row(conn, t).unwrap();
            assert_eq!(state, "none");
            assert_eq!(reason.as_deref(), Some(REASON_UNSUBSCRIBE));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn bulk_automated_sender_is_none_each_pattern() {
        for addr in [
            "no-reply@github.com",
            "NoReply@bank.com",
            "notification@slack.com",
            "notifications@linkedin.com",
            "MAILER-DAEMON@mx.example.com",
            "bounce+abc@lists.example.com",
        ] {
            assert!(is_automated_sender(addr), "{addr}");
        }
        for addr in [
            "anna@client.com",
            "reply-to-me@x.com",
            "bounces.thomas@x.com",
        ] {
            assert_eq!(is_automated_sender(addr), addr.contains("bounce"), "{addr}");
        }
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            let [inbox, ..] = seed(conn);
            let (_, t) = add(
                conn,
                M {
                    folder: inbox,
                    uid: 1,
                    msgid: "m1",
                    from: "no-reply@github.com",
                    date: 100,
                    refs: vec![],
                    unsub: false,
                },
            );
            run(conn);
            let (state, _, _, reason) = row(conn, t).unwrap();
            assert_eq!(state, "none");
            assert_eq!(reason.as_deref(), Some(REASON_SENDER));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn filed_threads_are_none_unless_a_copy_is_in_the_inbox() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            let [inbox, _, _, junk, trash, label] = seed(conn);
            // Only in a user label: filed.
            let (_, t_label) = add(
                conn,
                M {
                    folder: label,
                    uid: 1,
                    msgid: "l1",
                    from: "anna@client.com",
                    date: 100,
                    refs: vec![],
                    unsub: false,
                },
            );
            // In the label AND the inbox (Gmail label copy): on me.
            let (_, t_both) = add(
                conn,
                M {
                    folder: label,
                    uid: 2,
                    msgid: "b1",
                    from: "anna@client.com",
                    date: 100,
                    refs: vec![],
                    unsub: false,
                },
            );
            add(
                conn,
                M {
                    folder: inbox,
                    uid: 1,
                    msgid: "b1",
                    from: "anna@client.com",
                    date: 100,
                    refs: vec![],
                    unsub: false,
                },
            );
            // Junk only, trash only: none, even when he wrote last.
            let (_, t_junk) = add(
                conn,
                M {
                    folder: junk,
                    uid: 1,
                    msgid: "j1",
                    from: "spam@x.com",
                    date: 100,
                    refs: vec![],
                    unsub: false,
                },
            );
            let (_, t_trash) = add(
                conn,
                M {
                    folder: trash,
                    uid: 1,
                    msgid: "t1",
                    from: ME,
                    date: 100,
                    refs: vec![],
                    unsub: false,
                },
            );
            run(conn);
            assert_eq!(row(conn, t_label).unwrap().0, "none");
            assert_eq!(row(conn, t_label).unwrap().3.as_deref(), Some(REASON_FILED));
            assert_eq!(row(conn, t_both).unwrap().0, "on_me");
            assert_eq!(row(conn, t_junk).unwrap().0, "none");
            assert_eq!(row(conn, t_trash).unwrap().0, "none");
            assert_eq!(
                counts(conn)?,
                CourtCounts {
                    on_me: 1,
                    waiting: 0
                }
            );
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn a_local_draft_on_the_thread_marks_on_me_with_draft_started() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            let [inbox, sent, drafts_folder, ..] = seed(conn);
            add(
                conn,
                M {
                    folder: inbox,
                    uid: 1,
                    msgid: "m1",
                    from: "anna@client.com",
                    date: 100,
                    refs: vec![],
                    unsub: false,
                },
            );
            let (m2, t) = add(
                conn,
                M {
                    folder: sent,
                    uid: 1,
                    msgid: "m2",
                    from: ME,
                    date: 200,
                    refs: vec!["m1"],
                    unsub: false,
                },
            );
            run(conn);
            assert_eq!(row(conn, t).unwrap().0, "waiting");

            // A reply draft on the thread flips it, without a new message.
            let d = drafts::create(conn, "a1", "reply", Some(m2), "anna@client.com", "Re", "…")?;
            let stats = run(conn);
            assert_eq!(stats.written, 1);
            let (state, since, last, reason) = row(conn, t).unwrap();
            assert_eq!(state, "on_me");
            assert_eq!(reason.as_deref(), Some(REASON_DRAFT));
            assert_eq!((since, last), (200, m2), "last message unchanged");
            assert_eq!(counts(conn)?.on_me, 1);

            // Draft gone: back to waiting.
            drafts::delete(conn, d.id)?;
            run(conn);
            assert_eq!(row(conn, t).unwrap().0, "waiting");

            // A server draft copy in the Drafts folder is not the last word
            // (it is skipped), and its origin link counts as a draft.
            let (dm, _) = add(
                conn,
                M {
                    folder: drafts_folder,
                    uid: 1,
                    msgid: "d1",
                    from: ME,
                    date: 300,
                    refs: vec!["m1"],
                    unsub: false,
                },
            );
            run(conn);
            assert_eq!(
                row(conn, t).unwrap().2,
                m2,
                "draft copy never the last message"
            );
            drafts::create_server_draft(
                conn,
                "a1",
                "reply",
                None,
                "anna@client.com",
                "",
                "",
                "Re",
                "…",
                dm,
                "d1",
            )?;
            run(conn);
            assert_eq!(row(conn, t).unwrap().3.as_deref(), Some(REASON_DRAFT));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn folder_scope_touches_only_those_threads_and_gone_threads_lose_their_row() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            let [inbox, _, _, _, _, label] = seed(conn);
            let (_, t1) = add(
                conn,
                M {
                    folder: inbox,
                    uid: 1,
                    msgid: "m1",
                    from: "anna@client.com",
                    date: 100,
                    refs: vec![],
                    unsub: false,
                },
            );
            let (_, t2) = add(
                conn,
                M {
                    folder: label,
                    uid: 1,
                    msgid: "x1",
                    from: "bob@client.com",
                    date: 100,
                    refs: vec![],
                    unsub: false,
                },
            );
            let stats = pass(conn, &Scope::Folders(vec![inbox]), false).unwrap();
            assert_eq!((stats.examined, stats.written), (1, 1));
            assert!(row(conn, t1).is_some());
            assert!(row(conn, t2).is_none());
            assert_eq!(
                pass(conn, &Scope::Folders(vec![]), false).unwrap().examined,
                0
            );

            run(conn);
            assert!(row(conn, t2).is_some());
            conn.execute("DELETE FROM messages WHERE thread_id = ?1", params![t1])?;
            conn.execute("DELETE FROM threads WHERE id = ?1", params![t1])?;
            let stats = run(conn);
            assert_eq!(stats.deleted, 1);
            assert!(row(conn, t1).is_none());

            conn.execute("DELETE FROM messages WHERE thread_id = ?1", params![t2])?;
            let stats = pass(conn, &Scope::Threads(vec![t2]), false).unwrap();
            assert_eq!(stats.deleted, 1);
            assert!(row(conn, t2).is_none());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn list_rows_match_the_thread_list_shape_and_sort_oldest_first() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            let [inbox, ..] = seed(conn);
            let (_, newer) = add(
                conn,
                M {
                    folder: inbox,
                    uid: 1,
                    msgid: "m1",
                    from: "anna@client.com",
                    date: 500,
                    refs: vec![],
                    unsub: false,
                },
            );
            let (_, older) = add(
                conn,
                M {
                    folder: inbox,
                    uid: 2,
                    msgid: "o1",
                    from: "bob@client.com",
                    date: 100,
                    refs: vec![],
                    unsub: false,
                },
            );
            add(
                conn,
                M {
                    folder: inbox,
                    uid: 3,
                    msgid: "o2",
                    from: "bob@client.com",
                    date: 150,
                    refs: vec!["o1"],
                    unsub: false,
                },
            );
            run(conn);
            let rows = list(conn, "on_me", 0, 50)?;
            assert_eq!(
                rows.iter().map(|r| r.row.id).collect::<Vec<_>>(),
                vec![older, newer]
            );
            let first = &rows[0];
            assert_eq!(first.since, 150);
            assert_eq!(first.row.date, 150, "shaped by the thread's latest message");
            assert_eq!(first.row.message_count, 2);
            assert_eq!(first.row.from_name, "Anna Lee");
            assert!(!first.row.is_read);
            assert_eq!(first.reason, None);
            assert!(list(conn, "waiting", 0, 50)?.is_empty());
            assert!(list(conn, "bogus", 0, 50).is_err());
            assert_eq!(list(conn, "on_me", 1, 50)?.len(), 1);

            // Serialized shape: ThreadRow fields flattened, camelCase extras.
            let json = serde_json::to_value(first).unwrap();
            assert_eq!(json["id"], older);
            assert_eq!(json["fromAddr"], "bob@client.com");
            assert_eq!(json["since"], 150);
            assert!(json["reason"].is_null());
            assert!(json.get("row").is_none());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn nudge_line_names_the_oldest_and_its_age() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            let [inbox, ..] = seed(conn);
            assert_eq!(nudge_line_at(conn, 0)?, None);
            add(
                conn,
                M {
                    folder: inbox,
                    uid: 1,
                    msgid: "m1",
                    from: "anna@client.com",
                    date: 86_400 * 10,
                    refs: vec![],
                    unsub: false,
                },
            );
            add(
                conn,
                M {
                    folder: inbox,
                    uid: 2,
                    msgid: "p1",
                    from: "bob@client.com",
                    date: 86_400 * 14,
                    refs: vec![],
                    unsub: false,
                },
            );
            run(conn);
            let line = nudge_line_at(conn, 86_400 * 15 + 3600)?.unwrap();
            assert_eq!(line, "On you: 2 \u{b7} Oldest: Anna Lee (5d)");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn ai_reply_parses_strictly_and_falls_back() {
        let ok = parse_ai_reply(
            r#"[{"thread_id": 7, "needs_reply": true, "reason": "asks for the deck"},
                {"thread_id": 9, "needs_reply": false}]"#,
        )
        .unwrap();
        assert_eq!(ok.len(), 2);
        assert_eq!(ok[0].thread_id, 7);
        assert!(ok[0].needs_reply);
        assert_eq!(ok[0].reason.as_deref(), Some("asks for the deck"));
        assert_eq!(ok[1].reason, None);

        let fenced =
            parse_ai_reply("Sure:\n```json\n[{\"thread_id\":1,\"needs_reply\":false}]\n```")
                .unwrap();
        assert_eq!(fenced[0].thread_id, 1);

        assert!(parse_ai_reply("I cannot decide").is_none());
        assert!(parse_ai_reply("[{\"thread_id\": \"seven\", \"needs_reply\": true}]").is_none());
        assert!(parse_ai_reply("[{\"needs_reply\": true}]").is_none());
        assert!(parse_ai_reply("] oops [").is_none());
        assert!(parse_ai_reply("").is_none());
    }

    #[test]
    fn ai_counter_and_cap_read_settings() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            assert_eq!(ai_cap(conn)?, AI_CAP_DEFAULT);
            assert!(!ai_enabled(conn)?);
            queries::set_setting(conn, "fork_court_ai_cap", "50")?;
            queries::set_setting(conn, "fork_court_ai", "on")?;
            assert_eq!(ai_cap(conn)?, 50);
            assert!(ai_enabled(conn)?);
            assert_eq!(ai_used_today(conn, "2026-09-23")?, 0);
            queries::set_setting(conn, AI_USED_KEY, "2026-09-23:41")?;
            assert_eq!(ai_used_today(conn, "2026-09-23")?, 41);
            assert_eq!(ai_used_today(conn, "2026-09-24")?, 0, "a new day resets");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn ai_candidates_are_on_me_rows_without_a_model() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            let [inbox, sent, ..] = seed(conn);
            let (_, t1) = add(
                conn,
                M {
                    folder: inbox,
                    uid: 1,
                    msgid: "m1",
                    from: "anna@client.com",
                    date: 100,
                    refs: vec![],
                    unsub: false,
                },
            );
            add(
                conn,
                M {
                    folder: sent,
                    uid: 1,
                    msgid: "w1",
                    from: ME,
                    date: 100,
                    refs: vec![],
                    unsub: false,
                },
            );
            run(conn);
            let c = ai_candidates(conn, 10)?;
            assert_eq!(c.len(), 1);
            assert_eq!(c[0].thread_id, t1);
            assert_eq!(c[0].from, "Anna Lee <anna@client.com>");
            conn.execute("UPDATE fork_court SET model = 'm'", [])?;
            assert!(ai_candidates(conn, 10)?.is_empty());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn should_nudge_local_fires_once_a_day_after_the_time() {
        let d = |y, m, d, h, mi| {
            NaiveDate::from_ymd_opt(y, m, d)
                .unwrap()
                .and_hms_opt(h, mi, 0)
                .unwrap()
        };
        let now = d(2026, 9, 23, 9, 5);
        assert!(should_nudge_local("09:00", None, now));
        assert!(!should_nudge_local("09:00", None, d(2026, 9, 23, 8, 59)));
        assert!(should_nudge_local("09:00", None, d(2026, 9, 23, 9, 0)));
        assert!(!should_nudge_local(
            "09:00",
            Some(d(2026, 9, 23, 9, 1)),
            now
        ));
        assert!(should_nudge_local("09:00", Some(d(2026, 9, 22, 9, 1)), now));
        assert!(should_nudge_local(
            "09:00",
            Some(d(2026, 9, 22, 23, 59)),
            now
        ));
        assert!(!should_nudge_local("off", None, now));
        assert!(!should_nudge_local("", None, now));
        assert!(!should_nudge_local("9am", None, now));
        assert!(!should_nudge_local("25:00", None, now));
        assert!(should_nudge_local(" 9:00 ", None, now));
        // The unix-seconds wrapper agrees with the local core for "never yet".
        assert!(
            should_nudge_now("00:00", None, 1_800_000_000),
            "midnight has always passed"
        );
        assert!(!should_nudge_now("off", None, 1_800_000_000));
    }

    #[test]
    fn age_days_floors_and_never_goes_negative() {
        assert_eq!(age_days(0, 86_399), 0);
        assert_eq!(age_days(0, 86_400), 1);
        assert_eq!(age_days(100, 0), 0);
    }
}
