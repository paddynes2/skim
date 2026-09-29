// Fork (v1.1.1): the preset times for Snooze and Follow-up, and how a due time
// reads. Pure, so src/fork/tests can drive it with a fixed "now".

export interface Preset {
  id: string;
  /** i18n key of the label. */
  label: string;
  at: Date;
}

function at(base: Date, days: number, hour: number, min = 0): Date {
  const d = new Date(base);
  d.setDate(d.getDate() + days);
  d.setHours(hour, min, 0, 0);
  return d;
}

/** Next Monday 08:00 (a week on when today is Monday). */
function nextMonday(now: Date): Date {
  const add = (8 - now.getDay()) % 7 || 7;
  return at(now, add, 8);
}

/** Snooze: later today (if there is room), this evening, tomorrow, weekend, next week. */
export function snoozePresets(now: Date): Preset[] {
  const out: Preset[] = [];
  const later = new Date(now.getTime() + 3 * 3600_000);
  later.setMinutes(later.getMinutes() < 30 ? 0 : 30, 0, 0);
  if (later.getDate() === now.getDate() && later.getHours() < 18) {
    out.push({ id: "later", label: "fork.rem.later_today", at: later });
  }
  if (now.getHours() < 17) out.push({ id: "evening", label: "fork.rem.this_evening", at: at(now, 0, 18) });
  out.push({ id: "tomorrow", label: "fork.rem.tomorrow", at: at(now, 1, 8) });
  const day = now.getDay();
  if (day >= 1 && day <= 4) {
    out.push({ id: "weekend", label: "fork.rem.this_weekend", at: at(now, 6 - day, 9) });
  }
  out.push({ id: "next_week", label: "fork.rem.next_week", at: nextMonday(now) });
  return out;
}

/** Follow-up: "if no reply by", counted in working days, at 08:00. */
export function followupPresets(now: Date): Preset[] {
  const workDays = (n: number) => {
    const d = new Date(now);
    let left = n;
    while (left > 0) {
      d.setDate(d.getDate() + 1);
      if (d.getDay() !== 0 && d.getDay() !== 6) left--;
    }
    d.setHours(8, 0, 0, 0);
    return d;
  };
  return [
    { id: "1d", label: "fork.rem.in_1_day", at: workDays(1) },
    { id: "2d", label: "fork.rem.in_2_days", at: workDays(2) },
    { id: "3d", label: "fork.rem.in_3_days", at: workDays(3) },
    { id: "1w", label: "fork.rem.in_1_week", at: workDays(5) },
  ];
}

/** "Today 15:30", "Tomorrow 08:00", "Thu 08:00", "Mon 5 Oct 08:00". */
export function fmtDue(ts: number, now: Date, locale = "en-GB"): string {
  const d = new Date(ts * 1000);
  const time = new Intl.DateTimeFormat(locale, { hour: "2-digit", minute: "2-digit", hour12: false }).format(d);
  const dayStart = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const diff = Math.round((dayStart(d) - dayStart(now)) / 86_400_000);
  if (diff === 0) return `Today ${time}`;
  if (diff === 1) return `Tomorrow ${time}`;
  if (diff === -1) return `Yesterday ${time}`;
  if (diff > 1 && diff < 7) {
    return `${new Intl.DateTimeFormat(locale, { weekday: "short" }).format(d)} ${time}`;
  }
  return `${new Intl.DateTimeFormat(locale, { weekday: "short", day: "numeric", month: "short" }).format(d)} ${time}`;
}
