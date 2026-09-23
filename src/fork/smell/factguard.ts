// Fork (6.5.4): the fact guard, TS mirror of `fork::smell::facts` / `guard`
// in src-tauri/src/fork/smell.rs. Both sides must give the same answer: the
// Rust side filters what the model returned, this side re-checks a option
// before it is offered (and lets the mock demo filter its fixture the same
// way). Deliberately no regex: the two implementations are the same token
// walk so they cannot drift on a regex dialect.
//
// A fact is any token that carries a digit (R140,000 · 14:00 · 2026-09-23 · 10%),
// a URL, an email address, or a capitalised word that is not the sentence's
// first word and not "I". An option passes when every fact of the original
// appears in it verbatim.

const LEAD = new Set(['(', '"', "'", "[", "“", "‘"]);
const TRAIL = new Set([".", ",", ";", ":", "!", "?", ")", '"', "'", "]", "”", "’"]);

function trimPunct(tok: string): string {
  let a = 0;
  let b = tok.length;
  while (a < b && LEAD.has(tok[a])) a++;
  while (b > a && TRAIL.has(tok[b - 1])) b--;
  return tok.slice(a, b);
}

function isUpper(ch: string): boolean {
  return ch !== ch.toLowerCase() && ch === ch.toUpperCase();
}
function isAlpha(ch: string): boolean {
  return ch.toLowerCase() !== ch.toUpperCase();
}

/** The facts a sentence carries. `first` says whether to exempt the first
 *  token's capital (grammar, not a name) — true for the original sentence,
 *  false for a candidate, so a name moved to the front still counts. */
export function facts(sentence: string, first = true): Set<string> {
  const out = new Set<string>();
  const toks = sentence.split(/\s+/).filter(Boolean);
  toks.forEach((raw, i) => {
    const t = trimPunct(raw);
    if (!t) return;
    const lower = t.toLowerCase();
    if ([...t].some((c) => c >= "0" && c <= "9")) out.add(t);
    else if (lower.startsWith("http://") || lower.startsWith("https://") || lower.startsWith("www.")) out.add(t);
    else if (t.includes("@") && t.includes(".")) out.add(t);
    else if (t !== "I" && isUpper(t[0]) && isAlpha(t[0]) && (i > 0 || !first)) out.add(t);
  });
  return out;
}

/** True when `option` keeps every fact of `original`. */
export function guard(original: string, option: string): boolean {
  const have = facts(option, false);
  for (const f of facts(original, true)) if (!have.has(f)) return false;
  return true;
}

/** The facts `option` lost, for a message. */
export function missing(original: string, option: string): string[] {
  const have = facts(option, false);
  return [...facts(original, true)].filter((f) => !have.has(f));
}
