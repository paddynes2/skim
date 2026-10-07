<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "../../lib/i18n/index.svelte";
  import { dealsApi, type ParsedDeals } from "./api";
  import { dealsStore } from "./store.svelte";
  import { dealRows, serializeDealRows, invalidDealName, type DealEditorRow } from "./rows";
  let rows = $state<DealEditorRow[]>([]);
  let ready = $state(false), busy = $state(false), saved = $state(false), error = $state("");
  let parsed = $state<ParsedDeals | null>(null);
  const text = $derived(serializeDealRows(rows));
  const dirty = $derived(ready && text !== dealsStore.text);
  const invalid = $derived(rows.some(r => r.edited && (invalidDealName(r.name) || /[\r\n]/.test(r.entries))));
  function reset() { rows = dealRows(dealsStore.text); error = ""; saved = false; }
  async function load() { await dealsStore.load(); ready = dealsStore.loaded; if (ready) reset(); }
  onMount(() => { void load(); });
  $effect(() => {
    const current = text;
    if (!ready) return;
    parsed = null;
    const timer = setTimeout(() => { void dealsApi.preview(current).then(p => { if (current === text) parsed = p; }).catch(e => { error = String(e); }); }, 150);
    return () => clearTimeout(timer);
  });
  function add() { rows = [...rows, {id: Math.max(-1, ...rows.map(r => r.id)) + 1, name: "", entries: "", original: "", edited: true, note: false}]; }
  async function save() {
    if (!ready || invalid || busy) return;
    busy = true; error = "";
    try { await dealsStore.save(text); rows = dealRows(dealsStore.text); saved = true; } catch(e) { error = String(e); }
    finally { busy = false; }
  }
</script>
<section class="deals">
  <div class="head"><span class="microlabel">{t("fork.deals.settings")}</span>{#if saved}<span role="status">{t("fork.deals.settings_saved")}</span>{/if}</div>
  <p>{t("fork.deals.rows_note")}</p>
  {#if dealsStore.failed}<p class="error" role="alert">{t("fork.deals.settings_not_loaded")} <button onclick={() => void load()}>{t("fork.deals.retry")}</button></p>{/if}
  {#each rows.filter(r => !r.note) as row (row.id)}
    <div class="company-row">
      <label><span>{t("fork.deals.company")}</span><input bind:value={row.name} oninput={() => {row.edited=true; saved=false;}} disabled={busy || !ready} placeholder={t("fork.deals.company")} /></label>
      <label><span>{t("fork.deals.domains")}</span><input bind:value={row.entries} oninput={() => {row.edited=true; saved=false;}} disabled={busy || !ready} spellcheck="false" placeholder="company.example, person@example.com" /></label>
      <button class="remove" onclick={() => {rows=rows.filter(r => r.id !== row.id); saved=false;}} disabled={busy} aria-label={t("fork.deals.remove_company", {name: row.name || row.entries})}>&times;</button>
    </div>
  {/each}
  {#if rows.some(r => r.note && r.original.trim())}<details><summary>{t("fork.deals.retained_notes")}</summary><pre>{rows.filter(r => r.note && r.original.trim()).map(r => r.original).join("\n")}</pre></details>{/if}
  {#if parsed}{#each parsed.ignored as item}<p class="error">{t(`fork.deals.ignored_${item.why}`, {entry: item.entry})}</p>{/each}{/if}
  {#if invalid}<p class="error">{t("fork.deals.invalid_name")}</p>{/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <div class="actions"><button onclick={add} disabled={!ready || busy}>{t("fork.deals.add_company")}</button><span></span><button onclick={reset} disabled={!dirty || busy}>{t("fork.deals.cancel")}</button><button class="save" onclick={() => void save()} disabled={!dirty || busy || invalid}>{t("fork.deals.save")}</button></div>
</section>
<style>
  .deals{display:flex;flex-direction:column;gap:12px}.head,.actions{display:flex;align-items:center;gap:10px}.head{justify-content:space-between}.head span:last-child{font-size:12px}p{margin:0;color:var(--text-dim);font-size:12px;line-height:1.5}.company-row{display:grid;grid-template-columns:minmax(100px,1fr) minmax(160px,2fr) 28px;gap:8px;align-items:end}label{display:flex;flex-direction:column;gap:4px;min-width:0}label span{font-size:11px;color:var(--text-dim)}input{width:100%;min-width:0;box-sizing:border-box;padding:8px;border:1px solid var(--hairline-strong);border-radius:var(--radius-s);background:var(--surface);color:var(--text);font-size:12px}button{padding:7px 10px;border:1px solid var(--hairline-strong);border-radius:var(--radius-s);font-size:12px}button:hover:not(:disabled){background:var(--hover)}button:disabled{opacity:.5}.remove{padding:7px}.actions span{flex:1}.save{background:var(--primary);color:var(--on-primary)}.error{color:var(--danger)}details{font-size:12px;color:var(--text-dim)}pre{white-space:pre-wrap}input:focus-visible,button:focus-visible,summary:focus-visible{outline:2px solid var(--focus);outline-offset:2px}
</style>
