// Fork (3.4): Sent and Drafts update in real time.
//
// IDLE only watches INBOX, so Sent and Drafts waited for the 5-minute poll.
// Opening either folder, or bringing the window back, asks the engine for one
// targeted sync of it; Rust debounces to once per 20 s per folder. A unified
// virtual folder (negative id) is addressed by role, and the command resolves
// every real folder with that role across accounts.
import { invoke } from "@tauri-apps/api/core";
import type { Folder } from "../lib/types";

const ROLES = new Set(["sent", "drafts"]);

let current: Folder | null = null;

function request(folder: Folder): void {
  if (folder.role === null || !ROLES.has(folder.role)) return;
  const role = folder.id < 0 ? folder.role : null;
  // Freshness is best effort: a failure here changes nothing the user sees,
  // the poll still comes.
  invoke<void>("fork_sync_folder", { folderId: folder.id, role }).catch(() => {});
}

/** The list just switched to `folder` (any role; only Sent/Drafts sync). */
export function folderSelected(folder: Folder | null | undefined): void {
  current = folder ?? null;
  if (current) request(current);
}

/** The window regained focus: refresh whichever of Sent/Drafts is open. */
export function windowFocused(): void {
  if (current) request(current);
}
