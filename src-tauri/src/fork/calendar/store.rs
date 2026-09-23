//! SQL over the three `fork_cal_*` tables. Every function takes a connection
//! and returns `rusqlite::Result`, so it runs inside `Db::call` / `Db::read`
//! and inside an in-memory test database alike.

use super::model::{CalendarRow, EventRow, LOCAL_ID_PREFIX};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde_json::Value;

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ---- calendars --------------------------------------------------------------

const CAL_COLS: &str =
    "id, account_id, google_id, summary, color, is_primary, selected, access_role";

fn calendar_row(r: &Row) -> rusqlite::Result<CalendarRow> {
    Ok(CalendarRow {
        id: r.get(0)?,
        account_id: r.get(1)?,
        google_id: r.get(2)?,
        summary: r.get(3)?,
        color: r.get(4)?,
        is_primary: r.get::<_, i64>(5)? != 0,
        selected: r.get::<_, i64>(6)? != 0,
        access_role: r.get(7)?,
    })
}

pub fn list_calendars(conn: &Connection, account_id: &str) -> rusqlite::Result<Vec<CalendarRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {CAL_COLS} FROM fork_cal_calendars WHERE account_id = ?1
         ORDER BY is_primary DESC, summary COLLATE NOCASE"
    ))?;
    let rows = stmt.query_map([account_id], calendar_row)?;
    rows.collect()
}

pub fn get_calendar(conn: &Connection, id: i64) -> rusqlite::Result<Option<CalendarRow>> {
    conn.query_row(
        &format!("SELECT {CAL_COLS} FROM fork_cal_calendars WHERE id = ?1"),
        [id],
        calendar_row,
    )
    .optional()
}

/// Bring the stored list in line with Google's: new calendars are inserted
/// with Google's own `selected` flag as the starting point, known ones keep
/// the local `selected` choice, and ones Google no longer lists are removed
/// (their events cascade).
pub fn upsert_calendars(
    conn: &mut Connection,
    account_id: &str,
    fresh: &[CalendarRow],
) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    for c in fresh {
        tx.execute(
            "INSERT INTO fork_cal_calendars
                (account_id, google_id, summary, color, is_primary, selected, access_role)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(account_id, google_id) DO UPDATE SET
                summary = excluded.summary, color = excluded.color,
                is_primary = excluded.is_primary, access_role = excluded.access_role",
            params![
                account_id,
                c.google_id,
                c.summary,
                c.color,
                c.is_primary as i64,
                c.selected as i64,
                c.access_role
            ],
        )?;
    }
    let keep: Vec<Value> = fresh
        .iter()
        .map(|c| Value::from(c.google_id.as_str()))
        .collect();
    tx.execute(
        "DELETE FROM fork_cal_calendars WHERE account_id = ?1
           AND google_id NOT IN (SELECT value FROM json_each(?2))",
        params![account_id, Value::Array(keep).to_string()],
    )?;
    tx.commit()
}

pub fn set_selected(conn: &Connection, calendar_id: i64, selected: bool) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE fork_cal_calendars SET selected = ?2 WHERE id = ?1",
        params![calendar_id, selected as i64],
    )
    .map(|_| ())
}

pub fn delete_account_data(conn: &Connection, account_id: &str) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM fork_cal_calendars WHERE account_id = ?1",
        [account_id],
    )?;
    conn.execute(
        "DELETE FROM fork_cal_ops WHERE account_id = ?1",
        [account_id],
    )?;
    Ok(())
}

// ---- events -----------------------------------------------------------------

const EV_COLS: &str = "id, calendar_id, google_id, etag, status, summary, description, location, \
     start_ts, end_ts, all_day, start_date, end_date, time_zone, recurring_event_id, \
     organizer_email, attendees_json, self_response, transparency, hangout_link, html_link, \
     updated, local_only";

