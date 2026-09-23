// Typed wrappers around the Phase 7 / 8 commands (fork/google.rs,
// fork/calendar/commands.rs, fork/availability.rs). Argument names are what
// Tauri expects on the wire (camelCase of the Rust parameters).
import { invoke } from "@tauri-apps/api/core";
import type {
  CalStatus,
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
  patch: (eventId: number, input: EventInput, sendUpdates: SendUpdates) =>
    invoke<EventRow>("fork_cal_patch", { eventId, input, sendUpdates }),
  delete: (eventId: number, sendUpdates: SendUpdates) =>
    invoke<void>("fork_cal_delete", { eventId, sendUpdates }),
  rsvp: (eventId: number, response: RsvpResponse, sendUpdates: SendUpdates) =>
    invoke<EventRow>("fork_cal_rsvp", { eventId, response, sendUpdates }),
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
