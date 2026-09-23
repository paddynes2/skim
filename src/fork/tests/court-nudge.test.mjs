// node --test src/fork/tests/court-nudge.test.mjs  (Node 22 strips the types)
// The pure half of the Phase 10 nudge and row ages. Dates are built with the
// LOCAL constructor, so the assertions hold in any zone.
import test from "node:test";
import assert from "node:assert/strict";
import { ageDays, ageLabel, ageTone, nudgeParts, parseHhmm, shouldNudgeLocal, shouldNudgeNow } from "../court/nudge.ts";

const local = (y, m, d, h = 0, mi = 0) => new Date(y, m - 1, d, h, mi, 0, 0);

test("parseHhmm reads HH:MM, refuses off / blank / junk / out of range", () => {
  assert.equal(parseHhmm("09:00"), 540);
  assert.equal(parseHhmm(" 9:05 "), 545);
  assert.equal(parseHhmm("23:59"), 1439);
  assert.equal(parseHhmm("off"), null);
  assert.equal(parseHhmm("OFF"), null);
  assert.equal(parseHhmm(""), null);
  assert.equal(parseHhmm("24:00"), null);
  assert.equal(parseHhmm("09:60"), null);
  assert.equal(parseHhmm("nine"), null);
});

test("shouldNudgeLocal fires once a day, at or after the time, never before", () => {
  const setting = "09:00";
  // Before the time: never, even with no nudge on record.
  assert.equal(shouldNudgeLocal(setting, null, local(2026, 9, 23, 8, 59)), false);
  // At the time, nothing on record: fire.
  assert.equal(shouldNudgeLocal(setting, null, local(2026, 9, 23, 9, 0)), true);
  // Already fired today (earlier): not again.
  assert.equal(shouldNudgeLocal(setting, local(2026, 9, 23, 9, 0), local(2026, 9, 23, 17, 30)), false);
  // Fired yesterday: fire today once the time has passed.
  assert.equal(shouldNudgeLocal(setting, local(2026, 9, 22, 9, 0), local(2026, 9, 23, 9, 1)), true);
  assert.equal(shouldNudgeLocal(setting, local(2026, 9, 22, 9, 0), local(2026, 9, 23, 8, 0)), false);
  // A day missed (app closed) is not made up twice: one nudge on the next day.
  assert.equal(shouldNudgeLocal(setting, local(2026, 9, 20, 9, 0), local(2026, 9, 23, 12, 0)), true);
  // A "last" in the future (clock moved back) counts as done.
  assert.equal(shouldNudgeLocal(setting, local(2026, 9, 24, 9, 0), local(2026, 9, 23, 12, 0)), false);
  // "off" never fires.
  assert.equal(shouldNudgeLocal("off", null, local(2026, 9, 23, 12, 0)), false);
});

test("shouldNudgeNow is the unix-seconds shape of the same rule", () => {
  const at = local(2026, 9, 23, 9, 30).getTime() / 1000;
  assert.equal(shouldNudgeNow("09:00", null, at), true);
  assert.equal(shouldNudgeNow("10:00", null, at), false);
  assert.equal(shouldNudgeNow("09:00", at - 60, at), false);
  assert.equal(shouldNudgeNow("09:00", at - 86400, at), true);
});

test("ages: days floor at 0, label picks d / h / now, tone follows the thresholds", () => {
  const now = 1_800_000_000;
  assert.equal(ageDays(now - 5 * 86400 - 10, now), 5);
  assert.equal(ageDays(now + 100, now), 0);
  assert.equal(ageLabel(now - 3 * 86400, now), "3d");
  assert.equal(ageLabel(now - 5 * 3600, now), "5h");
  assert.equal(ageLabel(now - 59, now), "now");
  assert.equal(ageTone(0, 2, 5), "none");
  assert.equal(ageTone(2, 2, 5), "none");
  assert.equal(ageTone(3, 2, 5), "amber");
  assert.equal(ageTone(5, 2, 5), "amber");
  assert.equal(ageTone(6, 2, 5), "red");
});

test("nudgeParts names the oldest and its age, and is null with nothing on him", () => {
  const now = 1_800_000_000;
  const oldest = { fromName: "Anna Lee", fromAddr: "anna@x", since: now - 5 * 86400 };
  assert.deepEqual(nudgeParts(2, oldest, now), { n: 2, who: "Anna Lee", days: 5 });
  assert.deepEqual(nudgeParts(1, { ...oldest, fromName: "  " }, now), { n: 1, who: "anna@x", days: 5 });
  assert.equal(nudgeParts(0, null, now), null);
  assert.equal(nudgeParts(0, oldest, now), null);
});
