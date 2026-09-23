// Typed wrappers around the fork's IPC commands (src-tauri/src/fork/commands.rs).
import { invoke } from "@tauri-apps/api/core";

/** Mirrors `fork::restore::RemovalSnapshot`. */
export interface RemovalSnapshot {
  accountId: string;
  folderId: number;
  folderImapName: string;
  messageIds: string[];
}

export const forkApi = {
  /** Total messages across every folder with `role` (all accounts when null). */
  roleTotal: (role: string, accountId: string | null) =>
    invoke<number>("fork_role_total", { role, accountId }),
  /** Where these messages live and their RFC 822 ids, before they go. */
  removalSnapshot: (messageIds: number[]) =>
    invoke<RemovalSnapshot[]>("fork_removal_snapshot", { messageIds }),
  /** Queue a server-side restore for a snapshot (undo after the grace window). */
  restore: (snapshot: RemovalSnapshot, kind: string, destImapName: string | null) =>
    invoke<void>("fork_restore", { snapshot, kind, destImapName }),
};
