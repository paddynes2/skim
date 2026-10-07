import test from "node:test";
import assert from "node:assert/strict";
import { zonedParts, zonedTimestamp, eventTimestamp, makeRecurrence, changedOptions, eventOptions } from "../calendar/editor.ts";

test("time zones preserve instants and reject invalid wall times", () => {
  assert.equal(zonedTimestamp("2026-10-07", "13:00", "Africa/Johannesburg"), Date.parse("2026-10-07T11:00:00Z") / 1000);
  assert.deepEqual(zonedParts(Date.parse("2026-10-07T11:00:00Z") / 1000, "America/New_York"), { date: "2026-10-07", time: "07:00" });
  assert.equal(zonedTimestamp("2026-03-08", "02:30", "America/New_York"), null);
  assert.equal(zonedTimestamp("2026-11-01", "01:30", "America/New_York"), Date.parse("2026-11-01T05:30:00Z") / 1000);
  assert.equal(eventTimestamp("2026-11-01", "01:30", "America/New_York", Date.parse("2026-11-01T06:30:00Z") / 1000), Date.parse("2026-11-01T06:30:00Z") / 1000);
  assert.equal(zonedTimestamp("2026-02-30", "13:00", "UTC"), null);
  assert.equal(zonedTimestamp("2026-10-07", "25:00", "UTC"), null);
  assert.equal(zonedTimestamp("2026-10-07", "13:00", "Wrong/Zone"), null);
});
const rule = { frequency: "WEEKLY", interval: 2, days: ["MO", "WE"], monthly: "date", end: "count", count: 8, until: "2026-11-01" };
test("custom recurrence retains chosen weekdays, interval and end", () => {
  assert.deepEqual(makeRecurrence(rule, "2026-10-07", false, "Africa/Johannesburg"), ["RRULE:FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE;COUNT=8"]);
  assert.deepEqual(makeRecurrence({ ...rule, frequency: "MONTHLY", monthly: "weekday", end: "never" }, "2026-10-30", false, "UTC"), ["RRULE:FREQ=MONTHLY;INTERVAL=2;BYDAY=-1FR"]);
  assert.deepEqual(makeRecurrence({ ...rule, frequency: "MONTHLY", monthly: "date", end: "never" }, "2026-10-07", false, "UTC"), ["RRULE:FREQ=MONTHLY;INTERVAL=2;BYMONTHDAY=7"]);
  assert.deepEqual(makeRecurrence({ ...rule, end: "date" }, "2026-10-07", true, "UTC"), ["RRULE:FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE;UNTIL=20261101"]);
  assert.deepEqual(makeRecurrence({ ...rule, end: "date" }, "2026-10-07", false, "Africa/Johannesburg"), ["RRULE:FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE;UNTIL=20261101T215959Z"]);
  for (const change of [{ days: [] }, { interval: 0 }, { interval: 1.5 }, { count: 0 }, { end: "date", until: "2026-09-01" }]) assert.equal(makeRecurrence({ ...rule, ...change }, "2026-10-07", false, "UTC"), null);
});
test("unchanged advanced settings stay out of patches and explicit clears survive", () => {
  const before = eventOptions({ recurrence: ["RRULE:FREQ=YEARLY", "EXDATE:20261007"], reminders: { useDefault: false, overrides: [{ method: "email", minutes: 30 }] }, guestsCanModify: true });
  const after = structuredClone(before);
  assert.deepEqual(changedOptions(after, before), {});
  after.visibility = "private";
  assert.deepEqual(changedOptions(after, before), { visibility: "private" });
  after.recurrence = [];
  after.guestsCanModify = false;
  assert.deepEqual(changedOptions(after, before), { recurrence: [], visibility: "private", guestsCanModify: false });
  assert.equal(eventOptions({}, "transparent").transparency, "transparent");
});
