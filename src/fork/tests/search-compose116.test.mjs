import test from "node:test";
import assert from "node:assert/strict";
import {register} from "node:module";
register("./ts-resolve.mjs",import.meta.url);
const {highlightParts,matchingPassage}=await import("../search/passages.ts");
const {groupAttachments}=await import("../attachments/groups.ts");
const {parseSnippets,addSnippet,mentionsMissingAttachment,serialSaves,transferDraft}=await import("../compose/workflow.ts");
test("search passage shows late matches and highlights escaped Unicode text",()=>{
  const text="Intro. ".repeat(80)+"Caf\u00e9 <script>alert(1)</script> agreement is signed.";
  const snippet=matchingPassage(text,"cafe agreement");assert.match(snippet,/Caf\u00e9/);assert.ok(snippet.length<225);
  const parts=highlightParts(snippet,"cafe agreement");assert.ok(parts.some(p=>p.match&&p.text==="Caf\u00e9"));assert.equal(parts.map(p=>p.text).join(""),snippet);
  assert.equal(highlightParts("proposal [v2].pdf",'filename:"[v2].pdf"').filter(p=>p.match).map(p=>p.text).join(""),"[v2].pdf");
});
test("attachment grouping never merges same-name versions without equal content hashes",()=>{
  const files=[1,2,3,4].map(id=>({id,messageId:id,filename:"proposal.pdf",size:100,mimeType:"application/pdf",isInline:false}));
  assert.deepEqual(groupAttachments(files,{1:"a",2:"a",3:"b"}).map(g=>g.map(f=>f.id)),[[1,2],[3],[4]]);
  assert.equal(groupAttachments(files,{}).length,4);
});
test("snippets preserve exact text and attachment checks ignore quoted promises",()=>{
  const rows=addSnippet([],"Follow-up","A reusable paragraph.\nSecond line.");assert.deepEqual(parseSnippets(JSON.stringify(rows)),rows);
  assert.throws(()=>addSnippet(rows,"FOLLOW-UP","replacement"));assert.throws(()=>parseSnippets('[{"name":"lost"}]'));
  assert.equal(mentionsMissingAttachment("Please see the attached proposal.",0),true);assert.equal(mentionsMissingAttachment("Please see the attached proposal.",1),false);
  assert.equal(mentionsMissingAttachment("Thanks.\nOn Tuesday someone wrote:\n> See the attached file",0),false);assert.equal(mentionsMissingAttachment("No attachment is needed.",0),false);
});
test("serialized autosaves finish before popout and failed transitions keep the editor",async()=>{
  let release;const gate=new Promise(resolve=>release=resolve);const calls=[];
  const save=serialSaves(async snapshot=>{if(snapshot.body==="old")await gate;calls.push(snapshot);});
  const first=save({body:"old",files:[]});let finished=false;
  const moving=transferDraft(()=>save({body:"latest",files:["proposal.pdf"]}),async()=>{assert.equal(calls.at(-1).body,"latest");assert.deepEqual(calls.at(-1).files,["proposal.pdf"]);},()=>finished=true);
  await Promise.resolve();assert.equal(finished,false);release();await first;await moving;assert.equal(finished,true);assert.deepEqual(calls.map(c=>c.body),["old","latest"]);
  finished=false;await assert.rejects(transferDraft(async()=>{throw Error("disk");},async()=>assert.fail("Must not open"),()=>finished=true));assert.equal(finished,false);
  await assert.rejects(transferDraft(async()=>{},async()=>{throw Error("window");},()=>finished=true));assert.equal(finished,false);
});
