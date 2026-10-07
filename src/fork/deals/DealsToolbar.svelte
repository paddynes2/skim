<script lang="ts">
  import { t } from "../../lib/i18n/index.svelte";
  import { dealsApi, type ParsedDeals, type DealContext } from "./api";
  import { dealsStore } from "./store.svelte";
  import DealBadge from "./DealBadge.svelte";
  let {onselect, onthread, onevent, onsearch}: {onselect:(name:string)=>void; onthread:(id:number)=>void; onevent:(id:number,startTs:number)=>void; onsearch:(query:string)=>void} = $props();
  let companies = $state<ParsedDeals["deals"]>([]), overview = $state(false), context = $state<DealContext|null>(null), loading = $state(false), error = $state("");
  const company = $derived(companies.find(c => c.name === dealsStore.selectedCompany));
  $effect(() => { const text = dealsStore.text; let cancelled=false; void dealsApi.preview(text).then(p => {if(!cancelled) companies=p.deals;}).catch(e => {if(!cancelled) error=String(e);}); return () => {cancelled=true;}; });
  function select(name:string) {dealsStore.selectCompany(name); context=null; error=""; onselect(name);}
  $effect(() => { if(dealsStore.loaded && companies.length && dealsStore.selectedCompany && !companies.some(c=>c.name===dealsStore.selectedCompany)) select(""); });
  $effect(() => { const name=dealsStore.selectedCompany; if(!overview || !name){context=null;return;} let cancelled=false; loading=true;error=""; void dealsApi.context(name).then(c=>{if(!cancelled)context=c;}).catch(e=>{if(!cancelled)error=String(e);}).finally(()=>{if(!cancelled)loading=false;}); return ()=>{cancelled=true;}; });
  function retry(){overview=false;queueMicrotask(()=>overview=true);}
  function when(ts:number,allDay=false){return new Date(ts*1000).toLocaleString(undefined, allDay ? {weekday:"short",month:"short",day:"numeric",timeZone:"UTC"} : {weekday:"short",month:"short",day:"numeric",hour:"numeric",minute:"2-digit"});}
</script>
<div class="deal-tools">
  <div class="selector">
    {#if company}<DealBadge deal={company.name} domain={company.domains[0]??null}/>{/if}
    <select aria-label={t("fork.deals.company")} value={dealsStore.selectedCompany} onchange={e=>select(e.currentTarget.value)}><option value="">{t("fork.deals.all_companies")}</option>{#each companies as c}<option value={c.name}>{c.name}</option>{/each}</select>
    <button disabled={!company} aria-expanded={overview && !!company} onclick={()=>overview=!overview}>{t("fork.deals.overview")}</button>
  </div>
  {#if overview && company}
    <section class="overview" aria-label={t("fork.deals.company_overview",{name:company.name})}>
      <h2>{company.name}</h2><p class="scope">{t("fork.deals.local_scope")}</p>
      {#if loading}<p role="status">{t("fork.deals.loading")}</p>{:else if error}<p role="alert" class="error">{error} <button onclick={retry}>{t("fork.deals.retry")}</button></p>{:else if context}
        <h3>{t("fork.deals.conversations",{n:context.conversationCount})}</h3>
        {#each context.conversations as row}<button class="item" onclick={()=>onthread(row.id)}><strong>{row.subject}</strong><span>{row.fromName} &middot; {when(row.date)}</span></button>{:else}<p>{t("fork.deals.no_conversations")}</p>{/each}
        <h3>{t("fork.deals.people")}</h3>
        {#each context.people as person}<button class="item" onclick={()=>onsearch(`from:"${person.addr}"`)}><strong>{person.name||person.addr}</strong>{#if person.name}<span>{person.addr}</span>{/if}</button>{:else}<p>{t("fork.deals.no_people")}</p>{/each}
        <h3>{t("fork.deals.attachments")}</h3>
        {#each context.attachments as file}<button class="item" onclick={()=>onthread(file.threadId)}><strong>{file.filename}</strong><span>{when(file.date)}</span></button>{:else}<p>{t("fork.deals.no_attachments")}</p>{/each}
        <h3>{t("fork.deals.meetings")}</h3>
        {#each context.meetings as event}<button class="item" onclick={()=>onevent(event.id,event.startTs)}><strong>{event.summary}</strong><span>{when(event.startTs,event.allDay)}</span></button>{:else}<p>{t("fork.deals.no_meetings")}</p>{/each}
        <p class="scope">{t("fork.deals.overview_limits")}</p>
      {/if}
    </section>
  {/if}
</div>
<style>
  .deal-tools{border-bottom:1px solid var(--hairline);padding:10px 14px}.selector{display:flex;align-items:center;gap:8px}select{min-width:0;flex:1;background:var(--surface);color:var(--text);border:1px solid var(--hairline-strong);border-radius:var(--radius-s);padding:7px;font-size:12px}button{border:1px solid var(--hairline-strong);border-radius:var(--radius-s);padding:7px 10px;font-size:12px}button:hover:not(:disabled){background:var(--hover)}button:disabled{opacity:.45}.overview{max-height:55vh;overflow:auto;padding-top:14px}.overview h2{font-size:16px;margin:0 0 5px}.overview h3{font-size:12px;margin:18px 0 6px;color:var(--text-dim)}.overview p{font-size:12px;line-height:1.5;color:var(--text-dim)}.overview .scope{font-size:11px}.item{display:flex;flex-direction:column;align-items:flex-start;text-align:left;gap:4px;width:100%;border:0;border-radius:0;border-bottom:1px solid var(--hairline);padding:9px 0;overflow-wrap:anywhere}.item strong{font-size:12px;font-weight:500}.item span{font-size:11px;color:var(--text-dim)}.error{color:var(--danger)}button:focus-visible,select:focus-visible{outline:2px solid var(--focus);outline-offset:2px}
</style>
