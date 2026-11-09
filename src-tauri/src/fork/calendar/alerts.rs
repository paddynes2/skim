//! Meeting alerts: a native Windows toast before a meeting, even when Skim is
//! minimised, and a `calendar:alert` event for the in-app banner.
//!
//! One loop every [`TICK`] reads the cached events and asks [`due_alerts`]
//! (pure, tested) which alerts are due. Each alert fires once per
//! `(account, google_id, start_ts, lead)`: the keys live in memory and in
//! `fork_cal_alerts_fired`, so a restart does not refire one, and a meeting
//! that moves alerts again at its new time.
//!
//! The toast's body click opens Skim (`skim://open`, handled by
//! `notify::handle_skim_uri`); when the event has a meeting link, a **Join**
//! button opens it in the browser (protocol activation with the https URI).

use super::model::{is_local_id, EventRow};
use super::store::{self, FiredKey};
use crate::db::{queries, Db};
use crate::error::Result;
use crate::state::AppState;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, Manager};

/// How often the loop looks.
pub const TICK: std::time::Duration = std::time::Duration::from_secs(20);
/// An event that started longer ago than this never alerts (no flood after
/// sleep or startup).
pub const LATE_GRACE: i64 = 120;
/// Fired keys are kept while their event started less than this long ago.
pub const KEEP_KEYS: i64 = 2 * 86_400;
/// `fork_cal_alert_lead` when unset or unreadable.
pub const DEFAULT_LEAD_MIN: i64 = 1;
/// Google's longest reminder (four weeks), so the candidate query looks that far.
pub const MAX_LEAD_MIN: i64 = 40_320;
pub const EVT_ALERT: &str = "calendar:alert";
pub const KEY_ENABLED: &str = "fork_cal_alerts";
pub const KEY_LEAD: &str = "fork_cal_alert_lead";

const AUMID: &str = "com.skim.app";
/// Hosts whose https links count as a meeting link (and their subdomains).
const MEETING_HOSTS: &[&str] = &["meet.google.com", "zoom.us", "teams.microsoft.com"];
/// Event types that are not meetings.
const SKIP_TYPES: &[&str] = &["outOfOffice", "workingLocation", "focusTime"];

/// One alert to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alert {
    pub event_id: i64,
    pub account_id: String,
    pub title: String,
    pub start_ts: i64,
    /// The lead this alert stands for, in minutes (0 = at the start).
    pub lead_min: i64,
    pub join_url: Option<String>,
    /// The location, or "Google Meet" when there is only a Meet link.
    pub place: Option<String>,
    /// Every key this alert settles: its own, any longer lead that also came
    /// due (startup, wake), and the same meeting seen by another account.
    pub keys: Vec<FiredKey>,
    /// False for an entry that only records keys of an alert already shown
    /// (the same meeting alerted through another account).
    pub show: bool,
}

/// The `calendar:alert` payload.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AlertPayload {
    pub event_id: i64,
    pub account_id: String,
    pub title: String,
    pub start_ts: i64,
    pub lead_min: i64,
    pub join_url: Option<String>,
}

/// Whether an event is a meeting that may alert at all: timed, live, not
/// declined, a real (synced) event, and not out-of-office, working location
/// or focus time.
pub fn alertable(e: &EventRow) -> bool {
    !e.all_day
        && e.status != "cancelled"
        && e.self_response.as_deref() != Some("declined")
        && !e.local_only
        && !is_local_id(&e.google_id)
        && !e
            .event_type
            .as_deref()
            .is_some_and(|t| SKIP_TYPES.contains(&t))
}

