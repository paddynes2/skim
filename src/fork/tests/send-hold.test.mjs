// Fork (D95): no silent send. Mounts the real ComposeForm in Chromium with the
// Tauri boundary replaced by a recorder (invoke, events, the window), so no
// mail can leave. On 2026-11-06 a mail went out from the compose window that
// the owner did not mean to send; the window vanished and the only Undo was a
// toast in another window.
import test, { before, after } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, mkdirSync } from "node:fs";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { build, transform } from "esbuild";
import { compile, compileModule } from "svelte/compiler";
import { chromium } from "playwright";

const root = fileURLToPath(new URL("../../../", import.meta.url));
const en = JSON.parse(readFileSync(root + "src/lib/i18n/locales/en.json", "utf8"));
const START = new Date("2026-11-07T08:00:00Z").getTime();
let browser, page, js;

const FIXTURES = {
  core: `export class Channel{constructor(){this.onmessage=null;}}
export async function invoke(cmd,args){window.calls.push([cmd,args??{}]);const f=window.fixture;
 switch(cmd){
  case 'get_draft':return structuredClone(f.draft);
  case 'list_draft_attachments':return f.attachments??[];
  case 'list_accounts':return[{id:'a1',email:'me@example.com',displayName:'Me',signature:null}];
  case 'send_draft':return 77;
  case 'fork_cancel_scheduled':if(f.tooLate)throw new Error('too late to cancel');return f.draft.id;
  case 'add_draft_attachment':return{id:9,draftId:f.draft.id,filename:args.filename,mimeType:args.mimeType,size:args.data.length};
  case 'fork_draft_sync_status':return{state:'local',gmail:true};
  default:return null;}}`,
  event: `export async function listen(name,fn){(window.listeners[name]??=[]).push(fn);return()=>{window.listeners[name]=(window.listeners[name]||[]).filter(g=>g!==fn);};}
export async function emit(name,payload){window.emitted.push([name,payload]);for(const fn of window.listeners[name]??[])fn({payload});}
export async function once(){return()=>{};}`,
  window: `const w={onCloseRequested:async h=>{window.closeHandler=h;return()=>{};},close:async()=>{window.calls.push(['window.close']);if(window.closeHandler){let stop=false;await window.closeHandler({preventDefault(){stop=true;}});}},
 minimize:async()=>{},toggleMaximize:async()=>{},isMaximized:async()=>false,onResized:async()=>()=>{},startDragging:async()=>{},setTitle:async()=>{}};
export function getCurrentWindow(){return w;}`,
  opener: `export async function openUrl(){}export async function openPath(){}export async function revealItemInDir(){}`,
  dialog: `export async function open(){return null;}export async function save(){return null;}export async function message(){}export async function ask(){return false;}export async function confirm(){return false;}`,
  i18n: `const en=${JSON.stringify(en)};export function t(k,p={}){return(en[k]??k).replace(/\\{(\\w+)\\}/g,(_,n)=>p[n]??n);}
export const i18n={get locale(){return'en';}};export function setLocale(){}export const LOCALES=[];`,
};

