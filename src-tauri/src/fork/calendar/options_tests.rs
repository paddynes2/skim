use super::{commands, gapi, model, options::EventOptions, store};
use serde_json::json;

#[test]
fn advanced_fields_survive_sync_storage_and_partial_edits() {
    let db = crate::db::Db::open_in_memory().unwrap();
    db.with(|conn| {
        conn.execute("INSERT INTO accounts (id,email,provider,imap_host,smtp_host,created_at) VALUES ('a','me@example.test','gmail','imap','smtp',0)", [])?;
        store::upsert_calendars(conn, "a", &[model::CalendarRow { id: 0, account_id: "a".into(), google_id: "primary".into(), summary: "Work".into(), color: None, is_primary: true, selected: true, access_role: "owner".into() }])?;
        let cal = store::list_calendars(conn, "a")?[0].id;
        let value = json!({"id":"event","summary":"Planning","start":{"dateTime":"2026-10-07T11:00:00Z","timeZone":"Africa/Johannesburg"},"end":{"dateTime":"2026-10-07T12:00:00Z"},"recurrence":["RRULE:FREQ=WEEKLY;BYDAY=WE"],"visibility":"private","colorId":"4","transparency":"transparent","guestsCanModify":true,"guestsCanInviteOthers":false,"reminders":{"useDefault":false,"overrides":[{"method":"email","minutes":45}]},"attendees":[{"email":"guest@example.test","responseStatus":"accepted","optional":true}]});
        let original = model::event_from_json(cal, &value).unwrap();
        let id = store::insert_local_event(conn, &original)?;
        let mut cached = store::get_event(conn, id)?.unwrap();
        assert_eq!(cached.options, original.options);
        let patch: gapi::EventInput = serde_json::from_value(json!({"summary":"Renamed","options":{"colorId":"9","guestsCanModify":false}})).unwrap();
        commands::apply_input(&mut cached, &patch).unwrap();
        store::update_local_event(conn, &cached)?;
        let saved = store::get_event(conn, id)?.unwrap();
        assert_eq!(saved.options.color_id.as_deref(), Some("9"));
        assert_eq!(saved.options.guests_can_modify, Some(false));
        assert_eq!(saved.options.reminders, original.options.reminders);
        assert_eq!(saved.options.recurrence, original.options.recurrence);
        assert_eq!(saved.attendees_json, original.attendees_json);
        assert_eq!(saved.transparency.as_deref(), Some("transparent"));
        let body = gapi::event_body(&patch, None);
        assert_eq!(body, json!({"summary":"Renamed","colorId":"9","guestsCanModify":false}));
        Ok(())
    }).unwrap();
}
#[test]
fn clears_and_optional_guests_retain_server_responses() {
    let input: gapi::EventInput = serde_json::from_value(json!({"start_ts":1000,"end_ts":2000,"attendees":["guest@example.test"],"optional_attendees":["guest@example.test"],"options":{"recurrence":[],"reminders":{"useDefault":false,"overrides":[]},"colorId":""}})).unwrap();
    let existing =
        json!([{"email":"guest@example.test","responseStatus":"accepted","displayName":"Guest"}]);
    let body = gapi::event_body(&input, Some(&existing));
    assert_eq!(body["attendees"][0]["responseStatus"], "accepted");
    assert_eq!(body["attendees"][0]["optional"], true);
    assert_eq!(body["recurrence"], json!([]));
    assert_eq!(
        body["reminders"],
        json!({"useDefault":false,"overrides":[]})
    );
    assert!(body["colorId"].is_null());
    let local = commands::local_row_from_input(1, &input).unwrap();
    assert!(local.attendees_json.unwrap().contains("\"optional\":true"));
}
#[test]
fn invalid_advanced_fields_cannot_enter_the_queue() {
    for value in [
        json!({"visibility":"secret"}),
        json!({"transparency":"maybe"}),
        json!({"colorId":"12"}),
        json!({"recurrence":["DTSTART:20261007"]}),
        json!({"reminders":{"useDefault":false,"overrides":[{"method":"sms","minutes":10}]}}),
        json!({"reminders":{"useDefault":false,"overrides":[{"method":"popup","minutes":-1}]}}),
    ] {
        let options: EventOptions = serde_json::from_value(value).unwrap();
        assert!(options.validate().is_err());
        let input = gapi::EventInput {
            start_ts: Some(1000),
            options,
            ..Default::default()
        };
        assert!(commands::local_row_from_input(1, &input).is_err());
    }
}

#[test]
fn attachment_links_roundtrip_and_keep_remote_files_on_addition() {
    use super::options::{merge_attachment_additions, EventOptions};
    let mut options: EventOptions = serde_json::from_value(serde_json::json!({"attachments":[{"fileUrl":"https://docs.google.com/document/d/notes/edit","title":"Meeting notes","fileId":"notes"}]})).unwrap();
    options.validate().unwrap();
    let encoded = serde_json::to_string(&options).unwrap();
    let restored: EventOptions = serde_json::from_str(&encoded).unwrap();
    assert_eq!(restored, options);
    let remote = serde_json::json!([{"fileUrl":"https://drive.google.com/file/d/other/view","title":"Added elsewhere"}]);
    let requested = serde_json::to_value(options.attachments.as_ref().unwrap()).unwrap();
    let combined = merge_attachment_additions(&remote, &requested).unwrap();
    assert_eq!(combined.as_array().unwrap().len(), 2);
    assert_eq!(combined[0]["title"], "Added elsewhere");
    assert_eq!(
        merge_attachment_additions(&combined, &requested).unwrap(),
        combined
    );
    options.attachments.as_mut().unwrap()[0].file_url = "http://example.com/file".into();
    assert!(options.validate().is_err());
    options.attachments.as_mut().unwrap()[0].file_url = "javascript:alert(1)".into();
    assert!(options.validate().is_err());
    options.attachments.as_mut().unwrap()[0].file_url =
        "https://user:secret@example.com/file".into();
    assert!(options.validate().is_err());
    let unchanged = EventOptions::default();
    options.merge(&unchanged);
    assert_eq!(options.attachments.as_ref().unwrap().len(), 1);
}
