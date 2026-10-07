use super::{gapi::SendUpdates, model, split, store};
use crate::error::{Result, SkimError};
use serde_json::{json, Value};
use std::collections::HashMap;

fn master(rule: &str) -> Value {
    json!({"id":"series", "etag":"version1", "summary":"Standup",
        "start":{"dateTime":"2026-10-01T08:00:00Z","timeZone":"Europe/London"},
        "end":{"dateTime":"2026-10-01T08:30:00Z","timeZone":"Europe/London"},
        "recurrence":[rule], "extendedProperties":{"private":{"existing":"keep"}},
        "attendees":[{"email":"guest@example.test","responseStatus":"accepted"}]})
}

fn instance(day: u32) -> Value {
    let stamp = format!("2026-10-{day:02}T08:00:00Z");
    json!({"id":format!("instance{day}"),"recurringEventId":"series",
        "originalStartTime":{"dateTime":stamp}, "start":{"dateTime":stamp},
        "end":{"dateTime":format!("2026-10-{day:02}T08:30:00Z")}})
}

#[test]
fn following_count_uses_original_times_and_keeps_the_remaining_count() {
    let master = master("RRULE:FREQ=DAILY;COUNT=5");
    let mut moved = instance(3);
    moved["start"]["dateTime"] = json!("2026-10-04T09:00:00Z");
    let mut occurrences: Vec<Value> = (1..=5).map(instance).collect();
    occurrences[0]["status"] = json!("cancelled");
    occurrences[2] = moved.clone();
    let plan = split::plan(
        "calendar",
        &master,
        &moved,
        &json!({"summary":"Updated"}),
        &occurrences,
        "abc0123",
    )
    .unwrap();
    assert_eq!(
        plan.master_patch["recurrence"],
        json!(["RRULE:FREQ=DAILY;UNTIL=20261003T075959Z"])
    );
    assert_eq!(
        plan.replacement["recurrence"],
        json!(["RRULE:FREQ=DAILY;COUNT=3"])
    );
    assert_eq!(
        plan.replacement["start"]["dateTime"],
        "2026-10-04T09:00:00Z"
    );
    assert_eq!(plan.replacement["start"]["timeZone"], "Europe/London");
    assert_eq!(plan.replacement["summary"], "Updated");
    assert_eq!(
        plan.master_patch["extendedProperties"]["private"]["existing"],
        "keep"
    );
    assert!(plan.replacement["attendees"][0]
        .get("responseStatus")
        .is_none());
    assert!(split::plan(
        "calendar",
        &master,
        &moved,
        &json!({}),
        &occurrences[..4],
        "abc"
    )
    .is_err());
}

#[test]
fn following_preserves_until_and_handles_all_day_and_first_occurrence() {
    let master = master("RRULE:FREQ=DAILY;UNTIL=20261031T080000Z");
    let plan = split::plan("c", &master, &instance(3), &json!({}), &[], "abc").unwrap();
    assert_eq!(plan.replacement["recurrence"], master["recurrence"]);
    let first = split::plan(
        "c",
        &master,
        &instance(1),
        &json!({"summary":"Changed"}),
        &[],
        "abc",
    )
    .unwrap();
    assert!(first.replacement_id.is_none());
    assert_eq!(first.master_patch["summary"], "Changed");
    assert!(first.master_patch.get("recurrence").is_none());
    let mut all = master.clone();
    all["start"] = json!({"date":"2026-10-01"});
    all["end"] = json!({"date":"2026-10-02"});
    let selected = json!({"recurringEventId":"series", "originalStartTime":{"date":"2026-10-03"},
        "start":{"date":"2026-10-03"},"end":{"date":"2026-10-04"}});
    let plan = split::plan("c", &all, &selected, &json!({}), &[], "abc").unwrap();
    assert_eq!(
        plan.master_patch["recurrence"],
        json!(["RRULE:FREQ=DAILY;UNTIL=20261002"])
    );
    all["recurrence"] = json!(["RRULE:FREQ=DAILY", "EXDATE:20261004"]);
    assert!(split::plan("c", &all, &selected, &json!({}), &[], "abc").is_err());
}

