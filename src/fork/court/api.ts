// Typed wrappers around the Phase 10 commands (src-tauri/src/fork/court.rs).
// Argument names are what Tauri expects on the wire (camelCase of the Rust
// parameters).
import { invoke } from "@tauri-apps/api/core";
import type { CourtCounts, CourtRow, CourtState } from "./types";

/** Emitted by the Rust side after a pass moved at least one row (and after a
 *  forced recompute). Listeners refresh counts and the open view. */
export const COURT_UPDATED = "court:updated";

/** Virtual folder ids of the two views (plan: `-920`, `-921`). Mirror
 *  `fork::court::VF_ON_ME` / `VF_WAITING`. */
export const VF_ON_ME = -920;
export const VF_WAITING = -921;

export const courtApi = {
  /** Threads in a state, oldest `since` first. Row shape = `ThreadRow` + since/reason. */
  list: (state: CourtState, offset: number, limit: number) =>
    invoke<CourtRow[]>("fork_court_list", { courtState: state, offset, limit }),
  /** Sidebar badges. */
  counts: () => invoke<CourtCounts>("fork_court_counts"),
  /** Forced full pass: every thread re-classified, AI verdicts reset. */
  recompute: () => invoke<void>("fork_court_recompute"),
};
