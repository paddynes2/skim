//! Heat (v1.1.15, D67): every list row is tinted by how much its conversation
//! matters, from cool periwinkle to mint (`src/fork/heat/colour.ts`).
//!
//! A conversation's heat is one number in 0..=1, worked out here from what
//! `fork::priority` already stores; nothing new leaves the machine:
//! - a Deals conversation, or a sender on the always list: 1;
//! - otherwise the highest Jev score among its messages;
//! - otherwise 0 when one of its messages is mail Jev is never asked about
//!   (bulk, calendar, sign-in codes, unauthenticated, the exclude list);
//! - otherwise no heat yet (not scored, or older than the scoring window):
//!   the row stays untinted.
//!
//! v1.1.17 (D68): a conversation the user has replied to is at least
//! [`REPLIED_FLOOR`] (light seafoam), and a sender on the exclude list (the
//! `-` key) is cold whatever Jev said. The user's own messages carry no score.

use crate::error::Result;
use crate::fork::priority::{authenticated, excluded, is_calendar, is_sensitive, lists};
use crate::state::AppState;
use rusqlite::{params_from_iter, Connection};
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};
use tauri::State;

/// Emitted after new scores are stored, so shown rows re-read their heat.
pub const EVT_HEAT: &str = "heat:updated";

/// At most this many conversations per call: the shown rows and some.
const MAX_IDS: usize = 400;

/// The least heat of a conversation the user replied to: on the ramp, between
/// clear and seafoam, and under the Hot filter's 0.5.
pub const REPLIED_FLOOR: f64 = 0.45;

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Heat {
    pub thread_id: i64,
    /// `None`: a person wrote but there is no heat yet.
    pub heat: Option<f64>,
    /// Pastel (D92): a person, not a machine, wrote in this conversation.
    pub person: bool,
}

/// One message as the heat rule needs it.
#[derive(Debug, Clone, Default)]
pub struct Msg {
    pub thread_id: i64,
    pub from_name: String,
    pub from_addr: String,
    pub subject: String,
    pub preview: String,
    pub bulk: bool,
    pub spf: String,
    pub dkim: String,
    pub dmarc: String,
    /// `Some(None)`: asked, no usable answer. `None`: never asked.
    pub score: Option<Option<f64>>,
}

/// Mail Jev is never asked about, by the same rules `priority::candidates`
/// applies (own mail is dropped before this).
pub fn cold_by_rule(m: &Msg, exclude: &[String]) -> bool {
    m.bulk
        || excluded(&m.from_addr, exclude)
        || is_calendar(&m.subject, &m.from_addr)
        || is_sensitive(&m.subject, &m.preview)
        || !authenticated(&m.spf, &m.dkim, &m.dmarc)
}

/// Pastel (D92): local parts that mean a role or a machine, not a person.
const ROLE_LOCALS: &[&str] = &[
    "noreply",
    "no-reply",
    "no_reply",
    "donotreply",
    "do-not-reply",
    "notifications",
    "notification",
    "notify",
    "alerts",
    "alert",
    "mailer",
    "mailer-daemon",
    "postmaster",
    "bounce",
    "bounces",
    "support",
    "help",
    "team",
    "news",
    "newsletter",
    "newsletters",
    "marketing",
    "updates",
    "update",
    "billing",
    "invoices",
    "invoice",
    "accounts",
    "receipts",
    "security",
    "admin",
    "system",
    "automated",
    "digest",
    "reply",
    "replies",
    "feedback",
    "community",
    "events",
    "hello",
    "info",
    "sales",
    "service",
    "services",
    "welcome",
    "orders",
    "order",
    "shop",
    "store",
    "careers",
    "jobs",
    "consulting",
    "enquiries",
    "forms",
    "form",
    "contact",
    "website",
    "webmaster",
];
/// Words in a display name that mean a company or a product, not a person.
const ORG_WORDS: &[&str] = &[
    "support",
    "team",
    "developers",
    "developer",
    "news",
    "newsletter",
    "notifications",
    "billing",
    "inc",
    "ltd",
    "llc",
    "pty",
    "app",
    "hq",
    "updates",
    "digest",
    "platform",
    "advertising",
    "academy",
    "community",
    "events",
    "official",
    "the",
    "via",
    "from",
    "at",
    "and",
    "of",
    "for",
    "&",
    "|",
    "-",
    "+",
    "store",
    "shop",
    "group",
    "service",
    "services",
    "labs",
    "club",
    "products",
    "executive",
    "search",
    "consulting",
    "debtors",
    "enquiries",
    "form",
    "forms",
    "contact",
    "website",
    "submission",
    "submissions",
];
/// Jev scores under this, on mail with an unsubscribe header, read as a mailing.
const BULK_PERSON_FLOOR: f64 = 0.3;

