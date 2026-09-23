// Fork (7 / 8): fixtures for the calendar, Meet and /slots commands in the
// demo harness. The main session wires one line into `tauri-core.ts` before
// its switch:
//
//   import { forkCalendarInvoke } from "./fork-calendar";
//   const cal = forkCalendarInvoke(cmd, args);
//   if (cal) return "err" in cal ? Promise.reject(cal.err) : ok(cal.ok);
//
// The three setup states are picked by localStorage `skimdemo.fork_cal`:
// "unconfigured" (no client), "disconnected" (client, no grant), "connected"
// (the default: client + grant + a week of events). Everything is relative
// to the current week so the next-event chip and /slots stay live.
import * as db from "./data";

export type ForkCalVariant = "unconfigured" | "disconnected" | "connected";

export function forkCalVariant(): ForkCalVariant {
  try {
    const v = (globalThis as any).localStorage?.getItem("skimdemo.fork_cal");
    if (v === "unconfigured" || v === "disconnected") return v;
  } catch {}
  return "connected";
}

const SELF = db.ACCOUNT.email;
const NOW = Math.floor(Date.now() / 1000);
const ZONE = (() => {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC";
  } catch {
    return "UTC";
  }
})();

/** Monday 00:00 local of the current week. */
function weekStart(): Date {
  const d = new Date();
  d.setHours(0, 0, 0, 0);
  const dow = (d.getDay() + 6) % 7; // Mon=0
  d.setDate(d.getDate() - dow);
  return d;
}
const MON = weekStart();

const pad = (n: number) => String(n).padStart(2, "0");
const ymd = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;

/** Unix seconds of weekday `day` (0=Mon) at `hh:mm` local, this week. */
function at(day: number, hh: number, mm = 0): number {
  const d = new Date(MON);
  d.setDate(d.getDate() + day);
  d.setHours(hh, mm, 0, 0);
  return Math.floor(d.getTime() / 1000);
}
function dateOf(day: number): string {
  const d = new Date(MON);
  d.setDate(d.getDate() + day);
  return ymd(d);
}

export const CALENDARS = [
  { id: 1, account_id: "acc-1", google_id: SELF, summary: db.ACCOUNT.displayName ?? SELF, color: "#9fe1e7", is_primary: true, selected: true, access_role: "owner" },
  { id: 2, account_id: "acc-1", google_id: "team@brightwave.io", summary: "Team", color: "#fbd75b", is_primary: false, selected: true, access_role: "writer" },
  { id: 3, account_id: "acc-1", google_id: "en.usa#holiday@group.v.calendar.google.com", summary: "Holidays", color: "#7ae7bf", is_primary: false, selected: false, access_role: "reader" },
];

const ANNA = { email: "anna.weber@northwind.example", displayName: "Anna Weber", responseStatus: "accepted" };
const MARCUS = { email: "marcus@acme-partners.example", displayName: "Marcus Lee", responseStatus: "needsAction" };
const PRIYA = { email: "priya@brightwave.io", displayName: "Priya Nair", responseStatus: "accepted" };
const me = (responseStatus: string) => ({ email: SELF, self: true, responseStatus });

function row(partial: Partial<any> & { id: number; summary: string }): any {
  return {
    calendar_id: 1,
    google_id: `evt${partial.id}`,
    etag: `"${partial.id}"`,
    status: "confirmed",
    description: null,
    location: null,
    all_day: false,
    start_date: null,
    end_date: null,
    time_zone: ZONE,
    recurring_event_id: null,
    organizer_email: SELF,
    attendees_json: null,
    self_response: null,
    transparency: null,
    hangout_link: null,
    html_link: `https://calendar.google.com/calendar/event?eid=${partial.id}`,
    updated: new Date((NOW - 3600) * 1000).toISOString(),
    local_only: false,
    ...partial,
  };
}

