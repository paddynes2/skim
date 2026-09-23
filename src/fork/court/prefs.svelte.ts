// Court preferences (PLAN.md Phase 10): the `fork_court_*` settings keys the
// views and the nudge read. Kept out of `stores/prefs.svelte.ts` for the same
// reason the calendar keeps `calPrefs`: that file is edited by other phases at
// the same time. Read once through `courtPrefs.load()`, written back through
// `set_setting`. The Rust side reads `fork_court_ai` / `fork_court_ai_cap`
// itself; this store only edits them.
import { api } from "../../lib/api";

export const AI_CAP_DEFAULT = 200;
export const AMBER_DAYS_DEFAULT = 2;
export const RED_DAYS_DEFAULT = 5;
export const NUDGE_DEFAULT = "09:00";

const state = $state({
  /** The optional AI pass; off until he turns it on (D-10g). */
  ai: false,
  /** Threads per local day the AI pass may judge. */
  aiCap: AI_CAP_DEFAULT,
  /** How far back On me / Waiting look, in days; 0 = everything (Rust default 30). */
  windowDays: 30,
  /** Row age turns amber past this many days ... */
  amberDays: AMBER_DAYS_DEFAULT,
  /** ... and red past this many. */
  redDays: RED_DAYS_DEFAULT,
  /** "HH:MM" local, or "off". */
  nudge: NUDGE_DEFAULT,
  /** Unix seconds of the last nudge shown (internal row `fork_court_nudged_at`). */
  nudgedAt: null as number | null,
  loaded: false,
});

let loading: Promise<void> | null = null;

function persist(key: string, value: string) {
  void api.setSetting(key, value).catch(() => {});
}

function posInt(v: string | undefined, fallback: number): number {
  const n = Number(v);
  return Number.isInteger(n) && n >= 0 ? n : fallback;
}

const HHMM = /^\d{2}:\d{2}$/;

export const courtPrefs = {
  /** Apply a settings map. Unknown or absent keys keep defaults. */
  hydrate(s: Record<string, string>) {
    if (s.fork_court_ai !== undefined) state.ai = s.fork_court_ai === "on";
    if (s.fork_court_ai_cap !== undefined) state.aiCap = posInt(s.fork_court_ai_cap, AI_CAP_DEFAULT);
    if (s.fork_court_amber_days !== undefined) state.amberDays = posInt(s.fork_court_amber_days, AMBER_DAYS_DEFAULT);
    if (s.fork_court_window_days !== undefined)
      state.windowDays = s.fork_court_window_days.trim() === "all" ? 0 : posInt(s.fork_court_window_days, 30);
    if (s.fork_court_red_days !== undefined) state.redDays = posInt(s.fork_court_red_days, RED_DAYS_DEFAULT);
    if (s.fork_court_nudge !== undefined) {
      const v = s.fork_court_nudge.trim();
      if (v.toLowerCase() === "off" || HHMM.test(v)) state.nudge = v;
    }
    if (s.fork_court_nudged_at !== undefined) {
      const n = Number(s.fork_court_nudged_at);
      state.nudgedAt = Number.isFinite(n) && n > 0 ? n : null;
    }
    state.loaded = true;
  },
  /** Read the settings once; later calls resolve at once. */
  load(): Promise<void> {
    if (state.loaded) return Promise.resolve();
    loading ??= api
      .getSettings()
      .then((s) => this.hydrate(s))
      .catch(() => {
        state.loaded = true;
      })
      .finally(() => {
        loading = null;
      });
    return loading;
  },
  get loaded() {
    return state.loaded;
  },
  get ai() {
    return state.ai;
  },
  setAi(on: boolean) {
    state.ai = on;
    persist("fork_court_ai", on ? "on" : "off");
  },
  get aiCap() {
    return state.aiCap;
  },
  setAiCap(n: number) {
    if (!Number.isInteger(n) || n < 0) return;
    state.aiCap = n;
    persist("fork_court_ai_cap", String(n));
  },
  get windowDays() {
    return state.windowDays;
  },
  setWindowDays(n: number) {
    if (!Number.isInteger(n) || n < 0) return;
    state.windowDays = n;
    persist("fork_court_window_days", n === 0 ? "all" : String(n));
  },
  get amberDays() {
    return state.amberDays;
  },
  setAmberDays(n: number) {
    if (!Number.isInteger(n) || n < 0) return;
    state.amberDays = n;
    persist("fork_court_amber_days", String(n));
  },
  get redDays() {
    return state.redDays;
  },
  setRedDays(n: number) {
    if (!Number.isInteger(n) || n < 0) return;
    state.redDays = n;
    persist("fork_court_red_days", String(n));
  },
  get nudge() {
    return state.nudge;
  },
  /** "HH:MM" or "off"; anything else is ignored. */
  setNudge(v: string) {
    const s = v.trim();
    if (s.toLowerCase() === "off") {
      state.nudge = "off";
    } else if (HHMM.test(s)) {
      state.nudge = s;
    } else {
      return;
    }
    persist("fork_court_nudge", state.nudge);
  },
  get nudgedAt() {
    return state.nudgedAt;
  },
  /** Remember the nudge shown at `unixSecs`. The settings row is internal
   *  (not user-facing); until the main session allows it, `set_setting`
   *  refuses and the in-memory value still stops a second toast today. */
  setNudgedAt(unixSecs: number) {
    state.nudgedAt = unixSecs;
    persist("fork_court_nudged_at", String(unixSecs));
  },
};
