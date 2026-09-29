// Fork (v1.1.1): the pure half of the calendar time field. Parses what a person
// types ("2pm", "14:30", "1430", "9.15a") into "HH:MM", and builds the
// quarter-hour option list, with durations when an anchor (the start) is given.
// No DOM here, so src/fork/tests can drive it.

/** "HH:MM" (24h) from free text, or null when it is not a time. */
export function parseTime(raw: string): string | null {
  const s = raw.trim().toLowerCase().replace(/\s+/g, "");
  if (!s) return null;
  const m = s.match(/^(\d{1,2})(?:[:.h]?(\d{2}))?(a|am|p|pm)?$/);
  let h: number;
  let min: number;
  let mer: string | undefined;
  if (m) {
    h = Number(m[1]);
    min = m[2] ? Number(m[2]) : 0;
    mer = m[3];
  } else {
    // "1430" / "930" without a separator.
    const n = s.match(/^(\d{3,4})(a|am|p|pm)?$/);
    if (!n) return null;
    const digits = n[1].padStart(4, "0");
    h = Number(digits.slice(0, 2));
    min = Number(digits.slice(2));
    mer = n[2];
  }
  if (min > 59) return null;
  // A bare "2" or "4:30" in a work calendar means the afternoon; "02:00" and
  // "7" keep their face value.
  if (!mer && h >= 1 && h <= 6 && !/^0/.test(s)) h += 12;
  if (mer) {
    if (h < 1 || h > 12) return null;
    const pm = mer.startsWith("p");
    if (h === 12) h = pm ? 12 : 0;
    else if (pm) h += 12;
  }
  if (h > 23) return null;
  return `${String(h).padStart(2, "0")}:${String(min).padStart(2, "0")}`;
}

export function toMinutes(hhmm: string): number {
  const [h, m] = hhmm.split(":").map(Number);
  return h * 60 + m;
}

export function fromMinutes(total: number): string {
  const t = ((total % 1440) + 1440) % 1440;
  return `${String(Math.floor(t / 60)).padStart(2, "0")}:${String(t % 60).padStart(2, "0")}`;
}

/** "30 min", "1 h", "1 h 30". */
export function fmtDuration(min: number): string {
  if (min < 60) return `${min} min`;
  const h = Math.floor(min / 60);
  const r = min % 60;
  return r ? `${h} h ${r}` : `${h} h`;
}

export interface TimeOption {
  value: string;
  /** Duration from the anchor, when there is one. */
  note: string | null;
}

/**
 * Quarter-hour options for the dropdown. Without an anchor: the whole day.
 * With one (an end time on the same day): from 15 minutes after the anchor to
 * the end of the day, each with its duration, so the list reads like Google's.
 */
export function timeOptions(anchor: string | null, stepMin = 15): TimeOption[] {
  const out: TimeOption[] = [];
  if (anchor) {
    const a = toMinutes(anchor);
    for (let m = a + stepMin; m < 1440; m += stepMin) {
      out.push({ value: fromMinutes(m), note: fmtDuration(m - a) });
    }
    return out;
  }
  for (let m = 0; m < 1440; m += stepMin) out.push({ value: fromMinutes(m), note: null });
  return out;
}

/** Keep an event's length when its start moves: the new end, as date + time. */
export function shiftEnd(
  oldStart: { date: string; time: string },
  oldEnd: { date: string; time: string },
  newStart: { date: string; time: string },
): { date: string; time: string } {
  const ms = (d: { date: string; time: string }) => {
    const [y, mo, da] = d.date.split("-").map(Number);
    const [h, mi] = d.time.split(":").map(Number);
    return new Date(y, mo - 1, da, h, mi).getTime();
  };
  const len = ms(oldEnd) - ms(oldStart);
  if (!(len > 0)) return oldEnd;
  const end = new Date(ms(newStart) + len);
  const pad = (n: number) => String(n).padStart(2, "0");
  return {
    date: `${end.getFullYear()}-${pad(end.getMonth() + 1)}-${pad(end.getDate())}`,
    time: `${pad(end.getHours())}:${pad(end.getMinutes())}`,
  };
}
