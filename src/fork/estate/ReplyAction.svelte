<script lang="ts">
  import { onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { api, errorMessage } from "../../lib/api";
  import { t } from "../../lib/i18n/index.svelte";

  let { messageId }: { messageId: number } = $props();
  type Receipt = { status: string; detail?: string; localMessageId?: number | null; sources?: string[]; gaps?: string[] };
  let receipt = $state<Receipt | null>(null);
  let direction = $state("");
  let busy = $state(false);
  let error = $state("");
  let stopped = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let polls = $state(0);
  const running = $derived(busy || ["checking", "drafting", "saving"].includes(receipt?.status ?? ""));
  const syncing = $derived(receipt?.status === "ready" && !receipt.localMessageId);

  async function request(action: "start" | "status") {
    if (busy || stopped) return;
    busy = true;
    error = "";
    clearTimeout(timer);
    try {
      const result = await invoke<Receipt>("fork_estate_reply", { messageId, action, instruction: direction });
      if (stopped) return;
      receipt = result;
      if ((["checking", "drafting", "saving"].includes(result.status) || (result.status === "ready" && !result.localMessageId)) && polls++ < 240) {
        timer = setTimeout(() => void request("status"), result.status === "ready" ? 10000 : 5000);
      }
    } catch (e) {
      if (!stopped) error = errorMessage(e);
    } finally {
      busy = false;
    }
  }

  async function openDraft() {
    if (!receipt?.localMessageId || busy) return;
    busy = true;
    try {
      const draft = await api.editDraft(receipt.localMessageId);
      await api.openComposeWindow(draft.id);
    } catch (e) { error = errorMessage(e); }
    finally { busy = false; }
  }

  onDestroy(() => { stopped = true; clearTimeout(timer); });
</script>

<!-- Hallmark component review: P5 H4 E4 S5 R5 V3. Existing Skim tokens and controls. -->
<div class="estate-reply">
  {#if receipt?.status === "ready" && receipt.localMessageId}
    <button class="ai-btn" disabled={busy} onclick={openDraft}>{t("estate.open")}</button>
  {:else}
    <button class="ai-btn" disabled={busy || (running && !error && polls < 240)} onclick={() => { polls = 0; void request(receipt && receipt.status !== "failed" ? "status" : "start"); }}>
      {running && !error ? t("estate.working") : receipt && receipt.status !== "failed" ? t("estate.check") : t("estate.draft")}
    </button>
  {/if}
  {#if !receipt || receipt.status === "failed"}
    <details>
      <summary>{t("estate.direction")}</summary>
      <input aria-label={t("estate.direction_label")} placeholder={t("estate.direction_hint")} maxlength="2000" bind:value={direction} disabled={busy} />
    </details>
  {/if}
  <span class="status" role="status" aria-live="polite">
    {#if error}{error}
    {:else if receipt?.detail}{receipt.detail}
    {:else if syncing}{t("estate.syncing")}
    {:else if receipt?.status === "checking"}{t("estate.checking")}
    {:else if receipt?.status === "drafting"}{t("estate.drafting")}
    {:else if receipt?.status === "saving"}{t("estate.saving")}{/if}
  </span>
  {#if receipt?.sources?.length || receipt?.gaps?.length}
    <details class="context">
      <summary>{t("estate.context")}</summary>
      {#if receipt.gaps?.length}<p>{receipt.gaps.join(" ")}</p>{/if}
      <ul>{#each receipt.sources ?? [] as source}<li>{source}</li>{/each}</ul>
    </details>
  {/if}
</div>

<style>
  .estate-reply { display: flex; align-items: center; flex-wrap: wrap; gap: 8px; min-width: 0; }
  .ai-btn { padding: 7px 16px; border-radius: var(--radius-m); border: 1px solid var(--accent-dim); color: var(--accent); font-size: 13px; font-weight: 600; white-space: nowrap; }
  .ai-btn:hover { background: var(--accent-soft); }
  details { font-size: 12px; min-width: 0; }
  summary { cursor: pointer; padding: 5px 0; }
  input { width: min(300px, 100%); margin-top: 4px; }
  .status { font-size: 12px; overflow-wrap: anywhere; }
  .context { flex-basis: 100%; overflow-wrap: anywhere; }
  ul { padding-left: 18px; }
</style>
