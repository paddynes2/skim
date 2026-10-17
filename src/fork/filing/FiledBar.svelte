<script lang="ts">
  // v1.1.24 (D72): the reading pane says when Jev filed the open
  // conversation, with the one action that matters: put it back. Putting it
  // back also stops Jev filing that sender into that folder again.
  import { t } from "../../lib/i18n/index.svelte";
  import { errorMessage } from "../../lib/api";
  import { filingApi, type FiledNote } from "./api";

  let { threadId }: { threadId: number } = $props();
  let note = $state<FiledNote | null>(null);
  let busy = $state(false);
  let done = $state(false);
  let error = $state("");

  $effect(() => {
    const id = threadId;
    let stale = false;
    void filingApi
      .forThread(id)
      .then((n) => {
        if (!stale) note = n;
      })
      .catch(() => {});
    return () => {
      stale = true;
    };
  });

  async function undo() {
    busy = true;
    error = "";
    try {
      const moved = await filingApi.undo(threadId);
      if (moved > 0) done = true;
      else error = t("fork.filing.undo_nothing");
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
</script>

{#if note || done}
  <div class="filed-bar" role="note">
    {#if done}
      <span>{t("fork.filing.undone")}</span>
    {:else if note}
      <span>{t("fork.filing.bar", { folder: note.folder })}</span>
      <button class="linkish" disabled={busy} onclick={() => void undo()}>{t("fork.filing.undo")}</button>
    {/if}
    {#if error}<span class="error" role="alert">{error}</span>{/if}
  </div>
{/if}

<style>
  .filed-bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 12px;
    margin: 0 0 12px;
    padding: 8px 12px;
    border: 1px solid var(--hairline);
    border-radius: var(--radius-s);
    font-size: 12.5px;
    color: var(--text-dim);
  }
  .linkish {
    color: var(--primary);
    font-size: 12.5px;
  }
  .linkish:disabled {
    opacity: 0.6;
  }
  .error {
    color: var(--danger);
  }
  button:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
  }
</style>
