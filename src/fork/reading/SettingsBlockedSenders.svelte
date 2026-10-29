<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { errorMessage } from "../../lib/api";
  import { t } from "../../lib/i18n/index.svelte";
  type Row = { accountId: string; accountEmail: string; sender: string };
  let rows = $state<Row[]>([]);
  let loaded = $state(false), busy = $state(false), error = $state("");
  async function load() {
    error = "";
    try { rows = await invoke<Row[]>("fork_blocked_senders"); loaded = true; }
    catch (e) { error = errorMessage(e); }
  }
  onMount(() => { void load(); });
  async function unblock(row: Row) {
    busy = true; error = "";
    try {
      await invoke("fork_unblock_sender", { accountId: row.accountId, sender: row.sender });
      await load();
    } catch (e) { error = errorMessage(e); }
    finally { busy = false; }
  }
</script>
<section>
  <div class="microlabel">{t("fork.unsubscribe.settings")}</div>
  <p>{t("fork.unsubscribe.settings_note")}</p>
  {#if error}<p class="error" role="alert">{error}</p><button onclick={load}>{t("reading.retry")}</button>{/if}
  {#if loaded && !rows.length}<p>{t("fork.unsubscribe.empty")}</p>{/if}
  {#each rows as row (`${row.accountId}:${row.sender}`)}
    <div class="row"><div>{row.sender}<small>{row.accountEmail}</small></div><button disabled={busy} onclick={() => unblock(row)}>{t("fork.unsubscribe.unblock")}</button></div>
  {/each}
</section>
<style>
  section { border-top: 1px solid var(--hairline-strong); padding-top: 16px; margin-top: 16px; }
  .microlabel { font-size: 11px; text-transform: uppercase; letter-spacing: .08em; color: var(--text-dim); }
  p, small { color: var(--text-dim); font-size: 12px; }
  .row { display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 8px 0; overflow-wrap: anywhere; }
  small { display: block; }
  button { color: var(--text); background: transparent; border: 1px solid var(--hairline-strong); border-radius: 6px; padding: 4px 10px; cursor: pointer; }
  .error { color: var(--danger); }
</style>
