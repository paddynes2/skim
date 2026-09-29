// node --test src/fork/tests  (Node 22 strips the TypeScript types)
import test from "node:test";
import assert from "node:assert/strict";
import { fmtDuration, parseTime, shiftEnd, timeOptions } from "../calendar/time.ts";

test("parses what people type", () => {
  assert.equal(parseTime("14:30"), "14:30");
  assert.equal(parseTime("1430"), "14:30");
  assert.equal(parseTime("2pm"), "14:00");
  assert.equal(parseTime("2:30 PM"), "14:30");
  assert.equal(parseTime("12am"), "00:00");
  assert.equal(parseTime("12pm"), "12:00");
  assert.equal(parseTime("9.15a"), "09:15");
  assert.equal(parseTime("9"), "09:00");
  assert.equal(parseTime("2"), "14:00");
  assert.equal(parseTime("02:00"), "02:00");
  assert.equal(parseTime("930"), "09:30");
  assert.equal(parseTime("25:00"), null);
  assert.equal(parseTime("10:75"), null);
  assert.equal(parseTime("13pm"), null);
  assert.equal(parseTime("soon"), null);
  assert.equal(parseTime(""), null);
});

test("end options carry durations from the start", () => {
  const o = timeOptions("11:30");
  assert.deepEqual(o[0], { value: "11:45", note: "15 min" });
  assert.deepEqual(o[1], { value: "12:00", note: "30 min" });
  assert.deepEqual(o[5], { value: "13:00", note: "1 h 30" });
  assert.equal(o.at(-1).value, "23:45");
  assert.equal(timeOptions(null).length, 96);
  assert.equal(fmtDuration(120), "2 h");
});

test("moving the start keeps the length", () => {
  assert.deepEqual(
    shiftEnd({ date: "2026-10-01", time: "11:30" }, { date: "2026-10-01", time: "12:00" }, { date: "2026-10-01", time: "23:45" }),
    { date: "2026-10-02", time: "00:15" },
  );
  assert.deepEqual(
    shiftEnd({ date: "2026-10-01", time: "12:00" }, { date: "2026-10-01", time: "11:00" }, { date: "2026-10-01", time: "13:00" }),
    { date: "2026-10-01", time: "11:00" },
  );
});
