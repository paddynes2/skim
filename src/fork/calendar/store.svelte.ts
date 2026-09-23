// Calendar UI state (PLAN.md 7.5 / 7.6): the window of events on screen, the
// calendars and per-account connection status, the open event or quick-create
// draft, the next-event chip's candidate, and the four write paths (create,
// patch, delete, RSVP), every one of which asks about guests before it names
// `sendUpdates`. Started once from the shell (`calendar.start()`), which also
// registers the `g c` / `m` navigation hooks and the two backend events.
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { t } from "../../lib/i18n/index.svelte";
import { mail } from "../../lib/stores/mail.svelte";
import { ui } from "../../lib/stores/ui.svelte";
import { navHooks } from "../nav";
import { toast } from "../stores/toast.svelte";
import { CALENDAR_OPS_FAILED, CALENDAR_UPDATED, calendarApi, googleApi } from "./api";
import { calendarErrorText, otherGuests, roundUp } from "./guests";
import { guestsPrompt } from "./prompt.svelte";
import { calPrefs } from "./settings.svelte";
import type {
  CalStatus,
  CalView,
  CalendarOpsFailed,
  CalendarRow,
  EventInput,
  EventRow,
  RsvpResponse,
  SendUpdates,
} from "./types";

/** A quick-create in progress (click on an empty slot, or `n`). */
export interface Draft {
  start: Date;
  end: Date;
  allDay: boolean;
}

const MEET_NEW = "https://meet.new";
const UPCOMING_WINDOW_S = 60 * 60;
const UPCOMING_POLL_MS = 30_000;

const state = $state({
  view: "week" as CalView,
  /** The anchor the grid is positioned on. */
  date: new Date(),
  events: [] as EventRow[],
  calendars: [] as CalendarRow[],
  statuses: {} as Record<string, CalStatus>,
  selectedId: null as number | null,
  draft: null as Draft | null,
  loading: false,
  error: null as string | null,
  /** The event the titlebar chip shows, if one starts within the hour. */
  upcoming: null as EventRow | null,
  /** Unix seconds, ticked with the upcoming poll so "in 25m" moves. */
  now: Math.floor(Date.now() / 1000),
});

let range: { from: number; to: number } | null = null;
let started = false;
let pollTimer: ReturnType<typeof setInterval> | null = null;
let unlisten: (() => void)[] = [];

function ownEmails(): string[] {
  return mail.accounts.map((a) => a.email.toLowerCase());
}

function accountId(): string | null {
  return mail.account?.id ?? mail.accounts[0]?.id ?? null;
}

function connectedIds(): string[] {
  return Object.entries(state.statuses)
    .filter(([, s]) => s.connected)
    .map(([id]) => id);
}

function upsert(row: EventRow) {
  const i = state.events.findIndex((e) => e.id === row.id);
  if (i >= 0) state.events[i] = row;
  else state.events.push(row);
}

function drop(id: number) {
  state.events = state.events.filter((e) => e.id !== id);
  if (state.upcoming?.id === id) state.upcoming = null;
}

/** Ask about guests only when there is someone to reach. */
async function askGuests(kind: "create" | "update" | "delete", n: number): Promise<SendUpdates | null> {
  if (n === 0) return "none";
  return guestsPrompt.ask(kind, n);
}

async function refreshStatus(): Promise<void> {
  const next: Record<string, CalStatus> = {};
  for (const a of mail.accounts) {
    try {
      next[a.id] = await calendarApi.status(a.id);
    } catch {
      // the command is not wired yet, or the box has no client: not connected
    }
  }
  // Read the local map, not `state.statuses`: with no accounts nothing above
  // awaits, and a synchronous write-then-read inside a caller's $effect would
  // make that effect depend on what it just wrote (an update loop).
  const ids = Object.entries(next)
    .filter(([, s]) => s.connected)
    .map(([id]) => id);
  const wasConnected = connectedIds().length > 0;
  state.statuses = next;
  const cals: CalendarRow[] = [];
  for (const id of ids) {
    try {
      cals.push(...(await calendarApi.listCalendars(id)));
    } catch {
      // stays empty; the view shows the setup text
    }
  }
  state.calendars = cals;
  // The grid reports its range before the first status answer arrives; once
  // a connection shows up, fetch what it asked for.
  if (!wasConnected && ids.length > 0 && range) void loadRange(range.from, range.to);
}

async function loadRange(from: number, to: number): Promise<void> {
  range = { from, to };
  if (connectedIds().length === 0) {
    state.events = [];
    return;
  }
  state.loading = true;
  try {
    const rows = await calendarApi.events(from, to);
    if (range.from === from && range.to === to) state.events = rows;
    state.error = null;
  } catch (e: unknown) {
    state.error = calendarErrorText(e);
  } finally {
    state.loading = false;
  }
}

