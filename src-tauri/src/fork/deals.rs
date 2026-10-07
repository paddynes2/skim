//! Deals (v1.1.3, D56): one view of the conversations with people at Patrick's
//! live deals. It replaces Snoozed and Follow-ups, which he never used.
//!
//! The list is his, kept by hand in the `fork_deals` setting, one deal per
//! line:
//!
//! ```text
//! Acme: acme.com
//! Jo Bloggs: jo.bloggs@gmail.com
//! ```
//!
//! A domain matches its addresses and any subdomain's; a full address matches
//! only itself. A personal-mail provider (gmail.com, mweb.co.za and the like)
//! and the mailbox's own domain are never taken as a whole domain, or one line
//! would pull in half the mailbox. The list lives in the local settings table
//! only: the fork's repository is public.
//!
//! A thread is a deal conversation when any cached message of it outside Trash
//! and Spam has a matching sender or recipient; the first deal in the list
//! wins. All Mail is not synced, so a thread archived out of the Inbox stays
//! only while a Sent, Important, Starred or label copy of it is cached. Patrick
//! keeps his mail in the Inbox, so that is rare.
//!
//! Working out membership reads every cached message (tens of milliseconds on
//! his 30,000), so [`Cache`] keeps the answer until the list or the cached
//! mail changes (read and starred flags excepted).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection};
use serde::Serialize;
use tauri::State;

use crate::db::models::{Address, ThreadRow};
use crate::db::queries;
use crate::error::Result;
use crate::state::AppState;

pub const SETTING: &str = "fork_deals";

/// Never matched as a whole domain: personal mail lives there.
const PERSONAL: &[&str] = &[
    "gmail.com",
    "googlemail.com",
    "outlook.com",
    "hotmail.com",
    "hotmail.co.uk",
    "live.com",
    "msn.com",
    "yahoo.com",
    "yahoo.co.uk",
    "ymail.com",
    "icloud.com",
    "me.com",
    "mac.com",
    "aol.com",
    "proton.me",
    "protonmail.com",
    "pm.me",
    "gmx.com",
    "gmx.net",
    "mail.com",
    "zoho.com",
    "yandex.com",
    "hey.com",
    "fastmail.com",
    "btinternet.com",
    "sky.com",
    "comcast.net",
    "verizon.net",
    "att.net",
    "bigpond.com",
    "optusnet.com.au",
    "web.de",
    "t-online.de",
    "orange.fr",
    "free.fr",
    // South African ISPs
    "mweb.co.za",
    "iafrica.com",
    "telkomsa.net",
    "vodamail.co.za",
    "webmail.co.za",
    "absamail.co.za",
    "lantic.net",
    "icon.co.za",
    "cybersmart.co.za",
    "afrihost.co.za",
    "xsinet.co.za",
];

pub fn is_personal(domain: &str) -> bool {
    PERSONAL.contains(&domain)
}

/// Second-level labels that registries sell under a country code (`co.za`,
/// `gov.uk`, `com.mx`, `ne.jp`): with a two-letter country after them they are
/// a public suffix, not an organisation.
const GENERIC_SECOND_LEVEL: &[&str] = &[
    "co", "com", "org", "net", "ac", "gov", "edu", "or", "ne", "go", "mil", "ltd", "plc", "sch",
    "nic", "gob", "gv", "nom", "web", "info", "biz",
];

/// `co.za`, `gov.uk`, `com.mx`: a suffix anyone can register under.
pub fn is_two_label_suffix(domain: &str) -> bool {
    match domain.split_once('.') {
        Some((sld, cc)) => {
            !cc.contains('.')
                && cc.len() == 2
                && cc.chars().all(|c| c.is_ascii_alphabetic())
                && GENERIC_SECOND_LEVEL.contains(&sld)
        }
        None => false,
    }
}