fn event_row(r: &Row) -> rusqlite::Result<EventRow> {
    Ok(EventRow {
        id: r.get(0)?,
        calendar_id: r.get(1)?,
        google_id: r.get(2)?,
        etag: r.get(3)?,
        status: r.get(4)?,
        summary: r.get(5)?,
        description: r.get(6)?,
        location: r.get(7)?,
        start_ts: r.get(8)?,
        end_ts: r.get(9)?,
        all_day: r.get::<_, i64>(10)? != 0,
        start_date: r.get(11)?,
        end_date: r.get(12)?,
        time_zone: r.get(13)?,
        recurring_event_id: r.get(14)?,
        organizer_email: r.get(15)?,
        attendees_json: r.get(16)?,
        self_response: r.get(17)?,
        transparency: r.get(18)?,
        hangout_link: r.get(19)?,
        html_link: r.get(20)?,
        updated: r.get(21)?,
        local_only: r.get::<_, Option<i64>>(22)?.unwrap_or(0) != 0,
    })
}

fn upsert_event(conn: &Connection, e: &EventRow) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO fork_cal_events
            (calendar_id, google_id, etag, status, summary, description, location,
             start_ts, end_ts, all_day, start_date, end_date, time_zone, recurring_event_id,
             organizer_email, attendees_json, self_response, transparency, hangout_link,
             html_link, updated, local_only)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
                 ?18, ?19, ?20, ?21, ?22)
         ON CONFLICT(calendar_id, google_id) DO UPDATE SET
            etag = excluded.etag, status = excluded.status, summary = excluded.summary,
            description = excluded.description, location = excluded.location,
            start_ts = excluded.start_ts, end_ts = excluded.end_ts, all_day = excluded.all_day,
            start_date = excluded.start_date, end_date = excluded.end_date,
            time_zone = excluded.time_zone, recurring_event_id = excluded.recurring_event_id,
            organizer_email = excluded.organizer_email, attendees_json = excluded.attendees_json,
            self_response = excluded.self_response, transparency = excluded.transparency,
            hangout_link = excluded.hangout_link, html_link = excluded.html_link,
            updated = excluded.updated, local_only = excluded.local_only",
        params![
            e.calendar_id,
            e.google_id,
            e.etag,
            e.status,
            e.summary,
            e.description,
            e.location,
            e.start_ts,
            e.end_ts,
            e.all_day as i64,
            e.start_date,
            e.end_date,
            e.time_zone,
            e.recurring_event_id,
            e.organizer_email,
            e.attendees_json,
            e.self_response,
            e.transparency,
            e.hangout_link,
            e.html_link,
            e.updated,
            e.local_only as i64,
        ],
    )?;
    conn.query_row(
        "SELECT id FROM fork_cal_events WHERE calendar_id = ?1 AND google_id = ?2",
        params![e.calendar_id, e.google_id],
        |r| r.get(0),
    )
}

/// Replace one calendar's rows inside `[window_start, window_end)` with the
/// fresh pull, in one transaction. Rows that are `local_only` (a create whose
/// op has not landed) are kept; a fresh row with the same `google_id` as an
/// existing one updates it in place, keeping its row id.
pub fn replace_window(
    conn: &mut Connection,
    calendar_id: i64,
    window_start: i64,
    window_end: i64,
    fresh: &[EventRow],
) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    // Upsert first, prune second: a row Google still returns keeps its id
    // across pulls (the UI keys on it), instead of dying and coming back.
    for e in fresh {
        upsert_event(&tx, e)?;
    }
    let keep: Vec<Value> = fresh
        .iter()
        .map(|e| Value::from(e.google_id.as_str()))
        .collect();
    tx.execute(
        "DELETE FROM fork_cal_events
          WHERE calendar_id = ?1 AND local_only = 0
            AND end_ts > ?2 AND start_ts < ?3
            AND google_id NOT IN (SELECT value FROM json_each(?4))",
        params![
            calendar_id,
            window_start,
            window_end,
            Value::Array(keep).to_string()
        ],
    )?;
    tx.commit()
}

