// Pure helpers for the calendar UI (Phase 7.5): attendee parsing, "who else is
// on this event", local date/time formatting and the error-code → string map.
// No state, no IPC; `src/fork/tests` can drive every function.
import { getLocale, t } from "../../lib/i18n/index.svelte";
import type { Attendee, EventRow } from "./types";

type AttendeeSource = Pick<EventRow, "attendees_json" | "organizer_email">;

/** Google's attendee array out of the verbatim JSON column; [] on anything odd. */
export function parseAttendees(row: Pick<EventRow, "attendees_json">): Attendee[] {
  if (!row.attendees_json) return [];
  try {
    const v: unknown = JSON.parse(row.attendees_json);
    if (!Array.isArray(v)) return [];
    return v.filter(
      (a): a is Attendee => !!a && typeof a === "object" && typeof (a as Attendee).email === "string",
    );
  } catch {
    return [];
  }
}

/** `own` is the lowercased list of the user's mailbox addresses. */
export function isSelf(a: Attendee, own: string[]): boolean {
  return a.self === true || own.includes(a.email.toLowerCase());
}

/** Guests other than the user: the ones a `sendUpdates` answer is about. */
export function otherGuests(row: AttendeeSource, own: string[]): Attendee[] {
  return parseAttendees(row).filter((a) => !isSelf(a, own));
}

/** True when the user is on the guest list and someone else organises it:
 *  the RSVP buttons show. */
export function selfIsGuest(row: AttendeeSource, own: string[]): boolean {
  const me = parseAttendees(row).find((a) => isSelf(a, own));
  if (!me) return false;
  const org = row.organizer_email?.toLowerCase();
  return !org || !own.includes(org);
}

/** The other guests as the comma-separated value `AddressInput` edits. */
export function guestsValue(row: AttendeeSource, own: string[]): string {
  return otherGuests(row, own)
    .map((a) => a.email)
    .join(", ");
}

/** Comma/semicolon/whitespace-separated addresses → lowercased, unique,
 *  "name <addr>" reduced to addr. Anything without an @ is dropped. */
export function splitAddresses(value: string): string[] {
  const out: string[] = [];
  for (const raw of value.split(/[,;\n]/)) {
    let s = raw.trim();
    const m = s.match(/<([^>]+)>/);
    if (m) s = m[1].trim();
    s = s.toLowerCase();
    if (s.includes("@") && !out.includes(s)) out.push(s);
  }
  return out;
}

const pad = (n: number) => String(n).padStart(2, "0");

/** Local calendar date, "YYYY-MM-DD". */
export function localDate(d: Date): string {
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** Local wall clock, "HH:MM". */
export function localTime(d: Date): string {
  return `${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** "YYYY-MM-DD" + "HH:MM" as a local Date. */
export function fromDateTime(date: string, time: string): Date {
  const [y, m, d] = date.split("-").map(Number);
  const [hh, mm] = (time || "00:00").split(":").map(Number);
  return new Date(y, (m || 1) - 1, d || 1, hh || 0, mm || 0, 0, 0);
}

/** "YYYY-MM-DD" shifted by `days`, in local time. */
export function shiftDate(date: string, days: number): string {
  const d = fromDateTime(date, "00:00");
  d.setDate(d.getDate() + days);
  return localDate(d);
}

/** Round up to the next `stepMin` boundary (quick "new event" start). */
export function roundUp(d: Date, stepMin: number): Date {
  const out = new Date(d);
  out.setSeconds(0, 0);
  const rem = out.getMinutes() % stepMin;
  if (rem !== 0) out.setMinutes(out.getMinutes() + (stepMin - rem));
  return out;
}

/** The IANA zone the app runs in. */
export function localZone(): string {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC";
  } catch {
    return "UTC";
  }
}

export function fmtTime(ts: number): string {
  return new Intl.DateTimeFormat(getLocale(), { hour: "2-digit", minute: "2-digit" }).format(
    new Date(ts * 1000),
  );
}

export function fmtDay(d: Date): string {
  return new Intl.DateTimeFormat(getLocale(), { weekday: "short", day: "numeric", month: "short" }).format(d);
}

/** "Tue 24 Sep, 10:00 – 11:00", "Tue 24 Sep" for all-day, ranges across days spelled out. */
export function fmtWhen(row: Pick<EventRow, "all_day" | "start_ts" | "end_ts" | "start_date" | "end_date">): string {
  if (row.all_day && row.start_date) {
    const start = fromDateTime(row.start_date, "00:00");
    const endEx = row.end_date ? fromDateTime(row.end_date, "00:00") : start;
    const end = new Date(endEx);
    end.setDate(end.getDate() - 1);
    if (end.getTime() <= start.getTime()) return fmtDay(start);
    return `${fmtDay(start)} – ${fmtDay(end)}`;
  }
  const start = new Date(row.start_ts * 1000);
  const end = new Date(row.end_ts * 1000);
  const sameDay = start.toDateString() === end.toDateString();
  if (sameDay) return `${fmtDay(start)}, ${fmtTime(row.start_ts)} – ${fmtTime(row.end_ts)}`;
  return `${fmtDay(start)}, ${fmtTime(row.start_ts)} – ${fmtDay(end)}, ${fmtTime(row.end_ts)}`;
}

/** "in 25m" / "in 1h 05m" / "now". */
export function fmtIn(startTs: number, now: number): string {
  const mins = Math.round((startTs - now) / 60);
  if (mins <= 0) return t("fork.cal.now");
  if (mins < 60) return t("fork.cal.in_min", { n: mins });
  return t("fork.cal.in_hour", { h: Math.floor(mins / 60), m: pad(mins % 60) });
}

/** The `code` a backend SkimError carries, or null. */
export function errorCode(e: unknown): string | null {
  if (e && typeof e === "object" && "code" in e) return String((e as { code: unknown }).code);
  return null;
}

/** A calendar error as one sentence for the UI. Known codes map to their
 *  `fork.cal.err.*` string; anything else shows the backend message. */
export function calendarErrorText(e: unknown): string {
  const code = errorCode(e);
  const known = [
    "gcal_not_configured",
    "gcal_not_connected",
    "gcal_account_mismatch",
    "gcal_scope",
    "gcal_api",
    "gcal_input",
    "gcal_op",
    "oauth_cancelled",
    "network",
  ];
  if (code && known.includes(code)) {
    const text = t(`fork.cal.err.${code}`);
    // The mismatch error names the two addresses; keep them.
    if (code === "gcal_account_mismatch" && e && typeof e === "object" && "message" in e)
      return `${text} ${String((e as { message: unknown }).message)}`;
    return text;
  }
  if (e && typeof e === "object" && "message" in e) return String((e as { message: unknown }).message);
  if (typeof e === "string") return e;
  return t("fork.cal.err.gcal_api");
}
