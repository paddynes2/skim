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

// ---- Look A (D97) ----

/** The Pastel theme's fills (tokens.css). An event's Google colour maps to the nearest by hue. */
const PASTELS: { token: string; hue: number }[] = [
  { token: "--p-rose", hue: 352 },
  { token: "--p-amber", hue: 45 },
  { token: "--p-mint", hue: 140 },
  { token: "--p-mint-2", hue: 125 },
  { token: "--p-sky", hue: 199 },
  { token: "--p-peri", hue: 229 },
  { token: "--p-lilac", hue: 255 },
];

/** Hue (0..360) and saturation (0..1) of "#rrggbb"; null for anything else. */
export function hueOf(hex: string | null | undefined): { hue: number; sat: number; light: number } | null {
  const m = /^#?([0-9a-f]{6})$/i.exec((hex ?? "").trim());
  if (!m) return null;
  const n = parseInt(m[1], 16);
  const r = ((n >> 16) & 255) / 255, g = ((n >> 8) & 255) / 255, b = (n & 255) / 255;
  const max = Math.max(r, g, b), min = Math.min(r, g, b), d = max - min;
  const light = (max + min) / 2;
  const sat = d === 0 ? 0 : d / (1 - Math.abs(2 * light - 1));
  let hue = 0;
  if (d !== 0) hue = max === r ? ((g - b) / d) % 6 : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
  return { hue: (hue * 60 + 360) % 360, sat, light };
}

/** The pastel token for a calendar or event colour. Greys and unknowns get periwinkle. */
export function pastelFor(hex: string | null | undefined): string {
  const h = hueOf(hex);
  if (!h || h.sat < 0.15) return "--p-peri";
  // Greens: deep ones (Basil) take the stronger mint, light ones the soft mint.
  if (h.hue >= 90 && h.hue <= 170) return h.light < 0.35 ? "--p-mint-2" : "--p-mint";
  let best = PASTELS[0], dist = 999;
  for (const p of PASTELS) {
    const d = Math.min(Math.abs(p.hue - h.hue), 360 - Math.abs(p.hue - h.hue));
    if (d < dist) { dist = d; best = p; }
  }
  return best.token;
}

const JOIN_RE = /https:\/\/(?:[\w-]+\.)*(?:meet\.google\.com|zoom\.us|teams\.microsoft\.com|teams\.live\.com)\/[^\s"'<>)\]]+/i;

/** The meeting link and its service: Meet from conferenceData, else the first Meet, Zoom or Teams URL. */
export function joinLink(row: Pick<EventRow, "hangout_link" | "location" | "description">): { url: string; kind: "meet" | "zoom" | "teams" } | null {
  const url = row.hangout_link || JOIN_RE.exec(row.location ?? "")?.[0] || JOIN_RE.exec(row.description ?? "")?.[0];
  if (!url) return null;
  const kind = /zoom\.us/i.test(url) ? "zoom" : /teams\./i.test(url) ? "teams" : "meet";
  return { url: url.replace(/[.,;]+$/, ""), kind };
}

/** ISO 8601 week number. */
export function isoWeek(d: Date): number {
  const t = new Date(Date.UTC(d.getFullYear(), d.getMonth(), d.getDate()));
  const day = t.getUTCDay() || 7;
  t.setUTCDate(t.getUTCDate() + 4 - day);
  const y0 = new Date(Date.UTC(t.getUTCFullYear(), 0, 1));
  return Math.ceil(((t.getTime() - y0.getTime()) / 86_400_000 + 1) / 7);
}

/** An invite the user has not answered: they are a guest, not the organiser, and the answer is pending. */
export function needsReply(row: Pick<EventRow, "self_response" | "organizer_email" | "status">, own: string[]): boolean {
  if (row.status === "cancelled" || row.self_response !== "needsAction") return false;
  const org = (row.organizer_email ?? "").toLowerCase();
  return !own.includes(org);
}

/** "PN" for "Patrick Nesbitt", "W" for "wesley@x". */
export function initials(name: string): string {
  const base = name.includes("@") ? name.split("@")[0].replace(/[._-]+/g, " ") : name;
  const parts = base.trim().split(/\s+/).filter(Boolean);
  return ((parts[0]?.[0] ?? "") + (parts.length > 1 ? parts[parts.length - 1][0] : "")).toUpperCase() || "?";
}
