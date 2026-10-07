import test from "node:test";
import assert from "node:assert/strict";
import {readFileSync} from "node:fs";
import ts from "typescript";
import {compileModule} from "svelte/compiler";
import {withoutPending,insertAt} from "../undo-filter.ts";

// Compile the actual store with Svelte's server transform. Reactive rendering is
// covered by the browser suite. This exercises the store's async control flow,
// state and exported methods without a webview or any network/IPC connection.
const source=readFileSync(new URL("../../lib/stores/mail.svelte.ts",import.meta.url),"utf8");
const importNames=[];
const javascript=ts.transpileModule(source,{
  compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.ESNext},
  transformers:{before:[context=>root=>ts.visitNode(root,function visit(node){
    if(ts.isImportDeclaration(node)){
      if(!node.importClause?.isTypeOnly){for(const e of node.importClause?.namedBindings?.elements??[]){if(!e.isTypeOnly)importNames.push(e.name.text);}}
      return undefined;
    }
    return ts.visitEachChild(node,visit,context);
  })]},
}).outputText;
const compiled=compileModule(javascript,{filename:"mail.svelte.js",generate:"server"}).js.code.replace("'svelte/internal/server'",JSON.stringify(import.meta.resolve("svelte/internal/server")));
let serial=0;
function deferred(){let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return{promise,resolve,reject};}
function row(id){return{id,messageId:null,accountId:"a",fromName:"Person",fromAddr:"person@example.com",subject:`Mail ${id}`,date:id,isRead:true,isStarred:false,messageCount:1,hasAttachments:false,snippet:""};}
async function store(){
  const control={read:async()=>[],errors:[],company:""};
  const call=(kind)=>(...args)=>control.read(kind,args);
  const prefs={listOrder:"date",setListOrder(value){this.listOrder=value;}};
  const stubs={
    listen:async()=>()=>{},reportError:(...e)=>control.errors.push(e),t:key=>key,
    api:{listThreads:call("folder"),listMessages:call("flat"),listUnifiedThreads:call("unified"),listUnifiedMessages:call("unified-flat"),setSetting:async()=>{},getSettings:async()=>({active_account:"a"}),listAccounts:async()=>[{id:"a",email:"me@example.com"}],listFolders:async()=>[{id:1,role:"inbox",accountId:"a"}],takePendingOpen:async()=>null},
    prefs,searchApi:{searchThreads:call("search")},SEARCH_FOLDER_ID:-900,withoutToken:(q,t)=>q.replace(t,""),
    COURT_UPDATED:"court",courtApi:{list:call("court")},VF_ON_ME:-920,VF_WAITING:-921,courtCounts:{refresh:async()=>{}},
    dealsApi:{list:call("deals")},VF_DEALS:-924,dealsStore:{get selectedCompany(){return control.company;}},dealsCount:{refresh:async()=>{}},
    undo:{pendingKeys:new Set()},insertAt,withoutPending,folderSelected:async()=>{},SCHEDULED_FOLDER_ID:-999,
  };
  const key=`__skimStoreTest${++serial}`;globalThis[key]=stubs;
  const text=`const {${importNames.join(",")}} = globalThis[${JSON.stringify(key)}];\n${compiled}\n// instance ${serial}`;
  const module=await import(`data:text/javascript;base64,${Buffer.from(text).toString("base64")}`);
  delete globalThis[key];
  return{mail:module.mail,control,prefs};
}

test("late folder and filter responses cannot replace the current view",async()=>{
  const {mail,control}=await store();
  const old=deferred();control.read=(_kind,args)=>args[0]===1?old.promise:Promise.resolve([row(2)]);
  const pending=mail.selectFolder(1);await mail.selectFolder(2);old.resolve([row(1)]);await pending;
  assert.equal(mail.selectedFolderId,2);assert.deepEqual(mail.threads.map(r=>r.id),[2]);assert.equal(mail.threadsLoading,false);
  const filter=deferred();control.read=(_kind,args)=>args[3]==="unread"?filter.promise:Promise.resolve([row(4)]);
  const changing=mail.setListFilter("unread");await mail.setListFilter("starred");filter.resolve([row(3)]);await changing;
  assert.deepEqual(mail.threads.map(r=>r.id),[4]);assert.equal(mail.listFilter,"starred");
});

test("late search and company pages cannot append to another query in the same view",async()=>{
  const {mail,control}=await store();
  control.read=async()=>[row(1)];await mail.enterSearch("first");
  const old=deferred();control.read=(kind,args)=>kind==="search"&&args[1]>0?old.promise:Promise.resolve([row(2)]);
  const page=mail.loadMoreThreads();await mail.enterSearch("second");old.resolve([row(3)]);await page;
  assert.equal(mail.searchQuery,"second");assert.deepEqual(mail.threads.map(r=>r.id),[2]);
  control.company="First";control.read=async()=>[row(4)];await mail.selectDeals();
  const company=deferred();control.read=(_kind,args)=>args[0]>0?company.promise:Promise.resolve([row(5)]);
  const oldCompany=mail.loadMoreThreads();control.company="Second";await mail.selectDeals();company.resolve([row(6)]);await oldCompany;
  assert.deepEqual(mail.threads.map(r=>r.id),[5]);
});

test("stale read failure does not mark a newer view failed; current page failure is retryable",async()=>{
  const {mail,control}=await store();
  const old=deferred();control.read=(_kind,args)=>args[0]===1?old.promise:Promise.resolve([row(2)]);
  const pending=mail.selectFolder(1);await mail.selectFolder(2);old.reject(new Error("old folder unavailable"));await pending;
  assert.equal(mail.loadFailed,false);assert.deepEqual(mail.threads.map(r=>r.id),[2]);
  control.read=async()=>{throw new Error("page unavailable");};
  await mail.loadMoreThreads();
  assert.equal(mail.loadFailed,true);assert.deepEqual(mail.threads.map(r=>r.id),[2]);
  control.read=async()=>[row(3)];await mail.refreshThreads();
  assert.equal(mail.loadFailed,false);assert.deepEqual(mail.threads.map(r=>r.id),[3]);
});

test("boot initializes request identity before the first folder read",async()=>{
  const {mail,control}=await store();control.read=async()=>[row(1)];await mail.boot();
  for(let i=0;i<8 && mail.threadsLoading;i++)await new Promise(resolve=>setImmediate(resolve));
  assert.equal(mail.booted,true);assert.equal(mail.account.id,"a");assert.deepEqual(mail.threads.map(r=>r.id),[1]);assert.equal(control.errors.length,0);
});

test("background refresh keeps cached rows steady while an empty view shows loading",async()=>{
  const {mail,control}=await store();
  control.read=async()=>[row(1)];await mail.selectFolder(1);
  const refresh=deferred();control.read=()=>refresh.promise;
  const pending=mail.refreshThreads();
  assert.equal(mail.threadsLoading,false);
  assert.deepEqual(mail.threads.map(r=>r.id),[1]);
  refresh.resolve([row(2)]);await pending;
  assert.equal(mail.threadsLoading,false);
  assert.deepEqual(mail.threads.map(r=>r.id),[2]);
  control.read=async()=>[];await mail.selectFolder(2);
  const initial=deferred();control.read=()=>initial.promise;
  const loading=mail.refreshThreads();
  assert.equal(mail.threadsLoading,true);
  initial.resolve([row(3)]);await loading;
  assert.equal(mail.threadsLoading,false);
  assert.deepEqual(mail.threads.map(r=>r.id),[3]);
});
