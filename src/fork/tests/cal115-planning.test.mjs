import test from "node:test";
import assert from "node:assert/strict";
import { monthDays, overlappingEvents, availableCandidates } from "../calendar/planning.ts";

test("month navigator starts Monday and crosses leap-year boundaries", () => {
  const days = monthDays(new Date(2028, 1, 20));
  assert.equal(days.length, 42);
  assert.equal(days[0].getDay(), 1);
  assert.equal(days.filter((d) => d.getMonth() === 1).length, 29);
  assert.equal(days[41].getDay(), 0);
});
test("conflicts exclude touching endpoints, declined, free and cancelled events", () => {
  const rows = [
    { id: 1, start_ts: 100, end_ts: 200 },
    { id: 2, start_ts: 200, end_ts: 300 },
    { id: 3, start_ts: 150, end_ts: 250, self_response: "declined" },
    { id: 4, start_ts: 150, end_ts: 250, transparency: "transparent" },
    { id: 5, start_ts: 150, end_ts: 250, status: "cancelled" },
    { id: 6, start_ts: 150, end_ts: 250 },
  ];
  assert.deepEqual(overlappingEvents(rows, 100, 200, 6).map((r) => r.id), [1]);
  assert.deepEqual(overlappingEvents(rows, 200, 100), []);
});
test("slot suggestions never call unknown-only results available", () => {
  const slot = { start: 100, end: 200 };
  const unknown = { email: "unknown@example.test", status: "unknown", busy: [] };
  const result = { from_ts: 0, to_ts: 500, calendars: [unknown] };
  assert.deepEqual(availableCandidates(result, [slot]), []);
  result.calendars.push({ email: "self@example.test", status: "available", busy: [{ start_ts: 200, end_ts: 300 }] });
  assert.deepEqual(availableCandidates(result, [slot, { start: 150, end: 250 }, { start: 450, end: 550 }]), [slot]);
});

test("all-day conflicts use editor-zone dates, not stored UTC midnights", () => {
  const rows = [{ id: 1, all_day: true, start_date: "2026-10-08", end_date: "2026-10-09", start_ts: Date.parse("2026-10-08T00:00Z") / 1000, end_ts: Date.parse("2026-10-09T00:00Z") / 1000 }];
  const boundary = (date) => Date.parse(`${date}T00:00:00+02:00`) / 1000;
  const start = Date.parse("2026-10-07T22:30Z") / 1000;
  assert.equal(overlappingEvents(rows, start, start + 1800, undefined, boundary).length, 1);
  const after = Date.parse("2026-10-08T22:00Z") / 1000;
  assert.equal(overlappingEvents(rows, after, after + 1800, undefined, boundary).length, 0);
});

test("known busy periods still block suggestions on a partially unknown calendar", () => {
  const result = { from_ts: 0, to_ts: 500, calendars: [
    { email: "self@example.test", status: "unknown", busy: [{ start_ts: 100, end_ts: 200 }] },
    { email: "guest@example.test", status: "available", busy: [] },
  ] };
  assert.deepEqual(availableCandidates(result, [{ start: 100, end: 200 }, { start: 200, end: 300 }]), [{ start: 200, end: 300 }]);
});
