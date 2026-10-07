// Sidebar badge for Deals: unread deal conversations in the Inbox. Apart from
// the store so the mail store can import it without a cycle (the deals store
// imports mail).
import { dealsApi } from "./api";

const state = $state({ unread: 0, total: 0 });
let inflight: Promise<void> | null = null;
let again = false;

export const dealsCount = {
  get unread() {
    return state.unread;
  },
  /** Every deal conversation, for the list header (the list pages). */
  get total() {
    return state.total;
  },
  /** One read at a time; a call during a read schedules one more after it,
   *  so a list edit made mid-read is never answered with the older count. */
  refresh(): Promise<void> {
    if (inflight) {
      again = true;
      return inflight;
    }
    inflight = dealsApi
      .count()
      .then((c) => {
        state.unread = c?.unread ?? 0;
        state.total = c?.total ?? 0;
      })
      .catch(() => {})
      .finally(() => {
        inflight = null;
        if (again) {
          again = false;
          void dealsCount.refresh();
        }
      });
    return inflight;
  },
};
