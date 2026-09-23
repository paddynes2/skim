// Fork (4.3): the query string as the list header shows it — one chip per
// operator token. Rust (`fork/search_query.rs`) owns what the operators MEAN;
// this only splits the typed string so a chip can be removed by dropping its
// token and searching the rest again.

/** Operators the Rust parser understands; anything else stays free text. */
const KEYS = new Set([
  "from",
  "to",
  "cc",
  "subject",
  "is",
  "has",
  "in",
  "before",
  "after",
  "older_than",
  "older",
  "newer_than",
  "newer",
]);

const NEGATABLE = new Set(["from", "to", "cc", "subject", "is", "has"]);
const IS_VALUES = new Set(["unread", "read", "starred", "unstarred"]);

/** Split on whitespace outside double quotes, keeping the quotes. */
export function tokenize(query: string): string[] {
  const out: string[] = [];
  let cur = "";
  let quoted = false;
  for (const c of query) {
    if (c === '"') {
      quoted = !quoted;
      cur += c;
    } else if (/\s/.test(c) && !quoted) {
      if (cur) out.push(cur);
      cur = "";
    } else {
      cur += c;
    }
  }
  if (cur) out.push(cur);
  return out;
}

export interface SearchChip {
  /** The token exactly as typed — what `withoutToken` removes. */
  token: string;
  /** Operator name, or `"not"` for a `-word` exclusion. */
  key: string;
  /** The value with its quotes stripped. */
  value: string;
  negated: boolean;
}

/** The operator tokens of a query, in order. Free words are not chips. */
export function chipsOf(query: string): SearchChip[] {
  const chips: SearchChip[] = [];
  for (const token of tokenize(query)) {
    const negated = token.startsWith("-") && token.length > 1;
    const body = negated ? token.slice(1) : token;
    const colon = body.indexOf(":");
    if (colon > 0 && /^[A-Za-z0-9_]+$/.test(body.slice(0, colon))) {
      const key = body.slice(0, colon).toLowerCase();
      const value = stripQuotes(body.slice(colon + 1));
      // Same acceptance as the Rust parser: a negated `in:` / date stays free
      // text, `is:` and `has:` only take the values they know.
      const accepts =
        KEYS.has(key) &&
        (!negated || NEGATABLE.has(key)) &&
        (key !== "is" || IS_VALUES.has(value.toLowerCase())) &&
        (key !== "has" || /^attachments?$/i.test(value));
      if (accepts && value) {
        chips.push({ token, key, value, negated });
        continue;
      }
    }
    if (negated && colon === -1) {
      const value = stripQuotes(body);
      if (value) chips.push({ token, key: "not", value, negated: true });
    }
  }
  return chips;
}

function stripQuotes(v: string): string {
  return v.replace(/^"/, "").replace(/"$/, "").replace(/"/g, "");
}

/** The free text of a query (every non-chip token), for the header title. */
export function textOf(query: string): string {
  const chipTokens = new Set(chipsOf(query).map((c) => c.token));
  return tokenize(query)
    .filter((t) => !chipTokens.has(t))
    .join(" ");
}

/** The query with the first occurrence of `token` removed. */
export function withoutToken(query: string, token: string): string {
  const tokens = tokenize(query);
  const i = tokens.indexOf(token);
  if (i !== -1) tokens.splice(i, 1);
  return tokens.join(" ");
}

/** True when the query carries at least one operator or exclusion. */
export function hasOperators(query: string): boolean {
  return chipsOf(query).length > 0;
}
