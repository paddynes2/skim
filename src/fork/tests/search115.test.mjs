import test from "node:test";
import assert from "node:assert/strict";
import {register} from "node:module";
register("./ts-resolve.mjs",import.meta.url);
const {readFilters,applyFilters,parseSavedSearches,saveSearch}=await import("../search/filters.ts");
test("filter controls preserve unrelated and negative operators",()=>{
  const query='budget subject:"plan 2026" -from:spam from:old before:2026-01-01 is:starred';
  const next=applyFilters(query,{...readFilters(query),sender:"new person",company:"acme.example,person@gmail.com",before:"2026-10-07",attachments:true});
  assert.equal(next,'budget subject:"plan 2026" -from:spam is:starred from:"new person" company:"acme.example,person@gmail.com" before:"2026-10-07" has:attachment');
  assert.equal(readFilters(next).company,"acme.example,person@gmail.com");
  assert.equal(readFilters(next).attachments,true);
  const multiple="from:alpha from:beta -has:attachment";
  assert.equal(applyFilters(multiple,readFilters(multiple)),multiple);
});
test("saved searches round trip and invalid storage cannot become an empty overwrite",()=>{
  const rows=saveSearch([],"Deal files",'company:"acme.example" has:attachment');
  assert.deepEqual(parseSavedSearches(JSON.stringify(rows)),rows);
  assert.throws(()=>parseSavedSearches('{broken'));
  assert.throws(()=>parseSavedSearches('[{"name":"lost"}]'));
  assert.throws(()=>saveSearch(rows,"deal files","new"));
  assert.throws(()=>saveSearch([]," ","query"));
  assert.throws(()=>saveSearch(Array.from({length:30},(_,i)=>({name:`q${i}`,query:"q"})),"31","q"));
});
