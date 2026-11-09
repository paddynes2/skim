// Typed wrappers around the Phase 7 / 8 commands (fork/google.rs,
// fork/calendar/commands.rs, fork/availability.rs). Argument names are what
// Tauri expects on the wire (camelCase of the Rust parameters).
import { invoke } from "@tauri-apps/api/core";
import type {
  SearchResult,
  CalStatus,
  AvailabilityResult,
  CalendarSyncState,
  CalendarRow,
  ClientStatus,
  EventInput,
  EventRow,
  RsvpResponse,
  SendUpdates,
  Slot,
} from "./types";

export const CALENDAR_UPDATED = "calendar:updated";
export const CALENDAR_OPS_FAILED = "calendar:ops_failed";
/** A meeting is about to start (src-tauri fork/calendar/alerts.rs, D97). */
export const CALENDAR_ALERT = "calendar:alert";

export const googleApi = {
  /** The OAuth client in use, secret masked. */
  clientGet: () => invoke<ClientStatus>("fork_google_client_get"),
  /** Store a pasted client; an empty secret keeps the previous one. */
  clientSet: (clientId: string, clientSecret: string) =>
    invoke<ClientStatus>("fork_google_client_set", { clientId, clientSecret }),
  clientClear: () => invoke<ClientStatus>("fork_google_client_clear"),
  /** POST meet/v2/spaces; resolves to the meetingUri. Errors gcal_not_connected. */
  meetCreate: (accountId: string) => invoke<string>("fork_meet_create", { accountId }),
};

export const calendarApi = {
  discardSplit: (accountId: string, eventId: number, sendUpdates: SendUpdates) => invoke<void>("fork_cal_discard_split", {accountId, eventId, sendUpdates}),
  availability: (accountId: string, fromTs: number, toTs: number, attendees: string[], excludeEventId: number | null = null) => invoke<AvailabilityResult>("fork_cal_availability", { accountId, fromTs, toTs, attendees, excludeEventId }),
  syncState: (accountId: string) => invoke<CalendarSyncState>("fork_cal_sync_state", { accountId }),
  retry: (accountId: string, eventId: number | null = null) => invoke<number>("fork_cal_retry", { accountId, eventId }),
  status: (accountId: string) => invoke<CalStatus>("fork_cal_status", { accountId }),
  /** Opens the browser for consent; resolves once the grant is stored and the engine runs. */
  connect: (accountId: string) => invoke<CalStatus>("fork_cal_connect", { accountId }),
  disconnect: (accountId: string) => invoke<CalStatus>("fork_cal_disconnect", { accountId }),
  listCalendars: (accountId: string) =>
    invoke<CalendarRow[]>("fork_cal_list_calendars", { accountId }),
  setSelected: (calendarId: number, selected: boolean) =>
    invoke<void>("fork_cal_set_selected", { calendarId, selected }),
  /** Events on selected calendars overlapping [fromTs, toTs), unix seconds. */
  events: (fromTs: number, toTs: number, accountId: string | null = null) =>
    invoke<EventRow[]>("fork_cal_events", { fromTs, toTs, accountId }),
  /** Local row at once, op queued. Ask about guests before choosing sendUpdates. */
  create: (accountId: string, calendarId: number, input: EventInput, sendUpdates: SendUpdates) =>
    invoke<EventRow>("fork_cal_create", { accountId, calendarId, input, sendUpdates }),
  series: (eventId: number) => invoke<EventRow>("fork_cal_series", { eventId }),
  patch: (eventId: number, input: EventInput, sendUpdates: SendUpdates) =>
    invoke<EventRow>("fork_cal_patch", { eventId, input, sendUpdates }),
  delete: (eventId: number, sendUpdates: SendUpdates, series = false) =>
    invoke<void>("fork_cal_delete", { eventId, sendUpdates, series }),
  /** `comment` is the note to the organiser ("" clears it); `series` answers for every occurrence (D97). */
  rsvp: (eventId: number, response: RsvpResponse, sendUpdates: SendUpdates, comment: string | null = null, series = false) =>
    invoke<EventRow>("fork_cal_rsvp", { eventId, response, sendUpdates, comment, series }),
  /** Move to another calendar of the same account; a queued op (D97). */
  move: (eventId: number, toCalendarId: number, sendUpdates: SendUpdates) =>
    invoke<EventRow>("fork_cal_move", { eventId, toCalendarId, sendUpdates }),
  /** Calendar list writes (D97), direct calls that need the wider permission. */
  setColor: (calendarId: number, color: string) => invoke<CalendarRow>("fork_cal_set_color", { calendarId, color }),
  addCalendar: (accountId: string, calendar: string) => invoke<CalendarRow>("fork_cal_add_calendar", { accountId, calendar }),
  createCalendar: (accountId: string, summary: string, timeZone: string) =>
    invoke<CalendarRow>("fork_cal_create_calendar", { accountId, summary, timeZone }),
  /** Cached events plus Google's own search beyond the synced window (D97). */
  search: (query: string, accountId: string | null = null, limit = 100) =>
    invoke<SearchResult>("fork_cal_search", { query, accountId, limit }),
  syncNow: (accountId: string) => invoke<void>("fork_cal_sync_now", { accountId }),
};

export const availabilityApi = {
  /**
   * Free slots for /slots. `tz` is the IANA zone the working hours are walked
   * in (the sender's: Intl.DateTimeFormat().resolvedOptions().timeZone).
   */
  freeSlots: (
    durationMin: 30 | 45 | 60 | number,
    days: 3 | 5 | 10 | number,
    tz: string,
    accountId: string | null = null,
  ) => invoke<Slot[]>("fork_free_slots", { accountId, durationMin, days, tz }),
};
