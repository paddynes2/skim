// node --test src/fork/tests  (Node 22 strips the TypeScript types)
import test from "node:test";
import assert from "node:assert/strict";
import { fmtDue, followupPresets, snoozePresets } from "../reminders/when.ts";

// Thursday 1 October 2026, 10:15 local.
const THU = new Date(2026, 9, 1, 10, 15);
// Friday 2 October 2026, 17:30 local.
const FRI_EVE = new Date(2026, 9, 2, 17, 30);

test("follow-ups count working days and land at 08:00", () => {
  const p = Object.fromEntries(followupPresets(FRI_EVE).map((x) => [x.id, x.at]));
  assert.equal(p["1d"].getDay(), 1, "Friday + 1 working day = Monday");
  assert.equal(p["1d"].getHours(), 8);
  assert.equal(p["3d"].getDate(), 7, "Wednesday 7 Oct");
  assert.equal(p["1w"].getDate(), 9, "next Friday");
});

test("snooze presets fit the time of day and week", () => {
  const thu = snoozePresets(THU).map((p) => p.id);
  assert.deepEqual(thu, ["later", "evening", "tomorrow", "weekend", "next_week"]);
  const later = snoozePresets(THU)[0].at;
  assert.equal(later.getHours(), 13);
  assert.equal(later.getMinutes(), 0);
  const fri = snoozePresets(FRI_EVE).map((p) => p.id);
  assert.deepEqual(fri, ["tomorrow", "next_week"], "no later/evening at 17:30, no weekend on Friday");
  const nextWeek = snoozePresets(FRI_EVE).at(-1).at;
  assert.equal(nextWeek.getDay(), 1);
  assert.equal(nextWeek.getDate(), 5);
});

test("due times read relative to today", () => {
  const ts = (d) => Math.floor(d.getTime() / 1000);
  assert.equal(fmtDue(ts(new Date(2026, 9, 1, 15, 30)), THU), "Today 15:30");
  assert.equal(fmtDue(ts(new Date(2026, 9, 2, 8, 0)), THU), "Tomorrow 08:00");
  assert.equal(fmtDue(ts(new Date(2026, 9, 5, 8, 0)), THU), "Mon 08:00");
  assert.match(fmtDue(ts(new Date(2026, 9, 20, 8, 0)), THU), /^Tue 20 Oct 08:00$/);
});
