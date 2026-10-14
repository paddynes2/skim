// Which conversations are in To reply (D70): the sidebar count, the `D` key,
// the row menu and the view header read it. Apart from `toggle.ts` so the
// mail store can import it without a cycle (toggle imports the mail store).
import { repliesApi, type ScoringOff } from "./api";

const state = $state({ ids: [] as number[], off: null as ScoringOff | null });
let inflight: Promise<void> | null = null;
let again = false;

export const replies = {
  get count() {
    return state.ids.length;
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
