// Contrast gate for the four theme blocks in src/styles/tokens.css (PLAN.md 2.1).
//
//   node scripts/fork/contrast.mjs          # exit 0 = every theme passes
//   node scripts/fork/contrast.mjs --table  # also print every ratio
//
// Rules (WCAG 2.x): text tokens against every background they sit on:
//   --text >= 7 (AAA), --text-dim >= 4.5, --text-faint >= 4.5 (AA text);
//   --unread, --star, --focus >= 3 (1.4.11 non-text).
// Backgrounds: --bg, --surface, --selected, and --hover composited over --bg.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
export const TOKENS = resolve(ROOT, "src", "styles", "tokens.css");
export const THEMES = ["cold-light", "cold-dark", "warm-light", "warm-dark"];

/** Parse `tokens.css` into { theme: { token: value } }. `:root` is cold-light. */
export function parseTokens(css = readFileSync(TOKENS, "utf8")) {
  const out = {};
  const re = /(:root(?:\[data-theme="([a-z-]+)"\])?)\s*\{([^}]*)\}/g;
  let m;
  while ((m = re.exec(css))) {
    const theme = m[2] ?? "cold-light";
    out[theme] ??= {};
    for (const line of m[3].split("\n")) {
      const d = line.match(/^\s*(--[a-z0-9-]+)\s*:\s*([^;]+);/);
      if (d) out[theme][d[1]] = d[2].trim();
    }
  }
  // The other themes inherit what :root declares (fonts, radii) — colours are
  // all redeclared, but keep the fallback honest.
  for (const t of THEMES) out[t] = { ...out["cold-light"], ...(out[t] ?? {}) };
  return out;
}

/** "#rgb" | "#rrggbb" | "rgba(r, g, b, a)" → [r, g, b, a] (0-255, 0-1). */
export function parseColor(s) {
  s = s.trim();
  let m = s.match(/^#([0-9a-f]{3})$/i);
  if (m) return [...m[1]].map((c) => parseInt(c + c, 16)).concat(1);
  m = s.match(/^#([0-9a-f]{6})$/i);
  if (m) return [0, 2, 4].map((i) => parseInt(m[1].slice(i, i + 2), 16)).concat(1);
  m = s.match(/^rgba?\(\s*([\d.]+)\s*,\s*([\d.]+)\s*,\s*([\d.]+)\s*(?:,\s*([\d.]+))?\s*\)$/i);
  if (m) return [+m[1], +m[2], +m[3], m[4] === undefined ? 1 : +m[4]];
  throw new Error("unparsed colour: " + s);
}

/** Composite `fg` (with alpha) over opaque `bg`. */
export function over(fg, bg) {
  const a = fg[3];
  return [0, 1, 2].map((i) => Math.round(fg[i] * a + bg[i] * (1 - a))).concat(1);
}

export function luminance([r, g, b]) {
  const f = (c) => {
    c /= 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
}

export function contrast(a, b) {
  const [l1, l2] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (l1 + 0.05) / (l2 + 0.05);
}

export const RULES = [
  { token: "--text", min: 7 },
  { token: "--text-dim", min: 4.5 },
  { token: "--text-faint", min: 4.5 },
  { token: "--unread", min: 3 },
  { token: "--star", min: 3 },
  { token: "--focus", min: 3 },
];

/** Every (theme, token, background) ratio, with pass/fail. */
export function audit(tokens = parseTokens()) {
  const rows = [];
  for (const theme of THEMES) {
    const t = tokens[theme];
    const bg = parseColor(t["--bg"]);
    const backgrounds = {
      "--bg": bg,
      "--surface": parseColor(t["--surface"]),
      "--selected": parseColor(t["--selected"]),
      "--hover/bg": over(parseColor(t["--hover"]), bg),
    };
    for (const { token, min } of RULES) {
      if (!t[token]) {
        rows.push({ theme, token, on: "(missing)", ratio: 0, min, ok: false });
        continue;
      }
      const fg = over(parseColor(t[token]), bg);
      for (const [on, back] of Object.entries(backgrounds)) {
        const ratio = contrast(fg, back);
        rows.push({ theme, token, on, ratio, min, ok: ratio >= min });
      }
    }
  }
  return rows;
}

const isMain = process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain) {
  const rows = audit();
  const table = process.argv.includes("--table");
  let failed = 0;
  for (const r of rows) {
    if (!r.ok) failed++;
    if (table || !r.ok)
      console.log(
        `${r.ok ? "ok  " : "FAIL"} ${r.theme.padEnd(10)} ${r.token.padEnd(13)} on ${r.on.padEnd(10)} ${r.ratio.toFixed(2)} (min ${r.min})`,
      );
  }
  console.log(failed ? `contrast: ${failed} failing ratio(s)` : `contrast: all ${rows.length} ratios pass`);
  process.exit(failed ? 1 : 0);
}
