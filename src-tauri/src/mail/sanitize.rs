//! HTML email sanitization. Runs in Rust so unsanitized markup never crosses
//! the IPC boundary; the frontend additionally renders the result inside a
//! sandboxed iframe with a strict CSP.

use std::borrow::Cow;
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

pub struct SanitizedHtml {
    pub html: String,
    pub blocked_images: usize,
}

/// CSS properties allowed in inline `style` attributes.
const STYLE_ALLOWED: &[&str] = &[
    "color",
    "background",
    "background-color",
    "font-family",
    "font-size",
    "font-weight",
    "font-style",
    "text-align",
    "text-decoration",
    "line-height",
    "letter-spacing",
    "margin",
    "margin-top",
    "margin-bottom",
    "margin-left",
    "margin-right",
    "padding",
    "padding-top",
    "padding-bottom",
    "padding-left",
    "padding-right",
    "border",
    "border-top",
    "border-bottom",
    "border-left",
    "border-right",
    "border-radius",
    "border-collapse",
    "border-spacing",
    "width",
    "max-width",
    "min-width",
    "height",
    "display",
    "vertical-align",
    "white-space",
    "word-break",
    "overflow-wrap",
];

fn filter_style(value: &str) -> Option<String> {
    let mut kept = Vec::new();
    for decl in value.split(';') {
        let Some((prop, val)) = decl.split_once(':') else {
            continue;
        };
        let prop = prop.trim().to_lowercase();
        if !STYLE_ALLOWED.contains(&prop.as_str()) {
            continue;
        }
        // Drop an `!important` flag but KEEP the declaration. Bulk-mail
        // preheaders hide themselves with `display:none !important`; discarding
        // the whole declaration (as we used to) un-hid them, turning a screenful
        // of zero-width spacer glyphs into a giant blank scroll area. Inline
        // `!important` only maxes out specificity — which inline styles already
        // do — so it can't reach or override our iframe's protective stylesheet.
        let val = match val.find('!') {
            Some(i) => val[..i].trim(),
            None => val.trim(),
        };
        if val.is_empty() {
            continue;
        }
        let val_lower = val.to_lowercase();
        // No external fetches or layout escapes through CSS.
        if val_lower.contains("url(")
            || val_lower.contains("expression(")
            || val_lower.contains("fixed")
            || val_lower.contains("absolute")
        {
            continue;
        }
        // A viewport- or percentage-relative height lets email content size
        // itself to the iframe viewport, which our auto-height measurement then
        // grows — a feedback loop that ratchets the frame up to its cap. Email
        // bodies never legitimately need one (images use height:auto).
        if prop == "height"
            && (val_lower.contains('%')
                || val_lower.contains("vh")
                || val_lower.contains("vw")
                || val_lower.contains("vmin")
                || val_lower.contains("vmax"))
        {
            continue;
        }
        kept.push(format!("{prop}:{val}"));
    }
    if kept.is_empty() {
        None
    } else {
        Some(kept.join(";"))
    }
}