before(async () => {
  const bundle = await build({
    absWorkingDir: root, bundle: true, write: false, format: "iife", conditions: ["browser"],
    stdin: {
      resolveDir: root,
      contents: `import {mount,unmount} from 'svelte';import Compose from './src/components/ComposeForm.svelte';import {prefs} from './src/fork/stores/prefs.svelte';window.testApp={mount,unmount,Compose,prefs};`,
    },
    plugins: [{
      name: "fixtures",
      setup(b) {
        b.onLoad({ filter: /\.svelte$/, namespace: "file" }, (a) => ({
          contents: compile(readFileSync(a.path, "utf8"), { filename: a.path, generate: "client", css: "injected" }).js.code,
          resolveDir: dirname(a.path),
        }));
        b.onLoad({ filter: /\.svelte\.(ts|js)$/, namespace: "file" }, async (a) => {
          const ts = await transform(readFileSync(a.path, "utf8"), { loader: "ts" });
          return { contents: compileModule(ts.code, { filename: a.path, generate: "client" }).js.code, resolveDir: dirname(a.path), loader: "js" };
        });
        b.onResolve({ filter: /^@tauri-apps\/api\/(core|event|window)$/ }, (a) => ({ path: a.path.split("/").pop(), namespace: "fixture" }));
        b.onResolve({ filter: /^@tauri-apps\/plugin-(opener|dialog)$/ }, (a) => ({ path: a.path.split("-").pop(), namespace: "fixture" }));
        b.onLoad({ filter: /.*/, namespace: "fixture" }, (a) => ({ contents: FIXTURES[a.path] }));
      },
    }],
  });
  js = bundle.outputFiles[0].text;
  browser = await chromium.launch();
});
after(async () => await browser?.close());

const DRAFT = {
  id: 42, accountId: "a1", replyToMessageId: null, mode: "new",
  to: "Pharmacy <pharmacy@example.com>", cc: "", bcc: "", subject: "Script",
  body: "Please deliver the attached script.\n\nThanks!\n\nOn 2026-11-06 10:00, Doc <doc@example.com> wrote:\n> here is the script\n",
  originMessageId: null,
};

async function setup({ chrome = true, settings = {}, fixture = {} } = {}) {
  await page?.close();
  page = await browser.newPage({ viewport: { width: 760, height: 700 } });
  page.setDefaultTimeout(5000);
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  await page.clock.install({ time: START });
  await page.setContent(`<style>${readFileSync(root + "src/styles/tokens.css", "utf8")}${readFileSync(root + "src/styles/base.css", "utf8")}html,body,#app{height:100%;margin:0}body{font-family:'Segoe UI';background:var(--surface);color:var(--text)}</style><div id="app"></div>`);
  await page.evaluate(({ draft, fixture }) => {
    window.fixture = { draft, attachments: [{ id: 3, draftId: 42, filename: "script.pdf", mimeType: "application/pdf", size: 67000 }], ...fixture };
    window.calls = []; window.listeners = {}; window.emitted = [];
    document.documentElement.dataset.theme = "base-light";
  }, { draft: DRAFT, fixture });
  await page.addScriptTag({ content: js });
  await page.evaluate(({ chrome, settings }) => {
    window.testApp.prefs.hydrate(settings);
    window.instance = window.testApp.mount(window.testApp.Compose, {
      target: document.getElementById("app"),
      props: {
        draftId: 42, chrome,
        onSent: () => window.calls.push(["onSent"]),
        onClose: () => window.calls.push(["onClose"]),
        onDiscarded: () => window.calls.push(["onDiscarded"]),
      },
    });
  }, { chrome, settings });
  await page.locator("input.subject").waitFor();
  await page.clock.runFor(50);
  return errors;
}

const calls = (name) => page.evaluate((n) => window.calls.filter((c) => c[0] === n), name);
const button = (name) => page.getByRole("button", { name, exact: true });
const sendButton = () => page.locator("button.send");
const shot = (name) => { mkdirSync(root + ".cache", { recursive: true }); return page.screenshot({ path: root + `.cache/${name}.png` }); };

test("Ctrl+Enter sends from no field: subject, rich body, plain body, quoted text", async () => {
  await setup();
  await page.locator("input.subject").click();
  await page.keyboard.press("Control+Enter");
  await page.locator(".host").click();
  await page.keyboard.press("Control+Enter");
  await button(en["fork.reading.show_quoted"]).click();
  await button(en["fork.compose.edit_quoted"]).click();
  await page.locator("textarea.tail-edit").click();
  await page.keyboard.press("Control+Enter");
  await page.clock.runFor(2000);
  assert.deepEqual(await calls("send_draft"), [], "rich mode: a keyboard shortcut sent the mail");

  await setup({ settings: { fork_rich_text: "off" } });
  await page.locator("textarea.body").click();
  await page.keyboard.press("Control+Enter");
  await page.keyboard.press("Meta+Enter");
  await page.clock.runFor(2000);
  assert.deepEqual(await calls("send_draft"), [], "plain mode: a keyboard shortcut sent the mail");
  assert.equal(await sendButton().getAttribute("title"), null, "the Send button still advertises a shortcut");
});

