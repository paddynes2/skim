import type { EventRow } from "./types";
export function eventState(row: Pick<EventRow, "status" | "self_response">): "cancelled" | "declined" | "tentative" | "accepted" | "confirmed" {
  if (row.status === "cancelled") return "cancelled";
  if (row.self_response === "declined") return "declined";
  if (row.status === "tentative" || row.self_response === "tentative") return "tentative";
  return row.self_response === "accepted" ? "accepted" : "confirmed";
}
export function zoneClock(anchor: Date, hour: number, minute: number, zone: string, locale = "en"): string {
  const date = new Date(anchor.getFullYear(), anchor.getMonth(), anchor.getDate(), hour, minute);
  try { return date.toLocaleTimeString(locale, {timeZone:zone, hour:"2-digit", minute:"2-digit", hour12:false}); }
  catch { return ""; }
}
