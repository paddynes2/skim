import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
const server = spawn(process.execPath,['node_modules/vite/bin/vite.js','--config','demo/vite.demo.config.ts','--port','1428','--host','127.0.0.1'],{windowsHide:true,stdio:'pipe'});
let output='';server.stdout.on('data',data=>{output+=data;});server.stderr.on('data',data=>{output+=data;});
const browser = await chromium.launch();
try {
  for(let n=0;!output.includes('Local:');n++){if(n>100||server.exitCode!==null)throw new Error(output);await new Promise(resolve=>setTimeout(resolve,100));}
  const page = await browser.newPage({viewport:{width:1600,height:1000}});
  await page.goto('http://127.0.0.1:1428/');
  await page.locator('button.row').first().waitFor();
  const result = await page.evaluate(async () => {
    const {mail} = await import('/src/lib/stores/mail.svelte.ts');
    const {api} = await import('/src/lib/api.ts');
    const pause = () => new Promise(resolve => setTimeout(resolve,150));
    const original = api.listThreads, inbox = mail.selectedFolderId;
    const rows = await original(inbox,0,100);
    api.listThreads = async () => Array.from({length:100},(_,i)=>({...rows[i%rows.length],id:10000+i,date:rows[0].date-i*3600}));
    await mail.refreshThreads();await pause();
    const list = document.querySelector('.list .rows');list.scrollTop = 600;await pause();
    const initial = list.scrollTop;
    await mail.selectDeals();await pause();await mail.selectFolder(inbox);await pause();
    const restored = list.scrollTop;
    const query = 'launch';await mail.enterSearch(query);await pause();await mail.selectFolder(inbox);await pause();
    const afterSearch = list.scrollTop;
    api.listThreads = original;
    return {initial,restored,afterSearch};
  });
  assert.ok(result.initial>=600,JSON.stringify(result));assert.equal(result.restored,result.initial);assert.equal(result.afterSearch,result.initial);
  console.log('Navigation scroll continuity passed',JSON.stringify(result));
} finally {await browser.close();server.kill();}