/// Lead times in minutes, longest first, deduplicated. An event with its own
/// reminders (`useDefault: false`) alerts at its popup overrides only; every
/// other event at `default_lead` and again at the start.
pub fn leads(e: &EventRow, default_lead: i64) -> Vec<i64> {
    let mut out: Vec<i64> = match &e.options.reminders {
        Some(r) if !r.use_default => r
            .overrides
            .iter()
            .filter(|o| o.method == "popup")
            .map(|o| o.minutes.clamp(0, MAX_LEAD_MIN))
            .collect(),
        _ => vec![default_lead.clamp(0, MAX_LEAD_MIN), 0],
    };
    out.sort_unstable_by(|a, b| b.cmp(a));
    out.dedup();
    out
}

/// The alerts due at `now`. `events` are `(account_id, row)` pairs; `fired`
/// holds the keys already shown. Per event only the shortest due lead is
/// shown (one toast, not a burst, after a wake), and one meeting seen by two
/// accounts (same iCalUID and start) is shown once.
pub fn due_alerts(
    now: i64,
    events: &[(String, EventRow)],
    fired: &HashSet<FiredKey>,
    default_lead: i64,
) -> Vec<Alert> {
    let key = |account: &str, e: &EventRow, lead: i64| -> FiredKey {
        (account.to_string(), e.google_id.clone(), e.start_ts, lead)
    };
    // A twin (same iCalUID + start) already alerted for a lead counts as fired.
    let mut twin_fired: HashSet<(String, i64, i64)> = HashSet::new();
    for (account, e) in events {
        if let Some(uid) = &e.ical_uid {
            for lead in leads(e, default_lead) {
                if fired.contains(&key(account, e, lead)) {
                    twin_fired.insert((uid.clone(), e.start_ts, lead));
                }
            }
        }
    }
    let mut out: Vec<Alert> = vec![];
    let mut by_twin: HashMap<(String, i64), usize> = HashMap::new();
    for (account, e) in events {
        if !alertable(e) || e.start_ts < now - LATE_GRACE {
            continue;
        }
        let is_fired = |lead: i64| {
            fired.contains(&key(account, e, lead))
                || e.ical_uid
                    .as_ref()
                    .is_some_and(|uid| twin_fired.contains(&(uid.clone(), e.start_ts, lead)))
        };
        let due: Vec<i64> = leads(e, default_lead)
            .into_iter()
            .filter(|lead| e.start_ts - lead * 60 <= now)
            .collect();
        if due.is_empty() {
            continue;
        }
        let keys: Vec<FiredKey> = due.iter().map(|l| key(account, e, *l)).collect();
        let shortest = *due.last().expect("non-empty");
        let twin = e.ical_uid.clone().map(|uid| (uid, e.start_ts));
        if let Some(i) = twin.as_ref().and_then(|t| by_twin.get(t)) {
            // The same meeting from another account: settle its keys too.
            out[*i].keys.extend(keys);
            continue;
        }
        if is_fired(shortest) {
            // Already shown at this lead; any longer lead it also passed is
            // settled with it (only happens on a restart mid-way).
            let unsettled: Vec<FiredKey> =
                keys.into_iter().filter(|k| !fired.contains(k)).collect();
            if !unsettled.is_empty() {
                out.push(Alert {
                    keys: unsettled,
                    show: false,
                    ..alert_for(account, e, shortest)
                });
            }
            continue;
        }
        if let Some(t) = twin {
            by_twin.insert(t, out.len());
        }
        out.push(Alert {
            keys,
            ..alert_for(account, e, shortest)
        });
    }
    out
}

fn alert_for(account: &str, e: &EventRow, lead_min: i64) -> Alert {
    let place = e
        .location
        .as_deref()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .or_else(|| e.hangout_link.as_ref().map(|_| "Google Meet".to_string()));
    Alert {
        event_id: e.id,
        account_id: account.to_string(),
        title: if e.summary.trim().is_empty() {
            "(No title)".into()
        } else {
            e.summary.trim().to_string()
        },
        start_ts: e.start_ts,
        lead_min,
        join_url: join_url(e),
        place,
        keys: vec![],
        show: true,
    }
}

