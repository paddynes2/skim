// Pure helpers behind the undo store (PLAN.md 3.2). No Svelte here so the
// node test in scripts/fork/tests can import them directly.

export interface RowLike {
  id: number;
  messageId?: number | null;
}

/** Same identity as `rowKey` in the mail store: message in flat mode, thread
 *  when grouped. Duplicated here (three lines) to keep this file rune-free. */
export function keyOf(row: RowLike): number {
  return row.messageId ?? row.id;
}

/** Drop every row whose key is held for removal, so a refresh or a page load
 *  cannot resurrect a row the user just archived. */
export function withoutPending<T extends RowLike>(rows: T[], pending: ReadonlySet<number>): T[] {
  if (pending.size === 0) return rows;
  return rows.filter((r) => !pending.has(keyOf(r)));
}

/** Put a restored row back where it was. Past the end lands it last; a row
 *  whose key is already present is not inserted twice. */
export function insertAt<T extends RowLike>(rows: T[], row: T, index: number): T[] {
  const key = keyOf(row);
  if (rows.some((r) => keyOf(r) === key)) return rows;
  const at = Math.max(0, Math.min(rows.length, index));
  return [...rows.slice(0, at), row, ...rows.slice(at)];
}

/** Bounded undo stack: newest last, at most `depth` entries. */
export function pushBounded<T>(stack: T[], entry: T, depth: number): T[] {
  const next = [...stack, entry];
  return next.length > depth ? next.slice(next.length - depth) : next;
}
