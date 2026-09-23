// /slots (PLAN.md Phase 8): the pure half. `formatSlots` turns the backend's
// free slots into the plain list the composer inserts:
//
//   Tue 24 Sep: 10:00, 14:30 SAST (09:00, 13:30 BST)
//   Wed 25 Sep: 11:00 SAST (10:00 BST)
//
//   Or pick a time: https://cal.example/patrick
//
// `openSlotsPopover(insert)` is what the composer calls (toolbar button, or
// typing `/slots` at a line start); the shell mounts SlotsPopover.svelte while
// a request is pending. No IPC here: `src/fork/tests` can drive the formatter.
import type { Slot } from "../calendar/types";
import { slotsOpen } from "./open.svelte";

export const SLOT_DURATIONS = [30, 45, 60] as const;
export const SLOT_DAYS = [3, 5, 10] as const;

export interface SlotsFormat {
  /** The zone the slots were walked in (the sender's). */
  senderTz: string;
  /** The recipient's zone; "" or the sender's zone = no parenthesis. */
  recipientTz: string;
  bookingLink: string;
  /** Appears before the booking link; localised by the caller. */
  bookingLabel?: string;
  locale?: string;
}

/** Zone abbreviation ("SAST", "BST", "CET"), falling back to "GMT+2". The
 *  English locales differ in which zones they abbreviate, so several are tried. */
export function zoneAbbrev(tz: string, at: Date): string {
  let fallback = "";
  for (const locale of ["en-GB", "en-ZA", "en-US", "en-AU", "en-IE"]) {
    try {
      const part = new Intl.DateTimeFormat(locale, { timeZone: tz, timeZoneName: "short" })
        .formatToParts(at)
        .find((p) => p.type === "timeZoneName")?.value;
      if (!part) continue;
      if (!/^(GMT|UTC)/.test(part)) return part;
      fallback ||= part;
    } catch {
      // unknown zone for this runtime: try the next locale, else the fallback
    }
  }
  return fallback || tz;
}

/** "HH:MM" of an instant in a zone. */
export function timeIn(ts: number, tz: string): string {
  try {
    return new Intl.DateTimeFormat("en-GB", { timeZone: tz, hour: "2-digit", minute: "2-digit", hour12: false })
      .format(new Date(ts * 1000))
      .replace(/^24/, "00");
  } catch {
    return "??:??";
  }
}

/** Is the zone a real IANA name this runtime knows? */
export function validZone(tz: string): boolean {
  if (!tz) return false;
  try {
    new Intl.DateTimeFormat("en", { timeZone: tz });
    return true;
  } catch {
    return false;
  }
}

/** Group by the backend's day (walk order kept). */
export function groupByDay(slots: Slot[]): { day: string; label: string; slots: Slot[] }[] {
  const out: { day: string; label: string; slots: Slot[] }[] = [];
  for (const s of slots) {
    const last = out[out.length - 1];
    if (last && last.day === s.day) last.slots.push(s);
    else out.push({ day: s.day, label: s.label, slots: [s] });
  }
  return out;
}

export function formatSlots(slots: Slot[], f: SlotsFormat): string {
  if (slots.length === 0) return "";
  const showRecipient = !!f.recipientTz && f.recipientTz !== f.senderTz && validZone(f.recipientTz);
  const lines = groupByDay(slots).map((g) => {
    const at = new Date(g.slots[0].start * 1000);
    const mine = g.slots.map((s) => s.time).join(", ");
    let line = `${g.label}: ${mine} ${zoneAbbrev(f.senderTz, at)}`;
    if (showRecipient) {
      const theirs = g.slots.map((s) => timeIn(s.start, f.recipientTz)).join(", ");
      line += ` (${theirs} ${zoneAbbrev(f.recipientTz, at)})`;
    }
    return line;
  });
  let text = lines.join("\n");
  if (f.bookingLink) text += `\n\n${f.bookingLabel ?? "Or pick a time:"} ${f.bookingLink}`;
  return text;
}

/** Open the popover for a composer; `insert` writes the list at its cursor. */
export function openSlotsPopover(insert: (text: string) => void): void {
  slotsOpen.open(insert);
}

/** True when the text before the caret ends in `/slots` at a line start:
 *  the composer's trigger. The caller removes the token and opens the popover. */
export function isSlotsTrigger(textBeforeCaret: string): boolean {
  return /(^|\n)\/slots$/.test(textBeforeCaret);
}
