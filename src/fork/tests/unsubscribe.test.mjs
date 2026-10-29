import test, { before, after } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import { build } from "esbuild";
import { compile } from "svelte/compiler";

const root = fileURLToPath(new URL("../../../", import.meta.url));
let browser, page, js;
before(async () => {
  const bundle = await build({
    absWorkingDir: root, bundle: true, write: false, format: "iife", conditions: ["browser"],
    stdin: { resolveDir: root, contents: `import { mount, unmount } from 'svelte';
      import Action from './src/fork/reading/UnsubscribeAction.svelte';
      import Settings from './src/fork/reading/SettingsBlockedSenders.svelte';
      import { unsubscribeLink } from './src/fork/reading/unsubscribe.ts';
      window.testApp = { mount, unmount, Action, Settings, unsubscribeLink };` },
    plugins: [{ name: "svelte-and-mail-fixtures", setup(b) {
      b.onLoad({ filter: /\.svelte$/, namespace: "file" }, args => ({ contents: compile(readFileSync(args.path, "utf8"), { filename: args.path, generate: "client", css: "injected" }).js.code, resolveDir: fileURLToPath(new URL("../reading/", import.meta.url)) }));
      b.onResolve({ filter: /(?:lib\/api|lib\/i18n\/index\.svelte|@tauri-apps\/api\/core|@tauri-apps\/plugin-opener)$/ }, args => ({ path: args.path, namespace: "fixture" }));
      b.onLoad({ filter: /.*/, namespace: "fixture" }, args => {
        if (args.path.includes("i18n")) return { contents: `const en=${readFileSync(root + "src/lib/i18n/locales/en.json", "utf8")}; export function t(k,p={}) { return (en[k]??k).replace(/\\{(\\w+)\\}/g,(_,n)=>p[n]??n); }` };
        if (args.path.includes("opener")) return { contents: `export async function openUrl(url) { window.calls.push(['open',url]); if(window.fixture.openError) throw Error('Browser unavailable'); }` };
        if (args.path.includes("core")) return { contents: `export async function invoke(cmd,args) { window.calls.push([cmd,args]); if(window.fixture.blockError) throw Error('Database unavailable'); return cmd==='fork_blocked_senders' ? window.fixture.blocked??[] : undefined; }` };
        return { contents: `export const errorMessage=e=>e.message??String(e); export const api={
          async unsubscribe(id) { window.calls.push(['unsubscribe',id]); if(window.fixture.headerError) throw window.fixture.headerError; return window.fixture.header??'submitted'; },
          async getMessageBody(id,images,translated) { window.calls.push(['body',id,images,translated]); if(window.fixture.bodyError) throw Error('Body unavailable'); return {html:window.fixture.html??'',security:window.fixture.security}; }
        };` };
      });
    } }],
  });
  js = bundle.outputFiles[0].text;
  browser = await chromium.launch();
});
after(async () => { await browser?.close(); });

async function render(fixture = {}, header = false, width = 820, settings = false) {
  await page?.close();
  page = await browser.newPage({ viewport: { width, height: 520 } });
  page.setDefaultTimeout(5000);
  await page.setContent(`<style>${readFileSync(root + "src/styles/tokens.css", "utf8")} body {font-family: 'Segoe UI'; background:var(--surface); color:var(--text); padding:24px} button:focus-visible{outline:2px solid var(--focus)}</style><main id="app"></main>`);
  await page.evaluate(f => { window.fixture=f; window.calls=[]; }, fixture);
  await page.addScriptTag({ content: js });
  await page.evaluate(({header,settings}) => window.testApp.mount(settings ? window.testApp.Settings : window.testApp.Action, {target: document.getElementById('app'), props: { message: {id:42,folderId:7,canUnsubscribe:header,from:{addr:'news@list.example'}}}}), {header,settings});
}
const button = name => page.getByRole("button", { name, exact: true });

test("header requests are queued, opened pages are not reported as unsubscribed", async () => {
  await render({}, true);
  await button("Unsubscribe").click();
  await button("Unsubscribe requested").waitFor();
  assert.equal(await button("Unsubscribe requested").isDisabled(), true);
  assert.deepEqual(await page.evaluate(()=>window.calls), [["unsubscribe",42]]);
  await render({header:"opened"}, true);
  await button("Unsubscribe").click();
  await page.getByText("Unsubscribe page opened. Finish any steps there.").waitFor();
  assert.equal(await button("Unsubscribe").isEnabled(), true);
});

test("footer links open only after a click; direct links outrank preferences", async () => {
  await render({html:'<a href="https://list.example/preferences">Manage preferences</a><a href="https://list.example/u?a=1&amp;b=2"><b>Unsubscribe</b></a>'});
  assert.deepEqual(await page.evaluate(()=>window.calls), []);
  await button("Unsubscribe").click();
  await page.getByText("Unsubscribe page opened. Finish any steps there.").waitFor();
  assert.deepEqual(await page.evaluate(()=>window.calls), [["body",42,false,false],["open","https://list.example/u?a=1&b=2"]]);
});