/// The organisation's domain for a suggestion: `mail.acme.co.za` -> `acme.co.za`,
/// `eu.mail.acme.com` -> `acme.com`, `hmrc.gov.uk` stays. Only a suggestion; he
/// can edit it.
pub fn base_domain(domain: &str) -> String {
    let labels: Vec<&str> = domain.split('.').filter(|l| !l.is_empty()).collect();
    let keep = if labels.len() >= 3 && is_two_label_suffix(&labels[labels.len() - 2..].join(".")) {
        3
    } else {
        2
    };
    labels[labels.len().saturating_sub(keep)..].join(".")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Deal {
    pub name: String,
    pub domains: Vec<String>,
    pub addresses: Vec<String>,
    /// `.domain` for each domain, built once so matching allocates nothing.
    #[serde(skip)]
    dotted: Vec<String>,
}

/// Whether `domain` is a mailbox's own domain, a subdomain of one, or a parent
/// of one. `own` never holds personal-mail domains (see [`own_domains`]).
fn is_own(domain: &str, own: &HashSet<String>) -> bool {
    own.iter().any(|o| {
        domain == o || o.ends_with(&format!(".{domain}")) || domain.ends_with(&format!(".{o}"))
    })
}

/// An entry the parser could not use, and why. Settings shows these.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ignored {
    pub entry: String,
    /// `personal` (a personal-mail domain), `own` (the mailbox's own domain,
    /// a parent or a subdomain of it), `too_broad` (a bare suffix like `co.za`)
    /// or `not_an_address` (no dot, no @).
    pub why: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Parsed {
    pub deals: Vec<Deal>,
    pub ignored: Vec<Ignored>,
}

/// One entry as typed or pasted, reduced to a bare domain or address:
/// lower-case, the address inside `<...>`, without a scheme, a path, `www.`, a
/// leading `@` or a trailing dot.
fn clean_entry(raw: &str) -> String {
    let mut s = raw.trim().to_ascii_lowercase();
    if let (Some(a), Some(b)) = (s.find('<'), s.rfind('>')) {
        if a < b {
            s = s[a + 1..b].to_string();
        }
    }
    for prefix in ["https://", "http://", "mailto:"] {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest.to_string();
        }
    }
    let s = s.split(['/', '?', '#']).next().unwrap_or_default();
    let s = s.trim_start_matches('@').trim_end_matches('.');
    s.strip_prefix("www.").unwrap_or(s).to_string()
}

/// Characters a domain or an address can hold here.
fn well_formed(e: &str) -> bool {
    !e.is_empty()
        && e.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+' | '@'))
        && e.matches('@').count() <= 1
        && !e.starts_with('.')
        && !e.contains("..")
}

fn split_entries(rest: &str) -> impl Iterator<Item = &str> {
    rest.split([',', ';', ' ', '\t'])
}

/// The `fork_deals` text, in order. Blank lines and lines starting with `#`
/// are skipped; a line without a name is named after its first entry; a deal
/// left with nothing to match is dropped. `own` are the mailboxes' own domains.
pub fn parse(text: &str, own: &HashSet<String>) -> Parsed {
    let mut out = Parsed::default();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (name, rest) = match line.split_once(':') {
            // `mailto:` and `https:` start an entry, not a name.
            Some((n, r))
                if !n.trim().is_empty()
                    && !["mailto", "http", "https"]
                        .contains(&n.trim().to_ascii_lowercase().as_str()) =>
            {
                (n.trim().to_string(), r)
            }
            _ => (String::new(), line),
        };
        let mut deal = Deal {
            name,
            domains: Vec::new(),
            addresses: Vec::new(),
            dotted: Vec::new(),
        };
        for raw in split_entries(rest) {
            if raw.trim().is_empty() {
                continue;
            }
            let e = clean_entry(raw);
            let why = if !well_formed(&e) {
                "not_an_address"
            } else if e.contains('@') {
                if !deal.addresses.contains(&e) {
                    deal.addresses.push(e);
                }
                continue;
            } else if !e.contains('.') {
                "not_an_address"
            } else if is_two_label_suffix(&e) {
                // `co.za` would match every South African company.
                "too_broad"
            } else if is_personal(&e) {
                "personal"
            } else if is_own(&e, own) {
                "own"
            } else {
                if !deal.domains.contains(&e) {
                    deal.domains.push(e);
                }
                continue;
            };
            let entry = if e.is_empty() {
                raw.trim().to_string()
            } else {
                e
            };
            let ignored = Ignored {
                entry,
                why: why.into(),
            };
            // Reported once: Settings lists these, and a repeat says nothing new.
            if !out.ignored.contains(&ignored) {
                out.ignored.push(ignored);
            }
        }
        if deal.domains.is_empty() && deal.addresses.is_empty() {
            continue;
        }
        deal.dotted = deal.domains.iter().map(|d| format!(".{d}")).collect();
        if deal.name.is_empty() {
            deal.name = deal
                .domains
                .first()
                .or(deal.addresses.first())
                .cloned()
                .unwrap_or_default();
        }
        out.deals.push(deal);
    }
    out
}

/// Index of the first deal `addr` belongs to.
pub fn deal_for(deals: &[Deal], addr: &str) -> Option<usize> {
    let addr = addr.trim().to_ascii_lowercase();
    let domain = addr.rsplit_once('@').map(|(_, d)| d)?;
    deals.iter().position(|d| {
        d.addresses.contains(&addr)
            || d.domains
                .iter()
                .zip(&d.dotted)
                .any(|(dom, dotted)| domain == dom || domain.ends_with(dotted.as_str()))
    })
}

