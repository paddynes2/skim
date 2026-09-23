<script lang="ts">
  // Fork (6.4): "Sending… Undo". Listens for `fork:send-held` (a send left a
  // composer on a hold, in this window or a compose window) and shows the
  // toast for as long as the hold lasts. Undo cancels the op and puts the
  // draft back where it was: inline under the open thread, else in a window.
  // Mounted once in App.svelte next to <Toast />.
  import { listen } from "@tauri-apps/api/event";
  import { errorMessage } from "../../lib/api";
  import { t } from "../../lib/i18n/index.svelte";
  import { toast } from "../stores/toast.svelte";
  import { forkComposeApi, type SendHeld } from "./api";
  import { reopenDraft } from "./inline.svelte";

  /** A hold longer than this is a scheduled send, not an undo window: it gets
   *  a short confirmation instead of a countdown toast. */
  const UNDO_MAX_SECS = 60;

  async function undo(held: SendHeld) {
    try {
      const draftId = await forkComposeApi.cancelScheduled(held.opId);
      await reopenDraft(draftId);
    } catch (e) {
      toast.show({ text: errorMessage(e), ms: 4000 });
    }
  }

  function show(held: SendHeld) {
    const secs = held.notBefore - Math.floor(Date.now() / 1000);
    if (secs <= 0) return;
    if (secs > UNDO_MAX_SECS) {
      toast.show({
        text: t("fork.compose.scheduled_for", { when: held.label ?? "" }),
        ms: 5000,
        action: { label: t("fork.compose.undo"), run: () => void undo(held) },
      });
      return;
    }
    toast.show({
      text: t("fork.compose.sending"),
      // The toast leaves a second before the hold does: an undo it still
      // offers must still be inside the margin.
      ms: Math.max(1000, (secs - 1) * 1000),
      action: { label: t("fork.compose.undo"), run: () => void undo(held) },
    });
  }

  $effect(() => {
    let off: (() => void) | null = null;
    let gone = false;
    void listen<SendHeld>("fork:send-held", (e) => show(e.payload)).then((un) => {
      if (gone) un();
      else off = un;
    });
    return () => {
      gone = true;
      off?.();
    };
  });
</script>