/// Whether an alert should be shown, rather than only recorded.
pub fn shows(a: &Alert) -> bool {
    a.show
}

/// The meeting link: the Meet link, else the first https link in the
/// location, then the description, on Meet, Zoom or Teams.
pub fn join_url(e: &EventRow) -> Option<String> {
    if let Some(h) = e
        .hangout_link
        .as_deref()
        .filter(|h| h.starts_with("https://"))
    {
        return Some(h.to_string());
    }
    [e.location.as_deref(), e.description.as_deref()]
        .into_iter()
        .flatten()
        .find_map(first_meeting_link)
}

fn first_meeting_link(text: &str) -> Option<String> {
    let mut rest = text;
    while let Some(i) = rest.find("https://") {
        let tail = &rest[i..];
        let end = tail
            .find(|c: char| c.is_whitespace() || "\"'<>()[]{}|\\^`".contains(c))
            .unwrap_or(tail.len());
        let candidate = tail[..end].trim_end_matches(['.', ',', ';', ':', '!', '?']);
        if let Ok(u) = url::Url::parse(candidate) {
            let host = u.host_str().unwrap_or_default().to_ascii_lowercase();
            if u.scheme() == "https"
                && u.username().is_empty()
                && u.password().is_none()
                && MEETING_HOSTS
                    .iter()
                    .any(|m| host == *m || host.ends_with(&format!(".{m}")))
            {
                return Some(u.to_string());
            }
        }
        rest = &tail["https://".len()..];
    }
    None
}

/// The toast body: `"14:30 · in 5 min · Room 2"`, `"14:30 · now · Google Meet"`.
pub fn body_line(clock: &str, start_ts: i64, now: i64, place: Option<&str>) -> String {
    let when = if start_ts <= now {
        "now".to_string()
    } else {
        let mins = (start_ts - now + 59) / 60;
        format!("in {mins} min")
    };
    match place {
        Some(p) => format!("{clock} · {when} · {p}"),
        None => format!("{clock} · {when}"),
    }
}

/// The toast XML. A body click opens Skim; Join opens the meeting link.
pub fn toast_xml(title: &str, body: &str, join: Option<&str>) -> String {
    let actions = join
        .map(|u| {
            format!(
                "\n    <actions>\n        <action content=\"Join\" activationType=\"protocol\" arguments=\"{}\"/>\n    </actions>",
                xml_escape(u)
            )
        })
        .unwrap_or_default();
    format!(
        r#"<toast activationType="protocol" launch="skim://open">
    <visual>
        <binding template="ToastGeneric">
            <text>{title}</text>
            <text>{body}</text>
        </binding>
    </visual>{actions}
</toast>"#,
        title = xml_escape(title),
        body = xml_escape(body),
    )
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn show_toast(xml: &str) -> windows::core::Result<()> {
    use windows::core::HSTRING;
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
    let doc = XmlDocument::new()?;
    doc.LoadXml(&HSTRING::from(xml))?;
    let toast = ToastNotification::CreateToastNotification(&doc)?;
    let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(AUMID))?;
    notifier.Show(&toast)
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Parse the two settings: `(enabled, default lead in minutes)`.
pub fn parse_settings(enabled: Option<&str>, lead: Option<&str>) -> (bool, i64) {
    let enabled = enabled.map(str::trim) != Some("0");
    let lead = lead
        .and_then(|v| v.trim().parse::<i64>().ok())
        .filter(|v| (0..=MAX_LEAD_MIN).contains(v))
        .unwrap_or(DEFAULT_LEAD_MIN);
    (enabled, lead)
}

/// Start the one alert loop. Safe to call again: only the first call starts it.
pub fn start(app: AppHandle) {
    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let db = app.state::<AppState>().db.clone();
    tauri::async_runtime::spawn(async move {
        let mut fired: HashSet<FiredKey> = match db
            .call(|conn| {
                store::prune_fired_alerts(conn, now_unix() - KEEP_KEYS)?;
                store::fired_alerts(conn)
            })
            .await
        {
            Ok(keys) => keys.into_iter().collect(),
            Err(e) => {
                tracing::warn!(error = %e, "calendar alerts: cannot read fired keys");
                HashSet::new()
            }
        };
        let mut last_prune = now_unix();
        let mut tick = tokio::time::interval(TICK);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            if let Err(e) = run_once(&app, &db, &mut fired).await {
                tracing::warn!(error = %e, "calendar alerts tick failed");
            }
            let now = now_unix();
            if now - last_prune >= 3600 {
                last_prune = now;
                fired.retain(|k| k.2 >= now - KEEP_KEYS);
                let _ = db
                    .call(move |conn| store::prune_fired_alerts(conn, now - KEEP_KEYS))
                    .await;
            }
        }
    });
}

