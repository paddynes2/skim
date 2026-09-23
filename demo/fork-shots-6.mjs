// Phase 6 shots, run by the Phase 6 agent before its scenarios are merged into
// demo/fork-shots.mjs (owned by the main session). Same boot as the harness:
// the demo mocks on port 1421, four themes, 1440x900, PNGs under
// docs/fork/shots/6/. The scenario bodies are what docs/fork/pending/6.md asks
// the main session to paste into SCENARIOS.
//
//   node demo/fork-shots-6.mjs [scenario ...]
import { chromium } from "playwright";
import { spawn } from "node:child_process";
import { setTimeout as sleep } from "node:timers/promises";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { mkdirSync, existsSync } from "node:fs";

const DIR = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(DIR, "..");
const OUT = resolve(ROOT, "docs", "fork", "shots", "6");
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
/** Reply from the pane's footer: the composer mounts under the message. */
async function openInlineReply(page) {
  await openHero(page);
  await page.locator("footer.actions .btn", { hasText: "Reply" }).first().click();
  await page.locator(".inline-reply .compose-form").waitFor();
  await sleep(400);
}

export const SCENARIOS = {
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

async function shoot(browser, name, scenario, theme) {
  const flags = { "skimdemo.theme": theme, ...(scenario.flags ?? {}) };
  const context = await browser.newContext({ viewport: SIZE, deviceScaleFactor: 1 });
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  await context.addInitScript((f) => {
    try { for (const [k, v] of Object.entries(f)) localStorage.setItem(k, v); } catch {}
  }, flags);
  await page.goto(BASE + "/", { waitUntil: "domcontentloaded" });
  await scenario.setup(page);
  await sleep(500);
  mkdirSync(OUT, { recursive: true });
  const file = resolve(OUT, `${name}-${theme}.png`);
  await page.screenshot({ path: file });
  await context.close();
  if (errors.length) console.log(`  ! page errors in ${name}-${theme}: ${errors.join(" | ")}`);
  return file;
}

async function main() {
  const only = process.argv.slice(2);
  const names = only.length ? only : Object.keys(SCENARIOS);
  const server = await startServer();
  const browser = await chromium.launch();
  try {
    for (const name of names) {
      const sc = SCENARIOS[name];
      if (!sc) { console.log("unknown scenario " + name); continue; }
      for (const theme of THEMES) {
        try {
          console.log("  " + (await shoot(browser, name, sc, theme)));
        } catch (e) {
          console.log(`  ! ${name}-${theme}: ${e.message}`);
        }
      }
    }
  } finally {
    await browser.close();
    server.kill?.();
  }
}

main().catch((e) => { console.error(e); process.exit(1); });