// Ids 901 / 902 line up with demo/mock/fork-prep.ts (the Prep panel fixtures).
let EVENTS: any[] = [
  row({ id: 910, calendar_id: 2, summary: "Standup", start_ts: at(0, 9, 30), end_ts: at(0, 10), organizer_email: "team@brightwave.io", attendees_json: JSON.stringify([me("accepted"), PRIYA]), self_response: "accepted" }),
  row({
    id: 901,
    summary: "Q3 launch sync",
    start_ts: at(1, 10),
    end_ts: at(1, 11),
    location: "Google Meet",
    description: "Owners for the three open launch items; 4.2 redline.",
    attendees_json: JSON.stringify([me("accepted"), ANNA, MARCUS]),
    self_response: "accepted",
    hangout_link: "https://meet.google.com/abc-defg-hij",
  }),
  row({ id: 911, calendar_id: 2, summary: "Offsite", all_day: true, start_date: dateOf(2), end_date: dateOf(3), start_ts: at(2, 0), end_ts: at(3, 0), organizer_email: "team@brightwave.io", transparency: "transparent" }),
  row({
    id: 912,
    summary: "Pricing review",
    start_ts: at(2, 14),
    end_ts: at(2, 15),
    organizer_email: MARCUS.email,
    attendees_json: JSON.stringify([{ ...MARCUS, organizer: true, responseStatus: "accepted" }, me("needsAction")]),
    self_response: "needsAction",
  }),
  row({
    id: 913,
    summary: "Dentist",
    start_ts: at(3, 11),
    end_ts: at(3, 11, 30),
    organizer_email: "reception@smile.example",
    attendees_json: JSON.stringify([{ email: "reception@smile.example", organizer: true, responseStatus: "accepted" }, me("declined")]),
    self_response: "declined",
  }),
  row({ id: 914, summary: "Board prep", start_ts: at(3, 16), end_ts: at(3, 17), status: "tentative" }),
  row({ id: 902, summary: "Intro call", start_ts: at(4, 9), end_ts: at(4, 9, 45), attendees_json: JSON.stringify([me("accepted"), PRIYA]), self_response: "accepted", hangout_link: "https://meet.google.com/pqr-stuv-wxy" }),
  // The next-event chip's candidate: starts in 25 minutes, has a Meet link.
  row({
    id: 903,
    summary: "Call with Anna Weber",
    start_ts: NOW + 25 * 60,
    end_ts: NOW + 55 * 60,
    attendees_json: JSON.stringify([me("accepted"), ANNA]),
    self_response: "accepted",
    hangout_link: "https://meet.google.com/kln-mnop-qrs",
  }),
];
let nextId = 950;

function status(accountId: string) {
  const v = forkCalVariant();
  const configured = v !== "unconfigured";
  const connected = v === "connected" && accountId === "acc-1";
  return { configured, connected, engine_running: connected, pending_ops: 0 };
}

function clientGet() {
  return forkCalVariant() === "unconfigured"
    ? { configured: false, source: null, client_id: null, secret_masked: null }
    : { configured: true, source: "stored", client_id: "1234567890-abc.apps.googleusercontent.com", secret_masked: "••••ab12" };
}

function notConnected() {
  return { code: "gcal_not_connected", message: "Google Calendar is not connected for this account." };
}

function applyInput(e: any, input: any) {
  if (input.summary !== undefined) e.summary = input.summary;
  if (input.description !== undefined) e.description = input.description;
  if (input.location !== undefined) e.location = input.location;
  if (input.all_day !== undefined) e.all_day = input.all_day;
  if (input.start_ts !== undefined) e.start_ts = input.start_ts;
  if (input.end_ts !== undefined) e.end_ts = input.end_ts;
  if (input.start_date !== undefined) e.start_date = input.start_date;
  if (input.end_date !== undefined) e.end_date = input.end_date;
  if (input.time_zone !== undefined) e.time_zone = input.time_zone;
  if (input.attendees !== undefined) {
    const keep = e.attendees_json ? JSON.parse(e.attendees_json).filter((a: any) => a.self) : [];
    const list = [...keep, ...input.attendees.map((email: string) => ({ email, responseStatus: "needsAction" }))];
    e.attendees_json = list.length ? JSON.stringify(list) : null;
  }
  if (input.add_meet) e.hangout_link ??= "https://meet.google.com/new-link-xyz";
  e.updated = new Date().toISOString();
}

