//! Free slots for `/slots` (Phase 8). Pure over the stored events: busy =
//! events on selected calendars that are not cancelled, not declined and not
//! transparent; merged; then the working hours of each of the next N working
//! days are walked in the chosen zone on a 30-minute grid, at most three per
//! day, never earlier than now plus a 15-minute buffer.

use crate::error::{Result, SkimError};
use crate::fork::calendar::model::EventRow;
use crate::fork::calendar::store;
use crate::state::AppState;
use chrono::{Datelike, NaiveDate, NaiveTime, TimeZone, Utc, Weekday};
use chrono_tz::Tz;
use serde::Serialize;
use tauri::State;

pub const BUFFER_SECS: i64 = 15 * 60;
pub const STEP_SECS: i64 = 30 * 60;
pub const MAX_PER_DAY: usize = 3;
/// Calendar days scanned at most, so a working-days list of "none" terminates.
const SCAN_LIMIT: u32 = 90;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Busy {
    pub start: i64,
    pub end: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkingHours {
    /// Minutes after local midnight.
    pub start_min: u32,
    pub end_min: u32,
    pub days: Vec<Weekday>,
}

impl Default for WorkingHours {
    fn default() -> Self {
        Self {
            start_min: 9 * 60,
            end_min: 17 * 60,
            days: vec![
                Weekday::Mon,
                Weekday::Tue,
                Weekday::Wed,
                Weekday::Thu,
                Weekday::Fri,
            ],
        }
    }
}

/// One offered slot, unix seconds plus the pieces the composer prints in the
/// zone the hours were walked in.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Slot {
    pub start: i64,
    pub end: i64,
    /// `YYYY-MM-DD` in the walking zone.
    pub day: String,
    /// `Tue 24 Sep` in the walking zone.
    pub label: String,
    /// `10:00` in the walking zone.
    pub time: String,
}

/// `HH:MM` to minutes after midnight.
pub fn parse_hhmm(s: &str) -> Option<u32> {
    let (h, m) = s.trim().split_once(':')?;
    let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
    (h < 24 && m < 60).then_some(h * 60 + m)
}

/// `"1,2,3,4,5"` (Mon=1 .. Sun=7) to weekdays; unknown numbers are skipped.
pub fn parse_days(s: &str) -> Vec<Weekday> {
    s.split(',')
        .filter_map(|p| p.trim().parse::<u8>().ok())
        .filter_map(|n| match n {
            1 => Some(Weekday::Mon),
            2 => Some(Weekday::Tue),
            3 => Some(Weekday::Wed),
            4 => Some(Weekday::Thu),
            5 => Some(Weekday::Fri),
            6 => Some(Weekday::Sat),
            7 => Some(Weekday::Sun),
            _ => None,
        })
        .collect()
}

/// Whether a stored event blocks time.
pub fn is_busy(e: &EventRow) -> bool {
    e.status != "cancelled"
        && e.self_response.as_deref() != Some("declined")
        && e.transparency.as_deref() != Some("transparent")
        && e.end_ts > e.start_ts
}

/// Local midnight of a `YYYY-MM-DD` in `tz`, as unix seconds.
fn local_midnight(date: NaiveDate, tz: Tz) -> Option<i64> {
    let naive = date.and_time(NaiveTime::from_hms_opt(0, 0, 0)?);
    tz.from_local_datetime(&naive)
        .earliest()
        .or_else(|| tz.from_local_datetime(&naive).latest())
        .map(|d| d.timestamp())
}

/// Busy intervals from stored rows. An all-day event blocks its whole local
/// days in `tz` (its stored stamps are UTC midnights, a convention only).
pub fn busy_from_events(events: &[EventRow], tz: Tz) -> Vec<Busy> {
    let mut out = Vec::new();
    for e in events.iter().filter(|e| is_busy(e)) {
        if e.all_day {
            let parse = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok();
            let sd = e.start_date.as_deref().and_then(parse);
            let ed = e.end_date.as_deref().and_then(parse);
            if let (Some(sd), Some(ed)) = (sd, ed) {
                if let (Some(s), Some(t)) = (local_midnight(sd, tz), local_midnight(ed, tz)) {
                    out.push(Busy { start: s, end: t });
                    continue;
                }
            }
        }
        out.push(Busy {
            start: e.start_ts,
            end: e.end_ts,
        });
    }
    merge_busy(out)
}

