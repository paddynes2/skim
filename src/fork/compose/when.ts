// Fork (6.4): when to send. The presets behind the split Send button, the
// free-text parser behind "Pick…" (`3d`, `tomorrow 9am`, `fri 14:00`), and the
// label the Scheduled list shows. Pure over a `now`, so it is testable.

const DAYS = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];
const DAY_NAMES = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/** The default hour for a day without a time ("tomorrow", "fri"). */
export const DEFAULT_HOUR = 8;

export interface WhenPick {
  /** Unix seconds. */
  at: number;
  label: string;
}

function at(base: Date, hour: number, minute: number): Date {
  const d = new Date(base);
  d.setHours(hour, minute, 0, 0);
  return d;
}

function addDays(base: Date, n: number): Date {
  const d = new Date(base);
  d.setDate(d.getDate() + n);
  return d;
}

/** The next `weekday` (0 = Sunday) strictly after today, or today itself when
 *  `allowToday` and the day's time is still ahead (decided by the caller). */
function nextWeekday(now: Date, weekday: number, allowToday: boolean): Date {
  let delta = (weekday - now.getDay() + 7) % 7;
  if (delta === 0 && !allowToday) delta = 7;
  return addDays(now, delta);
}

function pad(n: number): string {
  return n < 10 ? `0${n}` : String(n);
}

function hhmm(d: Date): string {
  return `${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** "Tomorrow 08:00", "Fri 14:00" (inside a week), else "23 Sep 14:00". */
export function formatWhen(when: Date, now: Date): string {
  const dayStart = at(now, 0, 0);
  const days = Math.floor((at(when, 0, 0).getTime() - dayStart.getTime()) / 86_400_000);
  const time = hhmm(when);
  if (days === 0) return `Today ${time}`;
  if (days === 1) return `Tomorrow ${time}`;
  if (days > 1 && days < 7) return `${DAY_NAMES[when.getDay()].slice(0, 3)} ${time}`;
  return `${when.getDate()} ${MONTHS[when.getMonth()]} ${time}`;
}

/** The three fixed choices of the Send menu. */
export function presets(now: Date): { key: "tomorrow" | "monday" | "in2h"; when: Date }[] {
  const tomorrow = at(addDays(now, 1), DEFAULT_HOUR, 0);
  const monday = at(nextWeekday(now, 1, false), DEFAULT_HOUR, 0);
  const in2h = new Date(now.getTime() + 2 * 3_600_000);
  in2h.setSeconds(0, 0);
  return [
    { key: "tomorrow", when: tomorrow },
    { key: "monday", when: monday },
    { key: "in2h", when: in2h },
  ];
}

const DURATION = /^(\d+)\s*(m|min|mins|minute|minutes|h|hr|hrs|hour|hours|d|day|days|w|wk|week|weeks)$/;
const TIME = /^(\d{1,2})(?::(\d{2}))?\s*(am|pm)?$/;

function parseTime(token: string): { hour: number; minute: number } | null {
  if (token === "noon") return { hour: 12, minute: 0 };
  if (token === "midnight") return { hour: 0, minute: 0 };
  const m = TIME.exec(token);
  if (!m) return null;
  let hour = Number(m[1]);
  const minute = m[2] ? Number(m[2]) : 0;
  const ap = m[3];
  if (minute > 59) return null;
  if (ap) {
    if (hour < 1 || hour > 12) return null;
    if (ap === "am" && hour === 12) hour = 0;
    if (ap === "pm" && hour !== 12) hour += 12;
  } else if (!m[2]) {
    // A bare number without am/pm or minutes is a day, not a time ("3d" is
    // handled by DURATION; "fri 14" is too easy to misread).
    return null;
  } else if (hour > 23) {
    return null;
  }
  return { hour, minute };
}

/** Parse free text into a moment strictly after `now`, or `null`. */
export function parseWhen(input: string, now: Date): Date | null {
  const text = input
    .trim()
    .toLowerCase()
    .replace(/^(in|at|on|next)\s+/, "")
    .replace(/\s+/g, " ");
  if (!text) return null;

  const dur = DURATION.exec(text.replace(/\s+/g, ""));
  if (dur) {
    const n = Number(dur[1]);
    const unit = dur[2][0];
    const ms = { m: 60_000, h: 3_600_000, d: 86_400_000, w: 7 * 86_400_000 }[unit] ?? 0;
    if (n <= 0 || ms === 0) return null;
    const d = new Date(now.getTime() + n * ms);
    d.setSeconds(0, 0);
    return d > now ? d : null;
  }

  // "<day> <time>" | "<day>" | "<time>", where <time> may be "9 am" (two tokens).
  const tokens = text.split(" ");
  let dayToken: string | null = null;
  let timeToken: string | null = null;
  if (tokens.length >= 1 && /^[a-z]+$/.test(tokens[0]) && !parseTime(tokens[0])) {
    dayToken = tokens.shift() ?? null;
  }
  if (tokens.length > 0) timeToken = tokens.join("");
  if (dayToken === null && timeToken === null) return null;

  let time = timeToken !== null ? parseTime(timeToken) : null;
  if (timeToken !== null && time === null) return null;

  let day: Date;
  if (dayToken === null) {
    day = now;
  } else if (dayToken === "today" || dayToken === "tod") {
    day = now;
  } else if (dayToken === "tomorrow" || dayToken === "tmrw" || dayToken === "tmr") {
    day = addDays(now, 1);
  } else if (dayToken === "week") {
    day = nextWeekday(now, 1, false);
  } else {
    const dt = dayToken;
    const idx = DAYS.findIndex(
      (d, i) => dt === d || (dt.length > 3 && DAY_NAMES[i].toLowerCase().startsWith(dt)),
    );
    if (idx < 0) return null;
    // Today's weekday is fine when the wanted time is still ahead; a bare day
    // ("fri" on a Friday) means next week once the default hour has passed.
    const probe = time ?? { hour: DEFAULT_HOUR, minute: 0 };
    const today = at(now, probe.hour, probe.minute);
    day = nextWeekday(now, idx, now.getDay() === idx && today > now);
  }

  if (time === null) time = { hour: DEFAULT_HOUR, minute: 0 };
  let when = at(day, time.hour, time.minute);
  // A bare time already gone today rolls to tomorrow.
  if (dayToken === null && when <= now) when = at(addDays(now, 1), time.hour, time.minute);
  return when > now ? when : null;
}