fn addrs_of(json: Option<String>) -> Vec<String> {
    json.and_then(|j| serde_json::from_str::<Vec<Address>>(&j).ok())
        .map(|v| v.into_iter().map(|a| a.addr).collect())
        .unwrap_or_default()
}

/// Every configured mailbox address, lower-case.
fn own_addresses(conn: &Connection) -> rusqlite::Result<HashSet<String>> {
    let mut stmt = conn.prepare_cached("SELECT email FROM accounts")?;
    let emails = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(emails
        .iter()
        .map(|e| e.trim().to_ascii_lowercase())
        .collect())
}

/// The domains of the configured mailboxes, without personal-mail ones: a
/// gmail.com mailbox does not make every Gmail correspondent "own".
fn own_domains(conn: &Connection) -> rusqlite::Result<HashSet<String>> {
    Ok(own_addresses(conn)?
        .iter()
        .filter_map(|e| e.rsplit_once('@').map(|(_, d)| d.to_string()))
        .filter(|d| !is_personal(d))
        .collect())
}

/// The setting, parsed against the mailboxes' own domains.
pub fn read_list(conn: &Connection) -> rusqlite::Result<Parsed> {
    let text = queries::get_setting(conn, SETTING)?.unwrap_or_default();
    Ok(parse(&text, &own_domains(conn)?))
}

/// Outside Trash and Spam: where a message counts as part of a conversation.
const LIVE_FOLDER: &str = "(f.role IS NULL OR f.role NOT IN ('trash', 'junk'))";

/// Thread id -> index of its deal.
fn scan(conn: &Connection, deals: &[Deal]) -> rusqlite::Result<HashMap<i64, usize>> {
    let mut map: HashMap<i64, usize> = HashMap::new();
    if deals.is_empty() {
        return Ok(map);
    }
    let mut stmt = conn.prepare(&format!(
        "SELECT m.thread_id, m.from_addr, m.to_addrs, m.cc_addrs
           FROM messages m JOIN folders f ON f.id = m.folder_id
          WHERE m.thread_id IS NOT NULL AND {LIVE_FOLDER}"
    ))?;
    let mut rows = stmt.query([])?;
    while let Some(r) = rows.next()? {
        let tid: i64 = r.get(0)?;
        let from: Option<String> = r.get(1)?;
        let mut people = addrs_of(r.get(2)?);
        people.extend(addrs_of(r.get(3)?));
        people.extend(from);
        if let Some(i) = people.iter().filter_map(|a| deal_for(deals, a)).min() {
            map.entry(tid)
                .and_modify(|cur| *cur = (*cur).min(i))
                .or_insert(i);
        }
    }
    Ok(map)
}

/// The deal of one thread, from that thread's own rows.
fn deal_of_thread(
    conn: &Connection,
    deals: &[Deal],
    thread_id: i64,
) -> rusqlite::Result<Option<usize>> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT m.from_addr, m.to_addrs, m.cc_addrs
           FROM messages m JOIN folders f ON f.id = m.folder_id
          WHERE m.thread_id = ?1 AND {LIVE_FOLDER}"
    ))?;
    let mut rows = stmt.query(params![thread_id])?;
    let mut best: Option<usize> = None;
    while let Some(r) = rows.next()? {
        let from: Option<String> = r.get(0)?;
        let mut people = addrs_of(r.get(1)?);
        people.extend(addrs_of(r.get(2)?));
        people.extend(from);
        if let Some(i) = people.iter().filter_map(|a| deal_for(deals, a)).min() {
            best = Some(best.map_or(i, |b| b.min(i)));
        }
    }
    Ok(best)
}

pub type Membership = Arc<(Vec<Deal>, HashMap<i64, usize>)>;

/// The last membership answer. One per app, on `ForkState`.
///
/// Keyed by the list text, the mailbox addresses and the shape of the message
/// and thread tables: row counts, highest ids, and the sums of every message's
/// thread and folder. Read and starred changes leave the key alone, so the
/// syncs that only move flags never rescan; a message arriving, leaving,
/// moving folder or changing thread moves it. (SQLite's `data_version` was
/// tried first: it moves on every commit, flags included, which made each
/// sync event a full rescan.)
#[derive(Default)]
pub struct Cache {
    slot: Mutex<Option<(CacheKey, Membership)>>,
}

type CacheKey = (String, String, [i64; 6]);

fn shape(conn: &Connection) -> rusqlite::Result<(String, [i64; 6])> {
    conn.query_row(
        "SELECT (SELECT COALESCE(group_concat(email, ','), '') FROM accounts),
                (SELECT count(*) FROM messages), (SELECT COALESCE(max(id), 0) FROM messages),
                (SELECT COALESCE(sum(thread_id), 0) FROM messages),
                (SELECT COALESCE(sum(folder_id), 0) FROM messages),
                (SELECT count(*) FROM threads), (SELECT COALESCE(max(id), 0) FROM threads)",
        [],
        |r| {
            Ok((
                r.get(0)?,
                [
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                ],
            ))
        },
    )
}

