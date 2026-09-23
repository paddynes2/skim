// Fork visual check harness (PLAN.md 0.6).
//
// Renders the real app against the demo mocks in all four themes at 1440x900
// and writes PNGs to docs/fork/shots/<phase>/<scenario>-<theme>.png.
//
//   node demo/fork-shots.mjs <phase> [scenario ...]     # default: every scenario tagged for the phase
//   node demo/fork-shots.mjs list                       # print scenarios
//
// A scenario is { phase, setup(page, ctx) } — `setup` drives the UI into the
// state to shoot. Mock-side switches go through localStorage (`skimdemo.*`)
// via ctx.flag(key, value) before the app boots.
import { chromium } from "playwright";
import { spawn } from "node:child_process";
import { setTimeout as sleep } from "node:timers/promises";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { mkdirSync, existsSync } from "node:fs";

const DIR = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(DIR, "..");
const OUT = resolve(ROOT, "docs", "fork", "shots");
const PORT = 1421;
const BASE = `http://127.0.0.1:${PORT}`;
const SIZE = { width: 1440, height: 900 };
const THEMES = ["cold-light", "cold-dark", "warm-light", "warm-dark"];

async function openInbox(page) {
  await page.locator(".row", { hasText: "Q3 launch" }).first().waitFor({ timeout: 20000 });
}
async function openCalendar(page) {
  await openInbox(page);
  await page.locator(".sidebar .item.calendar").click();
  await page.locator(".fork-cal .ec-event").first().waitFor({ timeout: 20000 });
  await sleep(400);
}
async function openCalSettings(page) {
  await openInbox(page);
  await page.locator("button", { hasText: "Settings" }).first().click();
  const s = page.locator(".cal-settings");
  await s.waitFor();
  await s.scrollIntoViewIfNeeded();
  await sleep(300);
}
/** Reply from the pane's footer: the composer mounts under the message (6.2). */
async function openInlineReply(page) {
  await openHero(page);
  await page.locator("footer.actions .btn", { hasText: "Reply" }).first().click();
  await page.locator(".inline-reply .compose-form").waitFor();
  await sleep(400);
}
/** Open Settings and scroll a section heading into view. */
async function openSettingsTo(page, heading) {
  await openInbox(page);
  await page.locator("button", { hasText: "Settings" }).first().click();
  const h = page.locator(`text=${heading}`).first();
  await h.waitFor();
  await h.scrollIntoViewIfNeeded();
  await sleep(300);
}
async function openHero(page) {
  await openInbox(page);
  await page.locator('.row:has-text("Q3 launch")').first().click();
  await page.locator(".subject", { hasText: "Q3 launch" }).first().waitFor();
}

