// Fork (4.3): IPC wrapper for the search list command
// (`src-tauri/src/fork/search_query.rs::fork_search_threads`).
import { invoke } from "@tauri-apps/api/core";
import type { ThreadRow } from "../../lib/types";

/** Virtual folder id the list shows search results under. Mirrors
 *  `fork::search_query::SEARCH_FOLDER_ID`. */
export const SEARCH_FOLDER_ID = -900;

export const searchApi = {
  /** Search results grouped by thread, newest matching message first.
   *  `accountId` narrows to one mailbox; `null` searches every account. */
  searchThreads: (query: string, offset: number, limit: number, accountId: string | null) =>
    invoke<ThreadRow[]>("fork_search_threads", { query, offset, limit, accountId }),
};
