// The conversations the Inbox list shows under its Priority header. The list
// query already puts them first; this set only tells the layout where the
// section ends. Read after every list load so the two never drift for long.
import { priorityApi } from "./api";

const state = $state({ ids: new Set<number>(), held: null as number | null });
let inflight: Promise<void> | null = null;
let again = false;

export const priority = {
  get ids(): ReadonlySet<number> {
    return state.ids;
  },
  /** The open conversation kept in the Priority section after it was read
   *  (`sticky.ts`); drawn under the Priority header until another opens. */
  get held(): number | null {
    return state.held;
  },
  set held(id: number | null) {
    state.held = id;
  },
  /** What the list layout treats as Priority. */
  get section(): ReadonlySet<number> {
    return state.held === null || state.ids.has(state.held) ? state.ids : new Set([...state.ids, state.held]);
  },
  /** One read at a time; a call during a read schedules one more after it. */
  refresh(): Promise<void> {
    if (inflight) {
      again = true;
      return inflight;
    }
    inflight = priorityApi
      .ids()
      .then((ids) => {
        const next = new Set(ids);
        if (next.size !== state.ids.size || [...next].some((id) => !state.ids.has(id))) state.ids = next;
      })
      .catch(() => {})
      .finally(() => {
        inflight = null;
        if (again) {
          again = false;
          void priority.refresh();
        }
      });
    return inflight;
  },
};