/** Scenario registry. Add one entry per UI state the plan asks to shoot. */
export const SCENARIOS = {
  // Phase 0: today's inbox, reading pane open.
  inbox: { phase: "0", setup: async (page) => { await openHero(page); } },
  // Phase 2.0: the static A/B/C mock (one full-page shot, all themes inside).
  "mock-list-states": { phase: "2", file: "docs/fork/mocks/list-states.html", out: "docs/fork/mocks", full: true },
  // Phase 1.4: Starred in the sidebar with its total count.
  "sidebar-starred": { phase: "1", setup: async (page) => { await openInbox(page); } },
  // Phase 2.2-2.4: row states (unread, read, starred, selected, ticked) with
  // the hero open, plus the hover actions on the third row.
  "rows-hover": {
    phase: "2",
    setup: async (page) => {
      await openHero(page);
      // Tick one row (checkbox slot) and hover another so both states show.
      const marcus = page.locator('.row-wrap:has-text("Marcus Lee")').first();
      await marcus.locator("button.check").click({ force: true });
      await page.locator('.row-wrap:has-text("Priya Nair")').first().hover();
      await sleep(200);
    },
  },
  "rows-compact": {
    phase: "2",
    flags: { "skimdemo.fork_density": "compact" },
    setup: async (page) => { await openHero(page); },
  },
  "rows-avatars": {
    phase: "2",
    flags: { "skimdemo.fork_avatars": "on" },
    setup: async (page) => { await openHero(page); },
  },
  "rows-compact-avatars": {
    phase: "2",
    flags: { "skimdemo.fork_density": "compact", "skimdemo.fork_avatars": "on" },
    setup: async (page) => { await openHero(page); },
  },
  // Phase 3.1: filter chips (All is the default state, shot in rows-hover).
  "chips-unread": {
    phase: "3",
    setup: async (page) => {
      await openInbox(page);
      await page.locator(".chips .chip", { hasText: "Unread" }).click();
      await sleep(300);
    },
  },
  "chips-starred": {
    phase: "3",
    setup: async (page) => {
      await openInbox(page);
      await page.locator(".chips .chip", { hasText: "Starred" }).click();
      await sleep(300);
    },
  },
  // Phase 3.2: archive via the hover action, the undo toast appears.
  "undo-toast": {
    phase: "3",
    setup: async (page) => {
      await openHero(page);
      const row = page.locator('.row-wrap:has-text("Marcus Lee")').first();
      await row.hover();
      await row.locator(".actions .action").first().click();
      await page.locator(".toast").waitFor();
      await sleep(250);
    },
  },
  // Phase 3.3: `g` pressed, the destination hint is up.
  "go-hint": {
    phase: "3",
    setup: async (page) => {
      await openInbox(page);
      await page.keyboard.press("g");
      await page.locator(".go-hint").waitFor();
    },
  },
  // Phase 4: palette with an operator query typed.
  "search-palette": {
    phase: "4",
    setup: async (page) => {
      await openInbox(page);
      await page.keyboard.press("Control+K");
      await page.locator(".panel input").fill("from:anna is:unread launch");
      await sleep(400);
    },
  },
  // Phase 4: the list in search mode (title, chips with x, grouped results).
  "search-list": {
    phase: "4",
    setup: async (page) => {
      await openInbox(page);
      await page.keyboard.press("Control+K");
      await page.locator(".panel input").fill("from:anna is:unread launch");
      await page.keyboard.press("Shift+Enter");
      await page.locator(".search-chips .schip").first().waitFor();
      await sleep(300);
    },
  },
  // Phase 4: a chip removed (from: gone, is:unread stays) and the list re-run.
  "search-chip-removed": {
    phase: "4",
    setup: async (page) => {
      await openInbox(page);
      await page.keyboard.press("Control+K");
      await page.locator(".panel input").fill("from:anna is:unread launch");
      await page.keyboard.press("Shift+Enter");
      await page.locator(".search-chips .schip").first().waitFor();
      await page.locator(".search-chips .schip-x").first().click();
      await sleep(300);
    },
  },
  // Phase 5: quote folding. The pill is `button.fold-pill`; a lone forward has none.
  "fold-gmail-folded": { phase: "5", flags: { "skimdemo.fork_body": "gmail" }, setup: async (page) => { await openHero(page); await page.locator("button.fold-pill").waitFor(); } },
  "fold-gmail-unfolded": { phase: "5", flags: { "skimdemo.fork_body": "gmail" }, setup: async (page) => { await openHero(page); await page.locator("button.fold-pill").click(); await sleep(400); } },
  "fold-outlook-folded": { phase: "5", flags: { "skimdemo.fork_body": "outlook" }, setup: async (page) => { await openHero(page); await page.locator("button.fold-pill").waitFor(); } },
  "fold-outlook-unfolded": { phase: "5", flags: { "skimdemo.fork_body": "outlook" }, setup: async (page) => { await openHero(page); await page.locator("button.fold-pill").click(); await sleep(400); } },
  "fold-plain-folded": { phase: "5", flags: { "skimdemo.fork_body": "plain" }, setup: async (page) => { await openHero(page); await page.locator("button.fold-pill").waitFor(); } },
  "fold-forward-whole": { phase: "5", flags: { "skimdemo.fork_body": "forward" }, setup: async (page) => { await openHero(page); await sleep(400); } },
  // Phase 9: the CRM drawer (found / not found / not connected).
  "crm-found": {
    phase: "9",
    flags: { "skimdemo.fork_crm_drawer": "open" },
    setup: async (page) => { await openHero(page); await page.locator("aside.crm", { hasText: "Northwind Logistics" }).waitFor(); await sleep(300); },
  },
  "crm-not-found": {
    phase: "9",
    flags: { "skimdemo.fork_crm_drawer": "open" },
    setup: async (page) => {
      await openInbox(page);
      await page.locator(".row-wrap").nth(1).click();
      await page.locator("aside.crm .empty").waitFor();
    },
  },
  "crm-not-connected": {
    phase: "9",
    flags: { "skimdemo.fork_crm_drawer": "open", "skimdemo.fork_crm": "off" },
    setup: async (page) => { await openHero(page); await page.locator("aside.crm .ghost").waitFor(); },
  },
  // Phase 11: meeting prep (reminder toast, panel with / without a CRM card, brief).
  "prep-toast": {
    phase: "11",
    flags: { "skimdemo.prep_upcoming": "901" },
    setup: async (page) => {
      await openInbox(page);
      await page.locator(".toast", { hasText: "Q3 launch sync" }).waitFor();
      await sleep(200);
    },
  },
  "prep-panel": {
    phase: "11",
    flags: { "skimdemo.prep_upcoming": "901" },
    setup: async (page) => {
      await openInbox(page);
      await page.locator(".toast .action", { hasText: "Prep" }).click();
      await page.locator(".prep .guest").nth(1).waitFor();
      await sleep(300);
    },
  },
  "prep-panel-nocrm": {
    phase: "11",
    flags: { "skimdemo.prep_upcoming": "902" },
    setup: async (page) => {
      await openInbox(page);
      await page.locator(".toast .action", { hasText: "Prep" }).click();
      await page.locator(".prep .guest").first().waitFor();
      await sleep(300);
    },
  },
  "prep-brief": {
    phase: "11",
    flags: { "skimdemo.prep_upcoming": "901", "skimdemo.typingMs": "2" },
    setup: async (page) => {
      await openInbox(page);
      await page.locator(".toast .action", { hasText: "Prep" }).click();
      await page.locator(".prep .ai-btn").click();
      await page.locator(".prep .brief-text", { hasText: "Talking points" }).waitFor();
      await sleep(300);
    },
  },
  // Phase 12: the MCP section of Settings, on and off.
  "settings-mcp-on": {
    phase: "12",
    flags: { "skimdemo.fork_mcp": "on" },
    setup: async (page) => { await openSettingsTo(page, "Claude Code (MCP)"); },
  },
  "settings-mcp-off": {
    phase: "12",
    flags: { "skimdemo.fork_mcp": "off" },
    setup: async (page) => { await openSettingsTo(page, "Claude Code (MCP)"); },
  },
  // Phase 9: the Rebound section of Settings.
  "settings-crm": {
    phase: "9",
    setup: async (page) => { await openSettingsTo(page, "CRM (Rebound)"); },
  },
  // 6.2: the inline reply under the focused message, AI bar showing.
  "inline-reply-open": {
    phase: "6",
    setup: async (page) => {
      await openInlineReply(page);
      await page.locator(".inline-reply .ai-bar").waitFor();
      await page.locator(".inline-reply").scrollIntoViewIfNeeded();
    },
  },
  // 6.3: the rich editor with words typed, a bold run, the quote unfolded.
  "rich-editor-toolbar": {
    phase: "6",
    setup: async (page) => {
      await openInlineReply(page);
      const host = page.locator(".inline-reply .rich .host");
      await host.click();
      await page.keyboard.type("Thanks Anna, Thursday works. ");
      await page.keyboard.press("Control+b");
      await page.keyboard.type("Moved the review to 14:00");
      await page.keyboard.press("Control+b");
      await page.keyboard.type(" so we go through the numbers first.");
      await page.locator(".inline-reply .quote-pill").click();
      await sleep(300);
      await page.locator(".inline-reply").scrollIntoViewIfNeeded();
    },
  },
  // 6.4: the split Send button's menu, "Pick…" open.
  "send-later-menu": {
    phase: "6",
    setup: async (page) => {
      await openInlineReply(page);
      await page.locator(".inline-reply .send-later .caret").click();
      await page.locator(".inline-reply .send-later .menu").waitFor();
      await page.locator(".inline-reply .send-later .item", { hasText: "Pick" }).click();
      await page.locator(".inline-reply .send-later .free").fill("fri 14:00");
      await sleep(300);
      await page.locator(".inline-reply").scrollIntoViewIfNeeded();
    },
  },
  "scheduled-list": {
    phase: "6",
    setup: async (page) => {
      await openInbox(page);
      await page.locator(".sidebar .item", { hasText: "Scheduled" }).click();
      await page.locator(".scheduled .row").first().waitFor();
    },
  },
  // Phase 10: the court views. Rows carry an age badge ("9d · asks for the deck").
  "court-on-me": {
    phase: "10",
    setup: async (page) => {
      await openInbox(page);
      await page.locator(".sidebar .item", { hasText: "On me" }).click();
      await page.locator(".court-badge").first().waitFor();
      await page.locator('.row:has-text("Contract redline")').first().click();
      await sleep(300);
    },
  },
  "court-waiting": {
    phase: "10",
    setup: async (page) => {
      await openInbox(page);
      await page.locator(".sidebar .item", { hasText: "Waiting" }).click();
      await page.locator(".court-badge").first().waitFor();
      await sleep(300);
    },
  },
  "court-on-me-compact": {
    phase: "10",
    flags: { "skimdemo.fork_density": "compact" },
    setup: async (page) => {
      await openInbox(page);
      await page.locator(".sidebar .item", { hasText: "On me" }).click();
      await page.locator(".court-badge").first().waitFor();
    },
  },
  "court-sidebar-counts": {
    phase: "10",
    setup: async (page) => {
      await openInbox(page);
      await page.locator(".sidebar .item.court .count").first().waitFor();
    },
  },
  "court-nudge-toast": {
    phase: "10",
    // The settings row's Test button fires the toast on demand.
    setup: async (page) => {
      await openInbox(page);
      await page.locator("button", { hasText: "Settings" }).first().click();
      await page.locator("text=Ball in my court").first().scrollIntoViewIfNeeded();
      await page.locator(".court button.chip", { hasText: "Test" }).click();
      await page.keyboard.press("Escape");
      await page.locator(".toast").waitFor();
      await sleep(200);
    },
  },
  "court-settings": {
    phase: "10",
    setup: async (page) => {
      await openInbox(page);
      await page.locator("button", { hasText: "Settings" }).first().click();
      await page.locator("text=Ball in my court").first().waitFor();
      await page.locator("text=Ball in my court").first().scrollIntoViewIfNeeded();
    },
  },
  // Phase 6.5: AI-tell underlines in the composer (fixture draft with several tells).
  "smell-underlines": {
    phase: "6.5",
    flags: { "skimdemo.fork_smell_fixture": "on" },
    setup: async (page) => { await openInlineReply(page); await page.locator(".smell-health").waitFor(); await page.locator(".inline-reply").scrollIntoViewIfNeeded(); await sleep(500); },
  },
  "smell-popover": {
    phase: "6.5",
    flags: { "skimdemo.fork_smell_fixture": "on" },
    setup: async (page) => {
      await openInlineReply(page);
      await page.locator(".smell-health").waitFor();
      // Click inside the stock opener: the underline under the caret opens the popover.
      const host = page.locator(".inline-reply .rich .host");
      const box = await host.locator("text=I wanted to reach out").first().boundingBox();
      await page.mouse.click(box.x + 30, box.y + 10);
      await sleep(300);
      await page.locator(".smell-pop").waitFor();
      const ai = page.locator(".smell-pop .ai");
      if (await ai.count()) { await ai.click(); await page.locator(".smell-pop .opt").first().waitFor({ timeout: 15000 }).catch(() => {}); }
      await sleep(400);
    },
  },
  "smell-send-prompt": {
    phase: "6.5",
    flags: { "skimdemo.fork_smell_fixture": "on" },
    setup: async (page) => {
      await openInlineReply(page);
      await page.locator(".smell-health").waitFor();
      await page.locator(".inline-reply button.send").click();
      await page.locator(".smell-send").waitFor();
      await page.locator(".inline-reply").scrollIntoViewIfNeeded();
      await sleep(300);
    },
  },
  // Phase 7: the calendar screen.
  "cal-week": { phase: "7", setup: async (page) => { await openCalendar(page); } },
  "cal-day": { phase: "7", setup: async (page) => { await openCalendar(page); await page.keyboard.press("d"); await sleep(300); } },
  "cal-month": { phase: "7", setup: async (page) => { await openCalendar(page); await page.keyboard.press("m"); await sleep(300); } },
  "cal-agenda": { phase: "7", setup: async (page) => { await openCalendar(page); await page.keyboard.press("a"); await sleep(300); } },
  // Phase 7: the event panel on the Tuesday "Q3 launch sync" (guests, Meet, Prep).
  "cal-panel": {
    phase: "7",
    setup: async (page) => {
      await openCalendar(page);
      await page.locator(".ec-event", { hasText: "Q3 launch sync" }).first().click();
      await page.locator(".panel .attendees li").nth(1).waitFor();
      await sleep(250);
    },
  },
  // Phase 7: quick create from `n`.
  "cal-quick": {
    phase: "7",
    setup: async (page) => {
      await openCalendar(page);
      await page.keyboard.press("n");
      await page.locator(".panel input.title").waitFor();
      await sleep(250);
    },
  },
  // Phase 7: the "Send invites to the guests?" prompt (quick create + a guest + Create).
  "cal-guests-prompt": {
    phase: "7",
    setup: async (page) => {
      await openCalendar(page);
      await page.keyboard.press("n");
      await page.locator(".panel input.title").fill("Pilot kickoff");
      await page.locator(".panel .guests input").fill("anna.weber@northwind.example");
      await page.locator(".panel .foot .btn.primary").click();
      await page.locator(".dialog").waitFor();
      await sleep(200);
    },
  },
  // Phase 7: next-event chip ("Call with Anna Weber · in 25m · Join") is in every calendar shot's titlebar;
  // this one shoots the inbox so the chip shows over mail too.
  "cal-chip": { phase: "7", setup: async (page) => { await openInbox(page); await page.locator("[data-testid=next-event]").waitFor(); } },
  // Phase 7: Settings → Calendar in the three setup states.
  "cal-settings-connected": { phase: "7", setup: async (page) => { await openCalSettings(page); } },
  "cal-settings-disconnected": { phase: "7", flags: { "skimdemo.fork_cal": "disconnected" }, setup: async (page) => { await openCalSettings(page); } },
  "cal-settings-unconfigured": { phase: "7", flags: { "skimdemo.fork_cal": "unconfigured" }, setup: async (page) => { await openCalSettings(page); } },
  // Phase 7: the setup text on the calendar screen when nothing is connected.
  "cal-unconfigured": { phase: "7", flags: { "skimdemo.fork_cal": "unconfigured" }, setup: async (page) => {
    await openInbox(page);
    await page.locator(".sidebar .item.calendar").click();
    await page.locator(".fork-cal .setup").waitFor();
    await sleep(300);
  } },
  // Phase 7.6: Meet now → "Meet link copied" toast.
  "cal-meet-now": {
    phase: "7",
    setup: async (page) => {
      await openInbox(page);
      await page.keyboard.press("m");
      await page.locator(".toast", { hasText: "Meet link" }).waitFor();
    },
  },
  // Phase 8: the /slots popover with its preview (needs the composer seam; until then call
  // `openSlotsPopover` from the console: page.evaluate(() => window.__skimSlots?.()) or shoot via the toolbar button).
  "slots-popover": {
    phase: "8",
    setup: async (page) => {
      await openInbox(page);
      await openInlineReply(page);
      await page.locator(".inline-reply button.fork-tool").first().click();
      await page.locator(".pop .preview").waitFor();
      await sleep(200);
    },
  },
  // Phase 3.3: the palette lists the fork go-to rows with their G hints.
  "palette-fork": {
    phase: "3",
    setup: async (page) => {
      await openInbox(page);
      await page.keyboard.press("Control+K");
      await page.locator(".panel input").fill("go");
      await sleep(300);
    },
  },
  "settings-list": {
    phase: "2",
    setup: async (page) => {
      await openInbox(page);
      await page.locator("button", { hasText: "Settings" }).first().click();
      await page.locator("text=Density").first().waitFor();
      await page.locator("text=Density").first().scrollIntoViewIfNeeded();
    },
  },
  "focus-ring": {
    phase: "2",
    setup: async (page) => {
      await openInbox(page);
      await page.keyboard.press("Tab");
      await page.keyboard.press("Tab");
      await page.keyboard.press("Tab");
    },
  },
};

