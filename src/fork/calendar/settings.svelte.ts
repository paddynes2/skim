// Calendar preferences (PLAN.md 7.7): the `fork_cal_*` / `fork_booking_link`
// settings keys. Kept out of `stores/prefs.svelte.ts` so this phase does not
// edit a file two other phases are editing at the same time; the main session
// may fold `hydrate` into `prefs.hydrate` later. Read lazily through
// `calPrefs.load()` (once), written back through `set_setting` like prefs.
import { api } from "../../lib/api";
import type { CalView } from "./types";

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
  visibleStart: "06:00",
  visibleEnd: "22:00",
  /** Look A and the Google Calendar view settings (D97). */
  weekStart: 1 as 0 | 1 | 6,
  showWeekends: true,
  showDeclined: true,
  dimPast: true,
  weekNumbers: false,
  defaultView: "week" as CalView,
  customDays: 4,
  railOpen: true,
  addMeetDefault: false,
  /** Calendar ids hidden from the grid only; syncing and busy time are unaffected (D97). */
  hiddenCals: [] as number[],
  loaded: false,
});

const VIEWS: CalView[] = ["day", "week", "workweek", "custom", "month", "year", "agenda"];
const on = (v: string | undefined, d: boolean) => (v === undefined ? d : v === "1");

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
    if (s.fork_cal_visible_start && HHMM.test(s.fork_cal_visible_start)) state.visibleStart = s.fork_cal_visible_start;
    if (s.fork_cal_visible_end && HHMM.test(s.fork_cal_visible_end)) state.visibleEnd = s.fork_cal_visible_end;
    const ws = Number(s.fork_cal_week_start);
    if (ws === 0 || ws === 1 || ws === 6) state.weekStart = ws;
    state.showWeekends = on(s.fork_cal_show_weekends, true);
    state.showDeclined = on(s.fork_cal_show_declined, true);
    state.dimPast = on(s.fork_cal_dim_past, true);
    state.weekNumbers = on(s.fork_cal_week_numbers, false);
    state.railOpen = on(s.fork_cal_rail, true);
    state.addMeetDefault = on(s.fork_cal_add_meet, false);
    if (VIEWS.includes(s.fork_cal_default_view as CalView)) state.defaultView = s.fork_cal_default_view as CalView;
    try {
      const hc = JSON.parse(s.fork_cal_hidden_cals ?? "[]");
      if (Array.isArray(hc)) state.hiddenCals = hc.filter((x) => Number.isInteger(x));
    } catch {
      state.hiddenCals = [];
    }
    const cd = Number(s.fork_cal_custom_days);
    if (cd >= 2 && cd <= 7) state.customDays = cd;
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
  get visibleStart() { return state.visibleStart; },
  get visibleEnd() { return state.visibleEnd; },
  setVisibleHours(start: string, end: string) {
    if (!HHMM.test(start) || !HHMM.test(end) || end <= start) return;
    state.visibleStart = start; state.visibleEnd = end;
    persist("fork_cal_visible_start", start); persist("fork_cal_visible_end", end);
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
  get weekStart() { return state.weekStart; },
  setWeekStart(v: 0 | 1 | 6) { state.weekStart = v; persist("fork_cal_week_start", String(v)); },
  get showWeekends() { return state.showWeekends; },
  setShowWeekends(v: boolean) { state.showWeekends = v; persist("fork_cal_show_weekends", v ? "1" : "0"); },
  get showDeclined() { return state.showDeclined; },
  setShowDeclined(v: boolean) { state.showDeclined = v; persist("fork_cal_show_declined", v ? "1" : "0"); },
  get dimPast() { return state.dimPast; },
  setDimPast(v: boolean) { state.dimPast = v; persist("fork_cal_dim_past", v ? "1" : "0"); },
  get weekNumbers() { return state.weekNumbers; },
  setWeekNumbers(v: boolean) { state.weekNumbers = v; persist("fork_cal_week_numbers", v ? "1" : "0"); },
  get defaultView() { return state.defaultView; },
  setDefaultView(v: CalView) { if (!VIEWS.includes(v)) return; state.defaultView = v; persist("fork_cal_default_view", v); },
  get customDays() { return state.customDays; },
  setCustomDays(n: number) { if (n < 2 || n > 7) return; state.customDays = n; persist("fork_cal_custom_days", String(n)); },
  get railOpen() { return state.railOpen; },
  setRailOpen(v: boolean) { state.railOpen = v; persist("fork_cal_rail", v ? "1" : "0"); },
  get addMeetDefault() { return state.addMeetDefault; },
  setAddMeetDefault(v: boolean) { state.addMeetDefault = v; persist("fork_cal_add_meet", v ? "1" : "0"); },
  get hiddenCals() { return state.hiddenCals; },
  setHiddenCals(ids: number[]) { state.hiddenCals = [...new Set(ids)]; persist("fork_cal_hidden_cals", JSON.stringify(state.hiddenCals)); },
};