impl Cache {
    /// The deals and their threads. The key and the scan are read inside one
    /// transaction, so the key describes exactly the rows that were scanned.
    pub fn membership(&self, conn: &mut Connection) -> rusqlite::Result<Membership> {
        let tx = conn.transaction()?;
        let text = queries::get_setting(&tx, SETTING)?.unwrap_or_default();
        let (accounts, table) = shape(&tx)?;
        let key = (text, accounts, table);
        if let Ok(guard) = self.slot.lock() {
            if let Some((k, m)) = guard.as_ref() {
                if *k == key {
                    return Ok(m.clone());
                }
            }
        }
        let deals = parse(&key.0, &own_domains(&tx)?).deals;
        let map = scan(&tx, &deals)?;
        tx.commit()?;
        let m: Membership = Arc::new((deals, map));
        if let Ok(mut guard) = self.slot.lock() {
            *guard = Some((key, m.clone()));
        }
        Ok(m)
    }
}

/// A row is bold while any copy of a message in it is unread outside Sent,
/// Drafts, Trash and Spam (Gmail keeps one read flag per message, so an
/// Important copy is as good as the Inbox one, and his own sent copies never
/// count).
const ROW_UNREAD: &str = "EXISTS (SELECT 1 FROM messages u JOIN folders uf ON uf.id = u.folder_id
                WHERE u.thread_id = t.id AND u.is_read = 0
                  AND (uf.role IS NULL OR uf.role NOT IN ('sent', 'drafts', 'trash', 'junk')))";

/// The sidebar counts what the Inbox badge counts: conversations with an
/// unread message in the Inbox. Deal mail archived while unread does not hold
/// a number up that nothing in the Inbox can clear.
const INBOX_UNREAD: &str = "EXISTS (SELECT 1 FROM messages u JOIN folders uf ON uf.id = u.folder_id
                WHERE u.thread_id = t.id AND u.is_read = 0 AND uf.role = 'inbox')";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DealRow {
    #[serde(flatten)]
    pub row: ThreadRow,
    pub deal: String,
}

