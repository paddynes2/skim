// The fork's one action module (PLAN.md 3.2). Every archive, delete, spam,
// star and mark-read on a thread goes through here, so the keyboard, the
// row's hover buttons, the reading pane toolbar and the palette all do
// exactly the same thing: optimistic list update, the queued op, and (2.5)
// the cursor moving on to the next row.
import { api } from "../lib/api";
import { mail } from "../lib/stores/mail.svelte";
import type { ThreadRow } from "../lib/types";
import { prefs } from "./stores/prefs.svelte";

/** Roles in which "archive" makes no sense: the mail is already filed away
 *  (Sent), or leaving (Trash, Spam). Gmail would create an "Archive" label. */
const NO_ARCHIVE_ROLES = new Set(["sent", "trash", "junk"]);

/** Whether the Archive action is offered for mail in a folder with `role`
 *  (`null` = a user label, `undefined` = unknown/unified: offered). */
export function archiveOffered(role: string | null | undefined): boolean {
  return !(role && NO_ARCHIVE_ROLES.has(role));
}

export type RemovalKind = "archive" | "delete" | "spam";

/** The row to land on after `threadId` leaves the list, per the setting:
 *  the row below, or the one above when it was last (Superhuman). `null`
 *  means "clear the selection" (`list` setting, or nothing left). */
export function nextAfterRemoval(
  rows: ThreadRow[],
  threadId: number,
  mode = prefs.afterArchive,
): ThreadRow | null {
  if (mode === "list") return null;
  const index = rows.findIndex((t) => t.id === threadId);
  if (index === -1) return null;
  const rest = rows.filter((t) => t.id !== threadId);
  if (rest.length === 0) return null;
  const below = rest[index] ?? null; // the row that slides into this slot
  const above = rest[index - 1] ?? null;
  if (mode === "previous") return above ?? below;
  return below ?? above;
}

function select(row: ThreadRow | null) {
  mail.selectedThreadId = row?.id ?? null;
  mail.selectedMessageId = row?.messageId ?? null;
}

/** Remove a thread (archive / delete / spam): the row leaves now, the cursor
 *  moves on, the op is queued. `ids` are the thread's message ids. */
export function removeThread(thread: ThreadRow, kind: RemovalKind, ids: number[]): void {
  if (ids.length === 0) return;
  if (kind === "archive" && !archiveOffered(mail.selectedFolder?.role)) return;
  const wasOpen = mail.selectedThreadId === thread.id;
  const next = wasOpen ? nextAfterRemoval(mail.threads, thread.id) : null;
  mail.removeThreadFromList(thread.id);
  if (wasOpen) select(next);
  if (kind === "archive") void api.archiveMessages(ids);
  else if (kind === "delete") void api.deleteMessages(ids);
  else void api.reportSpam(ids);
}

/** Star or unstar a whole thread. */
export function setStarred(thread: ThreadRow, on: boolean, ids: number[]): void {
  if (ids.length === 0) return;
  mail.patchThreadRow(thread.id, { isStarred: on });
  void api.setStarred(ids, on);
}

/** Mark a whole thread read or unread. */
export function markRead(thread: ThreadRow, read: boolean, ids: number[]): void {
  if (ids.length === 0) return;
  mail.patchThreadRow(thread.id, { isRead: read });
  void api.markRead(ids, read);
}

export type Action =
  | RemovalKind
  | "star"
  | "unstar"
  | "toggle_star"
  | "read"
  | "unread"
  | "toggle_read";

/** Resolve the thread's message ids and run one action. The single entry the
 *  keyboard handler, hover buttons and palette use. */
export async function act(thread: ThreadRow, action: Action): Promise<void> {
  const ids = await api.threadMessageIds(thread.id);
  if (ids.length === 0) return;
  // Re-read the row: it may have changed while the ids were resolved.
  const live = mail.threads.find((t) => t.id === thread.id) ?? thread;
  switch (action) {
    case "archive":
    case "delete":
    case "spam":
      removeThread(live, action, ids);
      return;
    case "star":
      setStarred(live, true, ids);
      return;
    case "unstar":
      setStarred(live, false, ids);
      return;
    case "toggle_star":
      setStarred(live, !live.isStarred, ids);
      return;
    case "read":
      markRead(live, true, ids);
      return;
    case "unread":
      markRead(live, false, ids);
      return;
    case "toggle_read":
      markRead(live, !live.isRead, ids);
      return;
  }
}
