// node --test src/fork/tests  (Node 22 strips the TypeScript types)
import test from "node:test";
import assert from "node:assert/strict";
import { dueReminders, minutesUntil } from "../prep/due.ts";

const NOW = 1_800_000_000;
const ev = (id, startTs) => ({ id, summary: `E${id}`, startTs, guests: [{ email: "a@x", name: null }] });

test("minutesUntil rounds to whole minutes and never goes negative", () => {
  assert.equal(minutesUntil(NOW + 540, NOW), 9);
  assert.equal(minutesUntil(NOW + 29, NOW), 0);
  assert.equal(minutesUntil(NOW + 31, NOW), 1);
  assert.equal(minutesUntil(NOW - 120, NOW), 0);
});

test("dueReminders toasts each event once, earliest first", () => {
  const seen = new Set();
  const first = dueReminders([ev(2, NOW + 300), ev(1, NOW + 60)], seen);
  assert.deepEqual(first.map((e) => e.id), [1, 2]);
  // The same events on the next poll are not due again; a new one is.
  const second = dueReminders([ev(2, NOW + 240), ev(1, NOW), ev(3, NOW + 500)], seen);
  assert.deepEqual(second.map((e) => e.id), [3]);
  assert.deepEqual([...seen].sort(), [1, 2, 3]);
});