async function startServer() {
  try {
    const r = await fetch(BASE);
    if (r.ok) { console.log("Using the dev server already on " + BASE); return { kill() {} }; }
  } catch {}
  const viteBin = resolve(ROOT, "node_modules", "vite", "bin", "vite.js");
  if (!existsSync(viteBin)) throw new Error("Vite not found; run npm ci");
  const proc = spawn(process.execPath, [viteBin, "--config", "demo/vite.demo.config.ts", "--host", "127.0.0.1"], {
    cwd: ROOT, stdio: ["ignore", "ignore", "inherit"],
  });
  for (let i = 0; i < 120; i++) {
    try { const r = await fetch(BASE); if (r.ok) return proc; } catch {}
    await sleep(500);
  }
  throw new Error("Demo server did not start on " + BASE);
}

async function shoot(browser, phase, name, scenario, theme) {
  // The daily court nudge would otherwise pop over every shot taken after its time.
  const flags = { "skimdemo.theme": theme, "skimdemo.fork_court_nudge": "off" };
  const ctx = { flag: (k, v) => { flags[k] = v; }, theme };
  const context = await browser.newContext({ viewport: SIZE, deviceScaleFactor: 1, permissions: ["clipboard-read", "clipboard-write"] });
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  // Let the scenario set mock flags before boot: run a pre-pass with a throwaway page.
  if (scenario.flags) Object.assign(flags, typeof scenario.flags === "function" ? scenario.flags(theme) : scenario.flags);
  await context.addInitScript((f) => {
    try { for (const [k, v] of Object.entries(f)) localStorage.setItem(k, v); } catch {}
  }, flags);
  await page.goto(BASE + "/", { waitUntil: "domcontentloaded" });
  try {
    await scenario.setup(page, ctx);
  } catch (e) {
    // Keep what the page showed when the step failed, next to the errors.
    mkdirSync(resolve(OUT, phase), { recursive: true });
    await page.screenshot({ path: resolve(OUT, phase, `${name}-${theme}-FAILED.png`) }).catch(() => {});
    if (errors.length) console.log(`  ! page errors in ${name}-${theme}: ${errors.join(" | ")}`);
    throw e;
  }
  await sleep(500);
  const dir = resolve(OUT, phase);
  mkdirSync(dir, { recursive: true });
  const file = resolve(dir, `${name}-${theme}.png`);
  await page.screenshot({ path: file });
  await context.close();
  if (errors.length) console.log(`  ! page errors in ${name}-${theme}: ${errors.join(" | ")}`);
  return file;
}