/// The sender reads as a person: a display name of two to four capitalised
/// words with no company word and no digits, at an address that is not a role.
pub fn personal_sender(name: &str, addr: &str) -> bool {
    let local = addr.trim().to_ascii_lowercase();
    let local = local.split('@').next().unwrap_or("");
    let base = local.split(['+', '.']).next().unwrap_or("");
    let machine = [
        "noreply",
        "no-reply",
        "notification",
        "alerts",
        "news",
        "bounce",
        "mailer",
    ];
    if ROLE_LOCALS.contains(&local)
        || ROLE_LOCALS.contains(&base)
        || machine.iter().any(|m| {
            local.starts_with(&format!("{m}-"))
                || local.starts_with(&format!("{m}_"))
                || local.ends_with(&format!("_{m}"))
                || local.ends_with(&format!("-{m}"))
        })
    {
        return false;
    }
    let name = name.trim().trim_matches('"');
    if name.is_empty() || name.contains('@') || name.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    let words: Vec<&str> = name.split([' ', ',']).filter(|w| !w.is_empty()).collect();
    if !(2..=4).contains(&words.len()) {
        return false;
    }
    if words
        .iter()
        .any(|w| ORG_WORDS.contains(&w.trim_matches(['(', ')', '.']).to_lowercase().as_str()))
    {
        return false;
    }
    words.iter().all(|w| {
        w.chars()
            .find(|c| c.is_alphabetic())
            .is_none_or(|c| c.is_uppercase())
    })
}

/// Conversations in which a person wrote: a personal sender who is not the
/// user, not on the exclude list and not sending calendar mail. A person on a
/// mailing (unsubscribe header) counts only when Jev scored the conversation
/// at least [`BULK_PERSON_FLOOR`].
pub fn people(
    msgs: &[Msg],
    own: &HashSet<String>,
    exclude: &[String],
    heat: &BTreeMap<i64, f64>,
) -> HashSet<i64> {
    let mut out = HashSet::new();
    for m in msgs {
        if own.contains(&m.from_addr.trim().to_ascii_lowercase())
            || excluded(&m.from_addr, exclude)
            || is_calendar(&m.subject, &m.from_addr)
            || !personal_sender(&m.from_name, &m.from_addr)
        {
            continue;
        }
        if m.bulk && heat.get(&m.thread_id).copied().unwrap_or(0.0) < BULK_PERSON_FLOOR {
            continue;
        }
        out.insert(m.thread_id);
    }
    out
}

/// Pure: the tests drive it.
pub fn decide(
    msgs: &[Msg],
    own: &HashSet<String>,
    exclude: &[String],
    always: &[String],
    deals: &HashSet<i64>,
) -> BTreeMap<i64, f64> {
    let mut best: BTreeMap<i64, f64> = BTreeMap::new();
    let mut cold: HashSet<i64> = HashSet::new();
    let mut replied: HashSet<i64> = HashSet::new();
    // Conversations with someone other than the user who is not excluded.
    let mut open: HashSet<i64> = HashSet::new();
    for m in msgs {
        if own.contains(&m.from_addr.trim().to_ascii_lowercase()) {
            replied.insert(m.thread_id);
            continue;
        }
        if deals.contains(&m.thread_id) || excluded(&m.from_addr, always) {
            best.insert(m.thread_id, 1.0);
            open.insert(m.thread_id);
            continue;
        }
        if excluded(&m.from_addr, exclude) {
            cold.insert(m.thread_id);
            continue;
        }
        open.insert(m.thread_id);
        match m.score {
            Some(Some(s)) => {
                let slot = best.entry(m.thread_id).or_insert(s);
                if s > *slot {
                    *slot = s;
                }
            }
            Some(None) => {}
            None => {
                if cold_by_rule(m, exclude) {
                    cold.insert(m.thread_id);
                }
            }
        }
    }
    for t in replied {
        if open.contains(&t) {
            let slot = best.entry(t).or_insert(REPLIED_FLOOR);
            if *slot < REPLIED_FLOOR {
                *slot = REPLIED_FLOOR;
            }
        }
    }
    for t in cold {
        best.entry(t).or_insert(0.0);
    }
    best
}

