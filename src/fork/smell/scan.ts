// Fork (6.5.2): the AI-tell scanner that runs in the webview.
//
// The rules are OS's `tools/outreach/slop_scrub.py` (vendored as rules.json by
// scripts/fork/sync-smell-rules.ps1) compiled as JS RegExps: V8 supports the
// lookbehinds those patterns use, Rust's `regex` crate does not, which is why
// this lives in TS. `scripts/fork/smell-parity.mjs` asserts this scanner and the
// Python original agree finding for finding over the same corpus.
//
// Pure: no DOM, no Tauri. `compile()` once, `scan()` per keystroke (debounced by
// the caller). Offsets are UTF-16 code-unit offsets into the text it was given,
// which must be the user's own words only: the caller strips the signature and
// the quoted original (see `ownWords`).

export type Severity = "hard" | "warn" | "info";

/** One row of rules.json, as `slop_scrub.py --export-rules` writes it. */
export interface RuleRow {
  category: string;
  severity: Severity;
  pattern: string;
  flags: string;
  suggestion: string;
  min_hits: number;
}

export interface Threshold {
  warn: number;
  fail: number | null;
  direction: "hi" | "lo";
}

export interface RulesFile {
  os_commit: string | null;
  rules: RuleRow[];
  invisible_chars: Record<string, string>;
  homoglyph_scripts: { cyrillic: string; greek: string; latin: string };
  markdown_bleed: { bold: string; header: string };
  nosmell: {
    contrast: { pattern: string; flags: string };
    disclaimer: { pattern: string; flags: string };
    position: { pattern: string; flags: string };
    signpost: { pattern: string; flags: string };
    thresholds: Record<string, Threshold>;
    min_words_for_low_rules: number;
  };
}

/** A finding with a place in the text. `rule` is the stable id the ignore list
 *  stores (`<category>#<n>`, n = the pattern's ordinal inside its category). */
export interface Span {
  start: number;
  end: number;
  text: string;
  rule: string;
  category: string;
  severity: Severity;
  suggestion: string;
}

/** The document-level numbers the health line reads (no-smell's L2/L3 layer). */
export interface Health {
  words: number;
  hedgesPer1k: number;
  positionsPer1k: number;
  /** The most-repeated contrast frame ("rather than") and how often, if any repeats. */
  contrastRepeat: { phrase: string; count: number } | null;
  /** True when the draft is long enough to judge and states no position. */
  noPosition: boolean;
}

export interface ScanResult {
  spans: Span[];
  health: Health;
}

export interface CompiledRule {
  id: string;
  category: string;
  severity: Severity;
  re: RegExp;
  suggestion: string;
  minHits: number;
}

export interface Compiled {
  rules: CompiledRule[];
  invisible: Map<string, string>;
  cyrillic: RegExp;
  greek: RegExp;
  latin: RegExp;
  mdBold: RegExp;
  mdHeader: RegExp;
  contrast: RegExp;
  disclaimer: RegExp;
  position: RegExp;
  thresholds: Record<string, Threshold>;
  minWordsForLowRules: number;
}

export interface ScanOptions {
  /** Include the info tier (adverb crutches, Wh- openers). Off by default, as in OS. */
  includeInfo?: boolean;
  /** Rule ids (`Span.rule`) the user switched off in Settings. */
  ignoredRules?: ReadonlySet<string>;
  /** Do not flag `{{merge}}` variables (the text IS a template). */
  template?: boolean;
}

/** Patrick's own hard rule for outbound mail, not in OS's slop_scrub (owned
 *  there by broker_draft_commit.voice_check). Every occurrence is a span. */
export const DASH_RULE = "dash#0";
const DASH_SUGGESTION =
  "Em and en dashes are the house hard rule for outbound mail. Use a comma, a colon or a full stop.";
const INVISIBLE_SUGGESTION =
  "Invisible character survives copy-paste and fingerprints the generator. Normalise to plain ASCII whitespace.";
const QUOTE_SUGGESTION = "One dialect per document. Normalise to straight quotes.";
const MD_BOLD_SUGGESTION = "Asterisks ship literally in email, DM and SMS. Remove the markdown.";
const MD_HEAD_SUGGESTION = "A markdown header in plain-text copy is an instant AI flag. Use a sentence.";
const CONTRAST_SUGGESTION =
  "One contrast frame leaned on again and again is the tell. Keep one, rewrite the rest as plain statements.";

/** Compile every rule. Throws on the first pattern V8 rejects, naming it, so a
 *  bad export fails the build (`smell.test.mjs`) instead of failing silently
 *  in the composer. */