/// Resolve a `cid:` reference to the URL served by the `skim-cid` protocol.
/// (On Windows, Tauri custom protocols are exposed as `http://<scheme>.localhost/`.)
pub fn cid_url(message_id: i64, content_id: &str) -> String {
    format!(
        "http://skim-cid.localhost/{message_id}/{}",
        urlencode(content_id)
    )
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Sanitize an HTML email body.
///
/// * `message_id` — used to resolve `cid:` inline images.
/// * `allow_remote_images` — when false, http(s) image sources are emptied
///   and counted so the UI can offer a "show images" bar.
pub fn sanitize_email_html(
    html: &str,
    message_id: i64,
    allow_remote_images: bool,
) -> SanitizedHtml {
    let blocked = Arc::new(AtomicUsize::new(0));
    let blocked_in_filter = blocked.clone();

    let tags: HashSet<&str> = [
        "a",
        "abbr",
        "b",
        "blockquote",
        "br",
        "caption",
        "center",
        "code",
        "col",
        "colgroup",
        "dd",
        "div",
        "dl",
        "dt",
        "em",
        "font",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "hr",
        "i",
        "img",
        "li",
        "ol",
        "p",
        "pre",
        "q",
        "s",
        "small",
        "span",
        "strike",
        "strong",
        "sub",
        "sup",
        "table",
        "tbody",
        "td",
        "tfoot",
        "th",
        "thead",
        "tr",
        "u",
        "ul",
    ]
    .into();

    let mut builder = ammonia::Builder::empty();
    builder
        .tags(tags)
        .clean_content_tags(["script", "style", "svg", "math", "head", "title"].into())
        .generic_attributes(["style", "align", "valign", "dir"].into())
        .add_tag_attributes("a", ["href", "title"])
        .add_tag_attributes("img", ["src", "alt", "width", "height", "border"])
        .add_tag_attributes("td", ["colspan", "rowspan", "width", "height", "bgcolor"])
        .add_tag_attributes("th", ["colspan", "rowspan", "width", "height", "bgcolor"])
        .add_tag_attributes(
            "table",
            ["cellpadding", "cellspacing", "border", "width", "bgcolor"],
        )
        .add_tag_attributes("font", ["color", "size", "face"])
        // Fork (5.1): quote / signature markers. The filter below rewrites a
        // known client class to `skim-quote` / `skim-sig` and drops every
        // other class, id and type, so nothing arbitrary survives.
        .add_tag_attributes("div", ["class", "id"])
        .add_tag_attributes("blockquote", ["class", "type"])
        // `cid`/`data` are allowed through so the img filter below can see
        // them; anchors are re-restricted to http/https/mailto in the filter.
        .url_schemes(["http", "https", "mailto", "cid", "data"].into())
        .link_rel(Some("noopener noreferrer"))
        .attribute_filter(move |element, attribute, value| {
            if attribute == "style" {
                return filter_style(value).map(Cow::Owned);
            }
            if attribute == "class" || attribute == "id" || attribute == "type" {
                return fold_marker(element, attribute, value).map(Cow::Borrowed);
            }
            if element == "a" && attribute == "href" {
                let lower = value.trim().to_lowercase();
                if lower.starts_with("http://")
                    || lower.starts_with("https://")
                    || lower.starts_with("mailto:")
                {
                    return Some(Cow::Borrowed(value));
                }
                return None;
            }
            if element == "img" && attribute == "src" {
                let lower = value.trim().to_lowercase();
                if let Some(cid) = lower.strip_prefix("cid:") {
                    return Some(Cow::Owned(cid_url(message_id, cid.trim())));
                }
                if lower.starts_with("data:image/") {
                    return Some(Cow::Owned(value.to_string()));
                }
                if lower.starts_with("http://") || lower.starts_with("https://") {
                    if allow_remote_images {
                        return Some(Cow::Owned(value.to_string()));
                    }
                    blocked_in_filter.fetch_add(1, Ordering::Relaxed);
                    return Some(Cow::Borrowed(""));
                }
                return None;
            }
            Some(Cow::Borrowed(value))
        });

    let html = builder.clean(html).to_string();
    SanitizedHtml {
        html,
        blocked_images: blocked.load(Ordering::Relaxed),
    }
}

/// Fork (5.1): the only `class` / `id` / `type` values that leave the
/// sanitizer, rewritten to the fork's own marker names so the viewer's folding
/// stylesheet has one vocabulary. Everything else is dropped: a class or id is
/// otherwise a hook into the viewer stylesheet that mail must never reach.
fn fold_marker(element: &str, attribute: &str, value: &str) -> Option<&'static str> {
    match (element, attribute) {
        ("div", "class") => {
            let mut out = None;
            for class in value.split_ascii_whitespace() {
                match class {
                    "gmail_quote"
                    | "gmail_quote_container"
                    | "yahoo_quoted"
                    | "moz-cite-prefix"
                    | "protonmail_quote" => return Some("skim-quote"),
                    "gmail_signature" | "moz-signature" => out = Some("skim-sig"),
                    _ => {}
                }
            }
            out
        }
        ("div", "id") => match value {
            "divRplyFwdMsg" => Some("divRplyFwdMsg"),
            "appendonsend" => Some("appendonsend"),
            _ => None,
        },
        ("blockquote", "type") => value.eq_ignore_ascii_case("cite").then_some("cite"),
        _ => None,
    }
}

/// Render a plain-text body as safe HTML: escape, linkify, preserve wrapping.
///
/// Fork (5.2): the quoted tail (attribution line, `-----Original Message-----`,
/// `-- ` signature, trailing `>` lines; see `fork::fold::plain_fold_point`)
/// is wrapped in `<div class="skim-quote">` so the viewer can fold it. A body
/// with nothing above the fold point is rendered whole.
pub fn text_to_html(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    match crate::fork::fold::plain_fold_point(&lines) {
        Some(fold) => {
            let above = lines[..fold].join("\n");
            let below = lines[fold..].join("\n");
            format!(
                "<pre class=\"skim-plain\">{}</pre><div class=\"skim-quote\"><pre class=\"skim-plain\">{}</pre></div>",
                escape_linkify(&above),
                escape_linkify(&below)
            )
        }
        None => format!("<pre class=\"skim-plain\">{}</pre>", escape_linkify(text)),
    }
}

