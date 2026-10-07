<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "../../lib/i18n/index.svelte";
  import { api } from "../../lib/api";
  import { dealsStore } from "../deals/store.svelte";
  import { dealsApi, type ParsedDeals } from "../deals/api";
  import { readFilters, applyFilters, parseSavedSearches, saveSearch, type SavedSearch } from "./filters";
  let {query="", onsearch}: {query?: string; onsearch:(query:string)=>void} = $props();
  let open=$state(false), filters=$state(readFilters("")), free=$state(""), name=$state(""), saved=$state<SavedSearch[]>([]), ready=$state(false), busy=$state(false), error=$state("");
  let companies=$state<ParsedDeals["deals"]>([]);
  $effect(()=>{const q=query;filters=readFilters(q);free=q;});
  $effect(()=>{const text=dealsStore.text;let cancelled=false;void dealsApi.preview(text).then(p=>{if(!cancelled)companies=p.deals;}).catch(()=>{});return ()=>{cancelled=true;};});
  onMount(()=>{void api.getSettings().then(s=>{saved=parseSavedSearches(s?.fork_saved_searches);ready=true;}).catch(e=>{error=String(e);});});
  const result=$derived(applyFilters(free,filters));
  async function persist(items:SavedSearch[]){busy=true;error="";try{await api.setSetting("fork_saved_searches",JSON.stringify(items));saved=items;name="";}catch(e){error=String(e);}finally{busy=false;}}
  function save(){try{void persist(saveSearch(saved,name,result));}catch(e){error=String(e);}}
  function search(){if(!result.trim()){error=t("fork.search.enter_query");return;}if(filters.after && filters.before && filters.after>filters.before){error=t("fork.search.date_order");return;}error="";onsearch(result);open=false;}
</script>
<div class="search-tools">
  <button class="toggle" aria-expanded={open} onclick={()=>open=!open}>{t("fork.search.refine")}</button>
  {#if open}
    <form onsubmit={e=>{e.preventDefault();search();}}>
      <label class="wide">{t("fork.search.words")}<input bind:value={free} oninput={e=>{filters=readFilters(e.currentTarget.value);}} placeholder={t("fork.search.words_placeholder")} /></label>
      <label>{t("fork.search.sender")}<input bind:value={filters.sender} placeholder="name@example.com"/></label>
      <label>{t("fork.deals.company")}<select bind:value={filters.company}><option value="">{t("fork.deals.all_companies")}</option>{#each companies as c}<option value={[...c.domains,...c.addresses].join(",")}>{c.name}</option>{/each}{#if filters.company && !companies.some(c=>[...c.domains,...c.addresses].join(",")===filters.company)}<option value={filters.company}>{filters.company}</option>{/if}</select></label>
      <label>{t("fork.search.after")}<input type="date" bind:value={filters.after}/></label><label>{t("fork.search.before")}<input type="date" bind:value={filters.before}/></label>
      <label class="check wide"><input type="checkbox" bind:checked={filters.attachments}/>{t("fork.search.with_attachments")}</label>
      <div class="actions wide"><button type="button" onclick={()=>{filters=readFilters("");free="";error="";}}>{t("fork.search.clear_filters")}</button><button class="primary" type="submit">{t("fork.search.run")}</button></div>
      <div class="saved wide">
        <h3>{t("fork.search.saved")}</h3>
        {#each saved as item}<div class="saved-row"><button type="button" class="saved-name" title={item.query} onclick={()=>{onsearch(item.query);open=false;}}>{item.name}</button><button type="button" disabled={busy || !ready} aria-label={t("fork.search.remove_saved",{name:item.name})} onclick={()=>void persist(saved.filter(s=>s!==item))}>&times;</button></div>{/each}
        {#if !saved.length}<p>{t("fork.search.no_saved")}</p>{/if}
        <div class="save-row"><input aria-label={t("fork.search.saved_name")} bind:value={name} maxlength="80" placeholder={t("fork.search.saved_name")}/><button type="button" disabled={!ready || busy || !name.trim() || !result.trim()} onclick={save}>{t("fork.search.save")}</button></div>
      </div>
      {#if error}<p class="error wide" role="alert">{error}</p>{/if}
    </form>
  {/if}
</div>
<style>
  .search-tools{padding:8px 14px;border-bottom:1px solid var(--hairline)}button{padding:6px 9px;font-size:12px;border:1px solid var(--hairline-strong);border-radius:var(--radius-s)}button:hover:not(:disabled){background:var(--hover)}button:disabled{opacity:.5}.toggle{color:var(--text-dim);background:transparent}form{display:grid;grid-template-columns:minmax(0,1fr) minmax(0,1fr);gap:10px;padding-top:12px;max-height:65vh;overflow:auto}label{display:flex;flex-direction:column;gap:5px;font-size:11px;color:var(--text-dim);min-width:0}.wide{grid-column:1/-1}input:not([type=checkbox]),select{box-sizing:border-box;min-width:0;width:100%;padding:7px 8px;border:1px solid var(--hairline-strong);border-radius:var(--radius-s);background:var(--surface);color:var(--text);font-size:12px}.check{flex-direction:row;align-items:center;font-size:12px}.actions{display:flex;justify-content:space-between;gap:8px}.primary{background:var(--primary);color:var(--on-primary)}.saved{border-top:1px solid var(--hairline);padding-top:10px}.saved h3{font-size:12px;margin:0 0 8px}.saved p{font-size:12px;color:var(--text-dim)}.saved-row,.save-row{display:flex;gap:6px;margin-top:6px;min-width:0}.saved-name{flex:1;text-align:left;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.save-row input{flex:1}.error{color:var(--danger);font-size:12px;line-height:1.5}button:focus-visible,input:focus-visible,select:focus-visible{outline:2px solid var(--focus);outline-offset:2px}
</style>
