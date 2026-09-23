// Typed wrappers for the Phase 6 IPC commands (src-tauri/src/fork/compose.rs
// and src-tauri/src/fork/scheduler.rs).
import { invoke } from "@tauri-apps/api/core";

/** The "Scheduled" virtual folder id (PLAN.md 6.4). Negative, like the other
 *  virtual folders; -900 is search, -9 Important. */
export const SCHEDULED_FOLDER_ID = -910;

/** Mirrors `fork::scheduler::ScheduledSend`. */
export interface ScheduledSend {
  opId: number;
  draftId: number;
  accountId: string;
  to: string;
  subject: string;
  snippet: string;
  /** Unix seconds. */
  notBefore: number;
  label: string | null;
}

/** Payload of the `fork:send-held` event (a send left the composer on a hold). */
export interface SendHeld {
  opId: number;
  draftId: number;
  notBefore: number;
  label: string | null;
}

export const forkComposeApi = {
  /** The user's own words as HTML, when the rich editor wrote them. */
  draftHtmlGet: (draftId: number) =>
    invoke<string | null>("fork_draft_html_get", { draftId }),
  /** Store the words as HTML; `null` drops the row (the draft sends as text/plain). */
  draftHtmlSet: (draftId: number, wordsHtml: string | null) =>
    invoke<void>("fork_draft_html_set", { draftId, wordsHtml }),
  /** Undo / cancel a held send. Resolves to the draft id; rejects inside the
   *  1 s margin ("too late to undo"). */
  cancelScheduled: (opId: number) => invoke<number>("fork_cancel_scheduled", { opId }),
  scheduledList: () => invoke<ScheduledSend[]>("fork_scheduled_list"),
  /** Bring a scheduled send forward to now. */
  sendNow: (opId: number) => invoke<void>("fork_send_now", { opId }),
};