async fn run_once(app: &AppHandle, db: &Db, fired: &mut HashSet<FiredKey>) -> Result<()> {
    let now = now_unix();
    let (enabled, lead, events) = db
        .read("fork_cal_alerts", move |conn| {
            let (enabled, lead) = parse_settings(
                queries::get_setting(conn, KEY_ENABLED)?.as_deref(),
                queries::get_setting(conn, KEY_LEAD)?.as_deref(),
            );
            let events = if enabled {
                store::alert_candidates(conn, now - LATE_GRACE, now + MAX_LEAD_MIN * 60)?
            } else {
                vec![]
            };
            Ok((enabled, lead, events))
        })
        .await?;
    if !enabled {
        return Ok(());
    }
    let due = due_alerts(now, &events, fired, lead);
    if due.is_empty() {
        return Ok(());
    }
    let keys: Vec<FiredKey> = due.iter().flat_map(|a| a.keys.iter().cloned()).collect();
    // Record first: a crash after the toast must not show it again.
    let to_store = keys.clone();
    db.call(move |conn| store::record_fired_alerts(conn, &to_store, now))
        .await?;
    fired.extend(keys);
    for a in due.into_iter().filter(shows) {
        let _ = app.emit(
            EVT_ALERT,
            AlertPayload {
                event_id: a.event_id,
                account_id: a.account_id.clone(),
                title: a.title.clone(),
                start_ts: a.start_ts,
                lead_min: a.lead_min,
                join_url: a.join_url.clone(),
            },
        );
        let clock = chrono::DateTime::from_timestamp(a.start_ts, 0)
            .map(|d| d.with_timezone(&chrono::Local).format("%H:%M").to_string())
            .unwrap_or_default();
        let xml = toast_xml(
            &a.title,
            &body_line(&clock, a.start_ts, now, a.place.as_deref()),
            a.join_url.as_deref(),
        );
        // WinRT toast objects are !Send: build and show on a blocking thread.
        let _ = tokio::task::spawn_blocking(move || {
            if let Err(e) = show_toast(&xml) {
                tracing::warn!(error = %e, "failed to show meeting alert");
            }
        })
        .await;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::options::{EventOptions, Reminder, Reminders};
    use super::*;

    const NOW: i64 = 1_000_000;

    fn ev(id: i64, start: i64) -> EventRow {
        EventRow {
            id,
            calendar_id: 1,
            google_id: format!("g{id}"),
            status: "confirmed".into(),
            summary: format!("Meeting {id}"),
            start_ts: start,
            end_ts: start + 1800,
            ..Default::default()
        }
    }

    fn with(e: EventRow) -> Vec<(String, EventRow)> {
        vec![("a1".into(), e)]
    }

    fn none() -> HashSet<FiredKey> {
        HashSet::new()
    }

    #[test]
    fn default_lead_fires_once_then_again_at_the_start() {
        let e = ev(1, NOW + 60);
        let due = due_alerts(NOW, &with(e.clone()), &none(), 1);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].lead_min, 1);
        assert_eq!(due[0].keys, vec![("a1".into(), "g1".into(), NOW + 60, 1)]);
        // Not yet due two minutes out.
        assert!(due_alerts(NOW - 61, &with(e.clone()), &none(), 1).is_empty());
        // Refire suppression: the same key never alerts twice.
        let fired: HashSet<FiredKey> = due[0].keys.iter().cloned().collect();
        assert!(due_alerts(NOW + 30, &with(e.clone()), &fired, 1).is_empty());
        // Lead 0 at the start.
        let at_start = due_alerts(NOW + 60, &with(e), &fired, 1);
        assert_eq!(at_start.len(), 1);
        assert_eq!(at_start[0].lead_min, 0);
        assert!(shows(&at_start[0]));
    }

    #[test]
    fn declined_cancelled_all_day_and_non_meetings_never_alert() {
        let mut declined = ev(1, NOW);
        declined.self_response = Some("declined".into());
        let mut cancelled = ev(2, NOW);
        cancelled.status = "cancelled".into();
        let mut all_day = ev(3, NOW);
        all_day.all_day = true;
        let mut ooo = ev(4, NOW);
        ooo.event_type = Some("outOfOffice".into());
        let mut wl = ev(5, NOW);
        wl.event_type = Some("workingLocation".into());
        let mut focus = ev(6, NOW);
        focus.event_type = Some("focusTime".into());
        let mut local = ev(7, NOW);
        local.google_id = "local-abc".into();
        local.local_only = true;
        let mut ok = ev(8, NOW);
        ok.self_response = Some("tentative".into());
        ok.event_type = Some("default".into());
        let events: Vec<(String, EventRow)> =
            [declined, cancelled, all_day, ooo, wl, focus, local, ok]
                .into_iter()
                .map(|e| ("a1".to_string(), e))
                .collect();
        let due = due_alerts(NOW, &events, &none(), 1);
        assert_eq!(due.iter().map(|a| a.event_id).collect::<Vec<_>>(), [8]);
    }

    #[test]
    fn reminder_overrides_replace_the_default_lead() {
        let mut e = ev(1, NOW + 10 * 60);
        e.options = EventOptions {
            reminders: Some(Reminders {
                use_default: false,
                overrides: vec![
                    Reminder {
                        method: "popup".into(),
                        minutes: 10,
                    },
                    Reminder {
                        method: "email".into(),
                        minutes: 30,
                    },
                    Reminder {
                        method: "popup".into(),
                        minutes: 2,
                    },
                ],
            }),
            ..Default::default()
        };
        assert_eq!(leads(&e, 1), [10, 2]);
        let due = due_alerts(NOW, &with(e.clone()), &none(), 1);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].lead_min, 10);
        // No popup override at all: the event asked for no alert.
        e.options
            .reminders
            .as_mut()
            .unwrap()
            .overrides
            .retain(|o| o.method == "email");
        assert!(leads(&e, 1).is_empty());
        assert!(due_alerts(NOW + 600, &with(e.clone()), &none(), 1).is_empty());
        // useDefault: the setting's lead plus the start.
        e.options.reminders = Some(Reminders {
            use_default: true,
            overrides: vec![],
        });
        assert_eq!(leads(&e, 5), [5, 0]);
        assert_eq!(leads(&e, 0), [0]);
    }

    #[test]
    fn a_late_start_shows_one_toast_and_settles_every_passed_lead() {
        let e = ev(1, NOW + 30);
        let due = due_alerts(NOW, &with(e), &none(), 5);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].lead_min, 5);
        let mut e = ev(2, NOW);
        e.options.reminders = Some(Reminders {
            use_default: false,
            overrides: vec![
                Reminder {
                    method: "popup".into(),
                    minutes: 30,
                },
                Reminder {
                    method: "popup".into(),
                    minutes: 10,
                },
            ],
        });
        let due = due_alerts(NOW - 5 * 60, &with(e), &none(), 1);
        assert_eq!(due.len(), 1, "one toast, not a burst");
        assert_eq!(due[0].lead_min, 10);
        assert_eq!(due[0].keys.len(), 2);
    }

    #[test]
    fn an_event_that_started_five_minutes_ago_is_skipped() {
        assert!(due_alerts(NOW, &with(ev(1, NOW - 5 * 60)), &none(), 1).is_empty());
        // Inside the two-minute grace it still alerts once, as "now".
        let due = due_alerts(NOW, &with(ev(2, NOW - 90)), &none(), 1);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].lead_min, 0);
    }

    #[test]
    fn a_moved_meeting_alerts_again_at_its_new_time() {
        let e = ev(1, NOW + 60);
        let fired: HashSet<FiredKey> = due_alerts(NOW, &with(e.clone()), &none(), 1)
            .into_iter()
            .flat_map(|a| a.keys)
            .collect();
        let moved = EventRow {
            start_ts: NOW + 600,
            ..e
        };
        let due = due_alerts(NOW + 540, &with(moved), &fired, 1);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].start_ts, NOW + 600);
    }

    #[test]
    fn one_meeting_on_two_accounts_alerts_once() {
        let mut a = ev(1, NOW + 60);
        a.ical_uid = Some("uid-1@example.test".into());
        let mut b = ev(2, NOW + 60);
        b.ical_uid = Some("uid-1@example.test".into());
        let events = vec![("a1".to_string(), a.clone()), ("a2".to_string(), b.clone())];
        let due = due_alerts(NOW, &events, &none(), 1);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].keys.len(), 2, "both accounts' keys are settled");
        // A twin that appears after the first one fired stays quiet.
        let fired: HashSet<FiredKey> = [("a1".to_string(), "g1".to_string(), NOW + 60, 1)].into();
        let late = due_alerts(NOW + 10, &events, &fired, 1);
        assert!(late.iter().all(|a| !shows(a)), "{late:?}");
        assert_eq!(late.iter().map(|a| a.keys.len()).sum::<usize>(), 1);
        // Different start: two separate meetings.
        b.start_ts = NOW + 30;
        let due = due_alerts(NOW, &[("a1".into(), a), ("a2".into(), b)], &none(), 1);
        assert_eq!(due.len(), 2);
    }

    #[test]
    fn join_link_prefers_meet_then_known_hosts() {
        let mut e = ev(1, NOW);
        e.hangout_link = Some("https://meet.google.com/abc-defg-hij".into());
        e.location = Some("https://us02web.zoom.us/j/1".into());
        assert_eq!(
            join_url(&e).as_deref(),
            Some("https://meet.google.com/abc-defg-hij")
        );
        e.hangout_link = None;
        assert_eq!(join_url(&e).as_deref(), Some("https://us02web.zoom.us/j/1"));
        e.location = Some("Room 2".into());
        e.description = Some(
            r#"Agenda <a href="https://example.test/doc">doc</a>. Join: <a href="https://teams.microsoft.com/l/meetup-join/x">here</a>"#
                .into(),
        );
        assert_eq!(
            join_url(&e).as_deref(),
            Some("https://teams.microsoft.com/l/meetup-join/x")
        );
        e.description = Some("http://zoom.us/j/1 and https://notzoom.us/j/2".into());
        assert_eq!(join_url(&e), None);
    }

    #[test]
    fn toast_text_and_xml() {
        assert_eq!(
            body_line("14:30", NOW + 290, NOW, Some("Room 2")),
            "14:30 · in 5 min · Room 2"
        );
        assert_eq!(body_line("14:30", NOW + 30, NOW, None), "14:30 · in 1 min");
        assert_eq!(
            body_line("14:30", NOW, NOW, Some("Google Meet")),
            "14:30 · now · Google Meet"
        );
        let mut e = ev(1, NOW);
        e.hangout_link = Some("https://meet.google.com/x".into());
        assert_eq!(alert_for("a1", &e, 0).place.as_deref(), Some("Google Meet"));
        e.summary = "  ".into();
        assert_eq!(alert_for("a1", &e, 0).title, "(No title)");
        let xml = toast_xml(
            "Q&A <board>",
            "14:30 · now",
            Some("https://zoom.us/j/1?pwd=a&b=c"),
        );
        assert!(xml.contains("<text>Q&amp;A &lt;board&gt;</text>"));
        assert!(xml.contains(r#"launch="skim://open""#));
        assert!(xml.contains(
            r#"<action content="Join" activationType="protocol" arguments="https://zoom.us/j/1?pwd=a&amp;b=c"/>"#
        ));
        assert!(!toast_xml("T", "B", None).contains("<actions>"));
    }

    #[test]
    fn settings_default_on_with_a_one_minute_lead() {
        assert_eq!(parse_settings(None, None), (true, 1));
        assert_eq!(parse_settings(Some("1"), Some("5")), (true, 5));
        assert_eq!(parse_settings(Some("0"), Some("5")), (false, 5));
        assert_eq!(parse_settings(Some("1"), Some("soon")), (true, 1));
        assert_eq!(parse_settings(Some("1"), Some("-3")), (true, 1));
    }

    #[test]
    fn fired_keys_survive_a_restart_and_prune_by_event_start() {
        let db = crate::db::Db::open_in_memory().unwrap();
        db.with(|conn| {
            let keys = vec![
                ("a1".to_string(), "g1".to_string(), NOW, 1),
                ("a1".to_string(), "g1".to_string(), NOW, 0),
                ("a1".to_string(), "old".to_string(), NOW - 3 * 86_400, 1),
            ];
            store::record_fired_alerts(conn, &keys, NOW)?;
            store::record_fired_alerts(conn, &keys[..1], NOW + 5)?;
            assert_eq!(store::fired_alerts(conn)?.len(), 3, "a repeat is ignored");
            assert_eq!(store::prune_fired_alerts(conn, NOW - KEEP_KEYS)?, 1);
            let left: HashSet<FiredKey> = store::fired_alerts(conn)?.into_iter().collect();
            assert!(left.contains(&keys[0]) && left.contains(&keys[1]));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn candidates_are_timed_live_events_on_selected_calendars() {
        let db = crate::db::Db::open_in_memory().unwrap();
        db.with(|conn| {
            let cal = |gid: &str, selected: bool| super::super::model::CalendarRow {
                id: 0,
                account_id: "a1".into(),
                google_id: gid.into(),
                summary: gid.into(),
                color: None,
                is_primary: gid == "primary",
                selected,
                access_role: "owner".into(),
            };
            store::upsert_calendars(conn, "a1", &[cal("primary", true), cal("team", false)])?;
            let cals = store::list_calendars(conn, "a1")?;
            let (p, t) = (cals[0].id, cals[1].id);
            let mk = |c: i64, gid: &str, start: i64| EventRow {
                calendar_id: c,
                google_id: gid.into(),
                status: "confirmed".into(),
                start_ts: start,
                end_ts: start + 600,
                ..Default::default()
            };
            let mut all_day = mk(p, "allday", NOW);
            all_day.all_day = true;
            store::replace_window(
                conn,
                p,
                0,
                NOW * 2,
                &[
                    mk(p, "soon", NOW + 60),
                    mk(p, "far", NOW + 99_999_999),
                    all_day,
                ],
            )?;
            store::replace_window(conn, t, 0, NOW * 2, &[mk(t, "hidden", NOW + 60)])?;
            let got = store::alert_candidates(conn, NOW - LATE_GRACE, NOW + 3600)?;
            assert_eq!(got.len(), 1);
            assert_eq!(got[0].0, "a1");
            assert_eq!(got[0].1.google_id, "soon");
            Ok(())
        })
        .unwrap();
    }
}
