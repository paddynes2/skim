// Typed wrappers around the v1.1.1 reminder commands (src-tauri/src/fork/reminders.rs).
import { invoke } from "@tauri-apps/api/core";
import type { ThreadRow } from "../../lib/types";

export type ReminderKind = "snooze" | "followup";

/** Emitted after any set / clear / sweep that moved something. */
export const REMINDERS_UPDATED = "reminders:updated";

/** Virtual folder ids of the two views, next to court's -920 / -921. */
export const VF_SNOOZED = -922;
export const VF_FOLLOWUPS = -923;

export interface Reminder {
  kind: ReminderKind;
  setTs: number;
  dueTs: number;
}

/** A view row: exactly a `ThreadRow` plus the reminder. */
export interface ReminderRow extends ThreadRow {
  kind: ReminderKind;
  setTs: number;
  dueTs: number;
}

export interface ReminderCounts {
  snoozed: number;
  followups: number;
  followupsDue: number;
}

export interface DueNote {
  kind: ReminderKind;
  subject: string;
}

export const remindersApi = {
  set: (threadId: number, kind: ReminderKind, dueTs: number) =>
    invoke<void>("fork_reminder_set", { threadId, kind, dueTs }),
  followupOnSend: (replyToMessageId: number | null, subject: string, dueTs: number) =>
    invoke<void>("fork_reminder_followup_on_send", { replyToMessageId, subject, dueTs }),
  clear: (threadId: number, kind: ReminderKind) => invoke<void>("fork_reminder_clear", { threadId, kind }),
  seen: (threadId: number) => invoke<void>("fork_reminder_seen", { threadId }),
  get: (threadId: number) => invoke<Reminder[]>("fork_reminder_get", { threadId }),
  list: (kind: ReminderKind, offset: number, limit: number) =>
    invoke<ReminderRow[]>("fork_reminder_list", { kind, offset, limit }),
  counts: () => invoke<ReminderCounts>("fork_reminder_counts"),
};
