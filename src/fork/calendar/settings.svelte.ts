// Calendar preferences (PLAN.md 7.7): the `fork_cal_*` / `fork_booking_link`
// settings keys. Kept out of `stores/prefs.svelte.ts` so this phase does not
// edit a file two other phases are editing at the same time; the main session
// may fold `hydrate` into `prefs.hydrate` later. Read lazily through
// `calPrefs.load()` (once), written back through `set_setting` like prefs.
import { api } from "../../lib/api";

const state = $state({
  /** Minutes; the quick-create and `n` length. */
  defaultLen: 30,
  /** "HH:MM" local wall clock. */
  workStart: "09:00",
  workEnd: "17:00",
  /** "1,2,3,4,5", Mon=1 … Sun=7 (what fork_free_slots reads). */
  workDays: "1,2,3,4,5",
  /** IANA zone shown as the second clock and the default /slots recipient zone; "" = none. */
  secondTz: "",
  bookingLink: "",
  loaded: false,
});

let loading: Promise<void> | null = null;

function persist(key: string, value: string) {
  void api.setSetting(key, value).catch(() => {});
}

const HHMM = /^\d{2}:\d{2}$/;

export const calPrefs = {
  /** Apply a settings map. Unknown or absent keys keep defaults. */
  hydrate(s: Record<string, string>) {
    const len = Number(s.fork_cal_default_len);
    if ([15, 30, 45, 60].includes(len)) state.defaultLen = len;
    if (s.fork_cal_work_start && HHMM.test(s.fork_cal_work_start)) state.workStart = s.fork_cal_work_start;
    if (s.fork_cal_work_end && HHMM.test(s.fork_cal_work_end)) state.workEnd = s.fork_cal_work_end;
    if (s.fork_cal_work_days !== undefined) state.workDays = s.fork_cal_work_days;
    if (s.fork_cal_second_tz !== undefined) state.secondTz = s.fork_cal_second_tz;
    if (s.fork_booking_link !== undefined) state.bookingLink = s.fork_booking_link;
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
  get defaultLen() {
    return state.defaultLen;
  },
  setDefaultLen(n: number) {
    state.defaultLen = n;
    persist("fork_cal_default_len", String(n));
  },
  get workStart() {
    return state.workStart;
  },
  setWorkStart(v: string) {
    if (!HHMM.test(v)) return;
    state.workStart = v;
    persist("fork_cal_work_start", v);
  },
  get workEnd() {
    return state.workEnd;
  },
  setWorkEnd(v: string) {
    if (!HHMM.test(v)) return;
    state.workEnd = v;
    persist("fork_cal_work_end", v);
  },
  get workDays() {
    return state.workDays;
  },
  /** Mon=1 … Sun=7 as a set. */
  get workDaySet(): Set<number> {
    return new Set(
      state.workDays
        .split(",")
        .map((x) => Number(x.trim()))
        .filter((n) => n >= 1 && n <= 7),
    );
  },
  toggleWorkDay(day: number) {
    const set = this.workDaySet;
    if (set.has(day)) set.delete(day);
    else set.add(day);
    state.workDays = [...set].sort((a, b) => a - b).join(",");
    persist("fork_cal_work_days", state.workDays);
  },
  get secondTz() {
    return state.secondTz;
  },
  setSecondTz(v: string) {
    state.secondTz = v.trim();
    persist("fork_cal_second_tz", state.secondTz);
  },
  get bookingLink() {
    return state.bookingLink;
  },
  setBookingLink(v: string) {
    state.bookingLink = v.trim();
    persist("fork_booking_link", state.bookingLink);
  },
};
