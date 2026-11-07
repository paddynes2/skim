// Fork (D95), review round 2: what may still write the draft row, or start a
// send, while a send is held. Same harness as send-hold.test.mjs: the real
// ComposeForm in Chromium, the Tauri boundary recorded, no mail can leave.
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
  case 'get_draft':if(f.draftGone)throw{code:'mail',message:'draft not found'};return structuredClone(f.draft);
  case 'list_draft_attachments':return f.attachments??[];
  case 'list_accounts':return[{id:'a1',email:'me@example.com',displayName:'Me',signature:null}];
  case 'send_draft':return 77;
  case 'fork_cancel_scheduled':if(f.notScheduled)throw{code:'schedule',message:'not scheduled'};if(f.tooLate)throw{code:'schedule',message:'too late to undo'};return f.draft.id;
  case 'ai_key_status':return{provider:'anthropic',anthropic:!!f.ai,openrouter:false,custom:false};
  case 'ai_compose':return new Promise(()=>{});
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

test("Ctrl+S during the hold saves nothing", async () => {
  await setup();
  await sendButton().click();
  await page.locator(".hold").waitFor();
  const before = { local: (await calls("update_draft")).length, server: (await calls("save_server_draft")).length };
  await page.keyboard.press("Control+s");
  await page.clock.runFor(1500);
  assert.equal((await calls("save_server_draft")).length, before.server, "Ctrl+S wrote the draft to the server during the hold");
  assert.equal((await calls("update_draft")).length, before.local, "Ctrl+S rewrote the draft row during the hold");
});

test("no send while the AI is still writing", async () => {
  await setup({ fixture: { ai: true } });
  await page.locator("textarea.instruction, .ai-box textarea, form textarea").first().fill("write a short note");
  await page.keyboard.press("Control+Enter");
  await page.clock.runFor(300);
  assert.equal(await sendButton().isDisabled(), true, "Send is clickable while the AI streams");
  await sendButton().click({ force: true });
  await page.clock.runFor(300);
  assert.deepEqual(await calls("send_draft"), [], "a send started while the AI was writing");
});

test("a hold already cancelled elsewhere: Undo hands the draft back instead of saying it is going", async () => {
  await setup({ fixture: { notScheduled: true } });
  await sendButton().click();
  await button(en["fork.compose.undo"]).click();
  await sendButton().waitFor();
  assert.equal(await page.getByText(en["fork.compose.hold_too_late"], { exact: true }).count(), 0);
  assert.equal(await page.locator(".compose-body").evaluate((e) => e.inert), false);

  // Gone because it was sent: the draft row is deleted, so no hand-back.
  await setup({ fixture: { notScheduled: true } });
  await sendButton().click();
  await page.evaluate(() => (window.fixture.draftGone = true));
  await button(en["fork.compose.undo"]).click();
  await page.getByText(en["fork.compose.hold_too_late"], { exact: true }).waitFor();
  assert.equal(await page.locator(".compose-body").evaluate((e) => e.inert), true);
});
