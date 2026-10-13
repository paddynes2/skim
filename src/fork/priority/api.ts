// Priority (v1.1.11, D64): the Inbox's top section. Rust side: `fork::priority`.
import { invoke } from "@tauri-apps/api/core";

/** Emitted when the Priority set changed; the Inbox reloads its order. */
export const PRIORITY_UPDATED = "priority:updated";

export type PriorityStatus = {
  enabled: boolean;
  keyPresent: boolean;
  threshold: number;
  exclude: string;
  about: string;
  scoredToday: number;
  cap: number;
  count: number;
  lastError: string | null;
};

export const priorityApi = {
  ids: () => invoke<number[]>("fork_priority_ids"),
  status: () => invoke<PriorityStatus>("fork_priority_status"),
  /** An empty key removes the stored one. */
  setKey: (key: string) => invoke<void>("fork_priority_set_key", { key }),
  recompute: () => invoke<void>("fork_priority_recompute"),
};
