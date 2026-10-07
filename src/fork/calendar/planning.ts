import type { AvailabilityResult, EventRow } from "./types";

export function monthDays(anchor: Date): Date[] {
  const start = new Date(anchor.getFullYear(), anchor.getMonth(), 1, 12);
  start.setDate(start.getDate() - (start.getDay() + 6) % 7);
  return Array.from({ length: 42 }, (_, i) => new Date(start.getFullYear(), start.getMonth(), start.getDate() + i, 12));
}
export function overlappingEvents(rows: EventRow[], start: number, end: number, excluded?: number, dateBoundary: (date: string) => number | null = (date) => new Date(`${date}T00:00:00`).getTime() / 1000): EventRow[] {
  if (!Number.isFinite(start) || !Number.isFinite(end) || end <= start) return [];
  return rows.filter((r) => {
    const rowStart = r.all_day && r.start_date ? dateBoundary(r.start_date) : r.start_ts;
    const rowEnd = r.all_day && r.end_date ? dateBoundary(r.end_date) : r.end_ts;
    return r.id !== excluded && r.status !== "cancelled" && r.self_response !== "declined" && r.transparency !== "transparent" && r.options?.transparency !== "transparent" && rowStart !== null && rowEnd !== null && rowStart < end && rowEnd > start;
  });
}
/** Unknown calendars do not contribute a free-time claim. Callers must disclose them. */
export function availableCandidates(result: AvailabilityResult, candidates: { start: number; end: number }[]): { start: number; end: number }[] {
  const known = result.calendars.filter((c) => c.status === "available");
  if (!known.length) return [];
  return candidates.filter((c) => c.end > c.start && c.start >= result.from_ts && c.end <= result.to_ts && result.calendars.every((k) => k.busy.every((b) => b.start_ts >= c.end || b.end_ts <= c.start)));
}
