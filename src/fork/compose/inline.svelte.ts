// Fork (6.2): the inline reply. One composer at a time, mounted by ReadingPane
// under the focused message of the thread it belongs to. `R`/`A`/`F` (App.svelte
// via `replyInlineOrWindow`) and the pane's footer buttons both land here; the
// setting `fork_reply_inline` off, or no open thread, falls back to the window.
import { api } from "../../lib/api";
import { mail } from "../../lib/stores/mail.svelte";
import { prefs } from "../stores/prefs.svelte";

export type ReplyMode = "reply" | "reply_all" | "forward";

export interface ReplyTarget {
  messageId: number;
  threadId: number;
}

const state = $state({
  draftId: null as number | null,
  threadId: null as number | null,
  mode: "reply" as ReplyMode,
  /** The last inline draft that was sent, so an undo can put it back inline. */
  lastSent: null as { draftId: number; threadId: number } | null,
});

// ReadingPane registers where a reply would go (its reply target); read at
// the moment a key is pressed, never stored.
let target: (() => ReplyTarget | null) | null = null;

export const inlineReply = {
  get draftId() {
    return state.draftId;
  },
  get threadId() {
    return state.threadId;
  },
  get mode() {
    return state.mode;
  },
  get lastSent() {
    return state.lastSent;
  },
  setTarget(fn: (() => ReplyTarget | null) | null) {
    target = fn;
  },
  /** Create the reply draft for `messageId` and show it inline in `threadId`. */
  async open(mode: ReplyMode, messageId: number, threadId: number) {
    const draft = await api.getReplyTemplate(messageId, mode);
    state.mode = mode;
    state.threadId = threadId;
    state.draftId = draft.id;
  },
  /** Show an existing draft inline again (undo send). */
  reopen(draftId: number, threadId: number) {
    state.threadId = threadId;
    state.draftId = draftId;
  },
  /** The inline draft was sent: remember it for an undo, then unmount. */
  sent() {
    if (state.draftId !== null && state.threadId !== null) {
      state.lastSent = { draftId: state.draftId, threadId: state.threadId };
    }
    inlineReply.close();
  },
  close() {
    state.draftId = null;
    state.threadId = null;
  },
};

/** Where an undone send should reopen: inline when its thread is still the
 *  open one, else in a window. */
export async function reopenDraft(draftId: number): Promise<void> {
  const last = state.lastSent;
  if (last && last.draftId === draftId && mail.selectedThreadId === last.threadId) {
    inlineReply.reopen(draftId, last.threadId);
    return;
  }
  await api.openComposeWindow(draftId);
}

/** The keyboard's R / A / F: inline under the open message when the setting is
 *  on and a thread is open, else the window (upstream's `replyToSelected`). */
export async function replyInlineOrWindow(mode: ReplyMode): Promise<void> {
  if (prefs.replyInline && target) {
    const t = target();
    if (t) {
      await inlineReply.open(mode, t.messageId, t.threadId);
      return;
    }
  }
  const thread = mail.selectedThread;
  if (!thread) return;
  const detail = await api.getThread(thread.id);
  const latest = detail.messages[detail.messages.length - 1];
  if (!latest) return;
  const draft = await api.getReplyTemplate(latest.id, mode);
  await api.openComposeWindow(draft.id);
}
