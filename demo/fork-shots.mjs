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
  const server = await startServer();
  const browser = await chromium.launch();
  const written = [];
  try {
    for (const name of names) {
      for (const theme of THEMES) {
        written.push(await shoot(browser, phase, name, SCENARIOS[name], theme));
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