/// Deal conversations, newest first, shaped like the ordinary thread list.
pub fn list(
    conn: &mut Connection,
    cache: &Cache,
    offset: i64,
    limit: i64,
) -> rusqlite::Result<Vec<DealRow>> {
    let m = cache.membership(conn)?;
    let (deals, map) = (&m.0, &m.1);
    if map.is_empty() {
        return Ok(Vec::new());
    }
    let ids = serde_json::to_string(&map.keys().collect::<Vec<_>>()).unwrap_or_default();
    let sql = format!(
        "SELECT t.id,
                m.from_name, m.from_addr, m.subject, m.snippet, t.last_date,
                NOT {ROW_UNREAD},
                t.starred, max(m.has_attachments), t.message_count, t.account_id
           FROM threads t
           JOIN messages m ON m.thread_id = t.id
          WHERE t.id IN (SELECT value FROM json_each(?3))
            AND m.date = (SELECT max(m2.date) FROM messages m2 WHERE m2.thread_id = t.id)
          GROUP BY t.id
          ORDER BY t.last_date DESC, t.id DESC
          LIMIT ?1 OFFSET ?2"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params![limit, offset, ids], |r| {
            let id: i64 = r.get(0)?;
            let from_name: Option<String> = r.get(1)?;
            let from_addr: Option<String> = r.get(2)?;
            Ok(DealRow {
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
                deal: map
                    .get(&id)
                    .and_then(|i| deals.get(*i))
                    .map(|d| d.name.clone())
                    .unwrap_or_default(),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Deal conversations with an unread message in the Inbox, for the sidebar.
pub fn unread_count(conn: &mut Connection, cache: &Cache) -> rusqlite::Result<i64> {
    let m = cache.membership(conn)?;
    if m.1.is_empty() {
        return Ok(0);
    }
    let ids = serde_json::to_string(&m.1.keys().collect::<Vec<_>>()).unwrap_or_default();
    conn.query_row(
        &format!(
            "SELECT count(*) FROM threads t
              WHERE t.id IN (SELECT value FROM json_each(?1)) AND {INBOX_UNREAD}"
        ),
        params![ids],
        |r| r.get(0),
    )
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    /// The deal the thread already belongs to, if any.
    pub deal: Option<String>,
    /// A name for a new deal: the person's name for an address entry, the
    /// domain's first label otherwise.
    pub name: String,
    /// What the line would match: the organisation's domain, or the address
    /// on a personal-mail provider.
    pub entry: String,
}

/// For "Add to Deals" on an open thread: the first person in it who is not on
/// a mailbox's own domain, newest message first, sender before recipients.
pub fn suggest(conn: &Connection, thread_id: i64) -> rusqlite::Result<Option<Suggestion>> {
    let parsed = read_list(conn)?;
    let own = own_domains(conn)?;
    let own_addrs = own_addresses(conn)?;
    let deal = deal_of_thread(conn, &parsed.deals, thread_id)?
        .and_then(|i| parsed.deals.get(i))
        .map(|d| d.name.clone());

    let mut stmt = conn.prepare_cached(
        "SELECT from_name, from_addr, to_addrs, cc_addrs FROM messages
          WHERE thread_id = ?1 ORDER BY date DESC, id DESC",
    )?;
    let mut rows = stmt.query(params![thread_id])?;
    let mut pick: Option<(String, Option<String>)> = None;
    while let Some(r) = rows.next()? {
        let mut people: Vec<(String, Option<String>)> = Vec::new();
        let from: Option<String> = r.get(1)?;
        if let Some(f) = from {
            people.push((f, r.get(0)?));
        }
        for col in [2, 3] {
            let json: Option<String> = r.get(col)?;
            if let Some(list) = json.and_then(|j| serde_json::from_str::<Vec<Address>>(&j).ok()) {
                people.extend(list.into_iter().map(|a| (a.addr, a.name)));
            }
        }
        // Not him: not a mailbox address, not on a mailbox's own domain (the
        // same rule the parser applies, subdomains included).
        pick = people.into_iter().find(|(addr, _)| {
            let addr = addr.trim().to_ascii_lowercase();
            !own_addrs.contains(&addr)
                && addr
                    .rsplit_once('@')
                    .is_some_and(|(_, d)| !d.is_empty() && !is_own(d, &own))
        });
        if pick.is_some() {
            break;
        }
    }
    let Some((addr, person)) = pick else {
        return Ok(None);
    };
    let addr = addr.trim().to_ascii_lowercase();
    let domain = addr
        .rsplit_once('@')
        .map(|(_, d)| d.to_string())
        .unwrap_or_default();
    let (name, entry) = if is_personal(&domain) {
        let person = person
            .filter(|p| !p.trim().is_empty() && !p.contains('@'))
            .unwrap_or_else(|| addr.split('@').next().unwrap_or_default().to_string());
        (person.trim().to_string(), addr)
    } else {
        let domain = base_domain(&domain);
        let label = domain.split('.').next().unwrap_or_default();
        let mut chars = label.chars();
        let name = chars
            .next()
            .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
            .unwrap_or_default();
        (name, domain)
    };
    Ok(Some(Suggestion { deal, name, entry }))
}

/// `text` with `entry` added under `name`: appended to that deal's line when
/// the name exists (any case), else a new line at the end. An entry already on
/// the line is not added twice.
pub fn add_to(text: &str, name: &str, entry: &str) -> String {
    // A `:` in the name would split the line and a leading `#` would comment it
    // out, so both are dropped.
    let name = name
        .replace(':', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let name = name.trim_start_matches('#').trim();
    let entry = clean_entry(entry);
    if name.is_empty() || entry.is_empty() {
        return text.to_string();
    }
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    for line in lines.iter_mut() {
        let Some((n, rest)) = line.split_once(':') else {
            continue;
        };
        if n.trim().eq_ignore_ascii_case(name) {
            let present = split_entries(rest).any(|e| clean_entry(e) == entry);
            if !present {
                let rest = rest.trim();
                *line = if rest.is_empty() {
                    format!("{}: {entry}", n.trim())
                } else {
                    format!("{}: {rest}, {entry}", n.trim())
                };
            }
            return lines.join("\n");
        }
    }
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    lines.push(format!("{name}: {entry}"));
    lines.join("\n")
}

#[tauri::command]
pub async fn fork_deals_list(
    state: State<'_, AppState>,
    offset: i64,
    limit: i64,
) -> Result<Vec<DealRow>> {
    let cache = state.fork.deals.clone();
    state
        .db
        .read("fork_deals_list", move |conn| {
            list(conn, &cache, offset, limit)
        })
        .await
}

/// Every deal conversation, for the list header (the list itself pages).
pub fn total(conn: &mut Connection, cache: &Cache) -> rusqlite::Result<i64> {
    Ok(cache.membership(conn)?.1.len() as i64)
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DealCounts {
    pub unread: i64,
    pub total: i64,
}

#[tauri::command]
pub async fn fork_deals_count(state: State<'_, AppState>) -> Result<DealCounts> {
    let cache = state.fork.deals.clone();
    state
        .db
        .read("fork_deals_count", move |conn| {
            Ok(DealCounts {
                unread: unread_count(conn, &cache)?,
                total: total(conn, &cache)?,
            })
        })
        .await
}

#[tauri::command]
pub async fn fork_deals_suggest(
    state: State<'_, AppState>,
    thread_id: i64,
) -> Result<Option<Suggestion>> {
    state
        .db
        .read("fork_deals_suggest", move |conn| suggest(conn, thread_id))
        .await
}

/// How Settings reads `text`: the deals and the entries it ignored.
#[tauri::command]
pub async fn fork_deals_preview(state: State<'_, AppState>, text: String) -> Result<Parsed> {
    state
        .db
        .read("fork_deals_preview", move |conn| {
            Ok(parse(&text, &own_domains(conn)?))
        })
        .await
}

/// Adds `entry` under `name` and returns the new list text.
#[tauri::command]
pub async fn fork_deals_add(
    state: State<'_, AppState>,
    name: String,
    entry: String,
) -> Result<String> {
    state
        .db
        .call(move |conn| {
            let current = queries::get_setting(conn, SETTING)?.unwrap_or_default();
            let text = add_to(&current, &name, &entry);
            queries::set_setting(conn, SETTING, &text)?;
            Ok(text)
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::NewMessage;
    use crate::db::queries::insert_message;
    use crate::db::Db;

    const LIST: &str = "# live deals\n\
        Acme: acme.co.za, https://www.acme-group.com/\n\
        \n\
        Jo Bloggs: Jo.Bloggs@Gmail.com\n\
        Broad: gmail.com, mine.ai, intranet\n\
        mailto:solo@example.org\n\
        Beta: beta.io";

    fn own() -> HashSet<String> {
        ["mine.ai".to_string()].into()
    }

    #[test]
    fn parse_reads_names_entries_and_reports_what_it_ignored() {
        let p = parse(LIST, &own());
        let names: Vec<_> = p.deals.iter().map(|d| d.name.as_str()).collect();
        // "Broad" has nothing left to match and is dropped.
        assert_eq!(names, ["Acme", "Jo Bloggs", "solo@example.org", "Beta"]);
        assert_eq!(p.deals[0].domains, ["acme.co.za", "acme-group.com"]);
        assert_eq!(p.deals[1].addresses, ["jo.bloggs@gmail.com"]);
        assert!(p.deals[1].domains.is_empty());
        let ignored: Vec<_> = p
            .ignored
            .iter()
            .map(|i| (i.entry.as_str(), i.why.as_str()))
            .collect();
        assert_eq!(
            ignored,
            [
                ("gmail.com", "personal"),
                ("mine.ai", "own"),
                ("intranet", "not_an_address")
            ]
        );
        // A subdomain of the own domain is own too, a South African ISP is
        // personal, a bare country suffix is too broad.
        let p = parse("X: ai, mail.mine.ai, mweb.co.za, co.za", &own());
        assert!(p.deals.is_empty());
        let why: Vec<_> = p.ignored.iter().map(|i| i.why.as_str()).collect();
        assert_eq!(why, ["not_an_address", "own", "personal", "too_broad"]);
    }

    #[test]
    fn matching_takes_subdomains_but_not_lookalikes() {
        let deals = parse(LIST, &own()).deals;
        assert_eq!(deal_for(&deals, "ceo@acme.co.za"), Some(0));
        assert_eq!(deal_for(&deals, "it@mail.acme.co.za"), Some(0));
        assert_eq!(deal_for(&deals, "x@notacme.co.za"), None);
        assert_eq!(deal_for(&deals, "JO.BLOGGS@gmail.com"), Some(1));
        assert_eq!(deal_for(&deals, "someone.else@gmail.com"), None);
        assert_eq!(deal_for(&deals, "no-at-sign"), None);
    }

    #[test]
    fn base_domain_drops_mail_hosts_but_keeps_country_suffixes() {
        assert_eq!(base_domain("mail.acme.co.za"), "acme.co.za");
        assert_eq!(base_domain("acme.co.za"), "acme.co.za");
        assert_eq!(base_domain("eu.mail.acme.com"), "acme.com");
        assert_eq!(base_domain("acme.com"), "acme.com");
        assert_eq!(base_domain("verdantdata.ch"), "verdantdata.ch");
        // Suffixes outside any fixed list: never suggest a whole country.
        assert_eq!(base_domain("hmrc.gov.uk"), "hmrc.gov.uk");
        assert_eq!(base_domain("ventas.acme.com.mx"), "acme.com.mx");
        assert_eq!(base_domain("x.acme.ne.jp"), "acme.ne.jp");
    }

    #[test]
    fn pasted_entries_are_cleaned_or_reported_once() {
        let p = parse(
            "A: https://acme.com/about, <jo@beta.io>, gamma.io., gov.uk, com.mx\nB: gmail.com\nC: gmail.com, d.io, ex@mple@x",
            &own(),
        );
        assert_eq!(p.deals[0].domains, ["acme.com", "gamma.io"]);
        assert_eq!(p.deals[0].addresses, ["jo@beta.io"]);
        let ignored: Vec<_> = p
            .ignored
            .iter()
            .map(|i| (i.entry.as_str(), i.why.as_str()))
            .collect();
        // gmail.com twice is reported once (Settings lists these).
        assert_eq!(
            ignored,
            [
                ("gov.uk", "too_broad"),
                ("com.mx", "too_broad"),
                ("gmail.com", "personal"),
                ("ex@mple@x", "not_an_address")
            ]
        );
    }

    #[test]
    fn a_gmail_mailbox_does_not_make_gmail_people_own() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            conn.execute_batch(
                "INSERT INTO accounts (id, email, provider, imap_host, smtp_host, created_at)
                   VALUES ('a1', 'me@mine.ai', 'gmail', 'imap.gmail.com', 'smtp.gmail.com', 0),
                          ('a2', 'Me.Too@gmail.com', 'gmail', 'imap.gmail.com', 'smtp.gmail.com', 0);",
            )?;
            let own = own_domains(conn)?;
            assert_eq!(own, ["mine.ai".to_string()].into());
            assert!(own_addresses(conn)?.contains("me.too@gmail.com"));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn add_to_appends_to_a_named_line_or_adds_one() {
        let text = "Acme: acme.co.za\nBeta: beta.io\n";
        assert_eq!(
            add_to(text, "acme", "@acme.com"),
            "Acme: acme.co.za, acme.com\nBeta: beta.io"
        );
        assert_eq!(
            add_to(text, "Acme", "ACME.co.za"),
            "Acme: acme.co.za\nBeta: beta.io"
        );
        assert_eq!(
            add_to(text, "Gamma", "gamma.ai"),
            "Acme: acme.co.za\nBeta: beta.io\nGamma: gamma.ai"
        );
        assert_eq!(add_to("", "Jo", "jo@gmail.com"), "Jo: jo@gmail.com");
        assert_eq!(add_to(text, " ", "x.com"), text);
        // A colon or a leading # in a display name cannot break the line.
        assert_eq!(
            add_to("", "Jo: CFO", "jo@gmail.com"),
            "Jo CFO: jo@gmail.com"
        );
        assert_eq!(
            add_to("", "#1 Fan", "fan@gmail.com"),
            "1 Fan: fan@gmail.com"
        );
        assert_eq!(
            parse(&add_to("", "Jo: CFO", "jo@gmail.com"), &own()).deals[0].name,
            "Jo CFO"
        );
    }

    fn folder(conn: &Connection, imap: &str, role: Option<&str>) -> i64 {
        conn.execute(
            "INSERT INTO folders (account_id, imap_name, role, display_name, unread_count, sort_order)
             VALUES ('a1', ?1, ?2, ?1, 0, 0)",
            params![imap, role],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    struct Msg<'a> {
        folder: i64,
        uid: u32,
        id: &'a str,
        reply_to: Option<&'a str>,
        subject: &'a str,
        from: &'a str,
        to: &'a str,
        date: i64,
        read: bool,
    }

    fn put(conn: &mut Connection, m: Msg) {
        insert_message(
            conn,
            &NewMessage {
                account_id: "a1".into(),
                folder_id: m.folder,
                uid: m.uid,
                message_id: Some(m.id.into()),
                in_reply_to: m.reply_to.map(Into::into),
                subject: Some(m.subject.into()),
                from_addr: Some(m.from.into()),
                to_addrs: vec![Address {
                    name: None,
                    addr: m.to.into(),
                }],
                date: m.date,
                is_read: m.read,
                ..Default::default()
            },
        )
        .unwrap();
    }

    /// Inbox: an Acme thread with one unread, a Beta thread already read, a
    /// thread with nobody on the list. Sent: his reply to Acme, unread as Gmail
    /// sometimes leaves it. Important: a Delta mail archived out of the Inbox
    /// while unread. Trash: a Gamma message, which must not qualify.
    fn seed(conn: &mut Connection) -> i64 {
        conn.execute(
            "INSERT INTO accounts (id, email, provider, imap_host, smtp_host, created_at)
             VALUES ('a1', 'me@mine.ai', 'gmail', 'imap.gmail.com', 'smtp.gmail.com', 0)",
            [],
        )
        .unwrap();
        let inbox = folder(conn, "INBOX", Some("inbox"));
        let sent = folder(conn, "Sent", Some("sent"));
        let important = folder(conn, "Important", Some("important"));
        let trash = folder(conn, "Trash", Some("trash"));
        #[rustfmt::skip]
        let msgs = [
            Msg { folder: inbox, uid: 1, id: "<a1@x>", reply_to: None, subject: "Acme terms", from: "ceo@mail.acme.co.za", to: "me@mine.ai", date: 100, read: false },
            Msg { folder: sent, uid: 1, id: "<a2@x>", reply_to: Some("<a1@x>"), subject: "Re: Acme terms", from: "me@mine.ai", to: "ceo@mail.acme.co.za", date: 200, read: false },
            Msg { folder: inbox, uid: 2, id: "<b1@x>", reply_to: None, subject: "Beta kickoff", from: "me@mine.ai", to: "cto@beta.io", date: 300, read: true },
            Msg { folder: inbox, uid: 3, id: "<n1@x>", reply_to: None, subject: "Newsletter", from: "news@elsewhere.com", to: "me@mine.ai", date: 400, read: false },
            Msg { folder: trash, uid: 1, id: "<g1@x>", reply_to: None, subject: "Gamma", from: "x@gamma.ai", to: "me@mine.ai", date: 500, read: false },
            Msg { folder: important, uid: 1, id: "<d1@x>", reply_to: None, subject: "Delta memo", from: "cfo@delta.com", to: "me@mine.ai", date: 50, read: false },
        ];
        for m in msgs {
            put(conn, m);
        }
        queries::set_setting(
            conn,
            SETTING,
            "Acme: acme.co.za\nBeta: beta.io\nGamma: gamma.ai\nDelta: delta.com",
        )
        .unwrap();
        inbox
    }

    #[test]
    fn list_and_count_follow_the_setting_and_the_inbox() {
        let db = Db::open_in_memory().unwrap();
        let cache = Cache::default();
        db.with(|conn| {
            seed(conn);
            let rows = list(conn, &cache, 0, 50)?;
            let got: Vec<_> = rows
                .iter()
                .map(|r| (r.deal.as_str(), r.row.subject.as_str(), r.row.is_read))
                .collect();
            assert_eq!(
                got,
                [
                    ("Beta", "Beta kickoff", true),
                    ("Acme", "Re: Acme terms", false),
                    ("Delta", "Delta memo", false)
                ]
            );
            // Delta is bold in the list but not counted: it is not in the Inbox.
            assert_eq!(unread_count(conn, &cache)?, 1);
            // The header total is every deal conversation, not one page.
            assert_eq!(total(conn, &cache)?, 3);
            assert_eq!(list(conn, &cache, 0, 2)?.len(), 2);

            // Reading the Acme mail leaves only his unread Sent copy: not unread.
            conn.execute(
                "UPDATE messages SET is_read = 1 WHERE uid = 1 AND from_addr != 'me@mine.ai'",
                [],
            )?;
            assert_eq!(unread_count(conn, &cache)?, 0);

            // Editing the list is seen at once (the text is in the cache key).
            queries::set_setting(conn, SETTING, "Beta: beta.io")?;
            let rows = list(conn, &cache, 0, 50)?;
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].deal, "Beta");

            // So is new mail on this connection (the table shape is too).
            let inbox: i64 =
                conn.query_row("SELECT id FROM folders WHERE role = 'inbox'", [], |r| {
                    r.get(0)
                })?;
            put(
                conn,
                Msg {
                    folder: inbox,
                    uid: 9,
                    id: "<b2@x>",
                    reply_to: None,
                    subject: "Beta pricing",
                    from: "cfo@beta.io",
                    to: "me@mine.ai",
                    date: 600,
                    read: false,
                },
            );
            assert_eq!(list(conn, &cache, 0, 50)?.len(), 2);
            assert_eq!(unread_count(conn, &cache)?, 1);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn suggest_picks_the_outside_person_and_knows_the_deal() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            seed(conn);
            let tid = |conn: &Connection, subject: &str| -> i64 {
                conn.query_row(
                    "SELECT thread_id FROM messages WHERE subject = ?1",
                    params![subject],
                    |r| r.get(0),
                )
                .unwrap()
            };
            let acme = suggest(conn, tid(conn, "Acme terms"))?.unwrap();
            assert_eq!(acme.deal.as_deref(), Some("Acme"));
            assert_eq!(
                (acme.name.as_str(), acme.entry.as_str()),
                ("Acme", "acme.co.za")
            );
            let news = suggest(conn, tid(conn, "Newsletter"))?.unwrap();
            assert_eq!(news.deal, None);
            assert_eq!(
                (news.name.as_str(), news.entry.as_str()),
                ("Elsewhere", "elsewhere.com")
            );
            // The Gamma thread lives only in Trash: no deal.
            assert_eq!(suggest(conn, tid(conn, "Gamma"))?.unwrap().deal, None);
            Ok(())
        })
        .unwrap();
    }
}
