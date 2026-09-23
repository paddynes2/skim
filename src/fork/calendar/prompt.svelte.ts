// The "Send invites/updates to guests?" question (PLAN.md 7.4: `sendUpdates`
// is never implicit). Any create / patch / delete / RSVP that touches an
// event with guests other than Patrick awaits `guestsPrompt.ask(...)`, which
// resolves "all", "none", or null when he cancels. GuestsPrompt.svelte renders
// `pending` and calls `answer`.
import type { GuestsPromptKind, SendUpdates } from "./types";

interface Pending {
  kind: GuestsPromptKind;
  /** How many other guests the answer reaches. */
  n: number;
  resolve: (v: SendUpdates | null) => void;
}

let pending = $state<Pending | null>(null);

export const guestsPrompt = {
  get pending() {
    return pending;
  },
  /** Ask once; a second ask while one is up cancels the first. */
  ask(kind: GuestsPromptKind, n: number): Promise<SendUpdates | null> {
    if (pending) {
      const prev = pending;
      pending = null;
      prev.resolve(null);
    }
    return new Promise((resolve) => {
      pending = { kind, n, resolve };
    });
  },
  answer(v: SendUpdates | null) {
    const p = pending;
    if (!p) return;
    pending = null;
    p.resolve(v);
  },
};