/// Events on the account's selected calendars overlapping `[from, to)`,
/// including cancelled ones (the UI and availability filter by status).
/// `account_id = None` spans every account.
pub fn events_between(
    conn: &Connection,
    account_id: Option<&str>,
    from: i64,
    to: i64,
) -> rusqlite::Result<Vec<EventRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {} FROM fork_cal_events e
           JOIN fork_cal_calendars c ON c.id = e.calendar_id
          WHERE c.selected = 1 AND (?1 IS NULL OR c.account_id = ?1)
            AND e.end_ts > ?2 AND e.start_ts < ?3
          ORDER BY e.start_ts, e.id",
        EV_COLS
            .split(", ")
            .map(|c| format!("e.{c}"))
            .collect::<Vec<_>>()
            .join(", ")
    ))?;
    let rows = stmt.query_map(params![account_id, from, to], event_row)?;
    rows.collect()
}

pub fn get_event(conn: &Connection, id: i64) -> rusqlite::Result<Option<EventRow>> {
    conn.query_row(
        &format!("SELECT {EV_COLS} FROM fork_cal_events WHERE id = ?1"),
        [id],
        event_row,
    )
    .optional()
}

/// Insert a locally created event with a placeholder id; returns the row id.
pub fn insert_local_event(conn: &Connection, e: &EventRow) -> rusqlite::Result<i64> {
    let mut row = e.clone();
    row.google_id = format!("{LOCAL_ID_PREFIX}{}", uuid::Uuid::new_v4());
    row.local_only = true;
    row.etag = None;
    upsert_event(conn, &row)
}

/// Apply the user's edit to the local row at once (optimistic).
pub fn update_local_event(conn: &Connection, e: &EventRow) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE fork_cal_events SET
            summary = ?2, description = ?3, location = ?4, start_ts = ?5, end_ts = ?6,
            all_day = ?7, start_date = ?8, end_date = ?9, time_zone = ?10,
            attendees_json = ?11, self_response = ?12
         WHERE id = ?1",
        params![
            e.id,
            e.summary,
            e.description,
            e.location,
            e.start_ts,
            e.end_ts,
            e.all_day as i64,
            e.start_date,
            e.end_date,
            e.time_zone,
            e.attendees_json,
            e.self_response,
        ],
    )
    .map(|_| ())
}

/// After a create op lands: adopt Google's id and the server copy.
pub fn mark_synced(conn: &Connection, row_id: i64, server: &EventRow) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE fork_cal_events SET
            google_id = ?2, etag = ?3, status = ?4, hangout_link = ?5, html_link = ?6,
            updated = ?7, organizer_email = ?8, attendees_json = ?9, self_response = ?10,
            local_only = 0
         WHERE id = ?1",
        params![
            row_id,
            server.google_id,
            server.etag,
            server.status,
            server.hangout_link,
            server.html_link,
            server.updated,
            server.organizer_email,
            server.attendees_json,
            server.self_response,
        ],
    )
    .map(|_| ())
}

pub fn delete_event_row(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM fork_cal_events WHERE id = ?1", [id])
        .map(|_| ())
}

// ---- ops --------------------------------------------------------------------

pub fn queue_op(
    conn: &Connection,
    account_id: &str,
    kind: &str,
    payload: &Value,
) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO fork_cal_ops (account_id, kind, payload, created_at, attempts, state)
         VALUES (?1, ?2, ?3, ?4, 0, 'pending')",
        params![account_id, kind, payload.to_string(), now_unix()],
    )?;
    Ok(conn.last_insert_rowid())
}

