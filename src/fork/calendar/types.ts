// Shapes of the calendar IPC (src-tauri/src/fork/calendar, fork/google.rs,
// fork/availability.rs). Field names mirror the Rust structs (snake_case).

/** A calendar the account can see (fork_cal_calendars). */
export interface CalendarRow {
  id: number;
  account_id: string;
  google_id: string;
  summary: string;
  color: string | null;
  is_primary: boolean;
  /** Shown in the calendar view and counted as busy for /slots. */
  selected: boolean;
  access_role: string;
}

/** One event instance (fork_cal_events). Timestamps are unix seconds. */
export interface EventRow {
  id: number;
  calendar_id: number;
  /** Starts with "local-" until a create op has landed. */
  google_id: string;
  etag: string | null;
  status: "confirmed" | "tentative" | "cancelled" | string;
  summary: string;
  description: string | null;
  location: string | null;
  /** All-day rows: midnight UTC of start_date. */
  start_ts: number;
  /** Exclusive. All-day rows: midnight UTC of end_date. */
  end_ts: number;
  all_day: boolean;
  start_date: string | null;
  end_date: string | null;
  time_zone: string | null;
  recurring_event_id: string | null;
  organizer_email: string | null;
  /** Google's attendees array, verbatim JSON. */
  attendees_json: string | null;
  self_response: "accepted" | "declined" | "tentative" | "needsAction" | string | null;
  transparency: "opaque" | "transparent" | string | null;
  hangout_link: string | null;
  html_link: string | null;
  updated: string | null;
  local_only: boolean;
}

/** What create / patch take. Every field optional; a patch names only what changed. */
export interface EventInput {
  summary?: string;
  description?: string;
  location?: string;
  start_ts?: number;
  end_ts?: number;
  all_day?: boolean;
  /** YYYY-MM-DD, end exclusive. */
  start_date?: string;
  end_date?: string;
  /** IANA zone sent with dateTime. */
  time_zone?: string;
  /** Guest addresses; [] clears the list. */
  attendees?: string[];
  add_meet?: boolean;
}

/** Never implicit: the editor asks when the event has guests. */
export type SendUpdates = "all" | "none";

export type RsvpResponse = "accepted" | "declined" | "tentative";

export interface CalStatus {
  /** A Google OAuth client is available (pasted or built in). */
  configured: boolean;
  /** This account has a calendar grant. */
  connected: boolean;
  engine_running: boolean;
  pending_ops: number;
}

export interface ClientStatus {
  configured: boolean;
  source: "stored" | "built_in" | null;
  client_id: string | null;
  /** "••••" plus the last four characters; never the secret. */
  secret_masked: string | null;
}

/** One /slots offer; day, label and time are in the zone the hours were walked in. */
export interface Slot {
  start: number;
  end: number;
  day: string;
  label: string;
  time: string;
}

/** Payload of the "calendar:updated" and "calendar:ops_failed" events. */
export interface CalendarUpdated {
  account_id: string;
}
export interface CalendarOpsFailed {
  account_id: string;
  kind: "create" | "patch" | "delete" | "rsvp" | string;
  message: string;
}

/** Error codes the calendar commands answer with (SkimError.code). */
export type CalendarErrorCode =
  | "gcal_not_configured"
  | "gcal_not_connected"
  | "gcal_account_mismatch"
  | "gcal_send_updates"
  | "gcal_input"
  | "gcal_scope"
  | "gcal_api"
  | "gcal_op"
  | "oauth"
  | "oauth_cancelled"
  | "network";

// ---- UI additions (Phase 7.5 / 7.7, additive) ----

/** One entry of `EventRow.attendees_json` (Google's attendee shape; the fields the UI reads). */
export interface Attendee {
  email: string;
  displayName?: string;
  responseStatus?: "needsAction" | "declined" | "tentative" | "accepted" | string;
  self?: boolean;
  organizer?: boolean;
  optional?: boolean;
}

/** The four calendar views (keys d w m a). */
export type CalView = "day" | "week" | "month" | "agenda";

/** What the guests prompt is about; the wording differs per kind. */
export type GuestsPromptKind = "create" | "update" | "delete" | "rsvp";
