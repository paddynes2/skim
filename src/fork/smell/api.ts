// Fork (6.5.4): IPC wrapper for `fork_smell_rewrite`
// (src-tauri/src/fork/smell.rs) and the three smell settings.
import { Channel, invoke } from "@tauri-apps/api/core";
import { api, type AiEvent } from "../../lib/api";
import { SMELL_KEYS, parseSmellSettings, type SmellSettings } from "./settings";
export { SMELL_KEYS, parseSmellSettings, type SmellSettings };

/** Mirrors `fork::smell::RewriteResult`. */
export interface RewriteResult {
  /** The options that kept every fact of the sentence, at most 3. */
  options: string[];
  /** How many the fact guard threw away. */
  rejected: number;
}

export interface RewriteRequest {
  /** The flagged sentence, exactly as it stands in the draft. */
  sentence: string;
  /** One sentence of context either side ("" at the edges). */
  before: string;
  after: string;
  /** The rule's reason (its suggestion text). */
  reason: string;
}

export const smellApi = {
  /** Three rewrites of one sentence from the configured model, fact-guarded in
   *  Rust. `onEvent` gets the stream's liveness (`reasoning`, `delta`) so the
   *  popover can show the model is working; the text that matters is the
   *  return value. */
  rewrite: (req: RewriteRequest, requestId: string, onEvent?: (e: AiEvent) => void) => {
    const channel = new Channel<AiEvent>();
    channel.onmessage = (e) => onEvent?.(e);
    return invoke<RewriteResult>("fork_smell_rewrite", { ...req, requestId, channel });
  },
  cancel: (requestId: string) => invoke<void>("ai_cancel", { requestId }),
};

export async function loadSmellSettings(): Promise<SmellSettings> {
  return parseSmellSettings(await api.getSettings());
}

export async function ignoreRule(settings: SmellSettings, rule: string): Promise<void> {
  settings.ignored.add(rule);
  await api.setSetting(SMELL_KEYS.ignored, JSON.stringify([...settings.ignored].sort()));
}

export async function unignoreRule(settings: SmellSettings, rule: string): Promise<void> {
  settings.ignored.delete(rule);
  await api.setSetting(SMELL_KEYS.ignored, JSON.stringify([...settings.ignored].sort()));
}
