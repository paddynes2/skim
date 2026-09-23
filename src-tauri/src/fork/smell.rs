//! Fork (6.5.4): `fork_smell_rewrite`, the one-shot "✦ Rewrite" behind an
//! AI-tell underline, and the fact guard that filters what the model returns.
//!
//! The scanner itself runs in the webview (`src/fork/smell/scan.ts`): its
//! rules use lookbehinds the `regex` crate cannot compile. This module only
//! does what needs the provider key: one request through the same
//! `ai_context` / `anthropic::stream` / `openai_compat::stream` path every AI
//! command uses, asking for three rewrites of one sentence, then the guard.
//!
//! The fact guard is a pure function (`guard`), mirrored token for token in
//! `src/fork/smell/factguard.ts`: a rewrite that drops or changes a number,
//! date, amount, URL, email address or capitalised name of the original is
//! never shown. Both sides are token walks with no regex so they cannot drift
//! on a dialect.

use crate::commands::ai::{ai_context, AiContext, AiEvent};
use crate::db::queries;
use crate::error::Result;
use crate::state::AppState;
use serde::Serialize;
use std::collections::BTreeSet;
use tauri::ipc::Channel;
use tauri::State;

const MAX_TOKENS: u32 = 600;
const OPTIONS: usize = 3;

/// What the popover gets back: the options that kept every fact, and how
/// many the guard threw away (shown as "n rewrite(s) changed a fact").
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RewriteResult {
    pub options: Vec<String>,
    pub rejected: u32,
}

/// The house drafting rules, the way Patrick writes them (docs/NO-SMELL.md in
/// OS, compressed): the position first, no hedges, no contrast frames, no
/// dashes, plain words, short.
const HOUSE_RULES: &str = "\
Rewrite ONE sentence of an email so it reads as the writer's own plain English.
Rules, all binding:
- Say the position first. No wind-up, no announcement of what you are about to say.
- No hedges or disclaimers (no 'I think this might', no 'subject to', no 'proposed').
- No contrast frames: no 'not X but Y', no 'rather than', no 'it's not about X, it's about Y'.
- No em dashes and no en dashes. Use a comma, a colon or a full stop.
- Plain words. No business jargon, no AI vocabulary (delve, robust, leverage, tapestry, underscore).
- Keep it short: at most the length of the original.
- Keep every fact exactly as written: every number, date, time, amount, currency, URL, email address and name must appear unchanged. Do not add facts.
- Keep the meaning and the register of the context sentences.
Return ONLY a JSON array of exactly 3 strings, each a complete replacement for the sentence, nothing else.";

fn system_prompt(style_profile: Option<&str>) -> String {
    match style_profile.map(str::trim).filter(|s| !s.is_empty()) {
        Some(p) => format!("{HOUSE_RULES}\n\nThe writer's own style, from their sent mail:\n{p}"),
        None => HOUSE_RULES.to_string(),
    }
}

fn user_prompt(sentence: &str, before: &str, after: &str, reason: &str) -> String {
    let mut s = String::new();
    s.push_str("Why it was flagged: ");
    s.push_str(reason.trim());
    s.push('\n');
    if !before.trim().is_empty() {
        s.push_str("Sentence before (context only, do not rewrite): ");
        s.push_str(before.trim());
        s.push('\n');
    }
    s.push_str("SENTENCE TO REWRITE: ");
    s.push_str(sentence.trim());
    s.push('\n');
    if !after.trim().is_empty() {
        s.push_str("Sentence after (context only, do not rewrite): ");
        s.push_str(after.trim());
        s.push('\n');
    }
    s
}

// ---- the fact guard (pure; mirrored in factguard.ts) --------------------------

fn trim_punct(tok: &str) -> &str {
    const LEAD: &[char] = &['(', '"', '\'', '[', '\u{201C}', '\u{2018}'];
    const TRAIL: &[char] = &[
        '.', ',', ';', ':', '!', '?', ')', '"', '\'', ']', '\u{201D}', '\u{2019}',
    ];
    tok.trim_start_matches(LEAD).trim_end_matches(TRAIL)
}

