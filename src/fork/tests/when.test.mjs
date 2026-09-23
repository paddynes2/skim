// node --test src/fork/tests  (Node 22 strips the TypeScript types)
import test from "node:test";
import assert from "node:assert/strict";
import { formatWhen, parseWhen, presets } from "../compose/when.ts";

// Wednesday 2026-09-23 10:30 local.
const NOW = new Date(2026, 8, 23, 10, 30, 0, 0);
const local = (y, m, d, h, mi) => new Date(y, m - 1, d, h, mi, 0, 0);

test("presets: tomorrow 08:00, next Monday 08:00, in two hours", () => {
  const p = Object.fromEntries(presets(NOW).map((x) => [x.key, x.when]));
  assert.deepEqual(p.tomorrow, local(2026, 9, 24, 8, 0));
  assert.deepEqual(p.monday, local(2026, 9, 28, 8, 0));
  assert.deepEqual(p.in2h, local(2026, 9, 23, 12, 30));
  // On a Monday, "Monday" is next week's.
  const mon = local(2026, 9, 28, 9, 0);
  assert.deepEqual(presets(mon).find((x) => x.key === "monday").when, local(2026, 10, 5, 8, 0));
});

test("durations", () => {
  assert.deepEqual(parseWhen("3d", NOW), local(2026, 9, 26, 10, 30));
  assert.deepEqual(parseWhen("in 2 hours", NOW), local(2026, 9, 23, 12, 30));
  assert.deepEqual(parseWhen("45m", NOW), local(2026, 9, 23, 11, 15));
  assert.deepEqual(parseWhen("1w", NOW), local(2026, 9, 30, 10, 30));
  assert.equal(parseWhen("0d", NOW), null);
});

test("day words and weekdays default to 08:00", () => {
  assert.deepEqual(parseWhen("tomorrow", NOW), local(2026, 9, 24, 8, 0));
  assert.deepEqual(parseWhen("tomorrow 9am", NOW), local(2026, 9, 24, 9, 0));
  assert.deepEqual(parseWhen("Tomorrow 9:30 PM", NOW), local(2026, 9, 24, 21, 30));
  assert.deepEqual(parseWhen("fri 14:00", NOW), local(2026, 9, 25, 14, 0));
  assert.deepEqual(parseWhen("friday", NOW), local(2026, 9, 25, 8, 0));
  assert.deepEqual(parseWhen("next mon", NOW), local(2026, 9, 28, 8, 0));
  assert.deepEqual(parseWhen("next week", NOW), local(2026, 9, 28, 8, 0));
  // Today's weekday: ahead of now stays today, otherwise next week.
  assert.deepEqual(parseWhen("wed 15:00", NOW), local(2026, 9, 23, 15, 0));
  assert.deepEqual(parseWhen("wed", NOW), local(2026, 9, 30, 8, 0), "08:00 today has passed");
  assert.deepEqual(parseWhen("wed 9am", NOW), local(2026, 9, 30, 9, 0));
});

test("a bare time is today, or tomorrow once it has passed", () => {
  assert.deepEqual(parseWhen("14:00", NOW), local(2026, 9, 23, 14, 0));
  assert.deepEqual(parseWhen("at 9am", NOW), local(2026, 9, 24, 9, 0));
  assert.deepEqual(parseWhen("noon", NOW), local(2026, 9, 23, 12, 0));
  assert.deepEqual(parseWhen("today 11:00", NOW), local(2026, 9, 23, 11, 0));
  assert.equal(parseWhen("today 9:00", NOW), null, "already gone");
});

test("garbage and ambiguity are refused", () => {
  for (const s of ["", "   ", "soon", "fri 14", "25:00", "13pm", "9:75", "yesterday", "mon tue"]) {
    assert.equal(parseWhen(s, NOW), null, JSON.stringify(s));
  }
});

test("labels", () => {
  assert.equal(formatWhen(local(2026, 9, 23, 14, 0), NOW), "Today 14:00");
  assert.equal(formatWhen(local(2026, 9, 24, 8, 0), NOW), "Tomorrow 08:00");
  assert.equal(formatWhen(local(2026, 9, 25, 14, 5), NOW), "Fri 14:05");
  assert.equal(formatWhen(local(2026, 9, 30, 8, 0), NOW), "30 Sep 08:00");
});
