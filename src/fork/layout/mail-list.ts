import type { ThreadRow } from "../../lib/types";
export type ListItem = { key: string; bucket: string | null; thread: ThreadRow | null; top: number; height: number };
export function cleanPreview(value: string) {
  // Remove only recognizable quoted tails. Ordinary prose remains unchanged.
  return value.replace(/\s+/g, " ").split(/(?:\s+On .{5,160} wrote:|\s+-{2,}\s*Original Message\s*-{2,}|\s+Sent from my (?:iPhone|iPad|Android))/i)[0].trim();
}

export function dateBucket(unix: number, now = Date.now()) {
  const day = new Date(now); day.setHours(0, 0, 0, 0);
  const yesterday = new Date(day); yesterday.setDate(yesterday.getDate() - 1);
  return unix * 1000 >= day.getTime() ? "today" : unix * 1000 >= yesterday.getTime() ? "yesterday" : "earlier";
}

export function listLayout(rows: ThreadRow[], rowHeight: number, grouped: boolean, now: number) {
  let offset = 0, previous = "";
  const items: ListItem[] = [];
  for (const row of rows) {
    const bucket = dateBucket(row.date, now);
    if (grouped && bucket !== previous) {
      items.push({ key: `date:${bucket}:${row.messageId ?? row.id}`, bucket, thread: null, top: offset, height: 28 });
      offset += 28; previous = bucket;
    }
    items.push({ key: `row:${row.messageId ?? row.id}`, bucket: null, thread: row, top: offset, height: rowHeight });
    offset += rowHeight;
  }
  return { items, height: offset };
}

export function listWindow(items: ListItem[], scrollTop: number, viewportHeight: number, overscan = 8) {
  let lo = 0, hi = items.length;
  while (lo < hi) { const mid = (lo + hi) >>> 1; if (items[mid].top + items[mid].height <= scrollTop) lo = mid + 1; else hi = mid; }
  const start = Math.max(0, lo - overscan);
  let end = lo;
  while (end < items.length && items[end].top < scrollTop + viewportHeight) end++;
  return { start, end: Math.min(items.length, end + overscan) };
}
