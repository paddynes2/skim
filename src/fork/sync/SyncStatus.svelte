<script lang="ts">
  import { onMount } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { t } from "../../lib/i18n/index.svelte";
  import { mail } from "../../lib/stores/mail.svelte";
  import { syncStatusApi, type QueueStatus } from "./api";
  let queue=$state<QueueStatus|null>(null), error=$state(""), expanded=$state(false), busy=$state(false);
  let request=0;
  async function refresh(){const current=++request;try{const q=await syncStatusApi.status(mail.account?.id??null);if(current===request){queue=q;error="";}}catch(e){if(current===request)error=String((e as {message?:string})?.message??e);}}
  $effect(()=>{if(mail.booted){void mail.account?.id;void mail.unified;queue=null;void refresh();}});
  onMount(()=>{let dead=false;const stops:UnlistenFn[]=[];for(const event of ["ops:failed","sync:status","mail:updated"]){void listen(event,()=>void refresh()).then(stop=>{if(dead)stop();else stops.push(stop);});}const timer=setInterval(()=>void refresh(),15000);return()=>{dead=true;request++;clearInterval(timer);stops.forEach(stop=>stop());};});
  const label=$derived(mail.syncState==="syncing"?t("fork.sync.refreshing"):mail.syncState==="offline"?t("fork.sync.offline"):mail.syncState==="error"?t("fork.sync.trouble"):queue?.failed?t("fork.sync.failed",{n:queue.failed}):queue?.pending?t("fork.sync.pending",{n:queue.pending}):!queue?t("fork.sync.checking"):t("fork.sync.ready"));
  async function retry(id:number){busy=true;error="";try{await syncStatusApi.retry(id);await refresh();}catch(e){error=String((e as {message?:string})?.message??e);}finally{busy=false;}}
  async function sync(){busy=true;error="";try{await mail.syncNow();await refresh();}catch(e){error=String((e as {message?:string})?.message??e);}finally{busy=false;}}
  const known=new Set(["read","unread","star","unstar","send","rsvp","unsubscribe","save_draft","move","delete","archive","junk","rename_folder","delete_folder"]);
  function action(value:string){return known.has(value)?t(`fork.sync.action_${value}`):value.replaceAll("_"," ");}
</script>
<div class="sync-status" class:trouble={!!queue?.failed || mail.syncState==="error" || !!error}>
  <button title={label} class="status" aria-expanded={expanded} onclick={()=>{expanded=!expanded;if(expanded)void refresh();}}><span class="dot" class:working={mail.syncState==="syncing"}></span><span>{label}</span><span aria-hidden="true">{expanded?'\u2212':'+'}</span></button>
  {#if expanded}
    <div class="details">
      <p>{t("fork.sync.cache_note")}</p>
      {#if mail.syncMessage}<p>{mail.syncMessage}</p>{/if}
      {#if queue}
        {#if queue.pending}<p>{t("fork.sync.pending",{n:queue.pending})}</p>{/if}
        {#if queue.scheduled}<p>{t("fork.sync.scheduled",{n:queue.scheduled})}</p>{/if}
        {#if queue.failed}<p class="failure-count">{t("fork.sync.failed",{n:queue.failed})}</p>{/if}
        {#each queue.failures as item (item.id)}<div class="failure"><strong>{action(item.action)}</strong><span>{item.accountEmail}</span><span>{new Date(item.createdAt*1000).toLocaleString()}</span>{#if item.retryable}<button disabled={busy} onclick={()=>void retry(item.id)}>{t("fork.sync.retry_action")}</button>{:else}<span>{t("fork.sync.manual_review")}</span>{/if}</div>{/each}
        {#if queue.failed>queue.failures.length}<p>{t("fork.sync.first_failures")}</p>{/if}
      {/if}
      {#if error}<p role="alert" class="error">{error}</p>{/if}
      <div class="actions"><button disabled={busy || mail.syncState==="syncing"} onclick={()=>void sync()}>{t("fork.sync.now")}</button><button disabled={busy} onclick={()=>void refresh()}>{t("fork.sync.refresh_status")}</button></div>
    </div>
  {/if}
</div>
<style>
 .sync-status{border-top:1px solid var(--hairline);font-size:11px;color:var(--text-dim)}.status{display:flex;align-items:center;gap:7px;width:100%;padding:9px 10px;text-align:left}.status span:nth-child(2){flex:1;line-height:1.3}.dot{width:6px;height:6px;background:var(--text-faint);border-radius:50%;flex-shrink:0}.working{background:var(--primary)}.trouble .dot{background:var(--danger)}.details{max-height:40vh;overflow:auto;padding:0 10px 10px}.details p{font-size:11px;line-height:1.45;margin:5px 0;overflow-wrap:anywhere}.failure{display:flex;flex-direction:column;gap:4px;border-top:1px solid var(--hairline);padding:8px 0;overflow-wrap:anywhere}.failure strong{font-weight:600;color:var(--text)}.failure button{align-self:flex-start}.details button{border:1px solid var(--hairline-strong);border-radius:var(--radius-s);padding:5px 7px;font-size:11px}.actions{display:flex;flex-wrap:wrap;gap:5px;margin-top:8px}button:hover:not(:disabled){background:var(--hover)}button:disabled{opacity:.5}button:focus-visible{outline:2px solid var(--focus);outline-offset:2px}.error,.failure-count{color:var(--danger)}
:global(.sidebar.collapsed) .status{justify-content:center;padding:10px}:global(.sidebar.collapsed) .status span:not(.dot){display:none}:global(.sidebar.collapsed) .details{position:absolute;bottom:60px;left:55px;width:230px;background:var(--surface);border:1px solid var(--hairline-strong);padding:12px;z-index:20}
</style>
