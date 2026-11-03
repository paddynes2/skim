// Which conversations are in To reply (D70): the sidebar count, the `D` key,
// the row menu and the view header read it. Apart from `toggle.ts` so the
// mail store can import it without a cycle (toggle imports the mail store).
import { repliesApi, type ScoringOff } from "./api";

const state = $state({ ids: [] as number[], older: [] as number[], off: null as ScoringOff | null, showOlder: false });
let inflight: Promise<void> | null = null;
let again = false;

export const replies = {
  /** D93: this week's rows; older ones are folded and not counted. */
  get count() {
    return state.ids.length - state.older.length;
  },
  get olderCount() {
    return state.older.length;
  },
  get olderIds(): number[] {
    return state.older;
  },
  /** The view lists the older rows instead of this week's. */
  get showOlder() {
    return state.showOlder;
  },
  set showOlder(v: boolean) {
    state.showOlder = v;
  },
  /** Why new mail is not being checked, when it is not. */
  get off() {
    return state.off;
  },
  has(threadId: number): boolean {
    return state.ids.includes(threadId);
  },
  /** One read at a time; a call during a read schedules one more after it,
   *  so a mark made mid-read is never answered with the older set. A failed
   *  read keeps the last set. */
  refresh(): Promise<void> {
    if (inflight) {
      again = true;
      return inflight;
    }
    inflight = repliesApi
      .state()
      .then((s) => {
        if (!s || !Array.isArray(s.ids)) return;
        state.ids = s.ids;
        state.older = Array.isArray(s.older) ? s.older : [];
        if (!state.older.length) state.showOlder = false;
        state.off = s.off ?? null;
      })
      .catch(() => {})
      .finally(() => {
        inflight = null;
        if (again) {
          again = false;
          void replies.refresh();
        }
      });
    return inflight;
  },
};
