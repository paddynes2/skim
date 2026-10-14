// Typed wrappers around the To reply commands (src-tauri/src/fork/replies.rs,
// v1.1.19, D70). Argument names are what Tauri expects on the wire.
import { invoke } from "@tauri-apps/api/core";
import type { CourtRow } from "../court/types";

/** Emitted after new reply scores or a mark change. `court:updated` (a reply
 *  sent, a new message in) moves the list too. */
export const REPLIES_UPDATED = "replies:updated";

/** Virtual folder id of the view. Mirrors `fork::replies::VF_TO_REPLY`. */
export const VF_TO_REPLY = -925;

/** `done`: no reply needed. `needs`: a reply is owed. */
export type ReplyMark = "done" | "needs";

/** Why new mail is not being checked, when it is not. */
export type ScoringOff = "priority_off" | "no_key" | "paused";

export interface RepliesState {
  ids: number[];
  off: ScoringOff | null;
}

export const repliesApi = {
  /** The view's rows, newest first: court rows whose reason is the tag. */
  list: (offset: number, limit: number) => invoke<CourtRow[]>("fork_replies_list", { offset, limit }),
  /** Every conversation in To reply, and whether new mail is being checked. */
  state: () => invoke<RepliesState>("fork_replies_state"),
  /** Set or clear (null) the mark; resolves to the previous mark, for Undo. */
  mark: (threadId: number, mark: ReplyMark | null) =>
    invoke<ReplyMark | null>("fork_replies_mark", { threadId, mark }),
};
