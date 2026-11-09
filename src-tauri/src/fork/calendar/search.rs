//! Event search: the local cache first, Google's free-text `q` second.
//!
//! The local part matches every whitespace-separated term, case-insensitively,
//! against summary, description, location and the attendee JSON of events on
//! selected calendars. The remote part asks `events.list?q=` for two years
//! either side of now on each selected calendar of each connected account. A
//! remote failure never fails the search: the local hits come back with a
//! `remote_error` code. Remote hits are never stored.

use super::model::EventRow;
use super::{gapi, model, store};
use crate::error::Result;
use crate::fork::google;
use crate::state::AppState;
use serde::Serialize;
use std::collections::HashSet;
use tauri::State;

/// Hits returned when the caller names no limit.
pub const DEFAULT_LIMIT: u32 = 100;
/// The most a caller may ask for.
pub const MAX_LIMIT: u32 = 500;

/// One search hit: every `EventRow` field, flattened, plus where it came from.
/// `remote: true` means Google returned it and it is not in the local cache;
/// such a hit has `id: 0` and cannot be opened as a cached event.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SearchHit {
    #[serde(flatten)]
    pub event: EventRow,
    pub remote: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SearchResult {
    pub hits: Vec<SearchHit>,
    /// Error code of the first remote failure (`network`, `gcal_not_connected`,
    /// ...), `None` when every remote search answered.
    pub remote_error: Option<String>,
}

/// The lowercase terms of a query; empty when it has none.
pub fn terms(query: &str) -> Vec<String> {
    query.split_whitespace().map(str::to_lowercase).collect()
}

/// Whether every term occurs in one of the searched fields.
pub fn matches_terms(row: &EventRow, terms: &[String]) -> bool {
    if terms.is_empty() {
        return false;
    }
    let hay = [
        Some(row.summary.as_str()),
        row.description.as_deref(),
        row.location.as_deref(),
        row.attendees_json.as_deref(),
    ]
    .into_iter()
    .flatten()
    .map(str::to_lowercase)
    .collect::<Vec<_>>();
    terms.iter().all(|t| hay.iter().any(|h| h.contains(t)))
}

