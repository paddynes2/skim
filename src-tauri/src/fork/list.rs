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
}

impl ListOpts {
    /// From the loosely typed strings the IPC layer carries. Unknown values
    /// mean the default, never an error: a stale setting must not empty a list.
    pub fn parse(filter: Option<&str>, order: Option<&str>) -> Self {
        Self {
            filter: match filter {
                Some("unread") => Filter::Unread,
                Some("starred") => Filter::Starred,
                _ => Filter::All,
            },
            order: match order {
                Some("unread_first") => Order::UnreadFirst,
                _ => Order::Date,
            },
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

/// Splice filter and order into a base list query.
///
/// The base SQL must contain, for `Grouped`, ` GROUP BY t.id` and
/// ` ORDER BY t.last_date DESC`; for `Flat`, ` ORDER BY m.date DESC, m.id DESC`.
/// Those are the exact strings upstream's four queries carry; a missing marker
/// is a programming error and panics in tests (see below).
pub fn apply(base: &str, opts: ListOpts, shape: Shape) -> String {
    match shape {
        Shape::Grouped { folder_pred } => {
            let unread = format!(
                "EXISTS (SELECT 1 FROM messages m3 WHERE m3.thread_id = t.id AND {folder_pred} AND m3.is_read = 0)"
            );
            let filter = match opts.filter {
                Filter::All => String::new(),
                Filter::Unread => format!(" AND {unread}"),
                Filter::Starred => " AND t.starred = 1".to_string(),
            };
            let order = match opts.order {
                Order::Date => " ORDER BY t.last_date DESC".to_string(),
                Order::UnreadFirst => format!(" ORDER BY ({unread}) DESC, t.last_date DESC"),
            };
            let marker = " GROUP BY t.id";
            assert!(
                base.contains(marker),
                "grouped list SQL lost its GROUP BY marker"
            );
            let with_filter = base.replacen(marker, &format!("{filter}{marker}"), 1);
            let om = " ORDER BY t.last_date DESC";
            assert!(
                with_filter.contains(om),
                "grouped list SQL lost its ORDER BY marker"
            );
            with_filter.replacen(om, &order, 1)
        }
        Shape::Flat => {
            let filter = match opts.filter {
                Filter::All => "",
                Filter::Unread => " AND m.is_read = 0",
                Filter::Starred => " AND m.is_starred = 1",
            };
            let om = " ORDER BY m.date DESC, m.id DESC";
            assert!(base.contains(om), "flat list SQL lost its ORDER BY marker");
            let order = match opts.order {
                Order::Date => om.to_string(),
                Order::UnreadFirst => {
                    " ORDER BY (m.is_read = 0) DESC, m.date DESC, m.id DESC".to_string()
                }
            };
            base.replacen(om, &format!("{filter}{order}"), 1)
        }
    }
}

/// Every (filter, order) combination, for plan tests and exhaustive checks.
pub fn all_opts() -> Vec<ListOpts> {
    let mut out = Vec::new();
    for filter in [Filter::All, Filter::Unread, Filter::Starred] {
        for order in [Order::Date, Order::UnreadFirst] {
            out.push(ListOpts { filter, order });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const GROUPED: &str = "SELECT t.id FROM threads t JOIN messages m ON m.thread_id = t.id \
        WHERE m.folder_id = ?1 GROUP BY t.id ORDER BY t.last_date DESC LIMIT ?2 OFFSET ?3";
    const FLAT: &str =
        "SELECT m.id FROM messages m WHERE m.folder_id = ?1 ORDER BY m.date DESC, m.id DESC LIMIT ?2 OFFSET ?3";

    #[test]
    fn default_opts_leave_the_sql_unchanged() {
        let g = Shape::Grouped {
            folder_pred: "m3.folder_id = ?1",
        };
        assert_eq!(apply(GROUPED, ListOpts::default(), g), GROUPED);
        assert_eq!(apply(FLAT, ListOpts::default(), Shape::Flat), FLAT);
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
        assert!(sql.ends_with("ORDER BY t.last_date DESC LIMIT ?2 OFFSET ?3"));
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
        assert!(sql.contains("ORDER BY (EXISTS (SELECT 1 FROM messages m3"));
        assert!(sql.contains(") DESC, t.last_date DESC LIMIT"));
    }

    #[test]
    fn flat_variants() {
        let sql = apply(FLAT, ListOpts::parse(Some("unread"), None), Shape::Flat);
        assert!(sql
            .contains("WHERE m.folder_id = ?1 AND m.is_read = 0 ORDER BY m.date DESC, m.id DESC"));
        let sql = apply(
            FLAT,
            ListOpts::parse(Some("starred"), Some("unread_first")),
            Shape::Flat,
        );
        assert!(sql.contains(
            "AND m.is_starred = 1 ORDER BY (m.is_read = 0) DESC, m.date DESC, m.id DESC"
        ));
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
        assert_eq!(all.len(), 6);
        let mut seen = std::collections::HashSet::new();
        for o in all {
            assert!(seen.insert((o.filter as u8, o.order as u8)));
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