struct FakeTransport {
    events: HashMap<String, Value>,
    inserts: usize,
    patches: usize,
    deletes: usize,
    fail_insert: bool,
    lose_insert_reply: bool,
    fail_patch: bool,
    lose_patch_reply: bool,
}
impl FakeTransport {
    fn new(master: Value) -> Self {
        Self {
            events: HashMap::from([("series".into(), master)]),
            inserts: 0,
            patches: 0,
            deletes: 0,
            fail_insert: false,
            lose_insert_reply: false,
            fail_patch: false,
            lose_patch_reply: false,
        }
    }
}
fn network() -> SkimError {
    SkimError::other("network", "Test connection failure")
}
impl split::Transport for FakeTransport {
    async fn get(&mut self, id: &str) -> Result<Option<Value>> {
        Ok(self.events.get(id).cloned())
    }
    async fn insert(&mut self, body: &Value, _send: SendUpdates) -> Result<Value> {
        if self.fail_insert {
            return Err(network());
        }
        self.inserts += 1;
        let mut stored = body.clone();
        stored["etag"] = json!("replacement1");
        self.events
            .insert(body["id"].as_str().unwrap().into(), stored);
        if self.lose_insert_reply {
            return Err(network());
        }
        Ok(body.clone())
    }
    async fn patch(
        &mut self,
        id: &str,
        body: &Value,
        etag: &str,
        _send: SendUpdates,
    ) -> Result<Value> {
        if self.fail_patch {
            return Err(network());
        }
        let stored = self.events.get_mut(id).unwrap();
        assert_eq!(stored["etag"], etag);
        self.patches += 1;
        for (key, value) in body.as_object().unwrap() {
            stored[key] = value.clone();
        }
        stored["etag"] = json!("version2");
        if self.lose_patch_reply {
            return Err(network());
        }
        Ok(stored.clone())
    }
    async fn delete(&mut self, id: &str, etag: &str, _send: SendUpdates) -> Result<()> {
        assert_eq!(self.events[id]["etag"], etag);
        self.deletes += 1;
        self.events.remove(id);
        Ok(())
    }
}

#[tokio::test]
async fn split_retries_preserve_original_and_do_not_repeat_completed_writes() {
    let original = master("RRULE:FREQ=DAILY");
    let plan = split::plan("c", &original, &instance(3), &json!({}), &[], "abc0123").unwrap();
    let mut remote = FakeTransport::new(original.clone());
    remote.fail_insert = true;
    assert!(split::execute(&mut remote, &plan, SendUpdates::All)
        .await
        .is_err());
    assert_eq!(remote.events["series"], original);
    assert_eq!((remote.inserts, remote.patches), (0, 0));
    remote.fail_insert = false;
    remote.lose_insert_reply = true;
    remote.fail_patch = true;
    assert!(split::execute(&mut remote, &plan, SendUpdates::All)
        .await
        .is_err());
    assert_eq!(remote.events["series"], original);
    assert_eq!((remote.inserts, remote.patches), (1, 0));
    remote.fail_patch = false;
    remote.lose_patch_reply = true;
    assert!(split::execute(&mut remote, &plan, SendUpdates::All)
        .await
        .is_err());
    assert_eq!((remote.inserts, remote.patches), (1, 1));
    split::execute(&mut remote, &plan, SendUpdates::All)
        .await
        .unwrap();
    split::execute(&mut remote, &plan, SendUpdates::All)
        .await
        .unwrap();
    assert_eq!(
        (remote.inserts, remote.patches),
        (1, 1),
        "Each notification write occurs only once"
    );
}

