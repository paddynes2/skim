//! Quote and signature folding (PLAN.md Phase 5).
//!
//! Two jobs, both pure functions over strings so the sanitizer and the body
//! command can call them without any state:
//!
//! * `plain_fold_point`: where a plain-text body stops being the sender's own
//!   words. Same rules as `mail::parse::strip_quoted` (attribution line,
//!   `-----Original Message-----`, the `-- ` signature delimiter, a run of
//!   `>` lines), but it returns the line index instead of stripping, and it
//!   refuses to fold when nothing would be left visible above the fold.
//! * `has_fold`: whether sanitized HTML carries a fold marker with visible
//!   content above it. The markers are the ones `sanitize_email_html` lets
//!   through (`skim-quote`, `skim-sig`, Outlook's `divRplyFwdMsg` /
//!   `appendonsend`, `blockquote type="cite"`) and the one `text_to_html`
//!   emits (`skim-quote`).

/// Attribution line that introduces a quoted reply ("On ..., X wrote:" and
/// the RU/DE/FR forms `strip_quoted` knows).
fn is_attribution(trimmed: &str) -> bool {
    (trimmed.starts_with("On ") && trimmed.ends_with("wrote:"))
        || trimmed.ends_with("пишет:")
        || trimmed.ends_with("schrieb:")
        || trimmed.ends_with("a écrit :")
}

/// A line that ends the sender's own words outright.
fn is_hard_marker(line: &str) -> bool {
    let trimmed = line.trim_start();
    is_attribution(trimmed) || trimmed.starts_with("-----Original Message-----") || trimmed == "-- "
}

/// Index of the first line of `lines` that starts the foldable tail, or
/// `None` when the body should be shown whole.
///
/// The tail starts at the first hard marker, or at the first `>` line when
/// every line from there to the next hard marker (or the end) is quoted or
/// blank. An interleaved reply (quote, answer, quote, answer) therefore never
/// folds the answers away. When no non-blank line precedes the fold point
/// (bottom-posted reply, forward with nothing said) there is no fold: hiding
/// everything would show an empty message.
pub fn plain_fold_point(lines: &[&str]) -> Option<usize> {
    let hard = lines.iter().position(|l| is_hard_marker(l));
    let end = hard.unwrap_or(lines.len());
    let quote = lines[..end]
        .iter()
        .position(|l| l.trim_start().starts_with('>'))
        .filter(|&q| {
            lines[q..end]
                .iter()
                .all(|l| l.trim().is_empty() || l.trim_start().starts_with('>'))
        });
    let fold = match (hard, quote) {
        (Some(h), Some(q)) => h.min(q),
        (Some(h), None) => h,
        (None, Some(q)) => q,
        (None, None) => return None,
    };
    if lines[..fold].iter().all(|l| l.trim().is_empty()) {
        return None;
    }
    Some(fold)
}

/// Markers that the viewer's folded stylesheet hides, as they appear in the
/// sanitized output (ammonia serialises attributes as `name="value"`).
const MARKERS: &[&str] = &[
    "class=\"skim-quote\"",
    "class=\"skim-sig\"",
    "id=\"divRplyFwdMsg\"",
    "id=\"appendonsend\"",
    "type=\"cite\"",
];

/// True when sanitized HTML has a fold marker with visible text before it.
pub fn has_fold(html: &str) -> bool {
    let Some(first) = MARKERS.iter().filter_map(|m| html.find(m)).min() else {
        return false;
    };
    // Back up to the start of the tag carrying the marker: the tag's own
    // name is not content.
    let tag_start = html[..first].rfind('<').unwrap_or(first);
    has_visible_text(&html[..tag_start])
}

/// Whether HTML holds any text outside tags that is not whitespace or a
/// bare `&nbsp;`. `<img>` counts as content: a logo above a quote is visible.
fn has_visible_text(html: &str) -> bool {
    let mut in_tag = false;
    let mut text = String::new();
    for c in html.chars() {
        match (in_tag, c) {
            (false, '<') => in_tag = true,
            (true, '>') => in_tag = false,
            (false, c) => text.push(c),
            _ => {}
        }
    }
    let text = text.replace("&nbsp;", " ").replace('\u{a0}', " ");
    !text.trim().is_empty() || html.contains("<img")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fold(text: &str) -> Option<usize> {
        let lines: Vec<&str> = text.lines().collect();
        plain_fold_point(&lines)
    }

    #[test]
    fn folds_at_attribution_line() {
        assert_eq!(fold("Thanks!\n\nOn Mon, Jane wrote:\n> hi"), Some(2));
    }

    #[test]
    fn folds_at_original_message_and_signature() {
        assert_eq!(fold("Ok\n-----Original Message-----\nFrom: x"), Some(1));
        assert_eq!(fold("Ok\n-- \nPat"), Some(1));
        // `--` without the trailing space is not the delimiter.
        assert_eq!(fold("Ok\n--\nPat"), None);
    }

    #[test]
    fn folds_at_a_trailing_run_of_quote_lines() {
        assert_eq!(fold("Yes.\n\n> a\n> b\n\n> c"), Some(2));
    }

    #[test]
    fn interleaved_reply_does_not_fold_the_answers() {
        assert_eq!(fold("> q1\nanswer 1\n> q2\nanswer 2"), None);
        // ...but a signature after the interleaving still folds.
        assert_eq!(fold("> q1\nanswer 1\n> q2\nanswer 2\n-- \nsig"), Some(4));
    }

    #[test]
    fn bottom_posted_and_forward_do_not_fold() {
        assert_eq!(fold("On Mon, Jane wrote:\n> hi\n\nmy reply"), None);
        assert_eq!(fold("\n\n-----Original Message-----\nFrom: x"), None);
        assert_eq!(fold("> only\n> quotes"), None);
        assert_eq!(fold(""), None);
    }

    #[test]
    fn earliest_marker_wins() {
        assert_eq!(fold("Hi\n> q\n\n-- \nsig"), Some(1));
        assert_eq!(fold("Hi\n-- \nsig\n> q"), Some(1));
    }

    #[test]
    fn has_fold_needs_a_marker_and_content_above() {
        assert!(has_fold(
            "<div>Thanks</div><div class=\"skim-quote\">old</div>"
        ));
        assert!(has_fold("<p>Hi</p><div class=\"skim-sig\">-- </div>"));
        assert!(has_fold("<p>FYI</p><div id=\"divRplyFwdMsg\">From:</div>"));
        assert!(has_fold("<p>FYI</p><div id=\"appendonsend\"></div>"));
        assert!(has_fold(
            "<p>Sure</p><blockquote type=\"cite\">q</blockquote>"
        ));
        assert!(has_fold(
            "<img src=\"x\"><div class=\"skim-quote\">old</div>"
        ));
        assert!(!has_fold("<div class=\"skim-quote\">old</div>"));
        assert!(!has_fold(
            "<div>&nbsp; </div><div class=\"skim-quote\">old</div>"
        ));
        assert!(!has_fold("<p>no markers here</p>"));
        assert!(!has_fold(""));
    }
}