fn escape_linkify(text: &str) -> String {
    let escaped = text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    linkify(&escaped)
}

fn linkify(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find("http") {
        let (before, tail) = rest.split_at(pos);
        out.push_str(before);
        if tail.starts_with("http://") || tail.starts_with("https://") {
            let end = tail
                .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '<')
                .unwrap_or(tail.len());
            let (url, after) = tail.split_at(end);
            let trimmed = url.trim_end_matches(['.', ',', ')', ']', ';']);
            let extra = &url[trimmed.len()..];
            out.push_str(&format!(
                "<a href=\"{trimmed}\" rel=\"noopener noreferrer\">{trimmed}</a>{extra}"
            ));
            rest = after;
        } else {
            out.push_str("http");
            rest = &tail[4..];
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_scripts_and_event_handlers() {
        let s = sanitize_email_html(
            "<p onclick=\"evil()\">hi<script>alert(1)</script></p><style>body{}</style>",
            1,
            true,
        );
        assert!(!s.html.contains("script"));
        assert!(!s.html.contains("onclick"));
        assert!(!s.html.contains("body{}"));
        assert!(s.html.contains("hi"));
    }

    #[test]
    fn blocks_remote_images_and_counts() {
        let s = sanitize_email_html(
            "<img src=\"https://tracker.example/x.png\"><img src=\"data:image/png;base64,AA==\">",
            1,
            false,
        );
        assert_eq!(s.blocked_images, 1);
        assert!(!s.html.contains("tracker.example"));
        assert!(s.html.contains("data:image/png"));
    }

    #[test]
    fn rewrites_cid_references() {
        let s = sanitize_email_html("<img src=\"cid:logo@corp\">", 42, false);
        assert_eq!(s.blocked_images, 0);
        assert!(s.html.contains("http://skim-cid.localhost/42/logo%40corp"));
    }

    #[test]
    fn style_filter_drops_dangerous_values() {
        assert_eq!(
            filter_style("color: red; background: url(https://x); position: absolute"),
            Some("color:red".to_string())
        );
        assert_eq!(filter_style("position: fixed"), None);
    }

    #[test]
    fn style_filter_keeps_declaration_minus_important() {
        // `!important` is stripped, not the whole declaration — otherwise
        // `display:none !important` preheaders un-hide into a giant blank area.
        assert_eq!(
            filter_style("display: none !important"),
            Some("display:none".to_string())
        );
        assert_eq!(
            filter_style("color: red !important"),
            Some("color:red".to_string())
        );
    }

    #[test]
    fn style_filter_rejects_viewport_relative_height() {
        // Viewport/percentage heights feed the iframe auto-height loop.
        assert_eq!(filter_style("height: 100vh"), None);
        assert_eq!(filter_style("height: 100%"), None);
        assert_eq!(
            filter_style("height: 40px"),
            Some("height:40px".to_string())
        );
    }

    #[test]
    fn preheader_stays_hidden() {
        let s = sanitize_email_html(
            "<div style=\"display: none !important; max-width: 0px; overflow: hidden;\">spacer</div>",
            1,
            true,
        );
        assert!(s.html.contains("display:none"));
    }

    #[test]
    fn javascript_links_removed() {
        let s = sanitize_email_html("<a href=\"javascript:alert(1)\">x</a>", 1, true);
        assert!(!s.html.contains("javascript"));
    }

    #[test]
    fn plain_text_is_escaped_and_linkified() {
        let html = text_to_html("see <b> https://example.com/a.");
        assert!(html.contains("&lt;b&gt;"));
        assert!(html.contains("<a href=\"https://example.com/a\""));
        assert!(!html.contains("skim-quote"));
    }

    // Fork (5.1): client quote / signature markers survive as skim-* names.

    #[test]
    fn gmail_quote_and_signature_markers_survive() {
        let s = sanitize_email_html(
            "<div>Hi</div><div class=\"gmail_signature\">Pat</div><div class=\"gmail_quote\">On x wrote:<blockquote class=\"gmail_quote\" type=\"cite\">old</blockquote></div>",
            1,
            true,
        );
        assert!(s.html.contains("<div class=\"skim-sig\">Pat</div>"));
        assert!(s.html.contains("<div class=\"skim-quote\">"));
        assert!(s
            .html
            .contains("<blockquote type=\"cite\">old</blockquote>"));
        assert!(!s.html.contains("gmail"));
    }

    #[test]
    fn gmail_quote_container_marker_survives() {
        let s = sanitize_email_html(
            "<p>Hi</p><div class=\"gmail_quote gmail_quote_container\">old</div>",
            1,
            true,
        );
        assert!(s.html.contains("<div class=\"skim-quote\">old</div>"));
    }

    #[test]
    fn yahoo_proton_and_thunderbird_markers_survive() {
        let s = sanitize_email_html("<div class=\"yahoo_quoted\">old</div>", 1, true);
        assert!(s.html.contains("<div class=\"skim-quote\">old</div>"));
        let s = sanitize_email_html("<div class=\"protonmail_quote\">old</div>", 1, true);
        assert!(s.html.contains("<div class=\"skim-quote\">old</div>"));
        let s = sanitize_email_html(
            "<div class=\"moz-cite-prefix\">On x wrote:</div><div class=\"moz-signature\">-- </div>",
            1,
            true,
        );
        assert!(s
            .html
            .contains("<div class=\"skim-quote\">On x wrote:</div>"));
        assert!(s.html.contains("<div class=\"skim-sig\">-- </div>"));
    }

    #[test]
    fn outlook_reply_header_ids_survive() {
        let s = sanitize_email_html(
            "<div>FYI</div><div id=\"appendonsend\"></div><hr><div id=\"divRplyFwdMsg\" dir=\"ltr\"><b>From:</b> x</div>",
            1,
            true,
        );
        assert!(s.html.contains("<div id=\"appendonsend\">"));
        assert!(s.html.contains("id=\"divRplyFwdMsg\""));
        assert!(s.html.contains("dir=\"ltr\""));
    }

    #[test]
    fn arbitrary_classes_ids_and_types_never_survive() {
        let s = sanitize_email_html(
            "<div class=\"skim-quote\" id=\"skim\">a</div>\
             <div class=\"hero gmail_quotes\" id=\"divRplyFwdMsg2\">b</div>\
             <div class=\"gmail_quote_containerx\">c</div>\
             <blockquote class=\"gmail_quote\" type=\"quote\">d</blockquote>\
             <p class=\"gmail_quote\" id=\"appendonsend\">e</p>\
             <span class=\"gmail_signature\">f</span>",
            1,
            true,
        );
        assert!(!s.html.contains("class="), "{}", s.html);
        assert!(!s.html.contains("id="), "{}", s.html);
        assert!(!s.html.contains("type="), "{}", s.html);
        assert!(s.html.contains("<div>a</div>"));
        assert!(s.html.contains("<blockquote>d</blockquote>"));
    }

    #[test]
    fn a_forged_marker_class_cannot_be_smuggled_through() {
        // Only the rewrite produces `skim-*`; the literal name on the wire
        // is dropped, so mail cannot pre-fold or pre-hide its own content.
        let s = sanitize_email_html("<div class=\"skim-quote\">x</div>", 1, true);
        assert_eq!(s.html, "<div>x</div>");
        let s = sanitize_email_html("<div class=\"skim-sig\">x</div>", 1, true);
        assert_eq!(s.html, "<div>x</div>");
    }

    #[test]
    fn signature_class_does_not_outrank_a_quote_class() {
        let s = sanitize_email_html(
            "<div class=\"gmail_signature gmail_quote\">x</div>",
            1,
            true,
        );
        assert!(s.html.contains("class=\"skim-quote\""));
    }

    // Fork (5.2): plain text folds at the quoted tail.

    #[test]
    fn plain_text_wraps_the_quoted_tail() {
        let html = text_to_html("Thanks, sounds good.\n\nOn Mon, Jane wrote:\n> see <b>");
        assert_eq!(
            html,
            "<pre class=\"skim-plain\">Thanks, sounds good.\n</pre><div class=\"skim-quote\"><pre class=\"skim-plain\">On Mon, Jane wrote:\n&gt; see &lt;b&gt;</pre></div>"
        );
        assert!(crate::fork::fold::has_fold(&html));
    }

    #[test]
    fn plain_text_folds_signature_and_original_message() {
        let html = text_to_html("Hi\n-- \nPat");
        assert!(html.starts_with("<pre class=\"skim-plain\">Hi</pre><div class=\"skim-quote\">"));
        let html = text_to_html("Ok\n-----Original Message-----\nFrom: x");
        assert!(html.contains(
            "<div class=\"skim-quote\"><pre class=\"skim-plain\">-----Original Message-----"
        ));
    }

    #[test]
    fn plain_text_bottom_posted_or_forward_is_not_folded() {
        let html = text_to_html("On Mon, Jane wrote:\n> old\n\nMy reply below.");
        assert!(!html.contains("skim-quote"));
        assert!(!crate::fork::fold::has_fold(&html));
        let html = text_to_html("-----Original Message-----\nFrom: x\nbody");
        assert!(!html.contains("skim-quote"));
    }
}
