//! Which Google account a meeting link opens in (D101).
//!
//! A browser signed in to several Google accounts opens a bare Meet link in
//! its default account. Google's Meet and Calendar pages honour
//! `?authuser=<address>`, so every Join and "Open in Google Calendar" carries
//! the own address the meeting belongs to. The frontend twin is
//! `joinAs` / `withAccount` in `src/fork/calendar/presentation.ts`.

/// The own address a meeting belongs to: the guest Google marks as this
/// calendar (`self`), else the calendar's own address, else the account that
/// synced it. Only an address with a mailbox in Skim counts.
pub fn join_as(
    attendees_json: Option<&str>,
    calendar_id: &str,
    account_email: Option<&str>,
    own: &[String],
) -> Option<String> {
    let mine = |e: &str| own.iter().any(|o| o.eq_ignore_ascii_case(e));
    let self_email = attendees_json
        .and_then(|j| serde_json::from_str::<serde_json::Value>(j).ok())
        .and_then(|v| {
            v.as_array()?.iter().find_map(|a| {
                a.get("self")?
                    .as_bool()?
                    .then(|| a.get("email")?.as_str().map(str::to_string))
                    .flatten()
            })
        });
    if let Some(e) = self_email.filter(|e| mine(e)) {
        return Some(e.to_ascii_lowercase());
    }
    if mine(calendar_id) {
        return Some(calendar_id.to_ascii_lowercase());
    }
    account_email.map(str::to_ascii_lowercase)
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
    fn someone_elses_calendar_falls_back_to_the_syncing_account() {
        let a = r#"[{"email":"partner@other.test","self":true}]"#;
        assert_eq!(
            join_as(Some(a), "partner@other.test", Some("Me@Work.test"), &own()).as_deref(),
            Some("me@work.test")
        );
        assert_eq!(join_as(Some("not json"), "x", None, &own()), None);
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
