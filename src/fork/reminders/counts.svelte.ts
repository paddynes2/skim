// Sidebar badges for Snoozed / Follow-ups. Apart from the store so the mail
// store can import it without a cycle (the reminders store imports mail).
import { remindersApi } from "./api";

const state = $state({ snoozed: 0, followups: 0, followupsDue: 0 });
let inflight: Promise<void> | null = null;

export const reminderCounts = {
  get snoozed() {
    return state.snoozed;
  },
  get followups() {
    return state.followups;
  },
  get followupsDue() {
    return state.followupsDue;
  },
  refresh(): Promise<void> {
    inflight ??= remindersApi
      .counts()
      .then((c) => {
        if (!c) return;
        state.snoozed = c.snoozed ?? 0;
        state.followups = c.followups ?? 0;
        state.followupsDue = c.followupsDue ?? 0;
      })
      .catch(() => {})
      .finally(() => {
        inflight = null;
      });
    return inflight;
  },
};