test("Send keeps the window open: recipient, countdown, Undo, and nothing can change the mail", async () => {
  const errors = await setup();
  await sendButton().click();
  const bar = page.locator(".hold");
  await bar.getByText("Sending to pharmacy@example.com in 10 s", { exact: true }).waitFor();
  const [[, args]] = await calls("send_draft");
  assert.equal(args.heldByWindow, true);
  assert.equal(args.notBefore, START / 1000 + 10);
  assert.deepEqual(await calls("onSent"), [], "the window closed at once");
  assert.equal(await button(en["fork.compose.undo"]).evaluate((e) => e === document.activeElement), true, "Undo does not hold the focus");
  assert.equal(await page.locator(".compose-body").evaluate((e) => e.inert), true);
  await shot("send-hold-light");

  // Typing, a dropped file and a removed file must not reach the draft row
  // that the send reads when the hold ends.
  const saves = (await calls("update_draft")).length;
  await page.locator("input.subject").click({ force: true });
  await page.keyboard.type("XX");
  await page.evaluate(() => {
    const dt = new DataTransfer();
    dt.items.add(new File(["x"], "late.pdf", { type: "application/pdf" }));
    document.querySelector(".compose-form").dispatchEvent(new DragEvent("drop", { dataTransfer: dt, bubbles: true, cancelable: true }));
  });
  await page.locator(".chip .remove").click({ force: true });
  await page.clock.runFor(2000);
  assert.equal(await page.locator("input.subject").inputValue(), "Script");
  assert.deepEqual(await calls("remove_draft_attachment"), [], "a file was removed during the hold");
  assert.equal((await calls("update_draft")).length, saves, "an edit was saved during the hold");
  assert.deepEqual(await calls("add_draft_attachment"), [], "a file was added during the hold");
  await bar.getByText("Sending to pharmacy@example.com in 8 s", { exact: true }).waitFor();
  assert.deepEqual(errors, []);
});

test("Undo cancels the queued send and hands the same draft back for editing", async () => {
  await setup();
  await sendButton().click();
  await button(en["fork.compose.undo"]).click();
  assert.deepEqual((await calls("fork_cancel_scheduled")).map((c) => c[1]), [{ opId: 77 }]);
  await sendButton().waitFor();
  assert.equal(await page.locator(".hold").count(), 0);
  assert.equal(await page.locator(".compose-body").evaluate((e) => e.inert), false);
  await shot("send-hold-undone");

  // The draft is live again: an edit is saved, and a second Send queues anew.
  await page.locator("input.subject").click();
  await page.keyboard.type(" v2");
  await page.clock.runFor(1500);
  const saved = (await calls("update_draft")).map((c) => c[1].draft.subject);
  assert.ok(saved.includes("Script v2"), `edit after Undo was not saved: ${JSON.stringify(saved)}`);
  await sendButton().click();
  await page.locator(".hold").waitFor();
  assert.equal((await calls("send_draft")).length, 2);
});

