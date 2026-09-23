// Pure half of the daily nudge (PLAN.md Phase 10): when to fire, and the
// toast's text parts. Mirrors `fork::court::should_nudge_local` in Rust so the
// two sides agree; no DOM, no Svelte, so `node --test` covers it
// (src/fork/tests/court-nudge.test.mjs). The timer and the toast live in
// `CourtNudge.ts`.

/** "HH:MM" -> minutes since local midnight; `null` for "off", blank or junk. */
export function parseHhmm(setting: string): number | null {
  const s = setting.trim();
  if (s === "" || s.toLowerCase() === "off") return null;
  const m = /^(\d{1,2}):(\d{2})$/.exec(s);
  if (!m) return null;
  const h = Number(m[1]);
  const min = Number(m[2]);
  if (h > 23 || min > 59) return null;
  return h * 60 + min;
}

function sameLocalDay(a: Date, b: Date): boolean {
  return a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();
}

/** Fire once per local day, at or after the configured time, never before it,
 *  never twice on one day. Dates are compared on their LOCAL fields, like the
 *  Rust core compares `NaiveDateTime`s. */
export function shouldNudgeLocal(setting: string, lastNudgedAt: Date | null, now: Date): boolean {
  const at = parseHhmm(setting);
  if (at === null) return false;
  if (now.getHours() * 60 + now.getMinutes() < at) return false;
  if (lastNudgedAt === null) return true;
  // A last nudge later than now (clock moved back) counts as today's.
  if (lastNudgedAt.getTime() > now.getTime()) return false;
  return !sameLocalDay(lastNudgedAt, now);
}

/** Unix-seconds wrapper over `shouldNudgeLocal`, the shape the timer uses. */
export function shouldNudgeNow(setting: string, lastNudgedAt: number | null, nowSecs: number): boolean {
  return shouldNudgeLocal(
    setting,
    lastNudgedAt === null ? null : new Date(lastNudgedAt * 1000),
    new Date(nowSecs * 1000),
  );
}

/** Whole local days between `since` and `now` (unix seconds), floored at 0. */
export function ageDays(since: number, nowSecs: number): number {
  return Math.max(0, Math.floor((nowSecs - since) / 86400));
}

/** Short age for a row: "3d" past a day, "5h" past an hour, "now" under it. */
export function ageLabel(since: number, nowSecs: number): string {
  const secs = Math.max(0, nowSecs - since);
  if (secs >= 86400) return `${Math.floor(secs / 86400)}d`;
  if (secs >= 3600) return `${Math.floor(secs / 3600)}h`;
  return "now";
}

/** Which colour an age gets: red past `redDays`, amber past `amberDays`, else none. */
export function ageTone(days: number, amberDays: number, redDays: number): "none" | "amber" | "red" {
  if (days > redDays) return "red";
  if (days > amberDays) return "amber";
  return "none";
}

/** Parts of the nudge line ("On you: N · Oldest: X (5d)"); `null` when there
 *  is nothing on him, so no toast. `oldest` is the first row of the on-me
 *  view (it sorts oldest first). */
export function nudgeParts(
  count: number,
  oldest: { fromName: string; fromAddr: string; since: number } | null,
  nowSecs: number,
): { n: number; who: string; days: number } | null {
  if (count <= 0 || oldest === null) return null;
  const who = oldest.fromName.trim() || oldest.fromAddr;
  return { n: count, who, days: ageDays(oldest.since, nowSecs) };
}