export function compile(file: RulesFile): Compiled {
  const perCategory = new Map<string, number>();
  const rules: CompiledRule[] = file.rules.map((r) => {
    const n = perCategory.get(r.category) ?? 0;
    perCategory.set(r.category, n + 1);
    const id = `${r.category}#${n}`;
    return {
      id,
      category: r.category,
      severity: r.severity,
      re: rx(r.pattern, r.flags, id),
      suggestion: r.suggestion,
      minHits: Math.max(1, r.min_hits | 0),
    };
  });
  const invisible = new Map<string, string>();
  for (const [code, name] of Object.entries(file.invisible_chars)) {
    invisible.set(String.fromCodePoint(parseInt(code.slice(2), 16)), name);
  }
  const ns = file.nosmell;
  return {
    rules,
    invisible,
    cyrillic: rx(file.homoglyph_scripts.cyrillic, "", "homoglyph:cyrillic"),
    greek: rx(file.homoglyph_scripts.greek, "", "homoglyph:greek"),
    latin: rx(file.homoglyph_scripts.latin, "", "homoglyph:latin"),
    mdBold: rx(file.markdown_bleed.bold, "", "markdown_bleed:bold"),
    mdHeader: rx(file.markdown_bleed.header, "m", "markdown_bleed:header"),
    contrast: rx(ns.contrast.pattern, ns.contrast.flags, "nosmell:contrast"),
    disclaimer: rx(ns.disclaimer.pattern, ns.disclaimer.flags, "nosmell:disclaimer"),
    position: rx(ns.position.pattern, ns.position.flags, "nosmell:position"),
    thresholds: ns.thresholds,
    minWordsForLowRules: ns.min_words_for_low_rules,
  };
}

function rx(pattern: string, flags: string, id: string): RegExp {
  const f = flags.includes("g") ? flags : flags + "g";
  try {
    return new RegExp(pattern, f);
  } catch (e) {
    throw new Error(`smell rule ${id} does not compile as a JS RegExp: ${(e as Error).message}\n  ${pattern}`);
  }
}

/** Every match of a global regex, with the zero-length guard. */
function matches(re: RegExp, text: string): RegExpExecArray[] {
  re.lastIndex = 0;
  const out: RegExpExecArray[] = [];
  let m: RegExpExecArray | null;
  while ((m = re.exec(text)) !== null) {
    if (m[0].length === 0) re.lastIndex++;
    else out.push(m);
    if (out.length > 5000) break;
  }
  return out;
}

