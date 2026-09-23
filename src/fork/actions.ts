// The fork's one action module (PLAN.md 3.2). Every archive, delete, spam,
// move, star and mark-read goes through here, so the keyboard, the row's
// hover buttons, the reading pane toolbar, the bulk bar and the palette all do
// exactly the same thing: optimistic list update, the cursor moving on (2.5),
// an undo entry, and the queued op (held 8 s for removals, D3).
import { api } from "../lib/api";
import { t } from "../lib/i18n/index.svelte";
import { mail, rowKey } from "../lib/stores/mail.svelte";
import type { ThreadRow } from "../lib/types";
import { prefs } from "./stores/prefs.svelte";
import { undo } from "./stores/undo.svelte";

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

function apiCallFor(kind: RemovalKind, ids: number[]): () => void {
  if (kind === "archive") return () => void api.archiveMessages(ids);
  if (kind === "delete") return () => void api.deleteMessages(ids);
  return () => void api.reportSpam(ids);
}

/** Remove a thread (archive / delete / spam): the rows leave now, the cursor
 *  moves on, the call is held for the grace window. `ids` are the thread's
 *  message ids. */
export function removeThread(thread: ThreadRow, kind: RemovalKind, ids: number[]): void {
  if (ids.length === 0) return;
  if (kind === "archive" && !archiveOffered(mail.selectedFolder?.role)) return;
  const rows = mail.threads
    .map((row, index) => ({ row, index }))
    .filter(({ row }) => row.id === thread.id);
  if (rows.length === 0) return;
  const wasOpen = mail.selectedThreadId === thread.id;
  const reopen = wasOpen
    ? { threadId: thread.id, messageId: mail.selectedMessageId }
    : null;
  const next = wasOpen ? nextAfterRemoval(mail.threads, thread.id) : null;
  mail.removeThreadFromList(thread.id);
  if (wasOpen) select(next);
  undo.hold(kind, rows, ids, apiCallFor(kind, ids), { reopen });
}

/** Remove exactly these rows (the bulk path): ticked rows, not whole threads. */
export function removeRows(rows: ThreadRow[], kind: RemovalKind, ids: number[]): void {
  if (rows.length === 0 || ids.length === 0) return;
  if (kind === "archive" && !archiveOffered(mail.selectedFolder?.role)) return;
  const keys = new Set(rows.map(rowKey));
  const held = mail.threads
    .map((row, index) => ({ row, index }))
    .filter(({ row }) => keys.has(rowKey(row)));
  mail.removeRowsFromList([...keys]);
  undo.hold(kind, held, ids, apiCallFor(kind, ids));
}

/** A move the folder picker confirmed: same hold, with the destination
 *  remembered so a late undo can find the mail there. */
export function moveRows(
  rows: ThreadRow[],
  ids: number[],
  destImapName: string,
  fire: () => void,
): void {
  if (rows.length === 0 || ids.length === 0) return;
  const keys = new Set(rows.map(rowKey));
  const held = mail.threads
    .map((row, index) => ({ row, index }))
    .filter(({ row }) => keys.has(rowKey(row)));
  const openId = mail.selectedThreadId;
  const wasOpen = openId !== null && rows.some((r) => r.id === openId);
  const next = wasOpen && openId !== null ? nextAfterRemoval(mail.threads, openId) : null;
  mail.removeRowsFromList([...keys]);
  if (wasOpen) select(next);
  undo.hold("move", held, ids, fire, { destImapName });
}

/** Star or unstar a whole thread. Undo = the inverse. */
export function setStarred(thread: ThreadRow, on: boolean, ids: number[]): void {
  if (ids.length === 0) return;
  const before = thread.isStarred;
  mail.patchThreadRow(thread.id, { isStarred: on });
  void api.setStarred(ids, on);
  undo.pushFlag(t(on ? "reading.star" : "reading.unstar"), () => {
    mail.patchThreadRow(thread.id, { isStarred: before });
    void api.setStarred(ids, before);
  });
}

/** Mark a whole thread read or unread. Undo = the inverse. */
export function markRead(thread: ThreadRow, read: boolean, ids: number[]): void {
  if (ids.length === 0) return;
  const before = thread.isRead;
  mail.patchThreadRow(thread.id, { isRead: read });
  void api.markRead(ids, read);
  undo.pushFlag(t(read ? "reading.mark_read" : "reading.mark_unread"), () => {
    mail.patchThreadRow(thread.id, { isRead: before });
    void api.markRead(ids, before);
  });
}

/** Bulk read/unread over exactly these rows. Undo = the inverse per row. */
export function markRowsRead(rows: ThreadRow[], read: boolean, ids: number[]): void {
  if (rows.length === 0 || ids.length === 0) return;
  const keys = rows.map(rowKey);
  const before = rows.map((r) => ({ key: rowKey(r), read: r.isRead }));
  mail.patchRows(keys, { isRead: read });
  void api.markRead(ids, read);
  undo.pushFlag(t(read ? "reading.mark_read" : "reading.mark_unread"), () => {
    for (const b of before) mail.patchRows([b.key], { isRead: b.read });
    void api.markRead(ids, before.every((b) => b.read));
  });
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
