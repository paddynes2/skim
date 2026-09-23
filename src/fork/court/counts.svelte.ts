// The two sidebar badges (PLAN.md Phase 10): "On me (N)" / "Waiting (N)".
// Read from `fork_court_counts`; refreshed by the mail store's `court:updated`
// listener and by `courtStore.start()`. Kept apart from the store so
// `stores/mail.svelte.ts` can import it without a cycle (the court store
// imports the mail store).
import { courtApi } from "./api";

const state = $state({ onMe: 0, waiting: 0, loaded: false });

let inflight: Promise<void> | null = null;

export const courtCounts = {
  get onMe() {
    return state.onMe;
  },
  get waiting() {
    return state.waiting;
  },
  get loaded() {
    return state.loaded;
  },
  /** Re-read both counts. Concurrent calls share one request; a failed read
   *  (command not registered yet, no DB) keeps the last numbers. */
  refresh(): Promise<void> {
    inflight ??= courtApi
      .counts()
      .then((c) => {
        if (!c) return;
        state.onMe = c.onMe ?? 0;
        state.waiting = c.waiting ?? 0;
        state.loaded = true;
      })
      .catch(() => {})
      .finally(() => {
        inflight = null;
      });
    return inflight;
  },
};
