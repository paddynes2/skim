// Navigation hooks later phases register (calendar view, "On me" / "Waiting"
// court views, the CRM drawer, Meet now). Keys and palette rows call through
// here; a hook that is not registered yet is simply a no-op, so the key map
// can ship before the feature does.
export interface NavHooks {
  calendar?: () => void;
  court?: (state: "on_me" | "waiting") => void;
  /** v1.1.1: open Snoozed / Follow-ups. */
  reminders?: (kind: "snooze" | "followup") => void;
  /** v1.1.1: open the Snooze / Follow-up menu for the open thread (H / B). */
  reminderMenu?: (kind: "snooze" | "followup") => void;
  crmToggle?: () => void;
  meetNow?: () => void;
}

export const navHooks: NavHooks = {};