/// `(id, kind, payload, attempts)` of the oldest pending op, FIFO.
pub fn next_op(
    conn: &Connection,
    account_id: &str,
) -> rusqlite::Result<Option<(i64, String, String, i64)>> {
    conn.query_row(
        "SELECT id, kind, payload, attempts FROM fork_cal_ops
          WHERE account_id = ?1 AND state = 'pending' ORDER BY id LIMIT 1",
        [account_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )
    .optional()
}

pub fn finish_op(conn: &Connection, op_id: i64, success: bool) -> rusqlite::Result<()> {
    if success {
        conn.execute("DELETE FROM fork_cal_ops WHERE id = ?1", [op_id])?;
    } else {
        conn.execute(
            "UPDATE fork_cal_ops SET state = 'failed' WHERE id = ?1",
            [op_id],
        )?;
    }
    Ok(())
}

pub fn bump_attempts(conn: &Connection, op_id: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE fork_cal_ops SET attempts = attempts + 1 WHERE id = ?1",
        [op_id],
    )
    .map(|_| ())
}

/// Drop every pending op that names this local event row (a delete of a row
/// whose create never left the machine).
pub fn drop_ops_for_event(conn: &Connection, event_id: i64) -> rusqlite::Result<usize> {
    conn.execute(
        "DELETE FROM fork_cal_ops WHERE state = 'pending'
           AND json_extract(payload, '$.event_id') = ?1",
        [event_id],
    )
}

