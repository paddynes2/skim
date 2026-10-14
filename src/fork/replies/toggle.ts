// The `D` key and the row menu items (v1.1.19, D70). A mark lasts until the
// next message in the conversation.
import { t } from "../../lib/i18n/index.svelte";
import { mail } from "../../lib/stores/mail.svelte";
import { ui } from "../../lib/stores/ui.svelte";
import type { ThreadRow } from "../../lib/types";
import { nextAfterRemoval } from "../actions";
import { navHooks } from "../nav";
import { toast } from "../stores/toast.svelte";
import { repliesApi, type ReplyMark } from "./api";
import { replies } from "./store.svelte";

async function setMark(row: ThreadRow, mark: ReplyMark, text: string): Promise<void> {
  // In the view a done conversation leaves at once and the next one opens,
  // like archive; the server's `replies:updated` reloads the list after.
  if (mark === "done" && mail.courtView === "to_reply") {
    const wasOpen = mail.selectedThreadId === row.id;
    const next = wasOpen ? nextAfterRemoval(mail.threads, row.id) : null;
    mail.removeThreadFromList(row.id);
    if (wasOpen) {
      mail.selectedThreadId = next?.id ?? null;
      mail.selectedMessageId = next?.messageId ?? null;
    }
  }
  try {
    const prev = await repliesApi.mark(row.id, mark);
    void replies.refresh();
    toast.show({
      text,
      action: {
        label: t("fork.replies.undo"),
        run: () => void repliesApi.mark(row.id, prev).then(() => replies.refresh()),
      },
      ms: 6000,
    });
  } catch (e) {
    void replies.refresh();
    toast.show({ text: String((e as { message?: string })?.message ?? e), ms: 6000 });
  }
}

/** No reply needed: out of To reply until the next message. */
export function markDone(row: ThreadRow): Promise<void> {
  return setMark(row, "done", t("fork.replies.marked_done"));
}

/** A reply is owed: in To reply until the next message. */
export function markNeeds(row: ThreadRow): Promise<void> {
  return setMark(row, "needs", t("fork.replies.marked_needs"));
}

/** `D`. In To reply it marks the conversation done. Elsewhere it adds one
 *  that is not listed; on one that is, it says so and offers Done, so the key
 *  never takes a conversation out of a list the user is not looking at. */
export function replyKey(row: ThreadRow): Promise<void> {
  if (mail.courtView === "to_reply") return markDone(row);
  if (!replies.has(row.id)) return markNeeds(row);
  toast.show({
    text: t("fork.replies.already"),
    action: { label: t("fork.replies.menu_done"), run: () => void markDone(row) },
    ms: 6000,
  });
  return Promise.resolve();
}

export function openToReply(): void {
  ui.showMail();
  void mail.selectCourt("to_reply");
}

/** `g r`, the palette row and the first count. Returns the stop function. */
export function startReplies(): () => void {
  navHooks.toReply = openToReply;
  void replies.refresh();
  return () => {
    delete navHooks.toReply;
  };
}
