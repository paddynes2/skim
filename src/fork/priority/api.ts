// Priority (v1.1.11, D64): the Inbox's top section. Rust side: `fork::priority`.
import { invoke } from "@tauri-apps/api/core";

/** Emitted when the Priority set changed; the Inbox reloads its order. */
export const PRIORITY_UPDATED = "priority:updated";

export type PriorityMember = { threadId: number; reason: "deal" | "starred" | "always" | "jev" };

export type PriorityStatus = {
  enabled: boolean;
  keyPresent: boolean;
  threshold: number;
  exclude: string;
  about: string;
  always: string;
  alerts: boolean;
  scoredToday: number;
  cap: number;
  count: number;
  lastError: string | null;
};

export const priorityApi = {
  ids: () => invoke<number[]>("fork_priority_ids"),
  members: () => invoke<PriorityMember[]>("fork_priority_members"),
  /** `+` / `-`: always, or never, put this sender in Priority. */
  train: (sender: string, important: boolean | null) => invoke<void>("fork_priority_train", { sender, important }),
  /** Download the shown rows' bodies ahead (D65). */
  prefetch: (threadIds: number[]) => invoke<number>("fork_prefetch_threads", { threadIds }),
  status: () => invoke<PriorityStatus>("fork_priority_status"),
  /** An empty key removes the stored one. */
  setKey: (key: string) => invoke<void>("fork_priority_set_key", { key }),
  recompute: () => invoke<void>("fork_priority_recompute"),
};
