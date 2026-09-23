// Shapes of the Phase 11 prep IPC (src-tauri/src/fork/prep.rs). The Rust
// structs serialise camelCase, like the upstream ThreadRow they embed.
import type { ThreadRow } from "../../lib/types";
import type { CrmLookup } from "../crm/types";

/** A guest on the event who is not Patrick; `email` is lowercased. */
export interface Guest {
  email: string;
  name: string | null;
}

/** One of the guest's threads: a list row plus where its newest hit lives,
 *  so `mail.openLocation(folderId, id, hitMessageId)` can open it. */
export interface PrepThread extends ThreadRow {
  folderId: number;
  hitMessageId: number;
}

export interface PrepGuest {
  email: string;
  name: string | null;
  /** Newest first, at most 5, last 90 days. */
  threads: PrepThread[];
  /** The Rebound card (Phase 9 lookup response) or null when the guest is
   *  not in the CRM, the CRM is not connected, or Rust has no lookup seam yet
   *  (then the panel asks `fork_crm_lookup` itself). */
  crm: CrmLookup | null;
}

export interface PrepPayload {
  eventId: number;
  summary: string;
  startTs: number;
  endTs: number;
  guests: PrepGuest[];
}

/** An event starting within the reminder window that has external guests. */
export interface UpcomingEvent {
  id: number;
  summary: string;
  startTs: number;
  guests: Guest[];
}
