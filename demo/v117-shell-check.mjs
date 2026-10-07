import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import { mkdirSync } from 'node:fs';
const browser=await chromium.launch();
try {
 for(const theme of ['base-dark','base-light']) {
  const page=await browser.newPage({viewport:{width:1600,height:1000}});page.setDefaultTimeout(10000);
  const errors=[];page.on('pageerror',e=>errors.push(String(e)));
  await page.addInitScript(theme=>{localStorage.setItem('skimdemo.theme',theme);localStorage.setItem('skimdemo.fork_thread_multi','1');},theme);
  await page.route('https://www.google.com/s2/favicons**',route=>route.fulfill({contentType:'image/svg+xml',body:'<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64"><rect width="64" height="64" fill="white"/><path d="M12 50V14l40 36V14" stroke="#3165cd" stroke-width="8" fill="none"/></svg>'}));
  await page.route('**/demo/mock/fork-calendar.ts*',async route=>{const response=await route.fetch();await route.fulfill({response,body:(await response.text()).replace('Call with Anna Weber','Funding consultation: a very long meeting title for layout verification')});});
  await page.goto(process.env.SKIM_DEMO_URL??'http://127.0.0.1:1426/');
  await page.locator('[data-testid=next-event]').waitFor();
  await page.locator('.row',{hasText:'Q3 launch'}).first().click();await page.locator('.pane iframe').waitFor();
  await page.evaluate(()=>document.fonts.ready);
  for(const width of [1600,1200,900,700]) {
   await page.setViewportSize({width,height:900});
   const bounds=await page.evaluate(()=>{const chip=document.querySelector('[data-testid=next-event]'),time=chip.querySelector('.time'),join=chip.querySelector('.join'),ctl=document.querySelector('.titlebar .controls');const b=chip.getBoundingClientRect(),t=time.getBoundingClientRect(),j=join.getBoundingClientRect(),c=ctl.getBoundingClientRect();return{height:t.height,chipHeight:b.height,timeFits:time.scrollWidth<=time.clientWidth+1,joinFits:j.right<=b.right+1,controlsClear:b.right<c.left,topFits:t.top>=b.top,bottomFits:t.bottom<=b.bottom};});
   assert.ok(bounds.height<20,JSON.stringify({width,bounds}));assert.ok(bounds.timeFits&&bounds.joinFits&&bounds.controlsClear&&bounds.topFits&&bounds.bottomFits,JSON.stringify({width,bounds}));
  }
  await page.setViewportSize({width:1600,height:1000});
  await page.locator('button.row').first().click({button:'right'});await page.getByRole('menu').waitFor();await page.keyboard.press('Escape');await page.locator('.pane iframe').waitFor();
  await page.locator('.sidebar .item',{hasText:'Deals'}).click();
  await page.locator('.deal-logo-slot button').first().waitFor();
  const aligned=await page.locator('.court-wrap.deal').evaluateAll(rows=>rows.every(row=>{const logo=row.querySelector('.deal-logo-slot').getBoundingClientRect(),name=row.querySelector('.from-name').getBoundingClientRect();return logo.right+4<=name.left&&Math.abs(logo.y+logo.height/2-(row.getBoundingClientRect().y+row.getBoundingClientRect().height/2))<1;}));assert.ok(aligned,'logos stay in their own identity column');
  mkdirSync('docs/fork/shots/1.1.7',{recursive:true});await page.screenshot({path:`docs/fork/shots/1.1.7/deals-${theme}.png`});
  await page.locator('.deal-logo-slot button').first().click();await page.locator('.company-overview').waitFor();
  await page.locator('.sidebar .item',{hasText:'Inbox'}).click();
  await page.locator('button.row').first().click({button:'right'});await page.getByRole('menuitem',{name:'Add to Deals'}).click();await page.locator('.tools .menu').waitFor();
  await page.keyboard.press('Escape');
  await page.locator('.pane iframe').waitFor();
  mkdirSync('docs/fork/shots/1.1.7',{recursive:true});await page.screenshot({path:`docs/fork/shots/1.1.7/inbox-${theme}.png`});
  const openedSubject=await page.locator('.pane h1').textContent();
  const target=page.locator('button.row').filter({hasText:'Contract redline'}).first();
  await target.click({button:'right'});await page.keyboard.press('e');
  await page.getByRole('menu').waitFor({state:'hidden'});await target.waitFor({state:'hidden'});
  assert.equal(await page.locator('.pane h1').textContent(),openedSubject,'menu shortcut preserves the previously opened email');
  assert.deepEqual(errors,[]);await page.close();console.log('Shell checks passed',theme);
 }
} finally { await browser.close(); }
