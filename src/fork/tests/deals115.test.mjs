import test from "node:test";
import assert from "node:assert/strict";
import {dealRows,serializeDealRows,invalidDealName} from "../deals/rows.ts";
test("company row edits preserve untouched malformed entries and notes",()=>{
  const source="# local note\n  Acme : acme.example, gmail.com\nmalformed entry !\nmailto:person@example.com\n";
  const rows=dealRows(source);
  assert.equal(serializeDealRows(rows),source);
  assert.equal(rows[3].entries,"mailto:person@example.com");
  rows[1].name="New company";rows[1].edited=true;
  assert.equal(serializeDealRows(rows),"# local note\nNew company: acme.example, gmail.com\nmalformed entry !\nmailto:person@example.com\n");
  assert.equal(serializeDealRows(dealRows(source)),source);
  assert.ok(invalidDealName("Name: injected"));assert.ok(invalidDealName("#hidden"));assert.ok(!invalidDealName("Acme & Sons"));
});
