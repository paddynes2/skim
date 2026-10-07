<script lang="ts">
  import {t} from "../lib/i18n/index.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { api, errorMessage } from "../lib/api";
  import type { AttachmentMeta } from "../lib/types";
  import { groupAttachments } from "../fork/attachments/groups";
  let {attachments,onsource,sourceLabel}:{attachments:AttachmentMeta[];onsource?:(file:AttachmentMeta)=>void;sourceLabel?:(file:AttachmentMeta)=>string}=$props();
  let fingerprints=$state<Record<number,string>>({}), grouping=$state(false),error=$state(""),notice=$state(""),saving=$state(false);
  let selected=$state<AttachmentMeta|null>(null),preview=$state<{kind:string;dataUrl:string|null;text:string|null;truncated:boolean}|null>(null),loading=$state(false),previewError=$state("");
  let dialog=$state<HTMLDialogElement>();let previewToken=0;
  const groups=$derived(groupAttachments(attachments,fingerprints));
  $effect(()=>{const ids=attachments.map(a=>a.id);let stale=false;grouping=true;fingerprints={};void invoke<{id:number;sha256:string|null}[]>("fork_attachment_fingerprints",{attachmentIds:ids.slice(0,200)}).then(rows=>{if(!stale)fingerprints=Object.fromEntries(rows.filter(r=>r.sha256).map(r=>[r.id,r.sha256!]));}).catch(()=>{}).finally(()=>{if(!stale)grouping=false;});return()=>{stale=true;};});
  $effect(()=>{if(selected && dialog && !dialog.open)dialog.showModal();});
  function size(bytes:number){return bytes<1024?`${bytes} B`:bytes<1048576?`${Math.round(bytes/1024)} KB`:`${(bytes/1048576).toFixed(1)} MB`;}
  async function open(file:AttachmentMeta){selected=file;preview=null;previewError="";loading=true;const token=++previewToken;try{const result=await invoke<typeof preview>("fork_attachment_preview",{attachmentId:file.id});if(token===previewToken)preview=result;}catch(e){if(token===previewToken)previewError=errorMessage(e);}finally{if(token===previewToken)loading=false;}}
  function close(){previewToken++;selected=null;preview=null;dialog?.close();}
  async function saveAll(){saving=true;error="";notice="";try{const result=await invoke<{saved:number;failed:string[];cancelled:boolean}>("fork_save_attachments",{attachmentIds:groups.map(g=>g[0].id)});if(!result.cancelled)notice=t("fork.files.saved_count",{n:result.saved})+(result.failed.length?" "+t("fork.files.failed_names",{names:result.failed.join(", ")}):"");}catch(e){error=errorMessage(e);}finally{saving=false;}}
  async function save(id:number){previewError="";error="";try{await api.saveAttachment(id);}catch(e){if(selected)previewError=errorMessage(e);else error=errorMessage(e);}}
  async function original(id:number){try{await api.openAttachment(id);}catch(e){previewError=errorMessage(e);}}
