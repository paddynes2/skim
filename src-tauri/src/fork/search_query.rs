//! Search operators (PLAN.md Phase 4): `from:`, `to:`, `cc:`, `subject:`,
//! `is:`, `has:`, `in:`, `before:`, `after:`, `older_than:`, `newer_than:`,
//! quoted values and `-word` negation.
//!
//! Two halves, one owner:
//! - [`parse`] turns a typed query into free text (for FTS, see
//!   `commands::search::build_fts_query`) plus a [`Filters`] value.
//! - [`filter_sql`] turns a [`Filters`] into SQL clauses over `messages m`.
//!   The AI agent's `search_emails` tool (`ai/agent.rs`) and the palette's
//!   `search_messages` both go through it, so there is exactly one place that
//!   knows how a filter becomes SQL.
//!
//! [`fork_search_threads`] is the list-pane command: the same query, grouped
//! by thread, shaped like `list_threads` (4.3).

use crate::db::models::ThreadRow;
use crate::error::Result;
use crate::state::AppState;
use chrono::{DateTime, Local, NaiveDate, TimeZone};
use rusqlite::types::Value as SqlValue;
use serde::Serialize;
use tauri::State;

/// Virtual folder id the message list uses while it shows search results.
pub const SEARCH_FOLDER_ID: i64 = -900;

/// One `key:value` match term. `negated` for `-from:x`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct Term {
    pub value: String,
    pub negated: bool,
}

impl Term {
    pub fn new(value: impl Into<String>, negated: bool) -> Self {
        Self {
            value: value.into(),
            negated,
        }
    }

    /// A positive match term.
    pub fn plain(value: impl Into<String>) -> Self {
        Self::new(value, false)
    }
}

/// Where `in:` points.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum Place {
    /// A folder role (`inbox`, `sent`, `drafts`, `trash`, `junk`, ...).
    Role(String),
    /// A user label, matched by display name (case-insensitive).
    Label(String),
    /// Starred mail, whatever folder it sits in.
    Starred,
    /// Everything, including Trash and Spam (which are out by default).
    Anywhere,
}

/// Every filter a query can carry. Unset = no constraint.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct Filters {
    pub from: Vec<Term>,
    pub to: Vec<Term>,
    pub cc: Vec<Term>,
    pub subject: Vec<Term>,
    /// `is:unread` → `Some(true)`, `is:read` → `Some(false)`.
    pub unread: Option<bool>,
    /// `is:starred` → `Some(true)`, `is:unstarred` → `Some(false)`.
    pub starred: Option<bool>,
    pub has_attachment: Option<bool>,
    pub place: Option<Place>,
    /// Unix seconds, inclusive lower bound on `m.date`.
    pub after: Option<i64>,
    /// Unix seconds, exclusive upper bound on `m.date`.
    pub before: Option<i64>,
    /// `-word` free-text negations: messages whose FTS index matches any of
    /// these are dropped.
    pub not_text: Vec<String>,
}

impl Filters {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// A parsed query: the free text left over, plus the filters.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct ParsedQuery {
    pub text: String,
    pub filters: Filters,
}

// ---- tokenising ------------------------------------------------------------