test("Sent shows only on Skim's confirmation, then the window closes; no confirmation closes after the fallback", async () => {
  await setup();
  await sendButton().click();
  await page.clock.runFor(10_500);
  await page.getByText("Sending to pharmacy@example.com…", { exact: true }).waitFor();
  assert.equal(await button(en["fork.compose.undo"]).count(), 0, "Undo offered after the send time");
  assert.equal(await page.getByText("Sent to pharmacy@example.com", { exact: true }).count(), 0, "Sent before Skim confirmed");
  assert.deepEqual(await calls("onSent"), []);
  await page.evaluate(() => { for (const fn of window.listeners["mail:sent"] ?? []) fn({ payload: { draftId: 41 } }); });
  await page.clock.runFor(2000);
  assert.equal(await page.getByText("Sent to pharmacy@example.com", { exact: true }).count(), 0, "another draft's confirmation counted");
  await page.evaluate(() => { for (const fn of window.listeners["mail:sent"] ?? []) fn({ payload: { draftId: 42 } }); });
  await page.getByText("Sent to pharmacy@example.com", { exact: true }).waitFor();
  await page.clock.runFor(1300);
  assert.equal((await calls("onSent")).length, 1);

  await setup();
  await sendButton().click();
  await page.clock.runFor(10_500 + 119_000);
  assert.deepEqual(await calls("onSent"), [], "closed before the fallback");
  await page.clock.runFor(2000);
  assert.equal((await calls("onSent")).length, 1, "never closed without a confirmation");
});

test("with no undo hold the bar still names the recipient and waits for Sent", async () => {
  await setup({ settings: { fork_undo_send_secs: "0" } });
  await sendButton().click();
  await page.getByText("Sending to pharmacy@example.com…", { exact: true }).waitFor();
  const [[, args]] = await calls("send_draft");
  assert.equal(args.notBefore, null);
  assert.equal(await button(en["fork.compose.undo"]).count(), 0);
  await page.evaluate(() => { for (const fn of window.listeners["mail:sent"] ?? []) fn({ payload: { draftId: 42 } }); });
  await page.getByText("Sent to pharmacy@example.com", { exact: true }).waitFor();
});

test("closing the window during the hold hands Undo to the main window, once", async () => {
  await setup();
  await sendButton().click();
  await page.clock.runFor(3000);
  await button(en["a11y.close"]).click();
  await page.evaluate(() => window.closeHandler({ preventDefault() {} }));
  const handed = await page.evaluate(() => window.emitted.filter((e) => e[0] === "fork:send-held"));
  assert.deepEqual(handed, [["fork:send-held", { opId: 77, draftId: 42, notBefore: START / 1000 + 10, label: null }]]);
  assert.equal((await calls("onClose")).length, 1);

  // After the send time there is nothing left to undo: no hand-off.
  await setup();
  await sendButton().click();
  await page.clock.runFor(10_500);
  await button(en["a11y.close"]).click();
  assert.deepEqual(await page.evaluate(() => window.emitted.filter((e) => e[0] === "fork:send-held")), []);
});

test("too late to undo says so and keeps the send", async () => {
  await setup({ fixture: { tooLate: true } });
  await sendButton().click();
  await button(en["fork.compose.undo"]).click();
  await page.getByText(en["fork.compose.hold_too_late"], { exact: true }).waitFor();
  assert.equal(await button(en["fork.compose.undo"]).count(), 0);
  assert.equal(await page.locator(".compose-body").evaluate((e) => e.inert), true, "the draft was handed back after a refused cancel");
});

test("inline reply and Send later keep today's flow: no window hold, main toast stays", async () => {
  await setup({ chrome: false });
  await sendButton().click();
  await page.clock.runFor(100);
  const [[, args]] = await calls("send_draft");
  assert.equal(args.heldByWindow, false);
  assert.equal((await calls("onSent")).length, 1);
  assert.equal(await page.locator(".hold").count(), 0);

  await setup();
  await page.locator("button.caret").click();
  await page.getByRole("menuitem").first().click();
  await page.clock.runFor(100);
  const [[, later]] = await calls("send_draft");
  assert.equal(later.heldByWindow, false);
  assert.ok(later.notBefore > START / 1000 + 60, "Send later lost its time");
  assert.equal((await calls("onSent")).length, 1);
  assert.equal(await page.locator(".hold").count(), 0);
});
