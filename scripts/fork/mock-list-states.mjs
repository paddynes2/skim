// Builds docs/fork/mocks/list-states.html (PLAN.md 2.0): three message-list
// variants (A/B/C) rendered side by side in all four themes with 12 fixture
// rows. Static, self-contained; token values are read from tokens.css so the
// mock never drifts from the app.
//
//   node scripts/fork/mock-list-states.mjs
import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { parseTokens, THEMES } from "./contrast.mjs";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
const OUT = resolve(ROOT, "docs", "fork", "mocks", "list-states.html");
const tokens = parseTokens();

// Twelve real-shaped rows. Fixture people and subjects only.
const ROWS = [
  { from: "Anna Weber", n: 5, subj: "Q3 launch: final checklist and open questions", snip: "Pulling the last threads together before Thursday. Three things still need an owner.", date: "5:49 AM", unread: true, star: false, att: true, sel: true },
  { from: "Marcus Lee", n: 2, subj: "Contract redline v3 ready for your review", snip: "Legal signed off on everything except section 4.2. I left two comments.", date: "2:49 AM", unread: true, star: true, att: true },
  { from: "Priya Nair", n: 1, subj: "Design review moved to Friday 10:00", snip: "Heads up: I pushed the onboarding review to Friday so everyone can attend.", date: "Tue", unread: true, star: false },
  { from: "Stripe", n: 1, subj: "Your receipt from Brightwave Inc. ($2,400.00)", snip: "Thanks for your payment. This receipt is for your records. Invoice #A17-3390.", date: "Tue", unread: false, star: false, att: true },
  { from: "Jordan Fisher", n: 3, subj: "Re: Podcast invite, recording next week?", snip: "Loved the last episode. Would you be up for a 40-minute recording on Tuesday?", date: "Tue", unread: false, star: true, hover: true },
  { from: "GitHub", n: 1, subj: "[brightwave/app] 4 new pull requests need review", snip: "A summary of activity in repositories you watch. #418 Fix flaky test.", date: "Mon", unread: false, star: false },
  { from: "Sofia Ramos", n: 4, subj: "Offsite logistics: hotel + travel", snip: "Booking closes Monday. Please confirm your travel dates so I can book.", date: "Mon", unread: true, star: true },
  { from: "Microsoft account team", n: 1, subj: "Unusual sign-in activity on your account", snip: "We detected an unusual sign-in attempt to your account. Verify it was you.", date: "Sun", unread: false, star: false },
  { from: "Lena Hoffmann", n: 2, subj: "Board pack, draft 2", snip: "Attached the second draft with the revised cash flow section.", date: "Sun", unread: false, star: false, att: true, ticked: true },
  { from: "Notion", n: 1, subj: "Weekly digest: 12 updates in Product", snip: "Here is what changed in the workspaces you follow this week.", date: "Sat", unread: false, star: false },
  { from: "Tom Becker", n: 1, subj: "Quick one: pricing page toggle", snip: "Can we ship the annual toggle above the fold before Thursday?", date: "Sat", unread: true, star: false },
  { from: "Calendly", n: 1, subj: "New event: Intro call with Dana Park", snip: "Wednesday 14:00 to 14:30 SAST. Google Meet link inside.", date: "Fri", unread: false, star: false },
];

const VARIANTS = {
  A: "A (recommended): dot + weight, amber star in a fixed gutter, read rows dimmed",
  B: "B: A + 3px blue left bar and a faint row tint on unread rows",
  C: "C: A + unread subject in blue (Outlook style) instead of the bar",
};

function themeVars(theme) {
  const t = tokens[theme];
  const keep = ["--bg", "--surface", "--text", "--text-dim", "--text-faint", "--hairline", "--hover", "--selected", "--unread", "--star", "--focus", "--row-unread-tint", "--font-ui", "--font-mono"];
  return keep.map((k) => `${k}: ${t[k]};`).join(" ");
}

const CLIP = `<svg class="clip" width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3"><path d="M10.5 5.5L6 10a1.5 1.5 0 0 0 2.1 2.1l5-5a3 3 0 0 0-4.2-4.2l-5.5 5.5"/></svg>`;

