<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { t } from "../../lib/i18n/index.svelte";
  let { draftId, localStatus, revision, refreshKey, busy = false, onSave, onRetryLocal }:
    { draftId: number; localStatus: "saved"|"saving"|"dirty"|"error"; revision: number; refreshKey: number; busy?: boolean; onSave: () => Promise<void>; onRetryLocal: () => void } = $props();
  type Status = {state: "local"|"pending"|"failed"|"synced"; gmail: boolean};
  let status = $state<Status|null>(null), unavailable = $state(false), saving = $state(false);
  let request = 0, disposed = false;
  async function refresh() {
    const token = ++request, id = draftId;
    try {
      const result = await invoke<Status>("fork_draft_sync_status", {draftId:id});
      if (!disposed && token === request && id === draftId) { status = result; unavailable = false; }
    } catch { if (!disposed && token === request) { status = null; unavailable = true; } }
  }
  $effect(() => {
    void draftId; void revision; void refreshKey;
    status = null;
    if (localStatus === "saved") void refresh();
    else { request++; unavailable = false; }
  });
  onMount(() => {
    const stops: (()=>void)[] = [];
    for (const name of ["draft:sync", "drafts:updated", "ops:failed", "mail:updated"]) {
      void listen<{draftId?:number}>(name, event => {
        if (localStatus === "saved" && (!event.payload?.draftId || event.payload.draftId === draftId)) void refresh();
      }).then(stop => { if(disposed)stop();else stops.push(stop); }).catch(() => {});
    }
    return () => { disposed = true; request++; stops.forEach(stop => stop()); };
  });
  async function save() { saving = true; try { await onSave(); await refresh(); } finally { saving = false; } }
  const server = $derived(status?.gmail ? "Gmail" : t("fork.compose.mail_server"));
</script>
<span class="draft-sync" aria-live="polite">
  <span>{busy ? t("fork.compose.adding_files") : localStatus === "saving" ? t("fork.compose.saving_local") : localStatus === "dirty" ? t("fork.compose.unsaved") : localStatus === "error" ? t("fork.compose.save_failed") : unavailable ? t("fork.compose.status_unavailable") : status?.state === "synced" ? t("fork.compose.saved_server", {server}) : status?.state === "pending" ? t("fork.compose.waiting_server", {server}) : status?.state === "failed" ? t("fork.compose.failed_server", {server}) : t("fork.compose.saved_local")}</span>
  {#if localStatus === "error"}<button onclick={onRetryLocal}>{t("fork.compose.retry_save")}</button>
  {:else if unavailable}<button onclick={refresh}>{t("fork.compose.check_status")}</button>
  {:else if status && status.state !== "synced" && status.state !== "pending" && localStatus === "saved"}
    <button disabled={busy || saving} onclick={save}>{t(status.state === "failed" ? "fork.compose.retry_server" : "fork.compose.save_server", {server})}</button>
  {/if}
</span>
<style>
  .draft-sync{display:inline-flex;gap:10px;align-items:center;flex-wrap:wrap}button{color:var(--primary);font:inherit}button:disabled{opacity:.5}button:focus-visible{outline:2px solid var(--focus);outline-offset:3px}
</style>