/** Free slots: 10:00, 14:30, 16:00 local on the next `days` working days. */
function freeSlots(durationMin: number, days: number) {
  const out: any[] = [];
  const d = new Date();
  d.setHours(0, 0, 0, 0);
  let count = 0;
  while (count < days) {
    d.setDate(d.getDate() + 1);
    const dow = d.getDay();
    if (dow === 0 || dow === 6) continue;
    count++;
    // "Thu 24 Sep", the shape the Rust side emits (en-GB would say "Sept").
    const parts = new Intl.DateTimeFormat("en-US", { weekday: "short", day: "numeric", month: "short" }).formatToParts(d);
    const part = (t: string) => parts.find((p) => p.type === t)?.value ?? "";
    const label = `${part("weekday")} ${part("day")} ${part("month")}`;
    for (const [hh, mm] of [
      [10, 0],
      [14, 30],
      [16, 0],
    ]) {
      const s = new Date(d);
      s.setHours(hh, mm, 0, 0);
      const start = Math.floor(s.getTime() / 1000);
      out.push({ start, end: start + durationMin * 60, day: ymd(d), label, time: `${pad(hh)}:${pad(mm)}` });
    }
  }
  return out;
}

/** `{ ok }` for a handled command, `{ err }` for a typed error, null when not ours. */
export function forkCalendarInvoke(cmd: string, args: any = {}): { ok: unknown } | { err: unknown } | null {
  switch (cmd) {
    case "fork_google_client_get":
      return { ok: clientGet() };
    case "fork_google_client_set":
      return { ok: { configured: true, source: "stored", client_id: args.clientId, secret_masked: "••••" + String(args.clientSecret ?? "ab12").slice(-4) } };
    case "fork_google_client_clear":
      return { ok: { configured: false, source: null, client_id: null, secret_masked: null } };
    case "fork_meet_create":
      return status(args.accountId).connected ? { ok: "https://meet.google.com/abc-defg-hij" } : { err: notConnected() };
    case "fork_cal_status":
      return { ok: status(args.accountId) };
    case "fork_cal_connect":
      return { ok: { configured: true, connected: true, engine_running: true, pending_ops: 0 } };
    case "fork_cal_disconnect":
      return { ok: { configured: true, connected: false, engine_running: false, pending_ops: 0 } };
    case "fork_cal_list_calendars":
      return { ok: status(args.accountId).connected ? CALENDARS : [] };
    case "fork_cal_set_selected": {
      const c = CALENDARS.find((x) => x.id === args.calendarId);
      if (c) c.selected = !!args.selected;
      return { ok: undefined };
    }
    case "fork_cal_events": {
      if (!status("acc-1").connected) return { ok: [] };
      const selected = new Set(CALENDARS.filter((c) => c.selected).map((c) => c.id));
      return { ok: EVENTS.filter((e) => selected.has(e.calendar_id) && e.start_ts < args.toTs && e.end_ts > args.fromTs) };
    }
    case "fork_cal_create": {
      if (!status(args.accountId).connected) return { err: notConnected() };
      const e = row({ id: nextId++, summary: "", calendar_id: args.calendarId, google_id: `local-${nextId}`, local_only: true, start_ts: NOW, end_ts: NOW + 1800 });
      applyInput(e, args.input ?? {});
      EVENTS.push(e);
      return { ok: e };
    }
    case "fork_cal_patch": {
      const e = EVENTS.find((x) => x.id === args.eventId);
      if (!e) return { err: { code: "gcal_input", message: "no such event" } };
      applyInput(e, args.input ?? {});
      return { ok: { ...e } };
    }
    case "fork_cal_delete":
      EVENTS = EVENTS.filter((x) => x.id !== args.eventId);
      return { ok: undefined };
    case "fork_cal_rsvp": {
      const e = EVENTS.find((x) => x.id === args.eventId);
      if (!e) return { err: { code: "gcal_input", message: "no such event" } };
      e.self_response = args.response;
      if (e.attendees_json) {
        const list = JSON.parse(e.attendees_json);
        for (const a of list) if (a.self) a.responseStatus = args.response;
        e.attendees_json = JSON.stringify(list);
      }
      return { ok: { ...e } };
    }
    case "fork_cal_sync_now":
      return { ok: undefined };
    case "fork_free_slots":
      if (!status("acc-1").connected) return { err: notConnected() };
      return { ok: freeSlots(Number(args.durationMin) || 30, Number(args.days) || 5) };
    default:
      return null;
  }
}