function row(r, variant) {
  const cls = ["row", r.unread ? "unread" : "read", r.star ? "starred" : "", r.sel ? "selected" : "", r.hover ? "hover" : "", r.ticked ? "ticked" : ""].filter(Boolean).join(" ");
  const star = r.star ? `<span class="star">★</span>` : `<span class="star hollow">☆</span>`;
  const n = r.n > 1 ? ` <span class="n">${r.n}</span>` : "";
  const check = `<span class="check${r.ticked ? " on" : ""}"></span>`;
  return `<div class="${cls}" data-variant="${variant}">
  <div class="gutter"><span class="dot"></span>${star}</div>
  <div class="content">
    <div class="l1"><span class="from">${r.from}${n}</span><span class="meta">${r.att ? CLIP : ""}<span class="date">${r.date}</span>${check}</span></div>
    <div class="subj">${r.subj}</div>
    <div class="snip">${r.snip}</div>
  </div>
</div>`;
}

const css = `
  * { box-sizing: border-box; margin: 0; }
  body { font-family: "Hanken Grotesk", "Segoe UI", sans-serif; background: #222; color: #ddd; padding: 20px; }
  h1 { font-size: 16px; margin: 0 0 4px; } h2 { font-size: 13px; font-weight: 500; color: #aaa; margin: 18px 0 8px; }
  p { font-size: 12px; color: #aaa; }
  .themes { display: grid; grid-template-columns: repeat(4, 372px); gap: 14px; }
  .theme { font-family: var(--font-ui); background: var(--bg); color: var(--text); border: 1px solid #444; border-radius: 8px; overflow: hidden; }
  .theme .label { font-family: var(--font-mono); font-size: 10px; letter-spacing: .08em; text-transform: uppercase; color: var(--text-faint); padding: 8px 12px; border-bottom: 1px solid var(--hairline); }
  .row { display: grid; grid-template-columns: 20px 1fr; padding: 10px 12px 10px 8px; border-bottom: 1px solid var(--hairline); font-size: 13.5px; position: relative; }
  .row.hover { background: var(--hover); }
  .row.selected { background: var(--selected); box-shadow: inset 2px 0 0 var(--text); }
  .gutter { display: flex; flex-direction: column; align-items: center; gap: 6px; padding-top: 5px; }
  .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--unread); visibility: hidden; }
  .row.unread .dot { visibility: visible; }
  .star { font-size: 12px; line-height: 1; color: var(--star); }
  .star.hollow { color: var(--text-faint); visibility: hidden; }
  .row.hover .star.hollow { visibility: visible; }
  .content { min-width: 0; }
  .l1 { display: flex; justify-content: space-between; align-items: baseline; gap: 8px; }
  .from { color: var(--text-dim); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .row.unread .from { color: var(--text); font-weight: 700; }
  .n { font-family: var(--font-mono); font-size: 10px; color: var(--text-faint); }
  .meta { display: flex; align-items: center; gap: 6px; flex-shrink: 0; color: var(--text-faint); }
  .date { font-family: var(--font-mono); font-size: 10.5px; }
  .check { width: 13px; height: 13px; border: 1px solid var(--text-faint); border-radius: 3px; visibility: hidden; }
  .row.hover .check { visibility: visible; }
  .check.on { visibility: visible; background: var(--text); border-color: var(--text); }
  .subj { margin-top: 2px; color: var(--text-dim); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .row.unread .subj { color: var(--text); font-weight: 600; }
  .snip { margin-top: 2px; font-size: 12.5px; color: var(--text-faint); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  /* B: bar + tint (one token swap: --row-unread-tint) */
  .row[data-variant="B"].unread { background: var(--row-unread-tint); box-shadow: inset 3px 0 0 var(--unread); }
  .row[data-variant="B"].unread.selected { background: var(--selected); box-shadow: inset 3px 0 0 var(--unread); }
  /* C: subject in blue (one token swap: subject colour = --unread) */
  .row[data-variant="C"].unread .subj { color: var(--unread); }
`;

let html = `<!doctype html><html><head><meta charset="utf-8"><title>Skim fork list states A/B/C</title><style>${css}</style></head><body>
<h1>Message list variants (PLAN.md 2.0)</h1>
<p>12 fixture rows. States shown: unread, read, starred, both, selected (row 1), hover (row 5: hollow star + checkbox revealed), ticked (row 9), attachments, thread counts. The default build ships A; B and C are one token swap each (see the CSS in this file).</p>`;
for (const [v, title] of Object.entries(VARIANTS)) {
  html += `<h2>${title}</h2><div class="themes">`;
  for (const theme of THEMES) {
    html += `<div class="theme" style="${themeVars(theme)}"><div class="label">${theme} / variant ${v}</div>${ROWS.map((r) => row(r, v)).join("")}</div>`;
  }
  html += `</div>`;
}
html += `</body></html>`;
writeFileSync(OUT, html);
console.log("wrote " + OUT);