fn load(conn: &Connection, ids: &[i64]) -> rusqlite::Result<Vec<Msg>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let marks = vec!["?"; ids.len()].join(",");
    let key = "m.account_id || char(31) || lower(coalesce(m.from_addr, '')) || char(31) || \
coalesce(nullif(m.message_id, ''), 'id:' || m.id)";
    let mut stmt = conn.prepare(&format!(
        "SELECT m.thread_id, coalesce(m.from_addr, ''), coalesce(m.subject, ''),
                coalesce(m.snippet, ''), coalesce(m.list_unsubscribe, '') <> '',
                coalesce(m.auth_spf, ''), coalesce(m.auth_dkim, ''), coalesce(m.auth_dmarc, ''),
                s.msg_key IS NOT NULL, s.score, coalesce(m.from_name, '')
         FROM messages m
         JOIN folders f ON f.id = m.folder_id
         LEFT JOIN fork_priority_score s ON s.msg_key = {key}
         WHERE m.thread_id IN ({marks})
           AND coalesce(f.role, '') NOT IN ('drafts', 'trash', 'spam', 'junk')"
    ))?;
    let rows = stmt.query_map(params_from_iter(ids.iter()), |r| {
        let asked: bool = r.get(8)?;
        Ok(Msg {
            thread_id: r.get(0)?,
            from_name: r.get(10)?,
            from_addr: r.get(1)?,
            subject: r.get(2)?,
            preview: r.get(3)?,
            bulk: r.get(4)?,
            spf: r.get(5)?,
            dkim: r.get(6)?,
            dmarc: r.get(7)?,
            score: if asked { Some(r.get(9)?) } else { None },
        })
    })?;
    rows.collect()
}

pub fn heat_for(
    conn: &Connection,
    ids: &[i64],
    deals: &HashSet<i64>,
) -> rusqlite::Result<Vec<Heat>> {
    let ids: Vec<i64> = ids.iter().copied().take(MAX_IDS).collect();
    let own = crate::fork::court::own_addresses(conn)?;
    let l = lists(conn)?;
    let msgs = load(conn, &ids)?;
    let out = decide(&msgs, &own, &l.exclude, &l.always, deals);
    let persons = people(&msgs, &own, &l.exclude, &out);
    let mut ids: Vec<i64> = out.keys().chain(persons.iter()).copied().collect();
    ids.sort_unstable();
    ids.dedup();
    Ok(ids
        .into_iter()
        .map(|thread_id| Heat {
            thread_id,
            heat: out.get(&thread_id).copied(),
            person: persons.contains(&thread_id),
        })
        .collect())
}

