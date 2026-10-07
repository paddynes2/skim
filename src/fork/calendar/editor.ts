import type { EventOptions } from "./types";

export const WEEKDAYS = ["SU", "MO", "TU", "WE", "TH", "FR", "SA"];
export const EVENT_COLORS = ["", "#7986cb", "#33b679", "#8e24aa", "#e67c73", "#f6bf26", "#f4511e", "#039be5", "#616161", "#3f51b5", "#0b8043", "#d50000"];

export function eventOptions(options: EventOptions = {}, transparency?: string | null): EventOptions {
  return {
    recurrence: [], reminders: { useDefault: true, overrides: [] }, visibility: "default",
    colorId: "", transparency: transparency === "transparent" ? "transparent" : "opaque",
    guestsCanModify: false, guestsCanInviteOthers: true, guestsCanSeeOtherGuests: true,
    ...JSON.parse(JSON.stringify(options)),
  };
}
export function changedOptions(value: EventOptions, before: EventOptions): EventOptions {
  return Object.fromEntries(Object.entries(value).filter(([key, v]) => JSON.stringify(v) !== JSON.stringify(before[key as keyof EventOptions])));
}
export function zonedParts(timestamp: number, zone: string): { date: string; time: string } {
  const p = Object.fromEntries(new Intl.DateTimeFormat("en-CA", {
    timeZone: zone, year: "numeric", month: "2-digit", day: "2-digit",
    hour: "2-digit", minute: "2-digit", hourCycle: "h23",
  }).formatToParts(new Date(timestamp * 1000)).map((v) => [v.type, v.value]));
  return { date: `${p.year}-${p.month}-${p.day}`, time: `${p.hour}:${p.minute}` };
}
/** Reject missing daylight-saving times. Repeated times use the earlier instant. */
export function zonedTimestamp(date: string, time: string, zone: string): number | null {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(date) || !/^\d{2}:\d{2}$/.test(time)) return null;
  const wanted = Date.parse(`${date}T${time}:00Z`) / 1000;
  if (!Number.isFinite(wanted) || new Date(wanted * 1000).toISOString().slice(0, 16) !== `${date}T${time}`) return null;
  try {
    const offsets = new Set<number>();
    for (const delta of [-86400, 0, 86400]) {
      const probe = wanted + delta;
      const parts = zonedParts(probe, zone);
      offsets.add(Date.parse(`${parts.date}T${parts.time}:00Z`) / 1000 - probe);
    }
    const hits = [...offsets].map((offset) => wanted - offset).filter((ts) => {
      const p = zonedParts(ts, zone); return p.date === date && p.time === time;
    }).sort((a, b) => a - b);
    return hits[0] ?? null;
  } catch { return null; }
}
/** Keep a stored occurrence when its wall time is unchanged, including the second DST occurrence. */
export function eventTimestamp(date: string, time: string, zone: string, previous?: number): number | null {
  if (previous !== undefined) {
    try {
      const p = zonedParts(previous, zone);
      if (p.date === date && p.time === time) return previous;
    } catch { return null; }
  }
  return zonedTimestamp(date, time, zone);
}
export interface RepeatRule {
  frequency: "DAILY" | "WEEKLY" | "MONTHLY" | "YEARLY";
  interval: number;
  days: string[];
  monthly: "date" | "weekday";
  end: "never" | "date" | "count";
  until: string;
  count: number;
}
export function makeRecurrence(rule: RepeatRule, startDate: string, allDay: boolean, zone: string): string[] | null {
  if (!Number.isInteger(rule.interval) || rule.interval < 1 || rule.interval > 999) return null;
  const start = new Date(`${startDate}T12:00:00Z`);
  if (!Number.isFinite(start.getTime())) return null;
  const parts = [`FREQ=${rule.frequency}`, `INTERVAL=${rule.interval}`];
  if (rule.frequency === "WEEKLY") {
    if (!rule.days.length || rule.days.some((d) => !WEEKDAYS.includes(d))) return null;
    parts.push(`BYDAY=${rule.days.join(",")}`);
  }
  if (rule.frequency === "MONTHLY") {
    parts.push(rule.monthly === "weekday"
      ? `BYDAY=${Math.ceil(start.getUTCDate() / 7) === 5 ? -1 : Math.ceil(start.getUTCDate() / 7)}${WEEKDAYS[start.getUTCDay()]}`
      : `BYMONTHDAY=${start.getUTCDate()}`);
  }
  if (rule.end === "count") {
    if (!Number.isInteger(rule.count) || rule.count < 1 || rule.count > 9999) return null;
    parts.push(`COUNT=${rule.count}`);
  } else if (rule.end === "date") {
    if (rule.until < startDate) return null;
    const lastMinute = zonedTimestamp(rule.until, "23:59", zone);
    if (lastMinute === null) return null;
    parts.push(`UNTIL=${allDay ? rule.until.replaceAll("-", "") : new Date((lastMinute + 59) * 1000).toISOString().replace(/[-:]/g, "").replace(".000", "")}`);
  }
  return [`RRULE:${parts.join(";")}`];
}