#[tokio::test]
async fn split_refuses_external_edits_and_foreign_replacement_ids() {
    let original = master("RRULE:FREQ=WEEKLY;BYDAY=TH");
    let plan = split::plan("c", &original, &instance(8), &json!({}), &[], "abc0123").unwrap();
    let mut remote = FakeTransport::new(original);
    remote.events.get_mut("series").unwrap()["etag"] = json!("external-edit");
    assert_eq!(
        split::execute(&mut remote, &plan, SendUpdates::All)
            .await
            .unwrap_err()
            .code(),
        "gcal_conflict"
    );
    assert_eq!((remote.inserts, remote.patches), (0, 0));
    remote.events.get_mut("series").unwrap()["etag"] = json!("version1");
    remote
        .events
        .insert("abc0123".into(), json!({"id":"abc0123"}));
    assert!(split::execute(&mut remote, &plan, SendUpdates::All)
        .await
        .is_err());
    assert_eq!((remote.inserts, remote.patches), (0, 0));
}

#[tokio::test]
async fn discard_removes_only_the_marked_replacement_and_keeps_completed_splits() {
    let original = master("RRULE:FREQ=DAILY");
    let plan = split::plan("c", &original, &instance(3), &json!({}), &[], "abc0123").unwrap();
    let mut remote = FakeTransport::new(original.clone());
    remote.fail_patch = true;
    assert!(split::execute(&mut remote, &plan, SendUpdates::None)
        .await
        .is_err());
    split::discard(&mut remote, &plan, SendUpdates::None)
        .await
        .unwrap();
    assert_eq!(remote.events["series"], original);
    assert!(!remote.events.contains_key("abc0123"));
    assert_eq!(remote.deletes, 1);
    split::discard(&mut remote, &plan, SendUpdates::None)
        .await
        .unwrap();
    assert_eq!(
        remote.deletes, 1,
        "A lost discard reply must not delete another event"
    );
    remote
        .events
        .insert("abc0123".into(), json!({"id":"abc0123","etag":"foreign"}));
    assert!(split::discard(&mut remote, &plan, SendUpdates::None)
        .await
        .is_err());
    assert_eq!(remote.deletes, 1);
    remote.events.remove("abc0123");
    remote.fail_patch = false;
    split::execute(&mut remote, &plan, SendUpdates::None)
        .await
        .unwrap();
    split::discard(&mut remote, &plan, SendUpdates::None)
        .await
        .unwrap();
    assert!(remote.events.contains_key("abc0123"));
    assert_eq!(remote.deletes, 1, "A completed split must remain intact");
}

#[tokio::test]
async fn discard_refuses_to_remove_replacement_when_original_schedule_changed() {
    let original = master("RRULE:FREQ=DAILY");
    let plan = split::plan("c", &original, &instance(3), &json!({}), &[], "abc0123").unwrap();
    let mut remote = FakeTransport::new(original);
    remote.fail_patch = true;
    assert!(split::execute(&mut remote, &plan, SendUpdates::None)
        .await
        .is_err());
    remote.events.get_mut("series").unwrap()["recurrence"] =
        plan.master_patch["recurrence"].clone();
    assert!(split::discard(&mut remote, &plan, SendUpdates::None)
        .await
        .is_err());
    assert!(remote.events.contains_key("abc0123"));
    assert_eq!(remote.deletes, 0);
}

#[test]
fn discard_claim_prevents_retry_and_recovers_after_restart() {
    let db = crate::db::Db::open_in_memory().unwrap();
    db.with(|conn| {
        let id = store::queue_op(conn, "a", "split", &json!({"event_id":1}))?;
        store::finish_op(conn, id, false)?;
        assert!(store::sync_state(conn, "a")?.events[0].can_discard);
        assert!(store::claim_failed_split(conn, "b", 1)?.is_none());
        assert_eq!(store::claim_failed_split(conn, "a", 1)?.unwrap().0, id);
        assert_eq!(store::retry_failed(conn, "a", None)?, 0);
        assert!(store::claim_failed_split(conn, "a", 1)?.is_none());
        store::recover_interrupted_discards(conn, "a")?;
        assert!(store::sync_state(conn, "a")?.events[0].can_discard);
        assert_eq!(store::claim_failed_split(conn, "a", 1)?.unwrap().0, id);
        store::finish_op(conn, id, true)?;
        assert_eq!(store::retry_failed(conn, "a", None)?, 0);
        Ok(())
    })
    .unwrap();
}