/// The facts a sentence carries: any token with a digit (R140,000 · 14:00 ·
/// 2026-09-23 · 10%), a URL, an email address, or a capitalised word that is
/// not "I". `first` exempts the first token's capital (grammar, not a name):
/// true for the original, false for a candidate so a name moved to the front
/// still counts.
pub fn facts(sentence: &str, first: bool) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for (i, raw) in sentence.split_whitespace().enumerate() {
        let t = trim_punct(raw);
        if t.is_empty() {
            continue;
        }
        let lower = t.to_lowercase();
        let head = t.chars().next().unwrap_or(' ');
        let number = t.chars().any(|c| c.is_ascii_digit());
        let url = lower.starts_with("http://")
            || lower.starts_with("https://")
            || lower.starts_with("www.");
        let email = t.contains('@') && t.contains('.');
        let name = t != "I" && head.is_uppercase() && head.is_alphabetic() && (i > 0 || !first);
        if number || url || email || name {
            out.insert(t.to_string());
        }
    }
    out
}

/// True when `option` keeps every fact of `original`.
pub fn guard(original: &str, option: &str) -> bool {
    let have = facts(option, false);
    facts(original, true).iter().all(|f| have.contains(f))
}

/// The model's raw answer as a list of options: the first JSON array of
/// strings in it (models wrap arrays in prose or fences), or, failing that,
/// its non-empty numbered / plain lines. Never more than `OPTIONS`.
pub fn parse_options(raw: &str) -> Vec<String> {
    let trimmed = raw.trim();
    if let (Some(a), Some(b)) = (trimmed.find('['), trimmed.rfind(']')) {
        if a < b {
            if let Ok(serde_json::Value::Array(items)) =
                serde_json::from_str::<serde_json::Value>(&trimmed[a..=b])
            {
                let out: Vec<String> = items
                    .into_iter()
                    .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
                    .filter(|s| !s.is_empty())
                    .take(OPTIONS)
                    .collect();
                if !out.is_empty() {
                    return out;
                }
            }
        }
    }
    trimmed
        .lines()
        .map(|l| {
            l.trim()
                .trim_start_matches(|c: char| {
                    c.is_ascii_digit() || c == '.' || c == ')' || c == '-'
                })
                .trim()
                .trim_matches('"')
                .to_string()
        })
        .filter(|l| !l.is_empty() && !l.starts_with("```"))
        .take(OPTIONS)
        .collect()
}

/// Everything after the network: parse, guard, count. Pure, so the test can
/// drive it with a canned model answer and no key.
pub fn rewrite_from(raw: &str, sentence: &str) -> RewriteResult {
    let mut options = Vec::new();
    let mut rejected = 0u32;
    for o in parse_options(raw) {
        if o.trim() == sentence.trim() {
            rejected += 1; // a "rewrite" that changed nothing
        } else if guard(sentence, &o) {
            options.push(o);
        } else {
            rejected += 1;
        }
    }
    RewriteResult { options, rejected }
}

// ---- the request ---------------------------------------------------------------

/// One-shot completion over the configured provider; deltas are forwarded to
/// the channel for liveness only, the collected text is the answer.
async fn complete(
    ctx: &AiContext,
    system: String,
    user: String,
    channel: &Channel<AiEvent>,
) -> Result<String> {
    let mut out = String::new();
    let mut on_delta = |d: &str| {
        out.push_str(d);
        let _ = channel.send(AiEvent::Delta {
            text: d.to_string(),
        });
    };
    let mut reported = false;
    let mut on_reasoning = || {
        if !reported {
            reported = true;
            let _ = channel.send(AiEvent::Reasoning);
        }
    };
    let messages = vec![crate::ai::ChatMessage {
        role: "user",
        content: user,
    }];
    match &ctx.endpoint {
        None => {
            let request = crate::ai::anthropic::Request {
                model: ctx.model.clone(),
                system,
                messages,
                media: Vec::new(),
                max_tokens: MAX_TOKENS,
            };
            crate::ai::anthropic::stream(&ctx.key, &request, &mut on_delta, &mut on_reasoning)
                .await?;
        }
        Some(ep) => {
            let request = crate::ai::openai_compat::Request {
                model: ctx.model.clone(),
                system,
                messages,
                max_tokens: MAX_TOKENS,
            };
            crate::ai::openai_compat::stream(
                ep,
                &ctx.key,
                &request,
                &mut on_delta,
                &mut on_reasoning,
            )
            .await?;
        }
    }
    Ok(out)
}

