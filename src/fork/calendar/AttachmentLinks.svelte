<script lang="ts">
  import { untrack } from "svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { t } from "../../lib/i18n/index.svelte";
  import type { EventAttachment } from "./types";
  let { value = $bindable(), editable = false }: { value?: EventAttachment[]; editable?: boolean } = $props();
  const initiallyMissing = untrack(() => value === undefined);
  const files = $derived(value ?? []);
  let additions = $state<string[]>([]);
  let url = $state(""); let name = $state(""); let error = $state("");
  function add() {
    try {
      const link = new URL(url.trim());
      if (link.protocol !== "https:" || link.username || link.password) throw new Error();
      if (files.length >= 25) { error = t("fork.cal.files_limit"); return; }
      if (!files.some((f) => f.fileUrl === link.href)) { value = [...files, { fileUrl: link.href, title: name.trim() || link.hostname }]; additions = [...additions, link.href]; }
      url = ""; name = ""; error = "";
    } catch { error = t("fork.cal.files_invalid"); }
  }
  function safe(url: string) { try { const u = new URL(url); return u.protocol === "https:" && !u.username && !u.password; } catch { return false; } }
</script>
<div class="event-files">
  <h3>{t("fork.cal.files_notes")}</h3>
  {#each files as file}<div class="file-row"><button class="file" disabled={!safe(file.fileUrl)} title={file.fileUrl} onclick={() => openUrl(file.fileUrl)}><span>{file.title || file.fileUrl}</span><small>{t("fork.cal.open_file")}</small></button>{#if editable && additions.includes(file.fileUrl)}<button class="remove-link" aria-label={t("fork.cal.remove_new_link")} onclick={() => { const remaining = files.filter((f) => f !== file); value = remaining.length || !initiallyMissing ? remaining : undefined; additions = additions.filter((url) => url !== file.fileUrl); }}>&times;</button>{/if}</div>{/each}
  {#if editable}<div class="add-file"><input aria-label={t("fork.cal.file_name")} placeholder={t("fork.cal.file_name")} bind:value={name} maxlength="512"/><input type="url" aria-label={t("fork.cal.file_url")} placeholder="https://docs.google.com/..." bind:value={url}/><button disabled={!url.trim()} onclick={add}>{t("fork.cal.link_file")}</button></div><p>{t("fork.cal.files_help")}</p>{/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
</div>
<style>
  .file-row { display:flex;align-items:center;gap:8px; }.remove-link { width:24px;height:24px;font-size:18px;color:var(--text-dim); }
  .event-files { border-top:1px solid var(--hairline); padding-top:18px; margin-top:8px; }h3 { margin:0 0 10px;font-size:13px;font-weight:600; }.file { display:flex;justify-content:space-between;gap:12px;padding:9px 0;border-bottom:1px solid var(--hairline);width:100%;text-align:left;font-size:12px; }.file span { overflow-wrap:anywhere; }.file small { flex-shrink:0;color:var(--text-dim); }.add-file { display:grid;gap:7px;margin-top:12px; }.add-file input { padding:7px 9px;border:1px solid var(--hairline-strong);border-radius:var(--radius-s);font-size:12px;background:var(--surface-raised); }.add-file button { justify-self:start;padding:6px 10px;border:1px solid var(--hairline-strong);border-radius:var(--radius-s);font-size:12px; }.add-file button:disabled { opacity:.4; }p { font-size:11px;color:var(--text-dim);line-height:1.5;margin-top:8px; }.error { color:var(--danger); }
</style>
