import { chromium } from "playwright";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";

// Exercise the real reactive components with the local, fictional mail backend.
const server = spawn(process.execPath, ["node_modules/vite/bin/vite.js", "--config", "demo/vite.demo.config.ts", "--port", "1424", "--host", "127.0.0.1"], { windowsHide: true, stdio: "pipe" });
let output = "";
server.stdout.on("data", data => { output += data; });
server.stderr.on("data", data => { output += data; });
let browser;
try {
  for (let attempt = 0; !output.includes("Local:"); attempt++) {
    if (server.exitCode !== null || attempt > 100) throw new Error(`Demo server failed: ${output}`);
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1600, height: 1000 } });
  const errors = [];
  page.on("pageerror", error => errors.push(String(error)));
  await page.addInitScript(() => localStorage.setItem("skimdemo.fork_thread_multi", "1"));
  await page.goto("http://127.0.0.1:1424/");
  await page.locator(".row", { hasText: "Q3 launch" }).first().click();
  await page.locator(".pane iframe").waitFor();
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(500);
  const result = await page.evaluate(async () => {
    const { mail } = await import("/src/lib/stores/mail.svelte.ts");
    const { api } = await import("/src/lib/api.ts");
    const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
    const frame = document.querySelector(".pane iframe");
    const doc = frame.contentDocument;
    const scroll = document.querySelector(".pane .scroll");
    const initialY = frame.getBoundingClientRect().y;
    const initialScroll = scroll.scrollTop;
    const listY = document.querySelector(".list .rows").getBoundingClientRect().y;
    let loads = 0, loadingRows = 0, refreshNotes = 0, maxMovement = 0, maxListMovement = 0, details = 0;
    frame.addEventListener("load", () => loads++);
    const observer = new MutationObserver(records => {
      for (const record of records) for (const node of record.addedNodes) {
        if (node.nodeType !== 1) continue;
        if (node.matches(".load-status")) loadingRows++;
        if (node.matches(".refresh-note")) refreshNotes++;
      }
    });
    observer.observe(document.body, { childList: true, subtree: true });
    const timer = setInterval(() => {
      maxMovement = Math.max(maxMovement, Math.abs(frame.getBoundingClientRect().y - initialY));
      maxListMovement = Math.max(maxListMovement, Math.abs(document.querySelector(".list .rows").getBoundingClientRect().y - listY));
    }, 10);
    const read = api.listThreads;
    api.listThreads = async (...args) => { await pause(120); return read(...args); };
    for (let cycle = 0; cycle < 3; cycle++) await mail.refreshThreads();
    const thread = api.getThread;
    api.getThread = async (...args) => { details++; await pause(150); return thread(...args); };
    // A sync changes the open thread's metadata while its body is cached.
    mail.patchThreadRow(mail.selectedThreadId, { date: mail.selectedThread.date + 1 });
    await pause(300);
    clearInterval(timer); observer.disconnect();
    api.listThreads = read; api.getThread = thread;
    return { loads, loadingRows, refreshNotes, maxMovement, maxListMovement, details,
      sameFrame: frame === document.querySelector(".pane iframe"), sameDocument: doc === frame.contentDocument,
      sameScroll: initialScroll === scroll.scrollTop };
  });
  assert.equal(result.details, 1, "exercise a real open-thread refresh");
  assert.equal(result.loads, 0, "cached email must not reload its document");
  assert.equal(result.sameFrame, true);
  assert.equal(result.sameDocument, true);
  assert.equal(result.sameScroll, true);
  assert.equal(result.loadingRows, 0, "background sync must not insert a list loading row");
  assert.equal(result.refreshNotes, 0, "cached thread refresh must not insert a reading-pane row");
  assert.equal(result.maxMovement, 0, "open email must stay in place during sync");
  assert.equal(result.maxListMovement, 0, "message list must stay in place during sync");
  assert.deepEqual(errors, []);
  console.log("Refresh continuity passed:", JSON.stringify(result));
} finally {
  await browser?.close();
  server.kill();
}
