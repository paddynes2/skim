import test from "node:test";
import assert from "node:assert/strict";
import { listLayout } from "../layout/mail-list.ts";

const now = new Date(2026, 9, 13, 12).getTime();
const day = 86400000;
const headers = (layout) => layout.items.filter((i) => !i.thread).map((i) => i.bucket);
const order = (layout) => layout.items.map((i) => (i.thread ? i.thread.id : i.bucket));

test("the leading Priority rows get their own section, then the date sections resume", () => {
  // The query sorts Priority first: an older Priority row (9) sits above today's mail.
  const rows = [
    { id: 9, date: (now - 3 * day) / 1000 },
    { id: 8, date: now / 1000 },
    { id: 1, date: now / 1000 },
    { id: 2, date: (now - 3 * day) / 1000 },
  ];
  const layout = listLayout(rows, 80, true, now, new Set([9, 8]));
  assert.deepEqual(order(layout), ["priority", 9, 8, "today", 1, "earlier", 2]);
  assert.equal(layout.height, 3 * 28 + 4 * 80);
});

test("no set, or a set the rows do not lead with, changes nothing", () => {
  const rows = [{ id: 1, date: now / 1000 }, { id: 2, date: now / 1000 }];
  assert.deepEqual(headers(listLayout(rows, 80, true, now)), ["today"]);
  assert.deepEqual(headers(listLayout(rows, 80, true, now, new Set())), ["today"]);
  // A stale id further down is not pulled into a Priority header.
  assert.deepEqual(headers(listLayout(rows, 80, true, now, new Set([2]))), ["today"]);
});

test("flat message rows never take the Priority header", () => {
  const rows = [{ id: 9, messageId: 90, date: now / 1000 }];
  assert.deepEqual(headers(listLayout(rows, 80, true, now, new Set([9]))), ["today"]);
});

test("ungrouped lists have no headers at all", () => {
  const rows = [{ id: 9, date: now / 1000 }];
  assert.deepEqual(headers(listLayout(rows, 80, false, now, new Set([9]))), []);
});

import { keepOpenInPriority, priorityRun } from "../priority/sticky.ts";

const row = (id, messageId = null) => ({ id, messageId, date: 0 });
const ids = (rows) => rows.map((r) => r.id);

test("the open Priority conversation keeps its place after it is read", () => {
  const prev = [row(1), row(2), row(3), row(9), row(8)];
  const set = new Set([1, 2, 3]);
  // Opened 2 and read it: the query now sorts it by date, at the bottom.
  const next = [row(1), row(3), row(9), row(8), row(2)];
  const kept = keepOpenInPriority(prev, next, 2, set);
  assert.deepEqual(ids(kept.rows), [1, 2, 3, 9, 8]);
  assert.equal(kept.held, 2);
});

test("a row outside the Priority section, or nothing open, is left alone", () => {
  const prev = [row(1), row(9), row(8)];
  const next = [row(1), row(8), row(9)];
  assert.deepEqual(ids(keepOpenInPriority(prev, next, 9, new Set([1])).rows), [1, 8, 9]);
  assert.equal(keepOpenInPriority(prev, next, 9, new Set([1])).held, null);
  assert.deepEqual(ids(keepOpenInPriority(prev, next, null, new Set([1])).rows), [1, 8, 9]);
  // Gone from the list (archived): nothing to hold.
  assert.equal(keepOpenInPriority([row(1)], [row(8)], 1, new Set([1])).held, null);
});

test("the held row never lands below the end of the Priority section", () => {
  // Was 3rd of three; 2 was read elsewhere meanwhile, so the run is shorter.
  const prev = [row(1), row(2), row(3), row(9)];
  const next = [row(1), row(9), row(3), row(2)];
  const kept = keepOpenInPriority(prev, next, 3, new Set([1, 2, 3]));
  assert.deepEqual(ids(kept.rows), [1, 3, 9, 2]);
});

test("message rows never count as Priority", () => {
  assert.equal(priorityRun([row(1, 10), row(2)], new Set([1, 2])), 0);
});
