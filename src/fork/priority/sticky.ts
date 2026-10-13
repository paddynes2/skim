// Priority (v1.1.11, D64): reading a Priority conversation takes it out of
// Priority in the list query at once. Left alone, the row would jump to its
// date while it is open, and the next/previous keys would walk from there.
// So a list refresh keeps the open conversation where it was in the Priority
// section; the mail store lets go when another conversation is opened.
import type { ThreadRow } from "../../lib/types";

const isThread = (r: ThreadRow) => r.messageId == null;

/** Leading rows that are Priority conversations. */
export function priorityRun(rows: ThreadRow[], ids: ReadonlySet<number>): number {
  let n = 0;
  while (n < rows.length && isThread(rows[n]) && ids.has(rows[n].id)) n++;
  return n;
}

/**
 * `prev`: the rows on screen; `next`: the refreshed rows; `openId`: the open
 * conversation; `ids`: the Priority set the screen was drawn with (the old
 * one: the store reads the new set after the rows land).
 * Returns the rows to show, and the conversation now held in Priority
 * (`null` when nothing is held).
 */
export function keepOpenInPriority(
  prev: ThreadRow[],
  next: ThreadRow[],
  openId: number | null,
  ids: ReadonlySet<number>,
): { rows: ThreadRow[]; held: number | null } {
  if (openId == null) return { rows: next, held: null };
  const before = prev.findIndex((r) => isThread(r) && r.id === openId);
  if (before < 0 || before >= priorityRun(prev, ids)) return { rows: next, held: null };
  const after = next.findIndex((r) => isThread(r) && r.id === openId);
  if (after < 0) return { rows: next, held: null };
  const rest = next.filter((_, i) => i !== after);
  const at = Math.min(before, priorityRun(rest, ids));
  return { rows: [...rest.slice(0, at), next[after], ...rest.slice(at)], held: openId };
}
