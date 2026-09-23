// Typed wrappers around the fork's IPC commands (src-tauri/src/fork/commands.rs).
import { invoke } from "@tauri-apps/api/core";

export const forkApi = {
  /** Total messages across every folder with `role` (all accounts when null). */
  roleTotal: (role: string, accountId: string | null) =>
    invoke<number>("fork_role_total", { role, accountId }),
};