/// Split on whitespace outside double quotes. Quotes stay in the token so a
/// later stage can tell `from:"Jane Doe"` from `from:Jane Doe`.
fn tokenize(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in input.chars() {
        if c == '"' {
            quoted = !quoted;
            cur.push(c);
        } else if c.is_whitespace() && !quoted {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Strip one pair of surrounding quotes, and any stray quote characters.
fn unquote(v: &str) -> String {
    let v = v.strip_prefix('"').unwrap_or(v);
    let v = v.strip_suffix('"').unwrap_or(v);
    v.replace('"', "")
}

/// `key:value` split at the first colon, when `key` looks like an operator.
fn split_op(token: &str) -> Option<(&str, &str)> {
    let (key, value) = token.split_once(':')?;
    if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    Some((key, value))
}

/// Seconds in `Nd|Nw|Nm|Ny` (months are 30 days, years 365).
fn relative_secs(v: &str) -> Option<i64> {
    let v = v.trim();
    let (num, unit) = v.split_at(v.len().checked_sub(1)?);
    let n: i64 = num.parse().ok()?;
    if n <= 0 {
        return None;
    }
    let day = 86_400;
    let unit = match unit {
        "d" => day,
        "w" => 7 * day,
        "m" => 30 * day,
        "y" => 365 * day,
        _ => return None,
    };
    n.checked_mul(unit)
}

fn local_unix(naive: chrono::NaiveDateTime) -> i64 {
    Local
        .from_local_datetime(&naive)
        .single()
        .map(|dt| dt.timestamp())
        .unwrap_or_else(|| naive.and_utc().timestamp())
}

/// Local-midnight unix seconds for the start of the given day.
pub fn day_start(s: &str) -> Option<i64> {
    let d = NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok()?;
    Some(local_unix(d.and_hms_opt(0, 0, 0)?))
}

/// Local-midnight unix seconds for the start of the day *after* the given day,
/// so a `before` filter includes the whole named day.
pub fn day_end(s: &str) -> Option<i64> {
    let d = NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok()?;
    Some(local_unix(d.succ_opt()?.and_hms_opt(0, 0, 0)?))
}

// ---- parsing ---------------------------------------------------------------

/// Parse a query against the current clock.
pub fn parse(input: &str) -> ParsedQuery {
    parse_at(input, Local::now())
}

/// Parse a query; `now` anchors `older_than:` / `newer_than:`.
pub fn parse_at(input: &str, now: DateTime<Local>) -> ParsedQuery {
    let mut f = Filters::default();
    let mut text: Vec<String> = Vec::new();

    for token in tokenize(input) {
        let (negated, body) = match token.strip_prefix('-') {
            Some(rest) if !rest.is_empty() => (true, rest),
            _ => (false, token.as_str()),
        };

        let Some((key, raw)) = split_op(body) else {
            // Plain word (or a quoted phrase).
            if negated {
                let w = unquote(body);
                if !w.is_empty() {
                    f.not_text.push(w);
                } else {
                    text.push(token.clone());
                }
            } else {
                text.push(token.clone());
            }
            continue;
        };

        let key = key.to_ascii_lowercase();
        let value = unquote(raw);
        if value.is_empty() {
            text.push(token.clone());
            continue;
        }
        let lower = value.to_ascii_lowercase();

        let consumed = match key.as_str() {
            "from" => {
                f.from.push(Term::new(value, negated));
                true
            }
            "to" => {
                f.to.push(Term::new(value, negated));
                true
            }
            "cc" => {
                f.cc.push(Term::new(value, negated));
                true
            }
            "subject" => {
                f.subject.push(Term::new(value, negated));
                true
            }
            // `-is:unread` reads as `is:read`; same for the other pairs.
            "is" => match lower.as_str() {
                "unread" => {
                    f.unread = Some(!negated);
                    true
                }
                "read" => {
                    f.unread = Some(negated);
                    true
                }
                "starred" => {
                    f.starred = Some(!negated);
                    true
                }
                "unstarred" => {
                    f.starred = Some(negated);
                    true
                }
                _ => false,
            },
            "has" => match lower.as_str() {
                "attachment" | "attachments" => {
                    f.has_attachment = Some(!negated);
                    true
                }
                _ => false,
            },
            // `in:` has no negated form; `-in:x` stays free text.
            "in" if !negated => {
                f.place = Some(match lower.as_str() {
                    "inbox" | "sent" | "drafts" | "trash" | "archive" | "important" => {
                        Place::Role(lower.clone())
                    }
                    "spam" | "junk" => Place::Role("junk".into()),
                    "starred" => Place::Starred,
                    "all" | "anywhere" | "any" => Place::Anywhere,
                    _ => Place::Label(value),
                });
                true
            }
            "before" if !negated => match day_end(&value) {
                Some(ts) => {
                    f.before = Some(f.before.map_or(ts, |b| b.min(ts)));
                    true
                }
                None => false,
            },
            "after" if !negated => match day_start(&value) {
                Some(ts) => {
                    f.after = Some(f.after.map_or(ts, |a| a.max(ts)));
                    true
                }
                None => false,
            },
            "older_than" | "older" if !negated => match relative_secs(&lower) {
                Some(secs) => {
                    let ts = now.timestamp() - secs;
                    f.before = Some(f.before.map_or(ts, |b| b.min(ts)));
                    true
                }
                None => false,
            },
            "newer_than" | "newer" if !negated => match relative_secs(&lower) {
                Some(secs) => {
                    let ts = now.timestamp() - secs;
                    f.after = Some(f.after.map_or(ts, |a| a.max(ts)));
                    true
                }
                None => false,
            },
            _ => false,
        };
        if !consumed {
            // Unknown operator, or a known one with a value it cannot take:
            // the user probably meant the words.
            text.push(token.clone());
        }
    }

    ParsedQuery {
        text: text.join(" "),
        filters: f,
    }
}

// ---- SQL -------------------------------------------------------------------

fn like(v: &str) -> String {
    format!("%{v}%")
}

/// SQL for the filters, as ` AND <clause> AND <clause>...` over a query whose
/// messages table is aliased `m` (empty when nothing applies), plus the bound
/// values in `?` order. Positional `?` only: the caller prepends or appends
/// its own parameters around it.
///
/// Trash and Spam are excluded unless the query names a place (`in:`), like
/// every mail client's default search scope.
pub fn filter_sql(f: &Filters) -> (String, Vec<SqlValue>) {
    let mut clauses: Vec<String> = Vec::new();
    let mut params: Vec<SqlValue> = Vec::new();

    if let Some(a) = f.after {
        clauses.push("m.date >= ?".into());
        params.push(SqlValue::Integer(a));
    }
    if let Some(b) = f.before {
        clauses.push("m.date < ?".into());
        params.push(SqlValue::Integer(b));
    }
    for t in &f.from {
        let not = if t.negated { "NOT " } else { "" };
        clauses.push(format!("{not}(m.from_name LIKE ? OR m.from_addr LIKE ?)"));
        params.push(SqlValue::Text(like(&t.value)));
        params.push(SqlValue::Text(like(&t.value)));
    }
    // Recipients are stored as JSON `[{"name":..,"addr":..}]`; a substring
    // LIKE over the whole document matches either the name or the address.
    for t in &f.to {
        let not = if t.negated { "NOT " } else { "" };
        clauses.push(format!("{not}(COALESCE(m.to_addrs,'') LIKE ?)"));
        params.push(SqlValue::Text(like(&t.value)));
    }
    for t in &f.cc {
        let not = if t.negated { "NOT " } else { "" };
        clauses.push(format!("{not}(COALESCE(m.cc_addrs,'') LIKE ?)"));
        params.push(SqlValue::Text(like(&t.value)));
    }
    for t in &f.subject {
        let not = if t.negated { "NOT " } else { "" };
        clauses.push(format!("{not}(COALESCE(m.subject,'') LIKE ?)"));
        params.push(SqlValue::Text(like(&t.value)));
    }
    match &f.place {
        Some(Place::Role(role)) => {
            clauses.push("m.folder_id IN (SELECT id FROM folders WHERE role = ?)".into());
            params.push(SqlValue::Text(role.clone()));
        }
        Some(Place::Label(name)) => {
            clauses.push(
                "m.folder_id IN (SELECT id FROM folders WHERE role IS NULL \
                 AND lower(display_name) = lower(?))"
                    .into(),
            );
            params.push(SqlValue::Text(name.clone()));
        }
        Some(Place::Starred) => clauses.push("m.is_starred = 1".into()),
        Some(Place::Anywhere) => {}
        None => clauses.push(
            "m.folder_id NOT IN (SELECT id FROM folders WHERE role IN ('trash','junk'))".into(),
        ),
    }
    if let Some(on) = f.has_attachment {
        clauses.push(format!("m.has_attachments = {}", i32::from(on)));
    }
    if let Some(unread) = f.unread {
        clauses.push(format!("m.is_read = {}", i32::from(!unread)));
    }
    if let Some(starred) = f.starred {
        clauses.push(format!("m.is_starred = {}", i32::from(starred)));
    }
    for w in &f.not_text {
        clauses
            .push("m.id NOT IN (SELECT rowid FROM messages_fts WHERE messages_fts MATCH ?)".into());
        params.push(SqlValue::Text(format!("\"{}\"*", w.replace('"', ""))));
    }

    if clauses.is_empty() {
        (String::new(), params)
    } else {
        (format!(" AND {}", clauses.join(" AND ")), params)
    }
}

// ---- 4.3: grouped results for the list ---------------------------------------

/// Search results grouped by thread, newest matching message first, shaped
/// like `list_threads`. `account_id` narrows to one mailbox (`None` = all).
#[tauri::command]
pub async fn fork_search_threads(
    state: State<'_, AppState>,
    query: String,
    offset: i64,
    limit: i64,
    account_id: Option<String>,
) -> Result<Vec<ThreadRow>> {
    let parsed = parse(&query);
    state
        .db
        .read("fork_search_threads", move |conn| {
            search_threads(conn, &parsed, offset, limit, account_id.as_deref())
        })
        .await
}

/// The query behind [`fork_search_threads`], split out for tests.
pub fn search_threads(
    conn: &rusqlite::Connection,
    parsed: &ParsedQuery,
    offset: i64,
    limit: i64,
    account_id: Option<&str>,
) -> rusqlite::Result<Vec<ThreadRow>> {
    let fts = crate::commands::search::build_fts_query(&parsed.text);
    if fts.is_none() && parsed.filters.is_empty() {
        return Ok(Vec::new());
    }
    let (filter, filter_params) = filter_sql(&parsed.filters);
    let mut params: Vec<SqlValue> = Vec::new();
    let hit = match &fts {
        Some(q) => {
            params.push(SqlValue::Text(q.clone()));
            format!(
                "SELECT m.id, m.thread_id, m.date FROM messages_fts \
                 JOIN messages m ON m.id = messages_fts.rowid \
                 WHERE messages_fts MATCH ?{filter} AND (? IS NULL OR m.account_id = ?)"
            )
        }
        None => format!(
            "SELECT m.id, m.thread_id, m.date FROM messages m \
             WHERE 1=1{filter} AND (? IS NULL OR m.account_id = ?)"
        ),
    };
    params.extend(filter_params);
    let acct = account_id.map_or(SqlValue::Null, |a| SqlValue::Text(a.to_string()));
    params.push(acct.clone());
    params.push(acct);
    params.push(SqlValue::Integer(limit.clamp(0, 500)));
    params.push(SqlValue::Integer(offset.max(0)));

    // One row per thread, shaped by its newest matching message; unread and
    // attachment state read over the whole thread, as the list does.
    let sql = format!(
        "WITH hit AS ({hit})
         SELECT t.id, m.from_name, m.from_addr, m.subject, m.snippet, m.date,
                (NOT EXISTS (SELECT 1 FROM messages m3
                             WHERE m3.thread_id = t.id AND m3.is_read = 0)),
                t.starred, max(m2.has_attachments), t.message_count, t.account_id
         FROM threads t
         JOIN hit h ON h.thread_id = t.id
         JOIN messages m ON m.id = h.id
         JOIN messages m2 ON m2.thread_id = t.id
         WHERE h.date = (SELECT max(h2.date) FROM hit h2 WHERE h2.thread_id = t.id)
         GROUP BY t.id
         ORDER BY m.date DESC, t.id DESC
         LIMIT ? OFFSET ?"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(params.iter()), |r| {
            let from_name: Option<String> = r.get(1)?;
            let from_addr: Option<String> = r.get(2)?;
            Ok(ThreadRow {
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
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

// ---- tests -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 9, 23, 12, 0, 0).unwrap()
    }

    fn p(input: &str) -> ParsedQuery {
        parse_at(input, now())
    }

    fn t(v: &str) -> Term {
        Term::new(v, false)
    }

    fn not(v: &str) -> Term {
        Term::new(v, true)
    }

    fn f() -> Filters {
        Filters::default()
    }

    /// The 30+ input table: (input, expected text, expected filters).
    #[test]
    fn parses_the_table() {
        let day = 86_400;
        let n = now().timestamp();
        let cases: Vec<(&str, &str, Filters)> = vec![
            ("", "", f()),
            ("   ", "", f()),
            ("launch", "launch", f()),
            ("launch email", "launch email", f()),
            ("\"launch email\"", "\"launch email\"", f()),
            (
                "from:anna",
                "",
                Filters {
                    from: vec![t("anna")],
                    ..f()
                },
            ),
            (
                "from:\"Jane Doe\"",
                "",
                Filters {
                    from: vec![t("Jane Doe")],
                    ..f()
                },
            ),
            (
                "FROM:Anna launch",
                "launch",
                Filters {
                    from: vec![t("Anna")],
                    ..f()
                },
            ),
            (
                "from:anna from:bob",
                "",
                Filters {
                    from: vec![t("anna"), t("bob")],
                    ..f()
                },
            ),
            (
                "to:alex@example.com",
                "",
                Filters {
                    to: vec![t("alex@example.com")],
                    ..f()
                },
            ),
            (
                "cc:\"Priya Nair\" review",
                "review",
                Filters {
                    cc: vec![t("Priya Nair")],
                    ..f()
                },
            ),
            (
                "subject:redline",
                "",
                Filters {
                    subject: vec![t("redline")],
                    ..f()
                },
            ),
            (
                "subject:\"contract redline\" v3",
                "v3",
                Filters {
                    subject: vec![t("contract redline")],
                    ..f()
                },
            ),
            (
                "is:unread",
                "",
                Filters {
                    unread: Some(true),
                    ..f()
                },
            ),
            (
                "is:read",
                "",
                Filters {
                    unread: Some(false),
                    ..f()
                },
            ),
            (
                "is:starred",
                "",
                Filters {
                    starred: Some(true),
                    ..f()
                },
            ),
            (
                "is:unstarred",
                "",
                Filters {
                    starred: Some(false),
                    ..f()
                },
            ),
            (
                "-is:unread",
                "",
                Filters {
                    unread: Some(false),
                    ..f()
                },
            ),
            ("is:important", "is:important", f()),
            (
                "has:attachment",
                "",
                Filters {
                    has_attachment: Some(true),
                    ..f()
                },
            ),
            (
                "-has:attachment",
                "",
                Filters {
                    has_attachment: Some(false),
                    ..f()
                },
            ),
            ("has:link", "has:link", f()),
            (
                "in:inbox",
                "",
                Filters {
                    place: Some(Place::Role("inbox".into())),
                    ..f()
                },
            ),
            (
                "in:sent",
                "",
                Filters {
                    place: Some(Place::Role("sent".into())),
                    ..f()
                },
            ),
            (
                "in:drafts",
                "",
                Filters {
                    place: Some(Place::Role("drafts".into())),
                    ..f()
                },
            ),
            (
                "in:trash",
                "",
                Filters {
                    place: Some(Place::Role("trash".into())),
                    ..f()
                },
            ),
            (
                "in:spam",
                "",
                Filters {
                    place: Some(Place::Role("junk".into())),
                    ..f()
                },
            ),
            (
                "in:starred",
                "",
                Filters {
                    place: Some(Place::Starred),
                    ..f()
                },
            ),
            (
                "in:anywhere",
                "",
                Filters {
                    place: Some(Place::Anywhere),
                    ..f()
                },
            ),
            (
                "in:Clients",
                "",
                Filters {
                    place: Some(Place::Label("Clients".into())),
                    ..f()
                },
            ),
            (
                "in:\"Client Work\"",
                "",
                Filters {
                    place: Some(Place::Label("Client Work".into())),
                    ..f()
                },
            ),
            (
                "before:2026-09-01",
                "",
                Filters {
                    before: day_end("2026-09-01"),
                    ..f()
                },
            ),
            (
                "after:2026-09-01",
                "",
                Filters {
                    after: day_start("2026-09-01"),
                    ..f()
                },
            ),
            (
                "after:2026-09-01 before:2026-09-30",
                "",
                Filters {
                    after: day_start("2026-09-01"),
                    before: day_end("2026-09-30"),
                    ..f()
                },
            ),
            ("before:yesterday", "before:yesterday", f()),
            ("after:2026-13-40", "after:2026-13-40", f()),
            (
                "older_than:7d",
                "",
                Filters {
                    before: Some(n - 7 * day),
                    ..f()
                },
            ),
            (
                "older_than:2w",
                "",
                Filters {
                    before: Some(n - 14 * day),
                    ..f()
                },
            ),
            (
                "newer_than:1m",
                "",
                Filters {
                    after: Some(n - 30 * day),
                    ..f()
                },
            ),
            (
                "newer_than:1y",
                "",
                Filters {
                    after: Some(n - 365 * day),
                    ..f()
                },
            ),
            ("older_than:7", "older_than:7", f()),
            ("older_than:0d", "older_than:0d", f()),
            ("newer_than:xd", "newer_than:xd", f()),
            // The tighter of two bounds wins.
            (
                "newer_than:1w after:2020-01-01",
                "",
                Filters {
                    after: Some(n - 7 * day),
                    ..f()
                },
            ),
            (
                "-spam",
                "",
                Filters {
                    not_text: vec!["spam".into()],
                    ..f()
                },
            ),
            (
                "launch -newsletter -\"out of office\"",
                "launch",
                Filters {
                    not_text: vec!["newsletter".into(), "out of office".into()],
                    ..f()
                },
            ),
            (
                "-from:noreply",
                "",
                Filters {
                    from: vec![not("noreply")],
                    ..f()
                },
            ),
            (
                "-to:list@example.com",
                "",
                Filters {
                    to: vec![not("list@example.com")],
                    ..f()
                },
            ),
            ("-", "-", f()),
            ("foo:bar", "foo:bar", f()),
            ("http://x.y/z", "http://x.y/z", f()),
            ("from:", "from:", f()),
            ("from:\"\"", "from:\"\"", f()),
            ("-in:inbox", "-in:inbox", f()),
            (
                "from:anna to:alex subject:launch is:unread has:attachment in:inbox after:2026-09-01 checklist",
                "checklist",
                Filters {
                    from: vec![t("anna")],
                    to: vec![t("alex")],
                    subject: vec![t("launch")],
                    unread: Some(true),
                    has_attachment: Some(true),
                    place: Some(Place::Role("inbox".into())),
                    after: day_start("2026-09-01"),
                    ..f()
                },
            ),
        ];
        assert!(cases.len() >= 30);
        for (input, text, filters) in cases {
            let got = p(input);
            assert_eq!(got.text, text, "text of {input:?}");
            assert_eq!(got.filters, filters, "filters of {input:?}");
        }
    }

    #[test]
    fn tokenizer_keeps_quoted_phrases_together() {
        assert_eq!(
            tokenize("a \"b c\" d:\"e f\"  g"),
            vec!["a", "\"b c\"", "d:\"e f\"", "g"]
        );
        // An unterminated quote swallows the rest, never panics.
        assert_eq!(tokenize("a \"b c"), vec!["a", "\"b c"]);
    }

    #[test]
    fn empty_filters_only_hide_trash_and_spam() {
        let (sql, params) = filter_sql(&f());
        assert!(sql.contains("role IN ('trash','junk')"));
        assert!(params.is_empty());
        let (sql, _) = filter_sql(&Filters {
            place: Some(Place::Anywhere),
            ..f()
        });
        assert_eq!(sql, "");
    }

    #[test]
    fn sql_params_follow_the_placeholders() {
        let q = p("from:anna to:alex -cc:bob subject:x in:inbox after:2026-09-01 -junk");
        let (sql, params) = filter_sql(&q.filters);
        assert_eq!(sql.matches('?').count(), params.len());
        assert!(sql.starts_with(" AND "));
        assert!(sql.contains("NOT (COALESCE(m.cc_addrs,'') LIKE ?)"));
        assert!(sql.contains("messages_fts MATCH ?"));
        assert!(!sql.contains("'trash','junk'"));
    }

    // ---- against a seeded database ----

    #[allow(clippy::type_complexity)]
    fn seed(conn: &mut rusqlite::Connection) -> rusqlite::Result<()> {
        use crate::db::models::{Address, NewMessage};
        use crate::db::queries::insert_message;
        conn.execute_batch(
            "INSERT INTO accounts (id, email, provider, imap_host, smtp_host, created_at)
               VALUES ('a1','me@x','gmail','i','s',0), ('a2','me@y','custom','i','s',0);
             INSERT INTO folders (id, account_id, imap_name, role, display_name, sort_order)
               VALUES (1,'a1','INBOX','inbox','Inbox',0),
                      (2,'a1','[Gmail]/Trash','trash','Trash',1),
                      (3,'a1','Clients',NULL,'Clients',2),
                      (4,'a2','INBOX','inbox','Inbox',0);",
        )?;
        // (folder, uid, subject, from_name, from_addr, to, cc, date, read, starred, attachment, account)
        let msgs: [(
            i64,
            u32,
            &str,
            &str,
            &str,
            &str,
            &str,
            i64,
            bool,
            bool,
            bool,
            &str,
        ); 6] = [
            (
                1,
                1,
                "Q3 launch checklist",
                "Anna Weber",
                "anna@nw.example",
                "alex@me.example",
                "",
                100,
                false,
                false,
                true,
                "a1",
            ),
            (
                1,
                2,
                "Re: Q3 launch checklist",
                "Alex Me",
                "me@x",
                "anna@nw.example",
                "priya@bw.example",
                200,
                true,
                false,
                false,
                "a1",
            ),
            (
                1,
                3,
                "Contract redline",
                "Marcus Lee",
                "marcus@acme.example",
                "me@x",
                "",
                150,
                false,
                true,
                false,
                "a1",
            ),
            (
                2,
                1,
                "launch spam",
                "Spammer",
                "x@spam.example",
                "me@x",
                "",
                300,
                false,
                false,
                false,
                "a1",
            ),
            (
                3,
                1,
                "Client launch plan",
                "Sofia Ramos",
                "sofia@bw.example",
                "me@x",
                "",
                120,
                true,
                false,
                false,
                "a1",
            ),
            (
                4,
                1,
                "launch on the other mailbox",
                "Other",
                "o@other.example",
                "me@y",
                "",
                400,
                false,
                false,
                false,
                "a2",
            ),
        ];
        let addrs = |s: &str| -> Vec<Address> {
            if s.is_empty() {
                vec![]
            } else {
                vec![Address {
                    name: None,
                    addr: s.to_string(),
                }]
            }
        };
        for (folder, uid, subject, name, addr, to, cc, date, read, starred, att, acct) in msgs {
            insert_message(
                conn,
                &NewMessage {
                    account_id: acct.into(),
                    folder_id: folder,
                    uid,
                    message_id: Some(format!("<{folder}-{uid}@t>")),
                    in_reply_to: if subject.starts_with("Re: ") {
                        Some("<1-1@t>".into())
                    } else {
                        None
                    },
                    subject: Some(subject.into()),
                    from_name: Some(name.into()),
                    from_addr: Some(addr.into()),
                    to_addrs: addrs(to),
                    cc_addrs: addrs(cc),
                    date,
                    snippet: Some(subject.into()),
                    is_read: read,
                    is_starred: starred,
                    has_attachments: att,
                    ..Default::default()
                },
            )?;
        }
        Ok(())
    }

    fn subjects(rows: &[ThreadRow]) -> Vec<String> {
        rows.iter().map(|r| r.subject.clone()).collect()
    }

    fn run(conn: &rusqlite::Connection, q: &str) -> rusqlite::Result<Vec<ThreadRow>> {
        search_threads(conn, &p(q), 0, 50, None)
    }

    #[test]
    fn grouped_search_applies_every_operator() {
        let db = crate::db::Db::open_in_memory().unwrap();
        db.with(|conn| {
            seed(conn)?;
            // Two Q3 messages share a thread: one row, shaped by the newest hit,
            // unread because the thread has an unread message, attachment from
            // the older one.
            let rows = run(conn, "checklist")?;
            assert_eq!(subjects(&rows), vec!["Re: Q3 launch checklist"]);
            assert_eq!(rows[0].message_count, 2);
            assert!(!rows[0].is_read);
            assert!(rows[0].has_attachments);
            // Free text alone hides Trash; in:trash reaches it; in:anywhere both.
            assert_eq!(
                subjects(&run(conn, "launch")?),
                vec![
                    "launch on the other mailbox",
                    "Re: Q3 launch checklist",
                    "Client launch plan"
                ]
            );
            assert_eq!(
                subjects(&run(conn, "launch in:trash")?),
                vec!["launch spam"]
            );
            assert_eq!(run(conn, "launch in:anywhere")?.len(), 4);
            // from: matches name or address; -from: excludes.
            assert_eq!(
                subjects(&run(conn, "from:weber")?),
                vec!["Q3 launch checklist"]
            );
            assert_eq!(
                subjects(&run(conn, "launch -from:anna -from:other")?),
                vec!["Client launch plan"]
            );
            // to: / cc: read the JSON recipient columns.
            assert_eq!(
                subjects(&run(conn, "to:anna@nw")?),
                vec!["Re: Q3 launch checklist"]
            );
            assert_eq!(
                subjects(&run(conn, "cc:priya")?),
                vec!["Re: Q3 launch checklist"]
            );
            // subject:, is:, has:, in:<label>, dates, negated text.
            assert_eq!(
                subjects(&run(conn, "subject:redline")?),
                vec!["Contract redline"]
            );
            assert_eq!(
                subjects(&run(conn, "is:starred")?),
                vec!["Contract redline"]
            );
            assert_eq!(run(conn, "is:unread")?.len(), 3);
            assert_eq!(
                subjects(&run(conn, "has:attachment")?),
                vec!["Q3 launch checklist"]
            );
            assert_eq!(
                subjects(&run(conn, "in:clients")?),
                vec!["Client launch plan"]
            );
            assert_eq!(
                subjects(&run(conn, "launch -checklist -client")?),
                vec!["launch on the other mailbox"]
            );
            // Filters only (no text) still work; an empty query returns nothing.
            assert_eq!(run(conn, "in:inbox")?.len(), 3);
            assert!(run(conn, "")?.is_empty());
            // Account scoping and paging.
            let mine = search_threads(conn, &p("launch"), 0, 50, Some("a1"))?;
            assert_eq!(mine.len(), 2);
            let page2 = search_threads(conn, &p("launch"), 2, 1, None)?;
            assert_eq!(subjects(&page2), vec!["Client launch plan"]);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn date_filters_bound_the_rows() {
        let db = crate::db::Db::open_in_memory().unwrap();
        db.with(|conn| {
            seed(conn)?;
            let parsed = ParsedQuery {
                text: "launch".into(),
                filters: Filters {
                    after: Some(150),
                    before: Some(400),
                    place: Some(Place::Anywhere),
                    ..f()
                },
            };
            let rows = search_threads(conn, &parsed, 0, 50, None)?;
            assert_eq!(
                subjects(&rows),
                vec!["launch spam", "Re: Q3 launch checklist"]
            );
            Ok(())
        })
        .unwrap();
    }
}