#[test]
fn failed_local_edits_survive_refresh_and_retry_never_replays_completed_ops() {
    let db = crate::db::Db::open_in_memory().unwrap();
    db.with(|conn| {
        store::upsert_calendars(
            conn,
            "a",
            &[model::CalendarRow {
                id: 0,
                account_id: "a".into(),
                google_id: "primary".into(),
                summary: "Calendar".into(),
                color: None,
                is_primary: true,
                selected: true,
                access_role: "owner".into(),
            }],
        )?;
        let cid = store::list_calendars(conn, "a")?[0].id;
        let server = model::EventRow {
            google_id: "event1".into(),
            calendar_id: cid,
            summary: "Original".into(),
            start_ts: 100,
            end_ts: 200,
            ..Default::default()
        };
        store::replace_window(conn, cid, 0, 1000, std::slice::from_ref(&server))?;
        let mut local = store::events_between(conn, Some("a"), 0, 1000)?.remove(0);
        local.summary = "Edited offline".into();
        store::update_local_event(conn, &local)?;
        let done = store::queue_op(conn, "a", "patch", &json!({"event_id":local.id}))?;
        store::finish_op(conn, done, true)?;
        let failed = store::queue_op(conn, "a", "patch", &json!({"event_id":local.id}))?;
        store::record_op_error(conn, failed, "Permission denied")?;
        store::finish_op(conn, failed, false)?;
        store::replace_window(conn, cid, 0, 1000, &[server])?;
        assert_eq!(
            store::get_event(conn, local.id)?.unwrap().summary,
            "Edited offline"
        );
        store::replace_window(conn, cid, 0, 1000, &[])?;
        assert!(store::get_event(conn, local.id)?.is_some());
        let state = store::sync_state(conn, "a")?;
        assert_eq!(state.failed_ops, 1);
        assert_eq!(
            state.events[0].message.as_deref(),
            Some("Permission denied")
        );
        assert_eq!(store::retry_failed(conn, "other", None)?, 0);
        assert_eq!(store::retry_failed(conn, "a", Some(local.id))?, 1);
        assert_eq!(store::retry_failed(conn, "a", None)?, 0);
        assert_eq!(store::pending_op_count(conn, "a")?, 1);
        assert_eq!(store::next_op(conn, "a")?.unwrap().0, failed);
        store::finish_op(conn, failed, true)?;
        assert_eq!(store::retry_failed(conn, "a", None)?, 0);
        assert!(store::next_op(conn, "a")?.is_none());
        Ok(())
    })
    .unwrap();
}

#[test]
fn failed_split_blocks_only_the_same_series_until_explicit_retry() {
    let db = crate::db::Db::open_in_memory().unwrap();
    db.with(|conn| {
        let failed = store::queue_op(
            conn,
            "a",
            "split",
            &json!({"event_id":1,"_series_id":"series1"}),
        )?;
        store::finish_op(conn, failed, false)?;
        store::queue_op(
            conn,
            "a",
            "patch",
            &json!({"event_id":2,"_series_id":"series1"}),
        )?;
        assert!(store::next_op(conn, "a")?.is_none());
        let other = store::queue_op(
            conn,
            "a",
            "patch",
            &json!({"event_id":3,"_series_id":"series2"}),
        )?;
        assert_eq!(store::next_op(conn, "a")?.unwrap().0, other);
        store::finish_op(conn, other, true)?;
        assert_eq!(store::retry_failed(conn, "a", Some(1))?, 1);
        assert_eq!(store::next_op(conn, "a")?.unwrap().0, failed);
        Ok(())
    })
    .unwrap();
}
