// Total count of starred mail for the sidebar (PLAN.md 1.4): a total, not an
// unread count, refreshed whenever the folder list does.
import { listen } from "@tauri-apps/api/event";
import { forkApi } from "../api";
import type { Folder } from "../../lib/types";

let total = $state(0);
let key = "";
let listening = false;

async function refresh(accountId: string | null) {
  try {
    total = await forkApi.roleTotal("starred", accountId);
  } catch {
    total = 0;
  }
}

export const starredCount = {
  /** Read the total for the Starred folder shown; a first call per scope
   *  (or a `mail:updated`) refreshes it in the background. */
  for(folder: Folder, accountId: string | null): number {
    const next = `${folder.id}:${accountId ?? "*"}`;
    if (next !== key) {
      key = next;
      void refresh(accountId);
      if (!listening) {
        listening = true;
        void listen("mail:updated", () => void refresh(accountId));
        void listen("folders:updated", () => void refresh(accountId));
      }
    }
    return total;
  },
};
