// Filing (v1.1.24, D72): Jev moves automated mail out of the Inbox into named
// folders. Rust side: `fork::filing`.
import { invoke } from "@tauri-apps/api/core";

export type FilingMode = "off" | "preview" | "on";

export type FilingStatus = {
  mode: FilingMode;
  rules: string;
  never: string;
  threshold: number;
  scoredToday: number;
  cap: number;
  filedToday: number;
  /** Rule lines the parser skipped. */
  ignored: string[];
  /** Why Filing went back to preview on its own. */
  paused: string | null;
};

export type FilingPreview = {
  threadId: number;
  fromName: string;
  fromAddr: string;
  subject: string;
  date: number;
  folder: string;
  score: number;
};

export type FiledNote = { folder: string; score: number };

export const filingApi = {
  status: () => invoke<FilingStatus>("fork_filing_status"),
  preview: () => invoke<FilingPreview[]>("fork_filing_preview"),
  forThread: (threadId: number) => invoke<FiledNote | null>("fork_filing_for_thread", { threadId }),
  undo: (threadId: number) => invoke<number>("fork_filing_undo", { threadId }),
};
