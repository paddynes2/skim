// Navigation hooks later phases register (calendar view, "On me" / "Waiting"
// court views, the CRM drawer, Meet now). Keys and palette rows call through
// here; a hook that is not registered yet is simply a no-op, so the key map
// can ship before the feature does.
export interface NavHooks {
  calendar?: () => void;
  court?: (state: "on_me" | "waiting") => void;
  /** v1.1.3: open Deals (it replaced Snoozed / Follow-ups and H / B). */
  deals?: () => void;
  /** v1.1.19 (D70): open To reply. */
  toReply?: () => void;
  crmToggle?: () => void;
  meetNow?: () => void;
}

export const navHooks: NavHooks = {};
