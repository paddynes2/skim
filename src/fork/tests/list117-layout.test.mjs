import test from "node:test";
import assert from "node:assert/strict";
import { cleanPreview, dateBucket, listLayout, listWindow } from "../layout/mail-list.ts";

test("date sections keep exact virtual offsets without dropping boundary rows", () => {
  const now = new Date(2026, 9, 7, 12).getTime();
  const rows = [0, 1, 2, 3].map((id) => ({ id, date: (now - id * 86400000) / 1000 }));
  assert.deepEqual(rows.map(r => dateBucket(r.date, now)), ["today", "yesterday", "earlier", "earlier"]);
  const layout = listLayout(rows, 80, true, now);
  assert.deepEqual(layout.items.map(i => i.top), [0, 28, 108, 136, 216, 244, 324]);
  assert.equal(layout.height, 404);
  assert.deepEqual(listWindow(layout.items, 108, 108, 0), { start: 2, end: 4 });
  const plain = listLayout(rows, 36, false, now);
  assert.equal(plain.height, 144);
  assert.deepEqual(listWindow(plain.items, 143, 100, 0), { start: 3, end: 4 });
});

test("previews trim quoted tails but preserve normal sentences", () => {
  assert.equal(cleanPreview("Please review. On Tuesday, Anna wrote: old reply"), "Please review.");
  assert.equal(cleanPreview("Thanks! Sent from my iPhone"), "Thanks!");
  assert.equal(cleanPreview("On Tuesday we meet to discuss the original message."), "On Tuesday we meet to discuss the original message.");
  assert.equal(cleanPreview("  A\n useful\tpreview "), "A useful preview");
});
