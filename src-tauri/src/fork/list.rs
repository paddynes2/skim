//! List filter and order (PLAN.md 3.1): `All / Unread / Starred` chips and
//! "unread first". The four upstream list queries take a [`ListOpts`] and
//! hand their base SQL through [`apply`], which splices the clause in before
//! `GROUP BY` / `ORDER BY`. Kept here so `queries.rs` changes by a parameter
//! per function and nothing else.

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Filter {
    #[default]
    All,
    Unread,
    Starred,
    /// v1.1.17 (D68): conversations with a Jev score of [`HOT_SCORE`] or more.
    Hot,
}

/// The Hot filter's floor: mint and seafoam rows. The reply floor
/// (`fork::heat::REPLIED_FLOOR`) sits under it on purpose.
pub const HOT_SCORE: f64 = 0.5;

/// A message aliased `alias` has a stored score of at least [`HOT_SCORE`].
fn hot_pred(alias: &str) -> String {
    format!(
        "EXISTS (SELECT 1 FROM fork_priority_score sh WHERE sh.msg_key = {} AND sh.score >= {HOT_SCORE})",
        crate::fork::priority::msg_key(alias)
    )
}

/// v1.1.32 (D80): a Drafts message whose draft has a send queued (held for
/// the undo window or going out now) is hidden, so it leaves Drafts the moment
/// Send is pressed. Undo drops the op and it comes back; a send that failed
/// for good shows again so it can be fixed.
pub fn sending_pred(alias: &str) -> String {
    format!(
        "{alias}.id NOT IN (SELECT d.origin_message_id FROM drafts d          JOIN pending_ops ps ON ps.kind = 'send' AND ps.state <> 'failed'          AND json_extract(ps.payload, '$.draftId') = d.id          WHERE d.origin_message_id IS NOT NULL)"
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Order {
    #[default]
    Date,
    UnreadFirst,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ListOpts {
    pub filter: Filter,
    pub order: Order,
    /// v1.1.11 (D64): the Inbox puts its Priority conversations first. Never
    /// parsed from the IPC strings: `fork::priority::opts_for_*` sets it from
    /// the folder's role, so no other folder can turn it on.
    pub priority: bool,
}

impl ListOpts {
    /// From the loosely typed strings the IPC layer carries. Unknown values
    /// mean the default, never an error: a stale setting must not empty a list.
    pub fn parse(filter: Option<&str>, order: Option<&str>) -> Self {
        Self {
            filter: match filter {
                Some("unread") => Filter::Unread,
                Some("starred") => Filter::Starred,
                Some("hot") => Filter::Hot,
                _ => Filter::All,
            },
            order: match order {
                Some("unread_first") => Order::UnreadFirst,
                _ => Order::Date,
            },
            priority: false,
        }
    }
}

/// Which query shape the base SQL has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// Grouped by thread: rows are `t`, messages `m`, folder predicate on `m3`.
    /// `folder_pred` is the folder condition for the `m3` unread probe, e.g.
    /// `m3.folder_id = ?1` or `m3.folder_id IN (SELECT id FROM sel)`.
    Grouped { folder_pred: &'static str },
    /// One row per message `m`.
    Flat,
}

/// The date a grouped row shows and sorts by: `m`, the row's newest message
/// in this folder (D63, v1.1.10). A reply Patrick sends is filed in Sent only,
/// so it never moves or re-dates the conversation in Inbox; a draft lives in
/// Drafts only, so it moves nothing outside Drafts, and the Drafts view keeps
/// the draft's own time (D62).
pub const ROW_DATE: &str = "m.date";

/// Splice filter and order into a base list query.
///
/// The base SQL must contain, for `Grouped`, ` GROUP BY t.id` and
/// ` ORDER BY m.date DESC` ([`ROW_DATE`]); for `Flat`,
/// ` ORDER BY m.date DESC, m.id DESC`.
/// Those are the exact strings upstream's four queries carry; a missing marker
/// is a programming error and panics in tests (see below).
pub fn apply(base: &str, opts: ListOpts, shape: Shape) -> String {
    match shape {
        Shape::Grouped { folder_pred } => {
            let unread = format!(
                "EXISTS (SELECT 1 FROM messages m3 WHERE m3.thread_id = t.id AND {folder_pred} AND m3.is_read = 0)"
            );
            // v1.1.1: a snoozed thread is hidden until due; a due reminder
            // pins its thread to the top until it is opened.
            let hidden = format!(
                "{} AND {}",
                crate::fork::reminders::hidden_pred("t.id"),
                sending_pred("m")
            );
            let pinned = crate::fork::reminders::pinned_expr("t.id");
            let filter = match opts.filter {
                Filter::All => format!(" AND {hidden}"),
                Filter::Unread => format!(" AND {hidden} AND {unread}"),
                Filter::Starred => format!(" AND {hidden} AND t.starred = 1"),
                Filter::Hot => format!(
                    " AND {hidden} AND EXISTS (SELECT 1 FROM messages mh WHERE mh.thread_id = t.id AND {})",
                    hot_pred("mh")
                ),
            };
            // v1.1.11 (D64): a Priority conversation sorts first, ahead even of
            // a due reminder, so the Priority header always heads the list.
            // Only while it is still unread here: reading it drops it back to
            // its date at once, before the next priority pass runs.
            let first = if opts.priority {
                format!(
                    "({} AND {unread}) DESC, ",
                    crate::fork::priority::member_pred("t.id")
                )
            } else {
                String::new()
            };
            let order = match opts.order {
                Order::Date => format!(" ORDER BY {first}({pinned}) DESC, {ROW_DATE} DESC"),
                Order::UnreadFirst => {
                    format!(" ORDER BY {first}({pinned}) DESC, ({unread}) DESC, {ROW_DATE} DESC")
                }
            };
            let marker = " GROUP BY t.id";
            assert!(
                base.contains(marker),
                "grouped list SQL lost its GROUP BY marker"
            );
            let with_filter = base.replacen(marker, &format!("{filter}{marker}"), 1);
            let om = format!(" ORDER BY {ROW_DATE} DESC");
            assert!(
                with_filter.contains(&om),
                "grouped list SQL lost its ORDER BY marker"
            );
            with_filter.replacen(&om, &order, 1)
        }
        Shape::Flat => {
            let hidden = format!(
                "{} AND {}",
                crate::fork::reminders::hidden_pred("m.thread_id"),
                sending_pred("m")
            );
            let pinned = crate::fork::reminders::pinned_expr("m.thread_id");
            let filter = match opts.filter {
                Filter::All => format!(" AND {hidden}"),
                Filter::Unread => format!(" AND {hidden} AND m.is_read = 0"),
                Filter::Starred => format!(" AND {hidden} AND m.is_starred = 1"),
                Filter::Hot => format!(" AND {hidden} AND {}", hot_pred("m")),
            };
            let om = " ORDER BY m.date DESC, m.id DESC";
            assert!(base.contains(om), "flat list SQL lost its ORDER BY marker");
            let order = match opts.order {
                Order::Date => format!(" ORDER BY ({pinned}) DESC, m.date DESC, m.id DESC"),
                Order::UnreadFirst => format!(
                    " ORDER BY ({pinned}) DESC, (m.is_read = 0) DESC, m.date DESC, m.id DESC"
                ),
            };
            base.replacen(om, &format!("{filter}{order}"), 1)
        }
    }
}

/// Every (filter, order, priority) combination, for plan tests and
/// exhaustive checks.
pub fn all_opts() -> Vec<ListOpts> {
    let mut out = Vec::new();
    for filter in [Filter::All, Filter::Unread, Filter::Starred, Filter::Hot] {
        for order in [Order::Date, Order::UnreadFirst] {
            for priority in [false, true] {
                out.push(ListOpts {
                    filter,
                    order,
                    priority,
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const GROUPED: &str = "SELECT t.id FROM threads t JOIN messages m ON m.thread_id = t.id \
        WHERE m.folder_id = ?1 GROUP BY t.id ORDER BY m.date DESC LIMIT ?2 OFFSET ?3";
    const FLAT: &str =
        "SELECT m.id FROM messages m WHERE m.folder_id = ?1 ORDER BY m.date DESC, m.id DESC LIMIT ?2 OFFSET ?3";

    #[test]
    fn default_opts_add_only_the_reminder_clauses() {
        let g = Shape::Grouped {
            folder_pred: "m3.folder_id = ?1",
        };
        let hidden = format!(
            "{} AND {}",
            crate::fork::reminders::hidden_pred("t.id"),
            sending_pred("m")
        );
        let pinned = crate::fork::reminders::pinned_expr("t.id");
        assert_eq!(
            apply(GROUPED, ListOpts::default(), g),
            GROUPED
                .replace(" GROUP BY", &format!(" AND {hidden} GROUP BY"))
                .replace(
                    " ORDER BY m.date",
                    &format!(" ORDER BY ({pinned}) DESC, m.date")
                )
        );
        let hidden = format!(
            "{} AND {}",
            crate::fork::reminders::hidden_pred("m.thread_id"),
            sending_pred("m")
        );
        let pinned = crate::fork::reminders::pinned_expr("m.thread_id");
        assert_eq!(
            apply(FLAT, ListOpts::default(), Shape::Flat),
            FLAT.replace(
                " ORDER BY m.date",
                &format!(" AND {hidden} ORDER BY ({pinned}) DESC, m.date")
            )
        );
    }

    #[test]
    fn grouped_unread_filter_uses_the_folder_scoped_probe() {
        let g = Shape::Grouped {
            folder_pred: "m3.folder_id IN (SELECT id FROM sel)",
        };
        let sql = apply(GROUPED, ListOpts::parse(Some("unread"), None), g);
        assert!(sql.contains(
            "AND EXISTS (SELECT 1 FROM messages m3 WHERE m3.thread_id = t.id AND m3.folder_id IN (SELECT id FROM sel) AND m3.is_read = 0) GROUP BY t.id"
        ));
        assert!(sql.ends_with(") DESC, m.date DESC LIMIT ?2 OFFSET ?3"));
    }

    #[test]
    fn grouped_starred_and_unread_first() {
        let g = Shape::Grouped {
            folder_pred: "m3.folder_id = ?1",
        };
        let sql = apply(
            GROUPED,
            ListOpts::parse(Some("starred"), Some("unread_first")),
            g,
        );
        assert!(sql.contains(" AND t.starred = 1 GROUP BY t.id"));
        assert!(sql.contains(") DESC, (EXISTS (SELECT 1 FROM messages m3"));
        assert!(sql.contains(") DESC, m.date DESC LIMIT"));
    }

    #[test]
    fn flat_variants() {
        let sql = apply(FLAT, ListOpts::parse(Some("unread"), None), Shape::Flat);
        assert!(sql.contains(" AND m.is_read = 0 ORDER BY ("));
        assert!(sql.ends_with(") DESC, m.date DESC, m.id DESC LIMIT ?2 OFFSET ?3"));
        let sql = apply(
            FLAT,
            ListOpts::parse(Some("starred"), Some("unread_first")),
            Shape::Flat,
        );
        assert!(sql.contains("AND m.is_starred = 1 ORDER BY ("));
        assert!(sql.contains(") DESC, (m.is_read = 0) DESC, m.date DESC, m.id DESC"));
    }

    #[test]
    fn unknown_strings_fall_back_to_defaults() {
        assert_eq!(
            ListOpts::parse(Some("bogus"), Some("nope")),
            ListOpts::default()
        );
        assert_eq!(ListOpts::parse(None, None), ListOpts::default());
    }

    #[test]
    fn every_combination_is_enumerated_once() {
        let all = all_opts();
        assert_eq!(all.len(), 16);
        let mut seen = std::collections::HashSet::new();
        for o in all {
            assert!(seen.insert((o.filter as u8, o.order as u8, o.priority)));
        }
    }
}

/// Plan and behaviour tests against the real queries (upstream's seed shape).
#[cfg(test)]
mod query_tests {
    use super::*;
    use crate::db::models::NewMessage;
    use crate::db::queries::{
        insert_message, list_messages_opts, list_threads_opts, list_unified_messages_opts,
        list_unified_threads_opts,
    };
    use crate::db::Db;
    use rusqlite::{params, Connection};

    fn seed(conn: &mut Connection) -> i64 {
        conn.execute(
            "INSERT INTO accounts (id, email, provider, imap_host, smtp_host, created_at)
             VALUES ('a1', 'p@example.com', 'custom', 'imap.example.com', 'smtp.example.com', 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO folders (account_id, imap_name, role, display_name, unread_count, sort_order)
             VALUES ('a1', 'INBOX', 'inbox', 'Inbox', 0, 0)",
            [],
        )
        .unwrap();
        let folder: i64 = conn
            .query_row(
                "SELECT id FROM folders WHERE imap_name = 'INBOX'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        // Four threads: (uid, subject, date, read, starred)
        for (uid, subject, date, read, starred) in [
            (1u32, "old unread", 100i64, false, false),
            (2, "old read starred", 200, true, true),
            (3, "new read", 300, true, false),
            (4, "new unread starred", 400, false, true),
        ] {
            insert_message(
                conn,
                &NewMessage {
                    account_id: "a1".into(),
                    folder_id: folder,
                    uid,
                    message_id: Some(format!("<{uid}@example.com>")),
                    subject: Some(subject.into()),
                    from_addr: Some("sender@example.com".into()),
                    date,
                    is_read: read,
                    is_starred: starred,
                    ..Default::default()
                },
            )
            .unwrap();
        }
        folder
    }

    fn subjects(rows: &[crate::db::models::ThreadRow]) -> Vec<String> {
        rows.iter().map(|r| r.subject.clone()).collect()
    }

    #[test]
    fn a_draft_being_sent_leaves_the_list_and_returns_on_undo_or_failure() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            let folder = seed(conn);
            let row: i64 = conn
                .query_row("SELECT id FROM messages WHERE uid = 4", [], |r| r.get(0))
                .unwrap();
            conn.execute(
                "INSERT INTO drafts (id, account_id, updated_at, origin_message_id) VALUES (7, 'a1', 0, ?1)",
                params![row],
            )
            .unwrap();
            let listed = |conn: &Connection| -> Vec<Vec<String>> {
                vec![
                    subjects(&list_threads_opts(conn, folder, 0, 10, ListOpts::default()).unwrap()),
                    subjects(&list_messages_opts(conn, folder, 0, 10, ListOpts::default()).unwrap()),
                ]
            };
            let all = ["new unread starred", "new read", "old read starred", "old unread"];
            for l in listed(conn) {
                assert_eq!(l, all, "a draft at rest stays listed");
            }
            crate::db::bodies::enqueue_op(conn, "a1", "send", &serde_json::json!({ "draftId": 7 }))
                .unwrap();
            for l in listed(conn) {
                assert_eq!(l, &all[1..], "Send hides it at once");
            }
            conn.execute("UPDATE pending_ops SET state = 'failed'", []).unwrap();
            for l in listed(conn) {
                assert_eq!(l, all, "a failed send shows again");
            }
            conn.execute("DELETE FROM pending_ops", []).unwrap();
            for l in listed(conn) {
                assert_eq!(l, all, "Undo brings it back");
            }
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn folder_queries_filter_and_order() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            let folder = seed(conn);
            let unread = ListOpts::parse(Some("unread"), None);
            let starred = ListOpts::parse(Some("starred"), None);
            let first = ListOpts::parse(None, Some("unread_first"));
            for grouped in [true, false] {
                let run = |o: ListOpts| {
                    if grouped {
                        list_threads_opts(conn, folder, 0, 10, o).unwrap()
                    } else {
                        list_messages_opts(conn, folder, 0, 10, o).unwrap()
                    }
                };
                assert_eq!(
                    subjects(&run(ListOpts::default())),
                    [
                        "new unread starred",
                        "new read",
                        "old read starred",
                        "old unread"
                    ]
                );
                assert_eq!(subjects(&run(unread)), ["new unread starred", "old unread"]);
                assert_eq!(
                    subjects(&run(starred)),
                    ["new unread starred", "old read starred"]
                );
                assert_eq!(
                    subjects(&run(first)),
                    [
                        "new unread starred",
                        "old unread",
                        "new read",
                        "old read starred"
                    ]
                );
            }
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn unified_queries_filter_and_order() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            seed(conn);
            let unread = ListOpts::parse(Some("unread"), Some("unread_first"));
            let g = list_unified_threads_opts(conn, Some("inbox"), None, 0, 10, unread).unwrap();
            assert_eq!(subjects(&g), ["new unread starred", "old unread"]);
            let f = list_unified_messages_opts(
                conn,
                Some("inbox"),
                None,
                0,
                10,
                ListOpts::parse(Some("starred"), Some("unread_first")),
            )
            .unwrap();
            assert_eq!(subjects(&f), ["new unread starred", "old read starred"]);
            Ok(())
        })
        .unwrap();
    }

    /// Every grouped variant must still seek `idx_messages_thread_folder` for
    /// the `m2` subquery (upstream's `list_threads_seeks_the_thread_index`).
    #[test]
    fn every_grouped_variant_seeks_the_thread_index() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            for opts in all_opts() {
                let sql = apply(
                    crate::db::queries::LIST_THREADS_SQL,
                    opts,
                    Shape::Grouped {
                        folder_pred: "m3.folder_id = ?1",
                    },
                );
                let plan: Vec<String> = conn
                    .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))?
                    .query_map(params![1, 100, 0], |r| r.get::<_, String>(3))?
                    .collect::<Result<_, _>>()?;
                let m2 = plan
                    .iter()
                    .find(|step| step.contains(" m2 "))
                    .unwrap_or_else(|| panic!("no m2 step for {opts:?}: {plan:#?}"));
                assert!(
                    m2.contains("idx_messages_thread_folder"),
                    "{opts:?}: m2 must seek the thread index, got: {m2}"
                );
            }
            Ok(())
        })
        .unwrap();
    }
}