/// Sort and coalesce overlapping or touching intervals.
pub fn merge_busy(mut v: Vec<Busy>) -> Vec<Busy> {
    v.retain(|b| b.end > b.start);
    v.sort_by_key(|b| (b.start, b.end));
    let mut out: Vec<Busy> = Vec::with_capacity(v.len());
    for b in v {
        match out.last_mut() {
            Some(last) if b.start <= last.end => last.end = last.end.max(b.end),
            _ => out.push(b),
        }
    }
    out
}

/// The slots. `busy` need not be merged. `days` counts working days from the
/// current day in `tz`; a working day with nothing free still counts.
pub fn free_slots(
    busy: &[Busy],
    now: i64,
    days: u32,
    duration_min: u32,
    work: &WorkingHours,
    tz: Tz,
) -> Vec<Slot> {
    let busy = merge_busy(busy.to_vec());
    let duration = i64::from(duration_min.max(5)) * 60;
    let earliest = now + BUFFER_SECS;
    let mut out = Vec::new();
    if work.days.is_empty() || work.end_min <= work.start_min {
        return out;
    }
    let mut date = tz
        .timestamp_opt(now, 0)
        .single()
        .map(|d| d.date_naive())
        .unwrap_or_else(|| Utc.timestamp_opt(now, 0).unwrap().date_naive());
    let mut working_days = 0;
    for _ in 0..SCAN_LIMIT {
        if working_days >= days {
            break;
        }
        if work.days.contains(&date.weekday()) {
            working_days += 1;
            let mut per_day = 0;
            let mut offset = i64::from(work.start_min) * 60;
            let day_end = i64::from(work.end_min) * 60;
            while offset + duration <= day_end && per_day < MAX_PER_DAY {
                let local_start = date.and_time(
                    NaiveTime::from_num_seconds_from_midnight_opt(offset as u32, 0).unwrap(),
                );
                offset += STEP_SECS;
                // A wall-clock start inside a DST gap does not exist: skip it.
                // The end is instant arithmetic, so a slot that straddles the
                // gap still lasts exactly `duration`.
                let Some(start) = tz.from_local_datetime(&local_start).earliest() else {
                    continue;
                };
                let s = start.timestamp();
                let e = s + duration;
                if s < earliest {
                    continue;
                }
                if busy.iter().any(|b| b.start < e && b.end > s) {
                    continue;
                }
                out.push(Slot {
                    start: s,
                    end: e,
                    day: local_start.format("%Y-%m-%d").to_string(),
                    label: local_start.format("%a %d %b").to_string(),
                    time: local_start.format("%H:%M").to_string(),
                });
                per_day += 1;
            }
        }
        date = match date.succ_opt() {
            Some(d) => d,
            None => break,
        };
    }
    out
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Working hours out of the fork settings rows, defaults where unset.
pub fn working_hours_from(get: impl Fn(&str) -> Option<String>) -> WorkingHours {
    let d = WorkingHours::default();
    WorkingHours {
        start_min: get("fork_cal_work_start")
            .as_deref()
            .and_then(parse_hhmm)
            .unwrap_or(d.start_min),
        end_min: get("fork_cal_work_end")
            .as_deref()
            .and_then(parse_hhmm)
            .unwrap_or(d.end_min),
        days: get("fork_cal_work_days")
            .map(|s| parse_days(&s))
            .filter(|v| !v.is_empty())
            .unwrap_or(d.days),
    }
}

/// `/slots`: free slots over the account's selected calendars (all accounts
/// when `None`), walked in `tz` (IANA name, the sender's zone). `duration_min`
/// 30/45/60, `days` 3/5/10 in the popover. Working hours come from settings.
#[tauri::command]
pub async fn fork_free_slots(
    state: State<'_, AppState>,
    account_id: Option<String>,
    duration_min: u32,
    days: u32,
    tz: String,
) -> Result<Vec<Slot>> {
    let zone: Tz = tz
        .trim()
        .parse()
        .map_err(|_| SkimError::other("gcal_input", format!("unknown time zone {tz:?}")))?;
    let now = now_unix();
    // Enough rows to cover the scan: the working days requested can spread
    // over more than twice as many calendar days, plus long all-day events.
    let (from, to) = (
        now - 7 * 86_400,
        now + i64::from(days.clamp(1, 30)) * 3 * 86_400 + 7 * 86_400,
    );
    let (events, work) = state
        .db
        .read("fork_free_slots", move |conn| {
            let events = store::events_between(conn, account_id.as_deref(), from, to)?;
            let mut stmt = conn.prepare(
                "SELECT key, value FROM settings WHERE key IN
                   ('fork_cal_work_start', 'fork_cal_work_end', 'fork_cal_work_days')",
            )?;
            let settings: std::collections::HashMap<String, String> = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?;
            Ok((events, working_hours_from(|k| settings.get(k).cloned())))
        })
        .await?;
    let busy = busy_from_events(&events, zone);
    Ok(free_slots(
        &busy,
        now,
        days.clamp(1, 30),
        duration_min.clamp(5, 480),
        &work,
        zone,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(tz: Tz, s: &str) -> i64 {
        let naive = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap();
        tz.from_local_datetime(&naive).single().unwrap().timestamp()
    }

    fn ev(start: i64, end: i64) -> EventRow {
        EventRow {
            status: "confirmed".into(),
            start_ts: start,
            end_ts: end,
            ..Default::default()
        }
    }

    const JHB: Tz = chrono_tz::Africa::Johannesburg;
    const LON: Tz = chrono_tz::Europe::London;

    #[test]
    fn overlapping_events_merge_and_block_the_union() {
        // Thu 24 Sep 2026, "now" at 07:00 local so the whole day is ahead.
        let now = ts(JHB, "2026-09-24 07:00");
        let busy = busy_from_events(
            &[
                ev(ts(JHB, "2026-09-24 10:00"), ts(JHB, "2026-09-24 11:00")),
                ev(ts(JHB, "2026-09-24 10:30"), ts(JHB, "2026-09-24 12:00")),
            ],
            JHB,
        );
        assert_eq!(busy.len(), 1);
        assert_eq!(
            busy[0],
            Busy {
                start: ts(JHB, "2026-09-24 10:00"),
                end: ts(JHB, "2026-09-24 12:00")
            }
        );
        let slots = free_slots(&busy, now, 1, 30, &WorkingHours::default(), JHB);
        let times: Vec<&str> = slots.iter().map(|s| s.time.as_str()).collect();
        assert_eq!(times, ["09:00", "09:30", "12:00"]);
        assert_eq!(slots[0].day, "2026-09-24");
        assert_eq!(slots[0].label, "Thu 24 Sep");
        // 09:30 + 60 would run into 10:00, so a 60-minute grid skips it.
        let slots = free_slots(&busy, now, 1, 60, &WorkingHours::default(), JHB);
        let times: Vec<&str> = slots.iter().map(|s| s.time.as_str()).collect();
        assert_eq!(times, ["09:00", "12:00", "12:30"]);
    }

    #[test]
    fn all_day_busy_blocks_the_local_day() {
        let now = ts(JHB, "2026-09-24 07:00");
        let mut offsite = ev(0, 86_400);
        offsite.all_day = true;
        offsite.start_date = Some("2026-09-24".into());
        offsite.end_date = Some("2026-09-25".into());
        let busy = busy_from_events(&[offsite], JHB);
        assert_eq!(
            busy,
            [Busy {
                start: ts(JHB, "2026-09-24 00:00"),
                end: ts(JHB, "2026-09-25 00:00")
            }]
        );
        let slots = free_slots(&busy, now, 2, 30, &WorkingHours::default(), JHB);
        assert!(slots.iter().all(|s| s.day == "2026-09-25"), "{slots:?}");
        assert_eq!(slots.len(), 3);
    }

    #[test]
    fn declined_transparent_and_cancelled_are_ignored() {
        let now = ts(JHB, "2026-09-24 07:00");
        let block = |f: fn(&mut EventRow)| {
            let mut e = ev(ts(JHB, "2026-09-24 09:00"), ts(JHB, "2026-09-24 17:00"));
            f(&mut e);
            e
        };
        let declined = block(|e| e.self_response = Some("declined".into()));
        let ooo = block(|e| e.transparency = Some("transparent".into()));
        let gone = block(|e| e.status = "cancelled".into());
        let tentative = block(|e| e.self_response = Some("tentative".into()));
        assert!(busy_from_events(&[declined, ooo, gone], JHB).is_empty());
        assert_eq!(
            busy_from_events(&[tentative], JHB).len(),
            1,
            "tentative still blocks"
        );
        let slots = free_slots(&[], now, 1, 30, &WorkingHours::default(), JHB);
        assert_eq!(slots.len(), MAX_PER_DAY);
    }

    #[test]
    fn slots_never_start_in_the_past_and_respect_the_buffer() {
        // 09:50 local + 15 min buffer = 10:05, so 10:00 is out and 10:30 is first.
        let now = ts(JHB, "2026-09-24 09:50");
        let slots = free_slots(&[], now, 1, 30, &WorkingHours::default(), JHB);
        let times: Vec<&str> = slots.iter().map(|s| s.time.as_str()).collect();
        assert_eq!(times, ["10:30", "11:00", "11:30"]);
    }

    #[test]
    fn working_days_skip_the_weekend() {
        // Fri 25 Sep: 3 working days = Fri, Mon 28, Tue 29.
        let now = ts(JHB, "2026-09-25 07:00");
        let slots = free_slots(&[], now, 3, 30, &WorkingHours::default(), JHB);
        let days: std::collections::BTreeSet<&str> = slots.iter().map(|s| s.day.as_str()).collect();
        assert_eq!(
            days.into_iter().collect::<Vec<_>>(),
            ["2026-09-25", "2026-09-28", "2026-09-29"]
        );
    }

    #[test]
    fn dst_boundary_keeps_wall_clock_hours() {
        // London springs forward on Sun 29 Mar 2026. Fri 27 is GMT (09:00 =
        // 09:00Z); Mon 30 is BST (09:00 = 08:00Z). The grid stays on 09:00 local.
        let now = ts(LON, "2026-03-27 07:00");
        let slots = free_slots(&[], now, 2, 30, &WorkingHours::default(), LON);
        let fri = slots.iter().find(|s| s.day == "2026-03-27").unwrap();
        let mon = slots.iter().find(|s| s.day == "2026-03-30").unwrap();
        assert_eq!(fri.time, "09:00");
        assert_eq!(mon.time, "09:00");
        assert_eq!(fri.start, ts(chrono_tz::UTC, "2026-03-27 09:00"));
        assert_eq!(mon.start, ts(chrono_tz::UTC, "2026-03-30 08:00"));
        assert_eq!(mon.end - mon.start, 1800);
    }

    #[test]
    fn a_wall_clock_time_inside_the_gap_is_skipped() {
        // Working hours that span the 01:00 spring-forward gap on the day
        // itself (Sunday allowed): 01:00 and 01:30 do not exist in London.
        let now = ts(LON, "2026-03-29 00:00") - 3600;
        let work = WorkingHours {
            start_min: 0,
            end_min: 3 * 60,
            days: vec![Weekday::Sun],
        };
        let slots = free_slots(&[], now, 1, 30, &work, LON);
        let times: Vec<&str> = slots.iter().map(|s| s.time.as_str()).collect();
        assert_eq!(times, ["00:00", "00:30", "02:00"]);
    }

    #[test]
    fn zone_conversion_puts_johannesburg_nine_at_seven_utc() {
        let now = ts(JHB, "2026-09-24 07:00");
        let slots = free_slots(&[], now, 1, 30, &WorkingHours::default(), JHB);
        assert_eq!(slots[0].start, ts(chrono_tz::UTC, "2026-09-24 07:00"));
        // The same instant walked in London is a 08:00 slot.
        let slots = free_slots(&[], now, 1, 30, &WorkingHours::default(), LON);
        assert_eq!(slots[0].time, "09:00");
        assert_eq!(slots[0].start, ts(chrono_tz::UTC, "2026-09-24 08:00"));
    }

    #[test]
    fn settings_parse_with_defaults() {
        assert_eq!(parse_hhmm("08:30"), Some(510));
        assert_eq!(parse_hhmm("25:00"), None);
        assert_eq!(parse_hhmm("x"), None);
        assert_eq!(
            parse_days("1,3, 5,9"),
            [Weekday::Mon, Weekday::Wed, Weekday::Fri]
        );
        let w = working_hours_from(|k| match k {
            "fork_cal_work_start" => Some("08:00".into()),
            "fork_cal_work_days" => Some("".into()),
            _ => None,
        });
        assert_eq!(w.start_min, 480);
        assert_eq!(w.end_min, 17 * 60);
        assert_eq!(w.days.len(), 5);
    }

    #[test]
    fn empty_working_days_or_inverted_hours_yield_nothing() {
        let now = ts(JHB, "2026-09-24 07:00");
        let none = WorkingHours {
            days: vec![],
            ..WorkingHours::default()
        };
        assert!(free_slots(&[], now, 5, 30, &none, JHB).is_empty());
        let inverted = WorkingHours {
            start_min: 17 * 60,
            end_min: 9 * 60,
            ..WorkingHours::default()
        };
        assert!(free_slots(&[], now, 5, 30, &inverted, JHB).is_empty());
    }
}
