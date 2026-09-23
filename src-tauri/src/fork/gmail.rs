//! Gmail detection by host, not only by the onboarding `provider` label.
//!
//! A Google Workspace mailbox added with an app password comes in as
//! `provider = 'custom'` with `imap_host = imap.gmail.com`. Upstream keys every
//! Gmail-specific behaviour (archive = drop the INBOX label, no Sent mirror,
//! web calendar) on the label alone, so such an account archived into a brand
//! new "Archive" label and got every sent mail twice. Everything Gmail-shaped
//! asks here instead.

use crate::db::models::Account;

pub const GMAIL_IMAP_HOST: &str = "imap.gmail.com";

pub fn is_gmail_host(imap_host: &str) -> bool {
    imap_host.trim().eq_ignore_ascii_case(GMAIL_IMAP_HOST)
}

pub fn is_gmail(account: &Account) -> bool {
    account.provider == "gmail" || is_gmail_host(&account.imap_host)
}

/// The provider name the rest of the app should reason about: `gmail` for any
/// account that talks to Gmail's IMAP, else the stored label.
pub fn effective_provider<'a>(provider: &'a str, imap_host: &str) -> &'a str {
    if provider == "gmail" || is_gmail_host(imap_host) {
        "gmail"
    } else {
        provider
    }
}

/// Google runs the mail for this domain: its MX records point at Google.
pub fn is_google_mx<S: AsRef<str>>(hosts: &[S]) -> bool {
    hosts.iter().any(|h| {
        let h = h.as_ref().trim_end_matches('.').to_ascii_lowercase();
        h == "aspmx.l.google.com" || h.ends_with(".googlemail.com") || h.ends_with(".google.com")
    })
}

/// Whether archiving `uids` out of this folder means "remove the label"
/// (`\Deleted` + `UID EXPUNGE` inside the selected folder; Gmail keeps the
/// message in All Mail) rather than a move to an archive folder.
///
/// True for Gmail from INBOX and from any user label (`role` is `None`).
/// Role folders other than INBOX (Sent, Trash, Spam, Starred, All Mail) keep
/// upstream's move; the UI does not offer archive there (PLAN.md 1.2).
pub fn archive_by_expunge(account: &Account, imap_name: &str, role: Option<&str>) -> bool {
    if !is_gmail(account) {
        return false;
    }
    imap_name.eq_ignore_ascii_case("INBOX") || role.is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(provider: &str, host: &str) -> Account {
        Account {
            id: "a".into(),
            email: "p@example.com".into(),
            imap_user: None,
            display_name: None,
            provider: provider.into(),
            imap_host: host.into(),
            imap_port: 993,
            smtp_host: "smtp.example.com".into(),
            smtp_port: 587,
            smtp_security: "starttls".into(),
            auth_kind: "password".into(),
            signature: None,
        }
    }

    #[test]
    fn gmail_is_detected_by_label_or_host() {
        assert!(is_gmail(&account("gmail", "imap.gmail.com")));
        assert!(is_gmail(&account("custom", "imap.gmail.com")));
        assert!(is_gmail(&account("custom", "IMAP.GMAIL.COM")));
        assert!(is_gmail(&account("gmail", "imap.example.com")));
        assert!(!is_gmail(&account("custom", "imap.fastmail.com")));
        assert!(!is_gmail(&account("custom", "imap.gmail.com.evil.example")));
    }

    #[test]
    fn effective_provider_rewrites_only_gmail_hosts() {
        assert_eq!(effective_provider("custom", "imap.gmail.com"), "gmail");
        assert_eq!(effective_provider("custom", "imap.fastmail.com"), "custom");
        assert_eq!(
            effective_provider("microsoft", "outlook.office365.com"),
            "microsoft"
        );
    }

    #[test]
    fn google_mx_hosts() {
        assert!(is_google_mx(&["ASPMX.L.GOOGLE.COM."]));
        assert!(is_google_mx(&["alt1.aspmx.l.google.com", "mx.example.com"]));
        assert!(is_google_mx(&["aspmx2.googlemail.com"]));
        assert!(!is_google_mx(&["mail.protection.outlook.com"]));
        assert!(!is_google_mx(&["google.com.evil.example"]));
        assert!(!is_google_mx::<&str>(&[]));
    }

    #[test]
    fn archive_path_per_folder() {
        let gmail = account("custom", "imap.gmail.com");
        // INBOX and labels: expunge (drop the label).
        assert!(archive_by_expunge(&gmail, "INBOX", Some("inbox")));
        assert!(archive_by_expunge(&gmail, "inbox", Some("inbox")));
        assert!(archive_by_expunge(&gmail, "Clients", None));
        // Role folders: not the expunge path (and not offered by the UI).
        assert!(!archive_by_expunge(
            &gmail,
            "[Gmail]/Sent Mail",
            Some("sent")
        ));
        assert!(!archive_by_expunge(&gmail, "[Gmail]/Trash", Some("trash")));
        assert!(!archive_by_expunge(
            &gmail,
            "[Gmail]/Starred",
            Some("starred")
        ));
        // Not Gmail: never.
        let other = account("custom", "imap.fastmail.com");
        assert!(!archive_by_expunge(&other, "INBOX", Some("inbox")));
        assert!(!archive_by_expunge(&other, "Clients", None));
    }
}
