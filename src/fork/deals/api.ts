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
  dealDomain: string | null;
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
  personEntry: string;
  companyEntry: string | null;
  excluded: boolean;
}

export interface ScopeInput {threadId:number; name:string; scope:"conversation"|"person"|"company"; entry:string}
export const dealsApi = {
  catalog: () => invoke<ParsedDeals>("fork_deals_catalog"),
  previewScope: (input:ScopeInput) => invoke<{count:number;entry:string}>("fork_deals_preview_scope",{input}),
  applyScope: (input:ScopeInput) => invoke<string>("fork_deals_apply_scope",{input}),
  exclude: (threadId:number, excluded:boolean) => invoke<void>("fork_deals_exclude",{threadId,excluded}),
  saveContext: (company:string,notes:string,pinnedId:number|null) => invoke<void>("fork_deals_save_context",{company,notes,pinnedId}),
  list: (offset: number, limit: number, company: string | null = null) => invoke<DealRow[]>("fork_deals_list", { offset, limit, company }),
  context: (company: string) => invoke<DealContext>("fork_deals_context", { company }),
  count: () => invoke<{ unread: number; total: number }>("fork_deals_count"),
  suggest: (threadId: number) => invoke<DealSuggestion | null>("fork_deals_suggest", { threadId }),
  preview: (text: string) => invoke<ParsedDeals>("fork_deals_preview", { text }),
  /** Adds `entry` under `name`; returns the new list text. */
  add: (name: string, entry: string) => invoke<string>("fork_deals_add", { name, entry }),
};

export interface DealContext {
  conversationCount: number;
  conversations: DealRow[];
  people: { name: string | null; addr: string }[];
  attachments: DealAttachment[];
  notes: string;
  pinned: DealAttachment | null;
  meetings: { id: number; summary: string; startTs: number; allDay: boolean }[];
}

export interface DealAttachment {id:number;messageId:number;mimeType:string|null;size:number;isInline:boolean;threadId:number;filename:string;date:number}