/// The heat of the given conversations; ones without heat are left out.
#[tauri::command]
pub async fn fork_heat(state: State<'_, AppState>, thread_ids: Vec<i64>) -> Result<Vec<Heat>> {
    let cache = state.fork.deals.clone();
    state
        .db
        .call(move |conn| {
            let deals: HashSet<i64> = cache.membership(conn)?.1.keys().copied().collect();
            heat_for(conn, &thread_ids, &deals)
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(thread_id: i64, from: &str, score: Option<Option<f64>>) -> Msg {
        Msg {
            thread_id,
            from_addr: from.into(),
            subject: "Hello".into(),
            spf: "pass".into(),
            score,
            ..Msg::default()
        }
    }

    fn run(msgs: &[Msg]) -> BTreeMap<i64, f64> {
        let own: HashSet<String> = ["me@x.com".to_string()].into();
        decide(
            msgs,
            &own,
            &["alerts.com".into()],
            &["vip.com".into()],
            &[7].into(),
        )
    }

    #[test]
    fn the_highest_score_in_a_conversation_wins() {
        let out = run(&[
            msg(1, "a@b.com", Some(Some(0.2))),
            msg(1, "c@d.com", Some(Some(0.7))),
        ]);
        assert_eq!(out.get(&1), Some(&0.7));
    }

    #[test]
    fn own_mail_never_counts() {
        assert!(run(&[msg(1, "me@x.com", Some(Some(0.9)))]).is_empty());
    }

    #[test]
    fn a_reply_lifts_a_conversation_to_the_floor_never_down() {
        let low = run(&[msg(1, "a@b.com", Some(Some(0.1))), msg(1, "me@x.com", None)]);
        assert_eq!(low.get(&1), Some(&REPLIED_FLOOR));
        let high = run(&[msg(1, "a@b.com", Some(Some(0.7))), msg(1, "me@x.com", None)]);
        assert_eq!(high.get(&1), Some(&0.7));
        let unscored = run(&[msg(1, "a@b.com", None), msg(1, "me@x.com", None)]);
        assert_eq!(unscored.get(&1), Some(&REPLIED_FLOOR));
    }

    #[test]
    fn an_excluded_sender_is_cold_even_with_a_score_or_a_reply() {
        let out = run(&[
            msg(1, "x@alerts.com", Some(Some(0.9))),
            msg(1, "me@x.com", None),
        ]);
        assert_eq!(out.get(&1), Some(&0.0));
    }

    #[test]
    fn deals_and_the_always_list_are_hot() {
        let out = run(&[
            msg(7, "a@b.com", Some(Some(0.1))),
            msg(2, "x@vip.com", None),
        ]);
        assert_eq!(out.get(&7), Some(&1.0));
        assert_eq!(out.get(&2), Some(&1.0));
    }

    #[test]
    fn mail_never_asked_about_by_rule_is_cold() {
        let mut bulk = msg(1, "news@b.com", None);
        bulk.bulk = true;
        let alert = msg(2, "x@alerts.com", None);
        let mut cal = msg(3, "a@b.com", None);
        cal.subject = "Accepted: Call".into();
        let mut forged = msg(4, "a@b.com", None);
        forged.spf = String::new();
        let out = run(&[bulk, alert, cal, forged]);
        for t in 1..=4 {
            assert_eq!(out.get(&t), Some(&0.0), "thread {t}");
        }
    }

    #[test]
    fn unscored_mail_that_could_be_scored_has_no_heat_yet() {
        assert!(run(&[msg(1, "a@b.com", None)]).is_empty());
        assert!(run(&[msg(1, "a@b.com", Some(None))]).is_empty());
    }

    #[test]
    fn a_score_beats_a_cold_message_in_the_same_conversation() {
        let mut bulk = msg(1, "news@b.com", None);
        bulk.bulk = true;
        let out = run(&[bulk, msg(1, "a@b.com", Some(Some(0.6)))]);
        assert_eq!(out.get(&1), Some(&0.6));
    }

    #[test]
    fn a_person_is_a_capitalised_name_at_a_personal_address() {
        assert!(personal_sender("Nora Quill", "hi@quillhouse.example"));
        assert!(personal_sender("Ted Marlow", "ted@marlowtrips.example"));
        assert!(personal_sender("Renée Voss", "renee@vossworks.example"));
        assert!(!personal_sender("Acme Support", "support@acme.example"));
        assert!(!personal_sender(
            "Acme Developers",
            "acmedevelopers-noreply@acme.example"
        ));
        assert!(!personal_sender("Digest", "digest@send.acme.example"));
        assert!(!personal_sender("", "bot@hey.example"));
        assert!(!personal_sender(
            "Jo Lane Executive Search",
            "info@jolane.example"
        ));
        assert!(!personal_sender(
            "Dr Ines van Dam",
            "clinic.noreply@clinic.example"
        ));
        assert!(!personal_sender("Mia + Lou", "hello@mialou.example"));
    }

    #[test]
    fn people_skips_own_excluded_calendar_and_low_scored_mailings() {
        let own: HashSet<String> = ["me@x.com".to_string()].into();
        let exclude = vec!["warmup.example".to_string()];
        let m = |t: i64, name: &str, addr: &str, subject: &str, bulk: bool| Msg {
            thread_id: t,
            from_name: name.into(),
            from_addr: addr.into(),
            subject: subject.into(),
            bulk,
            ..Msg::default()
        };
        let msgs = vec![
            m(1, "Ann Lee", "ann@client.com", "Question", false),
            m(2, "Me Myself", "me@x.com", "Hi", false),
            m(3, "Pat Nes", "pat@warmup.example", "RE: hi", false),
            m(
                4,
                "Ann Lee",
                "ann@client.com",
                "Invitation: call @ Tue",
                false,
            ),
            m(5, "Ann Lee", "ann@list.com", "Newsletter", true),
            m(6, "Ann Lee", "ann@list.com", "Reply on list", true),
        ];
        let heat: BTreeMap<i64, f64> = [(5, 0.1), (6, 0.6)].into();
        let p = people(&msgs, &own, &exclude, &heat);
        let mut got: Vec<i64> = p.into_iter().collect();
        got.sort();
        assert_eq!(got, vec![1, 6]);
    }
}
