// The daily nudge (PLAN.md Phase 10): once a day at the configured local time
// ("09:00" default, "off" disables) a toast "On you: N · Oldest: X (5d)" whose
// action opens the On me view. Owns the timer and the toast; the decision and
// the text parts are the pure functions in `nudge.ts` (tested).
//
// This is the ONLY nudge: the Rust `should_nudge_now` / `nudge_line` helpers
// exist but are not wired to `notify.rs` (pending/10.md), so the shell does
// the whole thing while the window is open. Minute polling, one indexed read
// only when it is time.
import { t } from "../../lib/i18n/index.svelte";
import { navHooks } from "../nav";
import { toast } from "../stores/toast.svelte";
import { courtApi } from "./api";
import { courtCounts } from "./counts.svelte";
import { nudgeParts, shouldNudgeNow } from "./nudge";
import { courtPrefs } from "./prefs.svelte";

export const POLL_MS = 60_000;
/** Long: the toast is the whole point, and it pauses while hovered. */
export const TOAST_MS = 60_000;

/** Show the nudge now, whatever the clock says (the settings row's "Test"
 *  and the timer both come here). Returns false when there is nothing on him. */
export async function showCourtNudge(nowSecs = Date.now() / 1000): Promise<boolean> {
  await courtCounts.refresh();
  let oldest = null;
  try {
    oldest = (await courtApi.list("on_me", 0, 1))?.[0] ?? null;
  } catch {
    return false;
  }
  const parts = nudgeParts(courtCounts.onMe, oldest, nowSecs);
  if (!parts) return false;
  toast.show({
    text: t("fork.court.nudge", parts),
    ms: TOAST_MS,
    action: {
      label: t("fork.court.nudge_open"),
      run: () => navHooks.court?.("on_me"),
    },
  });
  return true;
}

/** Start the minute poll. Returns a stop function; `courtStore.start()`
 *  calls this once. A day with nothing on him marks itself nudged too, so
 *  the check does not run every minute for the rest of it. */
export function startCourtNudge(): () => void {
  let stopped = false;
  let busy = false;

  async function tick() {
    if (stopped || busy) return;
    busy = true;
    try {
      await courtPrefs.load();
      const now = Math.floor(Date.now() / 1000);
      if (!shouldNudgeNow(courtPrefs.nudge, courtPrefs.nudgedAt, now)) return;
      // One toast at a time (PLAN.md 3.2): an undo toast on screen wins, we
      // try again next minute.
      if (toast.current !== null) return;
      courtPrefs.setNudgedAt(now);
      await showCourtNudge(now);
    } catch {
      // No DB yet / command not registered: nothing to nudge about.
    } finally {
      busy = false;
    }
  }

  void tick();
  const timer = setInterval(() => void tick(), POLL_MS);
  return () => {
    stopped = true;
    clearInterval(timer);
  };
}