function pickUpcoming(rows: EventRow[], now: number): EventRow | null {
  return (
    rows
      .filter(
        (r) =>
          !r.all_day &&
          r.status !== "cancelled" &&
          r.self_response !== "declined" &&
          r.start_ts >= now - 60 &&
          r.start_ts - now <= UPCOMING_WINDOW_S,
      )
      .sort((a, b) => a.start_ts - b.start_ts)[0] ?? null
  );
}

async function refreshUpcoming(): Promise<void> {
  state.now = Math.floor(Date.now() / 1000);
  if (Object.keys(state.statuses).length === 0 && mail.accounts.length > 0) await refreshStatus();
  if (connectedIds().length === 0) {
    state.upcoming = null;
    return;
  }
  try {
    const rows = await calendarApi.events(state.now - 3600, state.now + 2 * 3600);
    state.upcoming = pickUpcoming(rows, state.now);
  } catch {
    state.upcoming = null;
  }
}

export const calendar = {
  // ---- read side ----
  get view() {
    return state.view;
  },
  get date() {
    return state.date;
  },
  get events() {
    return state.events;
  },
  get calendars() {
    return state.calendars;
  },
  get statuses() {
    return state.statuses;
  },
  get selectedId() {
    return state.selectedId;
  },
  get selected(): EventRow | null {
    return state.selectedId === null ? null : (state.events.find((e) => e.id === state.selectedId) ?? null);
  },
  get draft() {
    return state.draft;
  },
  get loading() {
    return state.loading;
  },
  get error() {
    return state.error;
  },
  get upcoming() {
    return state.upcoming;
  },
  get now() {
    return state.now;
  },
  /** The active mailbox (or the first one). */
  get accountId() {
    return accountId();
  },
  /** Any mailbox has a calendar grant. */
  get connected() {
    return connectedIds().length > 0;
  },
  /** A Google OAuth client exists (pasted or built in). */
  get configured() {
    return Object.values(state.statuses).some((s) => s.configured);
  },
  get ownEmails() {
    return ownEmails();
  },
  /** Calendars the user can write to, for the picker. */
  get writableCalendars() {
    return state.calendars.filter((c) => c.selected && c.access_role !== "reader" && c.access_role !== "freeBusyReader");
  },
  calendarOf(row: EventRow): CalendarRow | undefined {
    return state.calendars.find((c) => c.id === row.calendar_id);
  },
  /** True when the event's calendar accepts writes and the account is connected. */
  canEdit(row: EventRow): boolean {
    const cal = this.calendarOf(row);
    if (!cal) return false;
    if (!state.statuses[cal.account_id]?.connected) return false;
    return cal.access_role !== "reader" && cal.access_role !== "freeBusyReader";
  },

  // ---- lifecycle ----
  /** Register hooks + backend listeners and start the chip poll. Idempotent. */
  start(): () => void {
    if (started) return () => {};
    started = true;
    navHooks.calendar = () => ui.showCalendar();
    navHooks.meetNow = () => void calendar.meetNow();
    void listen(CALENDAR_UPDATED, () => void calendar.reload()).then((u) => unlisten.push(u));
    void listen<CalendarOpsFailed>(CALENDAR_OPS_FAILED, (e) => {
      toast.show({ text: `${t("fork.cal.ops_failed")} ${e.payload?.message ?? ""}`.trim(), ms: 10_000 });
    }).then((u) => unlisten.push(u));
    void calPrefs.load();
    void refreshStatus().then(refreshUpcoming);
    pollTimer = setInterval(() => void refreshUpcoming(), UPCOMING_POLL_MS);
    return () => {
      if (pollTimer) clearInterval(pollTimer);
      pollTimer = null;
      for (const u of unlisten) u();
      unlisten = [];
      delete navHooks.calendar;
      delete navHooks.meetNow;
      started = false;
    };
  },
  refreshStatus,
  /** Re-read the window on screen and the chip after a backend `calendar:updated`. */
  async reload(): Promise<void> {
    if (Object.keys(state.statuses).length === 0) await refreshStatus();
    if (range) await loadRange(range.from, range.to);
    await refreshUpcoming();
  },
  /** The grid reports its visible range (datesSet); fetch it. */
  loadRange(from: Date, to: Date): Promise<void> {
    return loadRange(Math.floor(from.getTime() / 1000), Math.floor(to.getTime() / 1000));
  },
  /** After a settings change (connect, calendars ticked): refetch everything. */
  async invalidate(): Promise<void> {
    await refreshStatus();
    await this.reload();
  },

  // ---- navigation ----
  setView(v: CalView) {
    state.view = v;
  },
  today() {
    state.date = new Date();
  },
  step(dir: 1 | -1) {
    const d = new Date(state.date);
    switch (state.view) {
      case "day":
        d.setDate(d.getDate() + dir);
        break;
      case "month":
        d.setDate(1);
        d.setMonth(d.getMonth() + dir);
        break;
      default:
        d.setDate(d.getDate() + 7 * dir);
    }
    state.date = d;
  },
  goto(d: Date) {
    state.date = d;
  },

  // ---- selection ----
  open(id: number) {
    state.draft = null;
    state.selectedId = id;
  },
  openDraft(d: Draft) {
    state.selectedId = null;
    state.draft = d;
  },
  /** `n`: a draft starting at the next half hour, default length. */
  newEvent() {
    const start = roundUp(new Date(), 30);
    const end = new Date(start.getTime() + calPrefs.defaultLen * 60_000);
    this.openDraft({ start, end, allDay: false });
  },
  close() {
    state.selectedId = null;
    state.draft = null;
  },

  // ---- writes: every one asks about guests before it names sendUpdates ----
  /** Resolves the new row, or null when the guests prompt was cancelled. Throws on a backend error. */
  async create(calendarId: number, input: EventInput): Promise<EventRow | null> {
    const cal = state.calendars.find((c) => c.id === calendarId);
    const acc = cal?.account_id ?? accountId();
    if (!acc) throw { code: "gcal_not_connected", message: "no account" };
    const own = ownEmails();
    const guests = (input.attendees ?? []).filter((e) => !own.includes(e.toLowerCase()));
    const su = await askGuests("create", guests.length);
    if (su === null) return null;
    const row = await calendarApi.create(acc, calendarId, input, su);
    upsert(row);
    void refreshUpcoming();
    return row;
  },
  /** False when cancelled at the guests prompt. Throws on a backend error. */
  async patch(id: number, input: EventInput): Promise<boolean> {
    const row = state.events.find((e) => e.id === id);
    const own = ownEmails();
    const reach = new Set<string>();
    if (row) for (const a of otherGuests(row, own)) reach.add(a.email.toLowerCase());
    for (const e of input.attendees ?? []) if (!own.includes(e.toLowerCase())) reach.add(e.toLowerCase());
    const su = await askGuests("update", reach.size);
    if (su === null) return false;
    const updated = await calendarApi.patch(id, input, su);
    upsert(updated);
    void refreshUpcoming();
    return true;
  },
  async remove(id: number): Promise<boolean> {
    const row = state.events.find((e) => e.id === id);
    const n = row ? otherGuests(row, ownEmails()).length : 0;
    const su = await askGuests("delete", n);
    if (su === null) return false;
    await calendarApi.delete(id, su);
    drop(id);
    if (state.selectedId === id) state.selectedId = null;
    return true;
  },
  /** RSVP: the organiser is someone else by definition, so the prompt always shows (D-7f). */
  async rsvp(id: number, response: RsvpResponse): Promise<boolean> {
    const su = await guestsPrompt.ask("rsvp", 1);
    if (su === null) return false;
    const updated = await calendarApi.rsvp(id, response, su);
    upsert(updated);
    void refreshUpcoming();
    return true;
  },

  // ---- Meet (7.6) ----
  /** Titlebar button / `m`: a fresh Meet, copied and opened; meet.new when not connected. */
  async meetNow(): Promise<void> {
    const acc = accountId();
    if (!acc || !state.statuses[acc]?.connected) {
      // openUrl only reports failure; say what happened either way.
      openUrl(MEET_NEW)
        .then(() => toast.show({ text: t("fork.cal.meet_new_opened"), ms: 4000 }))
        .catch((e: unknown) => toast.show({ text: String(e) }));
      return;
    }
    const linkPromise = googleApi.meetCreate(acc);
    let copied = false;
    // Hand the clipboard a promise inside the click gesture, so the write is
    // still user-activated when the link arrives.
    try {
      if (typeof ClipboardItem !== "undefined" && navigator.clipboard?.write) {
        await navigator.clipboard.write([
          new ClipboardItem({
            "text/plain": linkPromise.then((l) => new Blob([l], { type: "text/plain" })),
          }),
        ]);
        copied = true;
      }
    } catch {
      // fall through to writeText
    }
    let link: string;
    try {
      link = await linkPromise;
    } catch (e: unknown) {
      toast.show({ text: calendarErrorText(e) });
      return;
    }
    if (!copied) {
      try {
        await navigator.clipboard.writeText(link);
        copied = true;
      } catch {
        // clipboard blocked: the link still opens
      }
    }
    void openUrl(link);
    toast.show({ text: copied ? t("fork.cal.meet_copied") : t("fork.cal.meet_opened"), ms: 5000 });
  },
  /** A fresh Meet link for the composer, or null when not connected (caller opens meet.new). */
  async meetLink(): Promise<string | null> {
    const acc = accountId();
    if (!acc || !state.statuses[acc]?.connected) return null;
    return googleApi.meetCreate(acc);
  },
};
