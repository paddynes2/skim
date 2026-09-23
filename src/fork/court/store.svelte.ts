// Court lifecycle (PLAN.md Phase 10): registers `navHooks.court` (so `g o` /
// `g w`, the palette rows and the nudge's action all land in the views),
// loads the sidebar counts and starts the daily nudge. The view itself is
// the mail store: `mail.selectCourt(state)` shows the virtual folder -920 /
// -921 through the ordinary list, reading pane, keys and undo.
//
// `App.svelte` calls `courtStore.start()` once after boot (pending/10-ui.md).
import { mail } from "../../lib/stores/mail.svelte";
import { ui } from "../../lib/stores/ui.svelte";
import { navHooks } from "../nav";
import { courtCounts } from "./counts.svelte";
import { startCourtNudge } from "./CourtNudge";
import { courtPrefs } from "./prefs.svelte";
import type { CourtState } from "./types";

let started = false;
let stopNudge: (() => void) | null = null;

export const courtStore = {
  /** Open one of the two views from anywhere (keys, palette, sidebar, toast). */
  open(state: CourtState) {
    ui.showMail();
    void mail.selectCourt(state);
  },
  /** Register the hook, load prefs + counts, start the nudge. Idempotent. */
  start(): () => void {
    if (started) return () => {};
    started = true;
    navHooks.court = (state) => courtStore.open(state);
    void courtPrefs.load();
    void courtCounts.refresh();
    stopNudge = startCourtNudge();
    return () => {
      stopNudge?.();
      stopNudge = null;
      delete navHooks.court;
      started = false;
    };
  },
};
