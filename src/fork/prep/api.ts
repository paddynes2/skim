// Typed wrappers around the Phase 11 commands (src-tauri/src/fork/prep.rs).
// Argument names are what Tauri expects on the wire (camelCase of the Rust
// parameters). The brief streams over a Channel exactly like `aiStream` in
// src/lib/api.ts; the handler shape is the same so the panel renders it the
// way AiAsk renders an answer.
import { Channel, invoke } from "@tauri-apps/api/core";
import type { AiEvent } from "../../lib/api";
import type { PrepPayload, UpcomingEvent } from "./types";

export interface BriefHandlers {
  delta: (text: string) => void;
  /** The model is reasoning: alive, not still loading. */
  reasoning: () => void;
  done: () => void;
  error: (code: string, message: string) => void;
}

export const prepApi = {
  /** Guests, their last threads and CRM cards for one calendar event. */
  prep: (eventId: number) => invoke<PrepPayload>("fork_prep", { eventId }),
  /** Events starting within ten minutes that have external guests. */
  upcoming: () => invoke<UpcomingEvent[]>("fork_prep_upcoming"),
  /** Stream a short brief for the event. Returns a cancel function. */
  brief(eventId: number, on: BriefHandlers): () => void {
    const requestId = crypto.randomUUID();
    let cancelled = false;
    const channel = new Channel<AiEvent>();
    channel.onmessage = (event) => {
      if (cancelled) return;
      switch (event.type) {
        case "delta":
          on.delta(event.text);
          break;
        case "reasoning":
          on.reasoning();
          break;
        case "done":
          on.done();
          break;
        case "error":
          on.error(event.code, event.message);
          break;
        default:
          // progress / tool events never come from a one-shot brief.
          break;
      }
    };
    invoke("fork_prep_brief", { requestId, eventId, channel }).catch((e: unknown) => {
      if (cancelled) return;
      const err = e as { code?: string; message?: string } | string;
      on.error(
        typeof err === "object" && err?.code ? err.code : "ai",
        typeof err === "string" ? err : (err?.message ?? String(e)),
      );
    });
    return () => {
      cancelled = true;
      void invoke("ai_cancel", { requestId }).catch(() => {});
    };
  },
};