/// Merge local and remote hits: deduplicated by `(calendar_id, google_id)`
/// with the local row winning, sorted by start, capped at `limit`.
pub fn merge_hits(local: Vec<EventRow>, remote: Vec<SearchHit>, limit: usize) -> Vec<SearchHit> {
    let mut seen: HashSet<(i64, String)> = HashSet::new();
    let mut out = Vec::with_capacity(local.len() + remote.len());
    for row in local {
        if seen.insert((row.calendar_id, row.google_id.clone())) {
            out.push(SearchHit {
                event: row,
                remote: false,
            });
        }
    }
    for hit in remote {
        if seen.insert((hit.event.calendar_id, hit.event.google_id.clone())) {
            out.push(hit);
        }
    }
    out.sort_by(|a, b| {
        (a.event.start_ts, a.event.calendar_id, &a.event.google_id).cmp(&(
            b.event.start_ts,
            b.event.calendar_id,
            &b.event.google_id,
        ))
    });
    out.truncate(limit);
    out
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Search cached and Google events. `account_id` narrows both parts to one
/// account; `limit` defaults to [`DEFAULT_LIMIT`].
#[tauri::command]
pub async fn fork_cal_search(
    state: State<'_, AppState>,
    query: String,
    account_id: Option<String>,
    limit: Option<u32>,
) -> Result<SearchResult> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT) as usize;
    let terms = terms(&query);
    if terms.is_empty() {
        return Ok(SearchResult {
            hits: vec![],
            remote_error: None,
        });
    }
    let aid = account_id.clone();
    let (local, targets) = state
        .db
        .read("fork_cal_search", move |conn| {
            let local: Vec<EventRow> = store::searchable_events(conn, aid.as_deref())?
                .into_iter()
                .filter(|row| matches_terms(row, &terms))
                .collect();
            let accounts = crate::db::accounts::list(conn)?;
            let mut targets = vec![];
            for a in accounts {
                if aid.as_deref().is_some_and(|x| x != a.id) {
                    continue;
                }
                let cals = store::list_calendars(conn, &a.id)?
                    .into_iter()
                    .filter(|c| c.selected)
                    .collect::<Vec<_>>();
                targets.push((a.id, cals));
            }
            Ok((local, targets))
        })
        .await?;

    let now = now_unix();
    let query = query.trim().to_string();
    let mut remote_error: Option<String> = None;
    let mut fetches = vec![];
    if google::client_config().ok().flatten().is_none() {
        remote_error = Some("gcal_not_configured".into());
    } else {
        let mut any_connected = false;
        for (account, cals) in &targets {
            if !google::is_connected(account).unwrap_or(false) {
                continue;
            }
            any_connected = true;
            for cal in cals {
                let (account, cal, query) = (account.clone(), cal.clone(), query.clone());
                fetches.push(async move {
                    let items = gapi::search_events(&account, &cal.google_id, &query, now).await;
                    (cal, items)
                });
            }
        }
        if !any_connected {
            remote_error = Some("gcal_not_connected".into());
        }
    }
    let mut remote_rows = vec![];
    for (cal, items) in futures::future::join_all(fetches).await {
        match items {
            Ok(items) => remote_rows.extend(
                items
                    .iter()
                    .filter_map(|v| model::event_from_json(cal.id, v))
                    .filter(|row| row.status != "cancelled"),
            ),
            Err(e) => {
                if remote_error.is_none() {
                    remote_error = Some(e.code().to_string());
                }
            }
        }
    }
    // A remote hit already cached is shown as the cached row (real id).
    let remote = state
        .db
        .read("fork_cal_search_remote", move |conn| {
            let mut out = Vec::with_capacity(remote_rows.len());
            for row in remote_rows {
                match store::event_by_google_id(conn, row.calendar_id, &row.google_id)? {
                    Some(cached) => out.push(SearchHit {
                        event: cached,
                        remote: false,
                    }),
                    None => out.push(SearchHit {
                        event: EventRow { id: 0, ..row },
                        remote: true,
                    }),
                }
            }
            Ok(out)
        })
        .await?;
    Ok(SearchResult {
        hits: merge_hits(local, remote, limit),
        remote_error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: i64, cal: i64, gid: &str, start: i64, summary: &str) -> EventRow {
        EventRow {
            id,
            calendar_id: cal,
            google_id: gid.into(),
            status: "confirmed".into(),
            summary: summary.into(),
            start_ts: start,
            end_ts: start + 1800,
            ..Default::default()
        }
    }

    #[test]
    fn every_term_must_match_some_field_case_insensitively() {
        let mut e = row(1, 1, "g1", 0, "Quarterly Board pack");
        e.location = Some("Room Ä".into());
        e.attendees_json = Some(r#"[{"email":"cfo@example.test"}]"#.into());
        let t = |q: &str| matches_terms(&e, &terms(q));
        assert!(t("board"));
        assert!(t("BOARD quarterly"));
        assert!(t("board cfo@example"), "terms may hit different fields");
        assert!(t("room ä"), "lowercase beyond ASCII");
        assert!(!t("board budget"), "every term must match");
        assert!(!t("   "), "an empty query matches nothing");
        e.description = Some("Budget review".into());
        assert!(matches_terms(&e, &terms("board budget")));
    }

    #[test]
    fn merge_prefers_local_dedupes_sorts_and_caps() {
        let local = vec![
            row(7, 1, "shared", 300, "Local copy"),
            row(8, 1, "l2", 100, "L2"),
        ];
        let remote = vec![
            SearchHit {
                event: row(0, 1, "shared", 300, "Remote copy"),
                remote: true,
            },
            SearchHit {
                event: row(0, 2, "shared", 50, "Other calendar"),
                remote: true,
            },
            SearchHit {
                event: row(0, 1, "r1", 200, "Remote only"),
                remote: true,
            },
        ];
        let hits = merge_hits(local.clone(), remote.clone(), 100);
        let got: Vec<(&str, i64, bool)> = hits
            .iter()
            .map(|h| (h.event.summary.as_str(), h.event.id, h.remote))
            .collect();
        assert_eq!(
            got,
            [
                ("Other calendar", 0, true),
                ("L2", 8, false),
                ("Remote only", 0, true),
                ("Local copy", 7, false),
            ]
        );
        assert_eq!(merge_hits(local, remote, 2).len(), 2);
    }

    #[test]
    fn a_hit_serialises_flat_with_its_origin() {
        let hit = SearchHit {
            event: row(0, 3, "r1", 10, "Call"),
            remote: true,
        };
        let v = serde_json::to_value(&hit).unwrap();
        assert_eq!(v["id"], 0);
        assert_eq!(v["google_id"], "r1");
        assert_eq!(v["summary"], "Call");
        assert_eq!(v["remote"], true);
        assert!(v.get("event").is_none());
    }
}
