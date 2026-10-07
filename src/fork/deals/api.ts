// Typed wrappers around the v1.1.3 Deals commands (src-tauri/src/fork/deals.rs).
import { invoke } from "@tauri-apps/api/core";
import type { ThreadRow } from "../../lib/types";

/** Virtual folder id of the Deals view, after court's -920 / -921 and the
 *  retired Snoozed / Follow-ups ids -922 / -923. */
export const VF_DEALS = -924;

/** The settings key holding the list, one deal per line. */
export const DEALS_SETTING = "fork_deals";

/** A Deals row: exactly a `ThreadRow` plus the deal it belongs to. */
export interface DealRow extends ThreadRow {
  deal: string;
}

/** How the list text was read (Settings shows it). */
export interface ParsedDeals {
  deals: { name: string; domains: string[]; addresses: string[] }[];
  ignored: { entry: string; why: "personal" | "own" | "too_broad" | "not_an_address" }[];
}

/** What "Add to Deals" offers for an open thread. */
export interface DealSuggestion {
  /** The deal the thread already belongs to, if any. */
  deal: string | null;
  name: string;
  entry: string;
}

export const dealsApi = {
  list: (offset: number, limit: number) => invoke<DealRow[]>("fork_deals_list", { offset, limit }),
  count: () => invoke<number>("fork_deals_count"),
  suggest: (threadId: number) => invoke<DealSuggestion | null>("fork_deals_suggest", { threadId }),
  preview: (text: string) => invoke<ParsedDeals>("fork_deals_preview", { text }),
  /** Adds `entry` under `name`; returns the new list text. */
  add: (name: string, entry: string) => invoke<string>("fork_deals_add", { name, entry }),
};
