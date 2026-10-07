import { chromium } from "playwright";
import assert from "node:assert/strict";
import { execSync } from "node:child_process";
if (!process.env.SKIM_SKIP_BUILD) execSync("npm run demo:build", { stdio: "pipe", windowsHide: true });
const browser = await chromium.launch();
try {
  for (const rich of [false, true]) {
    const context = await browser.newContext({ viewport: { width: 1500, height: 1000 } });
    await context.addInitScript(rich => {
      localStorage.setItem("skimdemo.theme", rich ? "base-light" : "base-dark");
      localStorage.setItem("skimdemo.fork_rich_text", rich ? "on" : "off");
      localStorage.setItem("skimdemo.fork_thread_multi", "1");
      localStorage.setItem("skimdemo.fork_court_nudge", "off");
      localStorage.setItem("skimdemo.fork_smell", "off");
      localStorage.setItem("skimdemo.fork_smell_block_hard", "off");
      window.__skimCalls = [];
    }, rich);
    const page = await context.newPage(); page.setDefaultTimeout(12000);
    const errors=[]; page.on("pageerror", error=>errors.push(String(error)));
    await page.goto(process.env.SKIM_DEMO_URL ?? "http://127.0.0.1:1431/");
    await page.locator(".row", { hasText: "Q3 launch" }).first().click();
    await page.locator(".primary-reply").click();
    const form=page.locator(".compose-form.reply");
    const editor=form.locator(rich?"[contenteditable=true]":"textarea.body").first();
    await editor.fill("Please see the attached proposal.");
    await form.locator("button.send").click();
    await form.locator(".attachment-check").waitFor();
    assert.equal(await page.evaluate(()=>window.__skimCalls.includes("send_draft")),false);
    await form.getByRole("button",{name:"Keep editing",exact:true}).click();
    await page.evaluate(()=>localStorage.setItem("skimdemo.compose_save_error","on"));
    await editor.fill("Text that must survive a save error.");
    await form.getByRole("button",{name:"Retry save",exact:true}).waitFor();
    assert.match(await editor.evaluate(el=>el.value??el.textContent),/must survive/);
    await page.evaluate(()=>localStorage.removeItem("skimdemo.compose_save_error"));
    await form.getByRole("button",{name:"Retry save",exact:true}).click();
    await form.getByText("Saved on this device",{exact:true}).waitFor();
    await form.locator('input[type="file"]').setInputFiles({name:"proposal.txt",mimeType:"text/plain",buffer:Buffer.from("preserve this attachment")});
    await form.locator(".attach-row").getByText("proposal.txt",{exact:false}).waitFor();
    await page.evaluate(()=>localStorage.setItem("skimdemo.compose_open_error","on"));
    await form.locator("button.popout").click();
    await form.getByText("Demo window could not open",{exact:true}).waitFor();
    assert.match(await editor.evaluate(el=>el.value??el.textContent),/must survive/);
    await page.evaluate(()=>{localStorage.removeItem("skimdemo.compose_open_error");localStorage.setItem("skimdemo.compose_save_delay","700");window.__skimCalls=[];});
    await editor.fill("Older autosave in flight.");
    await page.waitForFunction(()=>window.__skimCalls.includes("update_draft"));
    await editor.fill("Newest text preserved through the window transition.");
    if(rich){await editor.press("Control+a");await form.getByRole("button",{name:"Bold",exact:true}).click();}
    await form.locator("button.popout").click();
    await page.waitForFunction(()=>localStorage.getItem("skimdemo.opened_draft")!==null);
    const moved=await page.evaluate(()=>JSON.parse(localStorage.getItem("skimdemo.opened_draft")));
    assert.match(moved.draft.body,/Newest text preserved/);
    assert.equal(moved.attachments.length,1); assert.equal(moved.attachments[0].filename,"proposal.txt");
    if(rich) {assert.match(moved.html,/Newest text preserved/);assert.match(moved.html,/<(?:b|strong)>/);}
    await form.waitFor({state:"detached"});
    await page.goto(`${process.env.SKIM_DEMO_URL ?? "http://127.0.0.1:1431/"}#/compose/${moved.draft.id}`);
    await page.reload();
    const reopened=page.locator(".compose-form");
    const reopenedEditor=reopened.locator(rich?"[contenteditable=true]":"textarea.body").first();
    await reopenedEditor.waitFor();
    assert.match(await reopenedEditor.evaluate(el=>el.value??el.textContent),/Newest text preserved/);
    await reopened.locator(".attach-row").getByText("proposal.txt",{exact:false}).waitFor();
    if(rich) assert.equal(await reopenedEditor.locator("b,strong").count(),1);
    await reopened.getByRole("button",{name:"Snippets",exact:true}).click();
    const snippets=reopened.getByRole("region",{name:"Reusable snippets"});
    await snippets.getByLabel("Name",{exact:true}).fill("Follow-up");
    await snippets.getByLabel("Text",{exact:true}).fill("A reusable follow-up paragraph.");
    await snippets.getByRole("button",{name:"Save snippet",exact:true}).click();
    await snippets.locator("button.insert").waitFor();
    await snippets.locator("button.insert").click();
    assert.match(await reopenedEditor.evaluate(el=>el.value??el.textContent),/A reusable follow-up paragraph/);
    assert.match(await page.evaluate(()=>localStorage.getItem("skimdemo.fork_compose_snippets")),/Follow-up/);
    assert.equal(await page.evaluate(()=>window.__skimCalls.includes("send_draft")),false);
    assert.deepEqual(errors,[]);
    console.log(`PASS ${rich?"light rich":"dark plain"}: attachment warning, save retry, failed popout, delayed save ordering, reopened content and files`);
    await context.close();
  }
} finally { await browser.close(); }
