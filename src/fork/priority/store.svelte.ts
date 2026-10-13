// The conversations the Inbox list shows under its Priority header. The list
// query already puts them first; this set only tells the layout where the
// section ends. Read after every list load so the two never drift for long.
import { priorityApi, type PriorityMember } from "./api";

const state = $state({ ids: new Set<number>(), reasons: new Map<number, PriorityMember["reason"]>(), held: null as number | null });
let inflight: Promise<void> | null = null;
let again = false;

export const priority = {
  get ids(): ReadonlySet<number> {
    return state.ids;
  },
  /** Why a conversation is in Priority (v1.1.12 row tags). */
  reason(id: number): PriorityMember["reason"] | undefined {
    return state.reasons.get(id);
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
      .members()
      .then((members) => {
        const next = new Set(members.map((m) => m.threadId));
        const reasons = new Map(members.map((m) => [m.threadId, m.reason] as const));
        const same = next.size === state.ids.size && members.every((m) => state.reasons.get(m.threadId) === m.reason);
        if (!same) {
          state.ids = next;
          state.reasons = reasons;
        }
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