pub fn pending_op_count(conn: &Connection, account_id: &str) -> rusqlite::Result<i64> {
    conn.query_row(
        "SELECT count(*) FROM fork_cal_ops WHERE account_id = ?1 AND state = 'pending'",
        [account_id],
        |r| r.get(0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    fn cal(account: &str, gid: &str, selected: bool) -> CalendarRow {
        CalendarRow {
            id: 0,
            account_id: account.into(),
            google_id: gid.into(),
            summary: gid.into(),
            color: None,
            is_primary: gid == "primary",
            selected,
            access_role: "owner".into(),
        }
    }

    fn ev(calendar_id: i64, gid: &str, start: i64, end: i64) -> EventRow {
        EventRow {
            calendar_id,
            google_id: gid.into(),
            status: "confirmed".into(),
            summary: gid.into(),
            start_ts: start,
            end_ts: end,
            ..Default::default()
        }
    }

    #[test]
    fn calendars_upsert_keeps_local_selection_and_prunes_missing() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            upsert_calendars(
                conn,
                "a1",
                &[cal("a1", "primary", true), cal("a1", "team", false)],
            )?;
            let ids: Vec<_> = list_calendars(conn, "a1")?;
            assert_eq!(ids.len(), 2);
            let team = ids.iter().find(|c| c.google_id == "team").unwrap();
            set_selected(conn, team.id, true)?;
            // Google now says: team unselected, primary renamed, "old" gone.
            let mut renamed = cal("a1", "primary", true);
            renamed.summary = "Patrick".into();
            upsert_calendars(conn, "a1", &[renamed, cal("a1", "team", false)])?;
            let after = list_calendars(conn, "a1")?;
            let team = after.iter().find(|c| c.google_id == "team").unwrap();
            assert!(team.selected, "the local choice survives a re-list");
            assert_eq!(after[0].summary, "Patrick");
            upsert_calendars(conn, "a1", &[cal("a1", "primary", true)])?;
            assert_eq!(
                list_calendars(conn, "a1")?.len(),
                1,
                "a dropped calendar is pruned"
            );
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn window_replace_keeps_local_only_rows_and_updates_known_ids() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            upsert_calendars(conn, "a1", &[cal("a1", "primary", true)])?;
            let cid = list_calendars(conn, "a1")?[0].id;
            replace_window(
                conn,
                cid,
                0,
                10_000,
                &[ev(cid, "g1", 100, 200), ev(cid, "g2", 300, 400)],
            )?;
            let local_id = insert_local_event(conn, &ev(cid, "ignored", 500, 600))?;
            // Fresh pull: g1 moved, g2 gone, g3 new. The local row must survive.
            replace_window(
                conn,
                cid,
                0,
                10_000,
                &[ev(cid, "g1", 150, 250), ev(cid, "g3", 700, 800)],
            )?;
            let rows = events_between(conn, Some("a1"), 0, 10_000)?;
            let ids: Vec<&str> = rows.iter().map(|r| r.google_id.as_str()).collect();
            assert_eq!(ids.len(), 3, "{ids:?}");
            assert!(ids.contains(&"g1") && ids.contains(&"g3"));
            assert!(!ids.contains(&"g2"));
            let local = rows.iter().find(|r| r.id == local_id).unwrap();
            assert!(local.local_only && local.google_id.starts_with("local-"));
            let g1 = rows.iter().find(|r| r.google_id == "g1").unwrap();
            assert_eq!(g1.start_ts, 150);
            // Once the create lands, the row adopts the server id and a later
            // pull with that id updates in place instead of duplicating.
            let mut server = ev(cid, "g9", 500, 600);
            server.etag = Some("\"e\"".into());
            mark_synced(conn, local_id, &server)?;
            replace_window(conn, cid, 0, 10_000, &[ev(cid, "g9", 500, 660)])?;
            let rows = events_between(conn, Some("a1"), 0, 10_000)?;
            let g9: Vec<_> = rows.iter().filter(|r| r.google_id == "g9").collect();
            assert_eq!(g9.len(), 1);
            assert_eq!(g9[0].id, local_id);
            assert_eq!(g9[0].end_ts, 660);
            assert!(!g9[0].local_only);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn events_between_only_reads_selected_calendars() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            upsert_calendars(
                conn,
                "a1",
                &[cal("a1", "primary", true), cal("a1", "team", false)],
            )?;
            let cals = list_calendars(conn, "a1")?;
            let (p, t) = (cals[0].id, cals[1].id);
            replace_window(conn, p, 0, 1000, &[ev(p, "p1", 100, 200)])?;
            replace_window(conn, t, 0, 1000, &[ev(t, "t1", 100, 200)])?;
            assert_eq!(events_between(conn, Some("a1"), 0, 1000)?.len(), 1);
            set_selected(conn, t, true)?;
            assert_eq!(events_between(conn, None, 0, 1000)?.len(), 2);
            // Window edges: overlap, not containment.
            assert_eq!(events_between(conn, None, 150, 160)?.len(), 2);
            assert_eq!(events_between(conn, None, 200, 300)?.len(), 0);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn ops_are_fifo_and_dropped_by_event() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            let a = queue_op(conn, "a1", "create", &serde_json::json!({"event_id": 5}))?;
            let b = queue_op(conn, "a1", "patch", &serde_json::json!({"event_id": 5}))?;
            let c = queue_op(conn, "a1", "delete", &serde_json::json!({"google_id": "x"}))?;
            assert_eq!(next_op(conn, "a1")?.map(|o| o.0), Some(a));
            assert_eq!(pending_op_count(conn, "a1")?, 3);
            assert_eq!(drop_ops_for_event(conn, 5)?, 2);
            assert_eq!(next_op(conn, "a1")?.map(|o| o.0), Some(c));
            bump_attempts(conn, c)?;
            assert_eq!(next_op(conn, "a1")?.map(|o| o.3), Some(1));
            finish_op(conn, c, false)?;
            assert_eq!(next_op(conn, "a1")?, None);
            assert_eq!(next_op(conn, "other")?, None);
            let _ = b;
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn disconnect_cascades_events_and_ops() {
        let db = Db::open_in_memory().unwrap();
        db.with(|conn| {
            upsert_calendars(conn, "a1", &[cal("a1", "primary", true)])?;
            let cid = list_calendars(conn, "a1")?[0].id;
            replace_window(conn, cid, 0, 1000, &[ev(cid, "p1", 100, 200)])?;
            queue_op(conn, "a1", "delete", &serde_json::json!({}))?;
            delete_account_data(conn, "a1")?;
            let n: i64 =
                conn.query_row("SELECT count(*) FROM fork_cal_events", [], |r| r.get(0))?;
            assert_eq!(n, 0);
            assert_eq!(pending_op_count(conn, "a1")?, 0);
            Ok(())
        })
        .unwrap();
    }
}