async function main() {
  const [phase, ...only] = process.argv.slice(2);
  if (!phase || phase === "list") {
    for (const [k, v] of Object.entries(SCENARIOS)) console.log(`${v.phase}\t${k}`);
    return;
  }
  const names = only.length ? only : Object.keys(SCENARIOS).filter((k) => SCENARIOS[k].phase === phase);
  if (!names.length) throw new Error("no scenarios for phase " + phase);
  for (const n of names) if (!SCENARIOS[n]) throw new Error("unknown scenario " + n);
  const needsServer = names.some((n) => !SCENARIOS[n].file);
  const server = needsServer ? await startServer() : { kill() {} };
  const browser = await chromium.launch();
  const written = [];
  try {
    for (const name of names) {
      const sc = SCENARIOS[name];
      if (sc.file) {
        // A static page (mock): one shot of the whole document.
        const page = await browser.newPage({ viewport: { width: 1600, height: 900 } });
        await page.goto("file:///" + resolve(ROOT, sc.file).replace(/\\/g, "/"));
        await sleep(300);
        const dir = sc.out ? resolve(ROOT, sc.out) : resolve(OUT, phase);
        mkdirSync(dir, { recursive: true });
        const file = resolve(dir, `${name}.png`);
        await page.screenshot({ path: file, fullPage: !!sc.full });
        await page.close();
        written.push(file);
        continue;
      }
      for (const theme of THEMES) {
        written.push(await shoot(browser, phase, name, sc, theme));
      }
    }
  } finally {
    await browser.close();
    server.kill?.("SIGTERM");
    await sleep(200);
  }
  console.log("\nShots:");
  for (const f of written) console.log("  " + f);
}

main().catch((e) => { console.error(e); process.exit(1); });