test("no link offers an explicit block with a cancel and a mailbox scope", async () => {
  await render({}, false, 390);
  await button("Unsubscribe").focus();
  await page.keyboard.press("Enter");
  await button("Block sender").waitFor();
  assert.match(await page.locator("main").innerText(), /this mailbox/);
  assert.equal(await page.evaluate(()=>window.calls.some(c=>c[0]==='fork_block_sender')), false);
  await button("Cancel").click();
  assert.equal(await button("Block sender").count(), 0);
  await button("Unsubscribe").click();
  await button("Block sender").click();
  await button("Sender blocked").waitFor();
  assert.deepEqual((await page.evaluate(()=>window.calls)).at(-1), ["fork_block_sender",{messageId:42,sender:"news@list.example",folderId:7}]);
  assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth <= innerWidth), true);
});

test("load and block failures remain visible and allow retry", async () => {
  await render({bodyError:true}); await button("Unsubscribe").click();
  await page.getByRole("alert").filter({hasText:"Body unavailable"}).waitFor();
  assert.equal(await button("Block sender").count(), 0);
  await render({blockError:true}); await button("Unsubscribe").click(); await button("Block sender").click();
  await page.getByRole("alert").filter({hasText:"Database unavailable"}).waitFor();
  assert.equal(await button("Block sender").isEnabled(), true);
});

test("flagged footer links keep the explicit open-anyway gate", async () => {
  const url="https://list.example/unsubscribe";
  await render({html:`<a href="${url}">Unsubscribe</a>`, security:{links:[{href:url}]}});
  await button("Unsubscribe").click(); await button("Open anyway").waitFor();
  assert.equal(await page.evaluate(()=>window.calls.some(c=>c[0]==='open')), false);
  await button("Open anyway").click();
  await page.getByText("Unsubscribe page opened. Finish any steps there.").waitFor();
});

test("unusable headers fall back; other header errors do not start another action", async () => {
  await render({headerError:{code:"unsubscribe",message:"No target"}}, true);
  await button("Unsubscribe").click(); await button("Block sender").waitFor();
  await render({headerError:{code:"db",message:"Database unavailable"}}, true);
  await button("Unsubscribe").click(); await page.getByRole("alert").waitFor();
  assert.deepEqual(await page.evaluate(()=>window.calls), [["unsubscribe",42]]);
});

test("unsafe, quoted and unrelated links are ignored without loading resources", async () => {
  await render();
  let requests=0; page.on("request",()=>requests++);
  const cases=[
    '<a href="javascript:alert(1)">Unsubscribe</a>',
    '<a href="file:///C:/Windows/test">Unsubscribe</a>',
    '<a href="https://user:pass@example.com/u">Unsubscribe</a>',
    '<a href="https://user@example.com/u">Unsubscribe</a>',
    '<a href="https://:pass@example.com/u">Unsubscribe</a>',
    '<blockquote><a href="https://list.example/u">Unsubscribe</a></blockquote>',
    '<div id="divRplyFwdMsg">Forwarded</div><a href="https://list.example/u">Unsubscribe</a>',
    '<a href="https://list.example/article">Read this article</a>',
    '<img src="https://tracker.invalid/pixel"><script>window.pwned=true</script>',
  ];
  assert.deepEqual(await page.evaluate(cs=>cs.map(window.testApp.unsubscribeLink),cases), cases.map(()=>null));
  assert.equal(requests,0);
  assert.equal(await page.evaluate(()=>window.pwned),undefined);
  assert.equal(await page.evaluate(()=>window.testApp.unsubscribeLink('<a href="https://list.example/preferences">Email preferences</a>')), 'https://list.example/preferences');
  assert.equal(await page.evaluate(()=>window.testApp.unsubscribeLink('<p>To unsubscribe, <a href="https://list.example/token">click here</a>.</p>')), 'https://list.example/token');
});

test("Settings shows mailbox scope and unblocks only the selected address", async () => {
  const row={accountId:'a',accountEmail:'me@example.com',sender:'news@list.example'};
  await render({blocked:[row]}, false, 820, true);
  await button("Unblock").waitFor();
  assert.match(await page.locator("main").innerText(), /me@example.com/);
  await button("Unblock").click();
  assert.ok((await page.evaluate(()=>window.calls)).some(c=>c[0]==='fork_unblock_sender'&&c[1].accountId==='a'&&c[1].sender===row.sender));
});

test("capture the fallback in the existing light and dark themes", async () => {
  await render(); await button("Unsubscribe").click(); await button("Block sender").waitFor();
  mkdirSync(root+'.cache/unsubscribe-review',{recursive:true});
  for (const theme of ['base-light','base-dark']) {
    await page.evaluate(t=>document.documentElement.dataset.theme=t,theme);
    await page.screenshot({path:root+`.cache/unsubscribe-review/${theme}.png`});
  }
});
