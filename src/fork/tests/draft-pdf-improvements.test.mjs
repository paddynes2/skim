import test,{before,after} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync,mkdirSync,existsSync} from 'node:fs';
import {dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
import {build} from 'esbuild';
import {compile} from 'svelte/compiler';
import {chromium} from 'playwright';
const root=fileURLToPath(new URL('../../../',import.meta.url));
let browser,page,js;
before(async()=>{
  const bundle=await build({absWorkingDir:root,bundle:true,write:false,format:'iife',conditions:['browser'],
    stdin:{resolveDir:root,contents:`import {mount,unmount} from 'svelte';import Files from './src/components/AttachmentChips.svelte';import Status from './src/fork/compose/DraftSyncStatus.svelte';import Rich from './src/fork/RichEditor.svelte';window.testApp={mount,unmount,Files,Status,Rich};`},
    plugins:[{name:'components',setup(b){
      b.onLoad({filter:/\.svelte$/,namespace:'file'},a=>({contents:compile(readFileSync(a.path,'utf8'),{filename:a.path,generate:'client',css:'injected'}).js.code,resolveDir:dirname(a.path)}));
      b.onResolve({filter:/(?:lib\/api|lib\/i18n\/index\.svelte|@tauri-apps\/api\/(?:core|event))$/},a=>({path:a.path,namespace:'fixture'}));
      b.onLoad({filter:/.*/,namespace:'fixture'},a=>{
        if(a.path.includes('i18n'))return{contents:`const en=${readFileSync(root+'src/lib/i18n/locales/en.json','utf8')};export function t(k,p={}){return(en[k]??k).replace(/\\{(\\w+)\\}/g,(_,n)=>p[n]??n);}`};
        if(a.path.endsWith('/event'))return{contents:`export async function listen(name,fn){(window.listeners[name]??=[]).push(fn);return()=>{window.listeners[name]=window.listeners[name].filter(f=>f!==fn);};}`};
        if(a.path.endsWith('/core'))return{contents:`export async function invoke(cmd,args){window.calls.push([cmd,args]);if(cmd==='fork_attachment_fingerprints')return[];if(cmd==='fork_draft_sync_status'){if(window.fixture.error)throw Error('offline');const value={state:window.fixture.state??'local',gmail:window.fixture.gmail??true};if(window.fixture.delay)await new Promise(r=>window.release=r);return value;}if(cmd==='fork_attachment_preview'){if(window.fixture.previewError)throw Error('Cannot read PDF');const value={kind:'pdf',dataUrl:window.fixture.png,page:args.page??1,pageCount:2};if(window.fixture.delay)await new Promise(r=>window.release=r);return value;} }`};
        return{contents:`export const errorMessage=e=>e.message;export const api={saveAttachment:async id=>window.calls.push(['save',id]),openAttachment:async id=>window.calls.push(['open',id])};`};
      });
    }}]});js=bundle.outputFiles[0].text;browser=await chromium.launch();
});
after(async()=>await browser?.close());
async function setup(fixture={},type='Files',dark=false){
 await page?.close();page=await browser.newPage({viewport:{width:1000,height:900}});page.setDefaultTimeout(5000);
 await page.setContent(`<style>${readFileSync(root+'src/styles/tokens.css','utf8')}${readFileSync(root+'src/styles/base.css','utf8')}body{font-family:'Segoe UI';background:var(--surface);color:var(--text);padding:24px}button{cursor:pointer}main{max-width:900px}</style><main id="app"></main>`);
 const png=existsSync(root+'.cache/pdf-proof.png')?'data:image/png;base64,'+readFileSync(root+'.cache/pdf-proof.png').toString('base64'):'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jRZkAAAAASUVORK5CYII=';
 await page.evaluate(({fixture,png,dark})=>{window.fixture={png,...fixture};window.calls=[];window.listeners={};document.documentElement.dataset.theme=dark?'base-dark':'base-light';},{fixture,png,dark});
 await page.addScriptTag({content:js});
 await page.evaluate(type=>{const props=type==='Rich'?{html:'<p>Keep <b>this</b> link</p>',onchange:(html,text)=>window.calls.push(['edit',html,text])}:type==='Files'?{attachments:[{id:3,filename:'Review.pdf',mimeType:'application/pdf',size:124000}]}:{draftId:42,localStatus:'saved',revision:0,refreshKey:0,onSave:async()=>{window.calls.push(['saveDraft']);window.fixture.state='pending';},onRetryLocal:()=>window.calls.push(['retryLocal'])};window.props=props;window.instance=window.testApp.mount(window.testApp[type],{target:document.getElementById('app'),props});},type);
}
const button=name=>page.getByRole('button',{name,exact:true});
async function emit(state){await page.evaluate(state=>{window.fixture.state=state;for(const fn of window.listeners['draft:sync']??[])fn({payload:{draftId:42}});},state);}
test('PDF pages, zoom, keyboard close and original file actions',async()=>{
 await setup();await button('Preview attachment').focus();await page.keyboard.press('Enter');await page.getByText('Page 1 of 2',{exact:true}).waitFor();
 assert.equal(await button('Previous page').isDisabled(),true);await button('Next page').click();await page.getByText('Page 2 of 2',{exact:true}).waitFor();assert.equal(await button('Next page').isDisabled(),true);
 await page.getByLabel('Zoom',{exact:true}).selectOption('2');assert.equal(await page.locator('.pdf-sheet').evaluate(e=>e.style.width),'200%');
 await button('Previous page').click();await page.getByText('Page 1 of 2',{exact:true}).waitFor();
 await page.getByLabel('Zoom',{exact:true}).selectOption('1');mkdirSync(root+'.cache',{recursive:true});await page.screenshot({path:root+'.cache/pdf-preview-light.png'});
 await button('Open original').click();assert.deepEqual((await page.evaluate(()=>window.calls)).at(-1),['open',3]);
 await page.keyboard.press('Escape');assert.equal(await page.locator('dialog').evaluate(e=>e.open),false);assert.equal(await button('Preview attachment').evaluate(e=>e===document.activeElement),true);
});
test('PDF failures retry; late page results cannot reopen a closed dialog',async()=>{
 await setup({previewError:true},'Files',true);await button('Preview attachment').click();await page.getByRole('alert').waitFor();await page.evaluate(()=>window.fixture.previewError=false);await button('Retry preview').click();await page.getByText('Page 1 of 2',{exact:true}).waitFor();
 await page.screenshot({path:root+'.cache/pdf-preview-dark.png'});
 await page.evaluate(()=>window.fixture.delay=true);await button('Next page').click();await page.getByText('Loading preview...',{exact:true}).waitFor();await page.keyboard.press('Escape');await page.evaluate(()=>window.release());assert.equal(await page.locator('dialog').evaluate(e=>e.open),false);
});
test('draft save remains pending until server confirmation, with retry and provider wording',async()=>{
 await setup({},'Status');await button('Save to Gmail').waitFor();await button('Save to Gmail').click();await page.getByText('Saved on this device. Waiting for Gmail...',{exact:true}).waitFor();assert.equal(await page.getByText('Saved to Gmail',{exact:true}).count(),0);
 await emit('synced');await page.getByText('Saved to Gmail',{exact:true}).waitFor();assert.equal(await button('Save to Gmail').count(),0);
 await emit('failed');await button('Retry save to Gmail').waitFor();await button('Retry save to Gmail').click();await page.getByText('Saved on this device. Waiting for Gmail...',{exact:true}).waitFor();
 await setup({state:'synced',gmail:false},'Status');await page.getByText('Saved to mail server',{exact:true}).waitFor();
});
test('draft status errors and stale receipts never become a false confirmation',async()=>{
 await setup({error:true},'Status');await button('Check status').waitFor();await page.evaluate(()=>window.fixture.error=false);await button('Check status').click();await button('Save to Gmail').waitFor();
 await page.evaluate(()=>{window.fixture.delay=true;window.fixture.state='synced';for(const fn of window.listeners['draft:sync'])fn({payload:{draftId:42}});});
 await page.evaluate(()=>{window.fixture.delay=false;window.fixture.state='local';for(const fn of window.listeners['draft:sync'])fn({payload:{draftId:42}});});
 await button('Save to Gmail').waitFor();await page.evaluate(()=>window.release());await button('Save to Gmail').waitFor();assert.equal(await page.getByText('Saved to Gmail',{exact:true}).count(),0);
 await page.evaluate(async()=>{await window.testApp.unmount(window.instance);});assert.equal(await page.evaluate(()=>Object.values(window.listeners).flat().length),0);
});


test('opening rich text is not an edit; typed changes preserve existing formatting',async()=>{
 await setup({},'Rich');const editor=page.locator('[contenteditable=true]');await editor.waitFor();
 await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
 assert.deepEqual(await page.evaluate(()=>window.calls),[]);
 await editor.focus();await page.keyboard.press('Control+End');await page.keyboard.type(' New words');
 await page.waitForFunction(()=>window.calls.some(c=>c[0]==='edit'&&c[2].includes('New words')));
 const last=(await page.evaluate(()=>window.calls)).at(-1);assert.match(last[1],/<b>this<\/b>/);assert.match(last[2],/New words/);
});