/// Three fact-guarded rewrites of one flagged sentence. `before` / `after`
/// are one sentence of context either side (empty at the edges); `reason` is
/// the rule's suggestion text. Reads `ai_style_profile` (from
/// `ai_analyze_style`) when the user has one. Never touches the draft: the
/// popover applies the option Patrick clicks.
#[tauri::command]
pub async fn fork_smell_rewrite(
    state: State<'_, AppState>,
    request_id: String,
    sentence: String,
    before: String,
    after: String,
    reason: String,
    channel: Channel<AiEvent>,
) -> Result<RewriteResult> {
    let _ = request_id; // one-shot and short: not registered for `ai_cancel`
    let ctx = ai_context(&state.db).await?;
    let profile = state
        .db
        .call(|conn| queries::get_setting(conn, "ai_style_profile"))
        .await?;
    let system = system_prompt(profile.as_deref());
    let user = user_prompt(&sentence, &before, &after, &reason);
    let raw = complete(&ctx, system, user, &channel).await?;
    let result = rewrite_from(&raw, &sentence);
    let _ = channel.send(AiEvent::Done {
        citations: Vec::new(),
    });
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn facts_keep_numbers_dates_urls_emails_and_names() {
        let f = facts(
            "Ali sends R140,000 on Tuesday 14:00 to jim@acme.com, see https://acme.com/x.",
            true,
        );
        for want in [
            "R140,000",
            "Tuesday",
            "14:00",
            "jim@acme.com",
            "https://acme.com/x",
        ] {
            assert!(f.contains(want), "missing {want} in {f:?}");
        }
        assert!(!f.contains("Ali"), "the first word's capital is grammar");
        assert!(
            facts("Ali sends it.", false).contains("Ali"),
            "a candidate keeps its first word"
        );
        assert!(!facts("So I said yes.", true).contains("I"));
    }

    #[test]
    fn guard_rejects_a_changed_amount_or_time() {
        let orig = "The fee is R140,000, due Tuesday 14:00.";
        assert!(guard(orig, "R140,000 is due Tuesday 14:00."));
        assert!(
            !guard(orig, "The fee is R14,000, due Tuesday 14:00."),
            "amount changed"
        );
        assert!(
            !guard(orig, "The fee is R140,000, due Tuesday 15:00."),
            "time changed"
        );
        assert!(
            !guard(orig, "The fee is R140,000, due Wednesday 14:00."),
            "day changed"
        );
        assert!(
            !guard(orig, "The fee is due Tuesday 14:00."),
            "amount dropped"
        );
    }

    #[test]
    fn guard_keeps_names_urls_and_emails() {
        let orig = "Send the pack to Jim Carter at jim@acme.com or via https://acme.com/upload.";
        assert!(guard(
            orig,
            "Jim Carter gets the pack at jim@acme.com or https://acme.com/upload."
        ));
        assert!(!guard(
            orig,
            "Send the pack to Jim at jim@acme.com or via https://acme.com/upload."
        ));
        assert!(!guard(
            orig,
            "Send the pack to Jim Carter at jim@acme.co or via https://acme.com/upload."
        ));
    }

    #[test]
    fn parse_options_reads_a_json_array_in_prose_or_fences() {
        let raw = "Here you go:\n```json\n[\"One.\", \"Two.\", \"Three.\", \"Four.\"]\n```";
        assert_eq!(parse_options(raw), vec!["One.", "Two.", "Three."]);
        assert_eq!(parse_options("[\"Only one.\"]"), vec!["Only one."]);
    }

    #[test]
    fn parse_options_falls_back_to_lines() {
        let raw = "1. First option.\n2) Second option.\n- Third option.\n";
        assert_eq!(
            parse_options(raw),
            vec!["First option.", "Second option.", "Third option."]
        );
        assert!(parse_options("   ").is_empty());
    }

    #[test]
    fn rewrite_from_filters_and_counts() {
        let sentence = "I wanted to reach out about the R140,000 invoice due Tuesday 14:00.";
        let raw = r#"["The R140,000 invoice is due Tuesday 14:00.",
                      "The R14,000 invoice is due Tuesday 14:00.",
                      "I wanted to reach out about the R140,000 invoice due Tuesday 14:00."]"#;
        let r = rewrite_from(raw, sentence);
        assert_eq!(
            r.options,
            vec!["The R140,000 invoice is due Tuesday 14:00."]
        );
        assert_eq!(r.rejected, 2, "one changed a fact, one changed nothing");
    }

    #[test]
    fn prompts_carry_the_rules_the_profile_and_the_context() {
        let s = system_prompt(Some("  short sentences, no sign-off  "));
        assert!(s.contains("No em dashes"));
        assert!(s.contains("short sentences, no sign-off"));
        assert_eq!(system_prompt(Some("  ")), HOUSE_RULES);
        let u = user_prompt("The one.", "", "After it.", "Stock closer.");
        assert!(u.contains("SENTENCE TO REWRITE: The one."));
        assert!(u.contains("Sentence after (context only, do not rewrite): After it."));
        assert!(!u.contains("Sentence before"));
        assert!(u.starts_with("Why it was flagged: Stock closer."));
    }
}
