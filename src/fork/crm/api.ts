// Typed wrappers around the Phase 9 commands (src-tauri/src/fork/crm.rs).
// Argument names are what Tauri expects on the wire (camelCase of the Rust
// parameters). Every call is read-only against the CRM.
import { invoke } from "@tauri-apps/api/core";
import type { CrmLookup, CrmStatus } from "./types";

export const crmApi = {
  /** Config + connection state; lists workspaces when none is chosen yet. */
  status: () => invoke<CrmStatus>("fork_crm_status"),
  /** Supabase password sign-in; keeps only the refresh token (Credential Manager). */
  connect: (email: string, password: string) =>
    invoke<CrmStatus>("fork_crm_connect", { email, password }),
  pickWorkspace: (id: string) => invoke<CrmStatus>("fork_crm_pick_workspace", { id }),
  /** Forgets the token, the caches and the chosen workspace; keeps the URLs. */
  disconnect: () => invoke<CrmStatus>("fork_crm_disconnect"),
  /** `null` leaves a field as it is; `""` resets it to the build-time default. */
  setConfig: (baseUrl: string | null, supabaseUrl: string | null, anonKey: string | null) =>
    invoke<CrmStatus>("fork_crm_set_config", { baseUrl, supabaseUrl, anonKey }),
  /** Person / company / deals / activities for an address; 10-minute cache in Rust. */
  lookup: (email: string) => invoke<CrmLookup>("fork_crm_lookup", { email }),
};
