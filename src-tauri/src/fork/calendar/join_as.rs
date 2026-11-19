//! Which Google account a meeting link opens in (D101).
//!
//! A browser signed in to several Google accounts opens a bare Meet link in
//! its default account. Google's Meet and Calendar pages honour
//! `?authuser=<address>`, so every Join and "Open in Google Calendar" carries
//! the own address the meeting belongs to. The frontend twin is
//! `joinAs` / `withAccount` in `src/fork/calendar/presentation.ts`.

/// The own address a meeting belongs to: the guest Google marks as this
/// calendar (`self`), else another invited own address (a meeting on someone
/// else's shared calendar), else the calendar's own address, else the account
/// that synced it. Only an address with a mailbox in Skim counts.
pub fn join_as(
    attendees_json: Option<&str>,
    calendar_id: &str,
    account_email: Option<&str>,
    own: &[String],
) -> Option<String> {
    let mine = |e: &str| own.iter().any(|o| o.eq_ignore_ascii_case(e));
    let guests: Vec<serde_json::Value> = attendees_json
        .and_then(|j| serde_json::from_str(j).ok())
        .unwrap_or_default();
    let email = |a: &serde_json::Value| a.get("email")?.as_str().map(str::to_string);
    let self_email = guests
        .iter()
        .find(|a| a.get("self").and_then(|v| v.as_bool()) == Some(true))
        .and_then(email);
    if let Some(e) = self_email.filter(|e| mine(e)) {
        return Some(e.to_ascii_lowercase());
    }
    if let Some(e) = guests.iter().filter_map(email).find(|e| mine(e)) {
        return Some(e.to_ascii_lowercase());
    }
    if mine(calendar_id) {
        return Some(calendar_id.to_ascii_lowercase());
    }
    account_email.map(str::to_ascii_lowercase)
}

/// The own address an invitation answer is given as (D103): the meeting's own
/// address when one is invited or owns the calendar; on a group calendar the
/// syncing account; `None` on another person's calendar that invites no own
/// address (answering there would answer for, or add a guest to, their event).
pub fn answer_as(
    attendees_json: Option<&str>,
    calendar_id: &str,
    account_email: &str,
    own: &[String],
) -> Option<String> {
    let who = join_as(attendees_json, calendar_id, Some(account_email), own)?;
    let person = calendar_id.contains('@')
        && !calendar_id
            .to_ascii_lowercase()
            .ends_with("calendar.google.com");
    let own_calendar = own.iter().any(|o| o.eq_ignore_ascii_case(calendar_id));
    let invited =
        attendees_json.is_some_and(|j| j.to_ascii_lowercase().contains(&format!("\"{who}\"")));
    if person && !own_calendar && !invited {
        return None;
    }
    Some(who)
}

/// The link opened as `email` when it is a Google Meet or Calendar page; any
/// other link unchanged.
pub fn with_account(link: &str, email: Option<&str>) -> String {
    let Some(email) = email.filter(|e| !e.is_empty()) else {
        return link.to_string();
    };
    let Ok(mut u) = url::Url::parse(link) else {
        return link.to_string();
    };
    let host = u.host_str().unwrap_or_default().to_ascii_lowercase();
    let google = host == "meet.google.com"
        || host == "calendar.google.com"
        || (host == "www.google.com" && u.path().starts_with("/calendar"));
    if u.scheme() != "https" || !google {
        return link.to_string();
    }
    let kept: Vec<(String, String)> = u
        .query_pairs()
        .filter(|(k, _)| k != "authuser")
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    u.query_pairs_mut()
        .clear()
        .extend_pairs(kept)
        .append_pair("authuser", email);
    u.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn own() -> Vec<String> {
        vec![
            "me@work.test".into(),
            "me@home.test".into(),
            "me@side.test".into(),
        ]
    }

    #[test]
    fn the_self_guest_wins_when_it_is_an_own_address() {
        let a = r#"[{"email":"boss@other.test","organizer":true},{"email":"Me@Home.test","self":true}]"#;
        assert_eq!(
            join_as(Some(a), "me@home.test", Some("me@work.test"), &own()).as_deref(),
            Some("me@home.test")
        );
    }

    #[test]
    fn a_shared_calendar_of_an_own_address_opens_as_that_address() {
        assert_eq!(
            join_as(None, "me@side.test", Some("me@work.test"), &own()).as_deref(),
            Some("me@side.test")
        );
    }

    #[test]
    fn an_invited_own_address_on_someone_elses_calendar_wins() {
        let a = r#"[{"email":"me@home.test"},{"email":"partner@other.test","self":true}]"#;
        assert_eq!(
            join_as(Some(a), "partner@other.test", Some("me@work.test"), &own()).as_deref(),
            Some("me@home.test")
        );
    }

    #[test]
    fn someone_elses_calendar_falls_back_to_the_syncing_account() {
        let a = r#"[{"email":"partner@other.test","self":true}]"#;
        assert_eq!(
            join_as(Some(a), "partner@other.test", Some("Me@Work.test"), &own()).as_deref(),
            Some("me@work.test")
        );
        assert_eq!(join_as(Some("not json"), "x", None, &own()), None);
    }

    #[test]
    fn an_answer_is_never_given_on_someone_elses_calendar_without_an_invite() {
        let theirs = r#"[{"email":"friend@other.test","self":true}]"#;
        assert_eq!(
            answer_as(Some(theirs), "friend@other.test", "me@work.test", &own()),
            None
        );
        let invited = r#"[{"email":"friend@other.test","self":true},{"email":"me@home.test"}]"#;
        assert_eq!(
            answer_as(Some(invited), "friend@other.test", "me@work.test", &own()).as_deref(),
            Some("me@home.test")
        );
        assert_eq!(
            answer_as(None, "me@home.test", "me@work.test", &own()).as_deref(),
            Some("me@home.test")
        );
        assert_eq!(
            answer_as(
                None,
                "c_1@group.calendar.google.com",
                "me@work.test",
                &own()
            )
            .as_deref(),
            Some("me@work.test")
        );
    }

    #[test]
    fn meet_and_calendar_links_carry_the_account_and_nothing_else_does() {
        assert_eq!(
            with_account("https://meet.google.com/abc-defg-hij", Some("me@home.test")),
            "https://meet.google.com/abc-defg-hij?authuser=me%40home.test"
        );
        assert_eq!(
            with_account(
                "https://meet.google.com/abc-defg-hij?authuser=0&hs=1",
                Some("me@side.test")
            ),
            "https://meet.google.com/abc-defg-hij?hs=1&authuser=me%40side.test"
        );
        assert_eq!(
            with_account(
                "https://www.google.com/calendar/event?eid=xyz",
                Some("me@work.test")
            ),
            "https://www.google.com/calendar/event?eid=xyz&authuser=me%40work.test"
        );
        assert_eq!(
            with_account("https://us02web.zoom.us/j/1", Some("me@work.test")),
            "https://us02web.zoom.us/j/1"
        );
        assert_eq!(
            with_account("https://www.google.com/search?q=x", Some("me@work.test")),
            "https://www.google.com/search?q=x"
        );
        assert_eq!(
            with_account("https://meet.google.com/abc", None),
            "https://meet.google.com/abc"
        );
    }
}
