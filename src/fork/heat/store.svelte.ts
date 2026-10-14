// Heat (v1.1.15, D67): the shown rows' heat, read from `fork_heat` and kept
// per conversation. Re-read when new scores land (`heat:updated`).
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export const HEAT_UPDATED = "heat:updated";

const state = $state({ heat: new Map<number, number>() });
let asked = "";

export const heat = {
  of(threadId: number): number | undefined {
    return state.heat.get(threadId);
  },
  /** Read the heat of these conversations; a repeat of the last ask is skipped
   *  unless `force`. */
  async refresh(threadIds: number[], force = false): Promise<void> {
    const key = threadIds.join(",");
    if (!threadIds.length || (!force && key === asked)) return;
    asked = key;
    try {
      const rows = await invoke<{ threadId: number; heat: number }[]>("fork_heat", { threadIds });
      const next = new Map(state.heat);
      for (const id of threadIds) next.delete(id);
      for (const r of rows) next.set(r.threadId, r.heat);
      state.heat = next;
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
