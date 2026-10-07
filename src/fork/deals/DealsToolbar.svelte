<script lang="ts">
  import { t } from "../../lib/i18n/index.svelte";
  import { dealsApi, type ParsedDeals } from "./api";
  import { dealsStore } from "./store.svelte";
  import DealBadge from "./DealBadge.svelte";
  import CompanyOverview from "./CompanyOverview.svelte";
  let {onselect, onthread, onevent, onsearch}: {onselect:(name:string)=>void; onthread:(id:number,messageId?:number)=>void; onevent:(id:number,startTs:number)=>void; onsearch:(query:string)=>void} = $props();
  let companies = $state<ParsedDeals["deals"]>([]), overview = $state(false), error = $state(""), catalogLoading = $state(false);
  const company = $derived(companies.find(c => c.name === dealsStore.selectedCompany));
  $effect(() => { void dealsStore.text; void dealsStore.revision; catalogLoading=true; let cancelled=false; void dealsApi.catalog().then(p => {if(!cancelled) companies=p.deals;}).catch(e => {if(!cancelled) error=String(e);}).finally(()=>{if(!cancelled)catalogLoading=false;}); return () => {cancelled=true;}; });
  function select(name:string) {dealsStore.selectCompany(name); error=""; onselect(name);}
  $effect(() => { if(!catalogLoading && !error && dealsStore.loaded && companies.length && dealsStore.selectedCompany && !companies.some(c=>c.name===dealsStore.selectedCompany)) select(""); });
  $effect(()=>{if(dealsStore.overviewRequest>0) overview=true;});

</script>
<div class="deal-tools">
  <div class="selector">
    {#if company}<DealBadge deal={company.name} domain={company.domains[0]??null} onclick={()=>overview=!overview}/>{/if}
    <select aria-label={t("fork.deals.company")} value={dealsStore.selectedCompany} onchange={e=>select(e.currentTarget.value)}><option value="">{t("fork.deals.all_companies")}</option>{#each companies as c}<option value={c.name}>{c.name}</option>{/each}</select>
    <button class="overview-toggle" disabled={!company} aria-expanded={overview && !!company} onclick={()=>overview=!overview}>{t("fork.deals.overview")}</button>
  </div>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if overview && company}
    <CompanyOverview company={company.name} {onthread} {onevent} {onsearch} />
  {/if}
</div>
<style>
  .error{font-size:12px;color:var(--danger)}
  .overview-toggle{color:var(--deal-ink);background:var(--deal-soft)}
  .deal-tools{border-bottom:1px solid var(--hairline);padding:10px 14px}.selector{display:flex;align-items:center;gap:8px}select{min-width:0;flex:1;background:var(--surface);color:var(--text);border:1px solid var(--hairline-strong);border-radius:var(--radius-s);padding:7px;font-size:12px}button{border:1px solid var(--hairline-strong);border-radius:var(--radius-s);padding:7px 10px;font-size:12px}button:hover:not(:disabled){background:var(--hover)}button:disabled{opacity:.45}button:focus-visible,select:focus-visible{outline:2px solid var(--focus);outline-offset:2px}
</style>