</script>
<div class="chips">
  <div class="attachment-head"><span>{t("fork.files.heading")}{#if attachments.length>1} &middot; {groups.length}{/if}</span>{#if attachments.length>1}<button disabled={saving||grouping||groups.length>200} onclick={saveAll}>{saving?t("fork.files.saving"):t("fork.files.save_all")}</button>{/if}</div>
  <div class="row">{#each groups as group (group[0].id)}{@const a=group[0]}
    <div class="file"><div class="chip"><button class="name" onclick={()=>void open(a)} title={t("fork.files.preview")}>{a.filename??t("fork.files.attachment")}<span class="size">{size(a.size)}</span></button><button class="save" onclick={()=>void save(a.id)} aria-label={t("fork.files.save_named",{name:a.filename??t("fork.files.attachment")})}>{t("fork.files.save")}</button></div>
    {#if group.length>1}<small>{t("fork.files.identical",{n:group.length})}</small>{/if}
    {#if onsource}<div class="sources">{#each group as source,i}<button onclick={()=>onsource?.(source)}>{sourceLabel?.(source)??t("fork.files.source_number",{n:i+1})}</button>{/each}</div>{/if}</div>
  {/each}</div>
  {#if notice}<p class="notice" role="status">{notice}</p>{/if}{#if error}<p class="error" role="alert">{error}</p>{/if}
</div>
<dialog bind:this={dialog} onclose={close} aria-label={t("fork.files.preview_title")}>
  {#if selected}<header><strong>{selected.filename??t("fork.files.attachment")}</strong><button onclick={close} aria-label={t("fork.files.close_preview")}>&times;</button></header><div class="preview">
    {#if loading}<p role="status">{t("fork.files.loading")}</p>{:else if preview?.kind==="image" && preview.dataUrl}<img src={preview.dataUrl} alt={selected.filename??t("fork.files.attachment")}/>{:else if preview?.kind==="pdf-text"}<p class="coverage">{t("fork.files.pdf_text")}{#if preview.truncated} {t("fork.files.pdf_limit")}{/if}</p><pre>{preview.text?.trim()||t("fork.files.pdf_empty")}</pre>{/if}
    {#if previewError}<p class="error" role="alert">{previewError}</p>{/if}
  </div><footer><button onclick={()=>void original(selected!.id)}>{t("fork.files.original")}</button><button onclick={()=>void save(selected!.id)}>{t("fork.files.save_file")}</button>{#if onsource}<button onclick={()=>{onsource?.(selected!);close();}}>{t("fork.files.source")}</button>{/if}</footer>{/if}
</dialog>
<style>
.chips{margin-top:16px}.attachment-head{display:flex;align-items:center;justify-content:space-between;color:var(--text-dim);font-size:12px}.attachment-head button,.sources button{color:var(--primary);font-size:12px}.row{display:flex;flex-wrap:wrap;gap:10px;margin-top:8px}.file{max-width:100%;min-width:0}.chip{display:flex;border:1px solid var(--hairline-strong);border-radius:var(--radius-s);overflow:hidden}.name{display:flex;align-items:center;gap:10px;min-width:0;padding:9px 11px;font-size:12.5px;text-align:left;overflow-wrap:anywhere;color:var(--text)}.name:hover,.save:hover{background:var(--hover)}.size{white-space:nowrap;color:var(--text-dim);font-size:11px}.save{border-left:1px solid var(--hairline);padding:8px 10px;font-size:11px;color:var(--text-dim)}small,.notice,.error,.coverage{font-size:12px;line-height:1.5;color:var(--text-dim)}.sources{display:flex;gap:10px;flex-wrap:wrap;margin-top:5px}.error{color:var(--danger)}button:disabled{opacity:.5}button:focus-visible{outline:2px solid var(--focus);outline-offset:2px}dialog{position:fixed;margin:auto;width:min(900px,90vw);max-height:88vh;padding:0;background:var(--surface);color:var(--text);border:1px solid var(--hairline-strong);border-radius:var(--radius-m)}dialog::backdrop{background:#0008}header,footer{display:flex;gap:14px;align-items:center;padding:14px 18px;border-bottom:1px solid var(--hairline)}header strong{flex:1;overflow-wrap:anywhere}header button{font-size:22px}.preview{overflow:auto;max-height:66vh;padding:18px}.preview img{display:block;max-width:100%;max-height:62vh;margin:auto;object-fit:contain}.preview pre{font:14px/1.7 var(--font-body);white-space:pre-wrap;overflow-wrap:anywhere}.coverage{margin-bottom:12px}footer{border-top:1px solid var(--hairline);border-bottom:0;flex-wrap:wrap}footer button{padding:7px 10px;border:1px solid var(--hairline-strong);border-radius:var(--radius-s);font-size:12px}
</style>
