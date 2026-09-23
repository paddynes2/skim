// node --test src/fork/tests  (Node 22 strips the TypeScript types)
import test from "node:test";
import assert from "node:assert/strict";
import { insertAt, keyOf, pushBounded, withoutPending } from "../undo-filter.ts";

const rows = [
  { id: 1, messageId: null },
  { id: 2, messageId: null },
  { id: 3, messageId: 30 },
];

test("keyOf prefers the message id (flat mode)", () => {
  assert.equal(keyOf(rows[0]), 1);
  assert.equal(keyOf(rows[2]), 30);
});

test("withoutPending drops held rows and is identity when nothing is held", () => {
  assert.equal(withoutPending(rows, new Set()), rows);
  assert.deepEqual(withoutPending(rows, new Set([2, 30])).map(keyOf), [1]);
});

test("insertAt restores at the old index, clamps, and never duplicates", () => {
  const back = { id: 2, messageId: null };
  const without = withoutPending(rows, new Set([2]));
  assert.deepEqual(insertAt(without, back, 1).map(keyOf), [1, 2, 30]);
  assert.deepEqual(insertAt(without, back, 99).map(keyOf), [1, 30, 2]);
  assert.deepEqual(insertAt(without, back, -5).map(keyOf), [2, 1, 30]);
  assert.equal(insertAt(rows, back, 1), rows, "already present: untouched");
});

test("pushBounded keeps the newest N", () => {
  let s = [];
  for (let i = 0; i < 25; i++) s = pushBounded(s, i, 20);
  assert.equal(s.length, 20);
  assert.equal(s[0], 5);
  assert.equal(s[19], 24);
});
