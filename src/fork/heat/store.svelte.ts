// Heat (v1.1.15, D67): the shown rows' heat, read from `fork_heat` and kept
// per conversation. Re-read when new scores land (`heat:updated`).
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export const HEAT_UPDATED = "heat:updated";

const state = $state({ heat: new Map<number, number>(), person: new Set<number>() });
let asked = "";

export const heat = {
  of(threadId: number): number | undefined {
    return state.heat.get(threadId);
  },
  /** Pastel (D92): a person, not a machine, wrote in this conversation. */
  person(threadId: number): boolean {
    return state.person.has(threadId);
  },
  /** Read the heat of these conversations; a repeat of the last ask is skipped
   *  unless `force`. */
  async refresh(threadIds: number[], force = false): Promise<void> {
    const key = threadIds.join(",");
    if (!threadIds.length || (!force && key === asked)) return;
    asked = key;
    try {
      const rows = await invoke<{ threadId: number; heat: number | null; person: boolean }[]>("fork_heat", { threadIds });
      const next = new Map(state.heat);
      const people = new Set(state.person);
      for (const id of threadIds) {
        next.delete(id);
        people.delete(id);
      }
      for (const r of rows) {
        if (r.heat !== null) next.set(r.threadId, r.heat);
        if (r.person) people.add(r.threadId);
      }
      state.heat = next;
      state.person = people;
    } catch {
      // Fail-open: rows stay as they are.
    }
  },
};

let lastIds: number[] = [];
/** Remember what is shown, so a scoring event re-reads exactly that. */
export function heatShown(threadIds: number[]) {
  lastIds = threadIds;
  void heat.refresh(threadIds);
}

void listen(HEAT_UPDATED, () => void heat.refresh(lastIds, true)).catch(() => {});
