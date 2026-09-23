// The pure half of the reminder (no store, no i18n) so `node --test
// src/fork/tests` can drive it. `reminder.ts` owns the timer and the toast.
import type { UpcomingEvent } from "./types";

/** Whole minutes from `nowSecs` to the event start, never below 0. */
export function minutesUntil(startTs: number, nowSecs: number): number {
  return Math.max(0, Math.round((startTs - nowSecs) / 60));
}

/** The events not yet toasted, earliest first. Marks them seen. */
export function dueReminders(events: UpcomingEvent[], seen: Set<number>): UpcomingEvent[] {
  const out: UpcomingEvent[] = [];
  for (const e of [...events].sort((a, b) => a.startTs - b.startTs || a.id - b.id)) {
    if (seen.has(e.id)) continue;
    seen.add(e.id);
    out.push(e);
  }
  return out;
}
