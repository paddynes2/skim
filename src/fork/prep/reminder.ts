// Ten-minutes-before reminder (PLAN.md Phase 11). While the app is open the
// shell polls `fork_prep_upcoming` once a minute; every event that comes back
// is toasted once ("Call with Anna in 8 min · Prep"), and Prep opens the panel.
//
// The pure half lives in `due.ts` (tested under `node --test src/fork/tests`);
// this file owns the timer and the toast.
import { t } from "../../lib/i18n/index.svelte";
import { toast } from "../stores/toast.svelte";
import { prepApi } from "./api";
import { dueReminders, minutesUntil } from "./due";
import { prepOpen } from "./open.svelte";
import type { UpcomingEvent } from "./types";

/** Poll cadence. Rust answers events inside a 10-minute window, so a minute
 *  is fine-grained enough and costs one indexed read. */
export const POLL_MS = 60_000;
/** How long the toast stays. Long: it is the whole point of the feature. */
export const TOAST_MS = 60_000;

/** The toast text: title, minutes, first guest (and how many more). */
export function reminderText(e: UpcomingEvent, nowSecs: number): string {
  const first = e.guests[0];
  const who = first ? (first.name ?? first.email) : "";
  const more = e.guests.length - 1;
  const title = e.summary.trim() || t("fork.prep.untitled");
  const mins = minutesUntil(e.startTs, nowSecs);
  return more > 0
    ? t("fork.prep.toast_more", { title, mins, who, n: more })
    : t("fork.prep.toast", { title, mins, who });
}

function showReminder(e: UpcomingEvent) {
  toast.show({
    text: reminderText(e, Date.now() / 1000),
    ms: TOAST_MS,
    action: {
      label: t("fork.prep.prep"),
      run: () => prepOpen.open(e.id),
    },
  });
}

/** Start polling. Returns a stop function; the shell calls this once from
 *  App.svelte after boot. One toast at a time: when several events are due in
 *  the same minute, the later ones wait for the next tick. */
export function startPrepReminder(): () => void {
  const seen = new Set<number>();
  const pending: UpcomingEvent[] = [];
  let stopped = false;

  async function tick() {
    if (stopped) return;
    let events: UpcomingEvent[] = [];
    try {
      events = await prepApi.upcoming();
    } catch {
      return; // no calendar, or not connected: nothing to remind about
    }
    if (stopped) return;
    pending.push(...dueReminders(events, seen));
    if (pending.length > 0 && toast.current === null) {
      const next = pending.shift();
      if (next) showReminder(next);
    }
  }

  void tick();
  const timer = setInterval(() => void tick(), POLL_MS);
  return () => {
    stopped = true;
    clearInterval(timer);
  };
}
