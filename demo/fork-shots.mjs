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
  const flags = { "skimdemo.theme": theme };
  const ctx = { flag: (k, v) => { flags[k] = v; }, theme };
  const context = await browser.newContext({ viewport: SIZE, deviceScaleFactor: 1 });
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  // Let the scenario set mock flags before boot: run a pre-pass with a throwaway page.
  if (scenario.flags) Object.assign(flags, typeof scenario.flags === "function" ? scenario.flags(theme) : scenario.flags);
  await context.addInitScript((f) => {
    try { for (const [k, v] of Object.entries(f)) localStorage.setItem(k, v); } catch {}
  }, flags);
  await page.goto(BASE + "/", { waitUntil: "domcontentloaded" });
  await scenario.setup(page, ctx);
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
