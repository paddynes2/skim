// Shapes of the Phase 10 IPC (src-tauri/src/fork/court.rs). Field names are
// the Rust structs' in camelCase, like `ThreadRow`.
import type { ThreadRow } from "../../lib/types";

/** The views. `none` rows exist in the table but are never listed.
 *  v1.1.19 (D70): `to_reply` is the court rows that need a reply
 *  (`fork_replies_list`); On me and Waiting have no sidebar row since D54. */
export type CourtState = "on_me" | "waiting" | "to_reply";

/** One row of "On me" / "Waiting": exactly a `ThreadRow` (so the message list
 *  renders it unchanged) plus when the ball landed and why. */
export interface CourtRow extends ThreadRow {
  /** Date of the thread's last message (unix seconds); the views sort on it, oldest first. */
  since: number;
  /** Stored reason, when the rule or the AI gave one ("draft started", "asks for the deck"). */
  reason: string | null;
}

export interface CourtCounts {
  onMe: number;
  waiting: number;
}