const FENCE = /```[\s\S]*?```/g;
const INLINE_CODE = /`[^`]*`/g;
const BLOCKQUOTE = /^\s*>.*$/gm;

/** slop_scrub's `_strip_noise`, but offset-preserving: fenced code, inline code
 *  and `>` quote lines become spaces of the same length, so a span found in the
 *  masked text points at the same place in the original. */
export function maskNoise(text: string): string {
  const blank = (s: string) => s.replace(/[^\n]/g, " ");
  return text.replace(FENCE, blank).replace(INLINE_CODE, blank).replace(BLOCKQUOTE, blank);
}

/** The RFC 3676 sign-off delimiter the composer puts above a signature, and the
 *  "On … wrote:" attribution above a quoted original: the same two marks
 *  `ComposeForm.splitTail` uses. Returns the user's own words and where they end. */
export function ownWords(body: string): { text: string; end: number } {
  const sig = body.indexOf("\n\n-- \n");
  const quote = body.indexOf("\n\nOn ");
  const marks = [sig, quote >= 0 && body.slice(quote).includes(" wrote:\n") ? quote : -1].filter((i) => i >= 0);
  const end = marks.length ? Math.min(...marks) : body.length;
  return { text: body.slice(0, end), end };
}

/** Trim a match to what slop_scrub reports (`m.group(0).strip()`). */
function trimmed(m: RegExpExecArray): { start: number; end: number; text: string } | null {
  const raw = m[0];
  const lead = raw.length - raw.trimStart().length;
  const text = raw.trim();
  if (!text) return null;
  const start = m.index + lead;
  return { start, end: start + text.length, text };
}

export function scan(text: string, c: Compiled, opts: ScanOptions = {}): ScanResult {
  const ignored = opts.ignoredRules ?? new Set<string>();
  const masked = maskNoise(text);
  const spans: Span[] = [];
  const push = (s: Omit<Span, "text"> & { text?: string }) => {
    if (ignored.has(s.rule)) return;
    spans.push({ ...s, text: s.text ?? text.slice(s.start, s.end) });
  };

  for (const r of c.rules) {
    if (r.severity === "info" && !opts.includeInfo) continue;
    if (r.category === "placeholder_unfilled" && opts.template) continue;
    if (ignored.has(r.id)) continue;
    const hits = matches(r.re, masked);
    if (hits.length < r.minHits) continue;
    for (const m of hits) {
      const t = trimmed(m);
      if (t) push({ ...t, rule: r.id, category: r.category, severity: r.severity, suggestion: r.suggestion });
    }
  }

  // Forensic checks run on the RAW text, as in OS: a fence does not make an
  // invisible character safe.
  for (let i = 0; i < text.length; i++) {
    const name = c.invisible.get(text[i]);
    if (name) {
      push({
        start: i, end: i + 1, text: text[i],
        rule: "invisible_char#0", category: "invisible_char", severity: "hard",
        suggestion: `${name}. ${INVISIBLE_SUGGESTION}`,
      });
    }
  }
  for (const m of matches(/\S+/g, text)) {
    const w = m[0];
    c.latin.lastIndex = 0;
    c.cyrillic.lastIndex = 0;
    c.greek.lastIndex = 0;
    if (!c.latin.test(w)) continue;
    if (c.cyrillic.test(w) || c.greek.test(w)) {
      push({
        start: m.index, end: m.index + w.length, text: w,
        rule: "homoglyph#0", category: "homoglyph", severity: "hard",
        suggestion: "Latin word containing a Cyrillic or Greek letter. Replace with the ASCII letter.",
      });
    }
  }
  const curlyDq = /[“”]/.test(text) && text.includes('"');
  const curlySq = text.includes("’") && text.includes("'");
  if (curlyDq || curlySq) {
    const cls = (curlyDq ? "“”" : "") + (curlySq ? "’" : "");
    for (const m of matches(new RegExp(`[${cls}]`, "g"), text)) {
      push({
        start: m.index, end: m.index + 1,
        rule: "quote_dialect#0", category: "quote_dialect", severity: "hard", suggestion: QUOTE_SUGGESTION,
      });
    }
  }
  // Destination is always an email body here, so markdown bleed is on (OS `--plaintext`).
  for (const m of matches(c.mdBold, masked)) {
    const t = trimmed(m);
    if (t) push({ ...t, rule: "markdown_bleed#0", category: "markdown_bleed", severity: "warn", suggestion: MD_BOLD_SUGGESTION });
  }
  for (const m of matches(c.mdHeader, masked)) {
    const t = trimmed(m);
    if (t) push({ ...t, rule: "markdown_bleed#1", category: "markdown_bleed", severity: "warn", suggestion: MD_HEAD_SUGGESTION });
  }
  // Patrick's rule: hard, every occurrence.
  for (const m of matches(/[—–]/g, text)) {
    push({ start: m.index, end: m.index + 1, rule: DASH_RULE, category: "dash", severity: "hard", suggestion: DASH_SUGGESTION });
  }

  // no-smell's document layer. A contrast frame repeated past the email
  // threshold underlines every occurrence of that frame.
  const words = text.split(/\s+/).filter(Boolean).length;
  const contrastHits = matches(c.contrast, masked);
  const byFrame = new Map<string, RegExpExecArray[]>();
  for (const m of contrastHits) {
    const key = m[0].toLowerCase().split(/\s+/).slice(0, 2).join(" ");
    const list = byFrame.get(key) ?? [];
    list.push(m);
    byFrame.set(key, list);
  }
  let top: { phrase: string; count: number } | null = null;
  for (const [phrase, list] of byFrame) {
    if (!top || list.length > top.count) top = { phrase, count: list.length };
  }
  const repeatWarn = c.thresholds.contrast_repeat?.warn ?? 3;
  if (top && top.count >= repeatWarn && !ignored.has("contrast_repeat#0")) {
    for (const m of byFrame.get(top.phrase) ?? []) {
      const t = trimmed(m);
      if (t) push({ ...t, rule: "contrast_repeat#0", category: "contrast_repeat", severity: "warn", suggestion: CONTRAST_SUGGESTION });
    }
  }
  const per1k = (n: number) => (words ? Math.round((1000 * n * 100) / words) / 100 : 0);
  const positionsPer1k = per1k(matches(c.position, masked).length);
  const positionWarn = c.thresholds.positions?.warn ?? 3;
  const health: Health = {
    words,
    hedgesPer1k: per1k(matches(c.disclaimer, masked).length),
    positionsPer1k,
    contrastRepeat: top && top.count >= 2 ? top : null,
    noPosition: words >= c.minWordsForLowRules && positionsPer1k < positionWarn,
  };

  spans.sort((a, b) => a.start - b.start || a.end - b.end);
  return { spans, health };
}

/** Findings that block Send when `fork_smell_block_hard` is on. */
export function mustFix(result: ScanResult): Span[] {
  return result.spans.filter((s) => s.severity === "hard");
}
