<script lang="ts">
  // Fork (v1.1.3): Deals for the open thread. `mode="tools"` renders the
  // toolbar button and its "Add to Deals" panel; `mode="status"` renders the
  // line under the subject naming the deal the thread belongs to. Both read
  // the same suggestion for `threadId`, and re-read it when the list changes.
  import { t } from "../../lib/i18n/index.svelte";
  import { ui } from "../../lib/stores/ui.svelte";
  import { toast } from "../stores/toast.svelte";
  import { type DealSuggestion, type ScopeInput, dealsApi } from "./api";
  import { dealsStore } from "./store.svelte";

  let { threadId, mode }: { threadId: number; mode: "tools" | "status" } = $props();

  let suggestion = $state<DealSuggestion | null>(null);
  let open = $state(false);
  let name = $state("");
  let entry = $state("");
  let saving = $state(false);
  let toolButton = $state<HTMLButtonElement>();
  let scope = $state<ScopeInput["scope"]>("company");
  let preview = $state<{count:number;entry:string}|null>(null);
  let previewError = $state("");
  let requestSerial = 0, previousThread = 0, handledRequest = 0;
  function errorMessage(e:unknown) {return String((e as {message?:string})?.message??e);}


  async function load(id: number) {
    const request=++requestSerial;
    try {
      const s = await dealsApi.suggest(id);
      if (id === threadId && request === requestSerial) suggestion = s;
    } catch {
      if(id === threadId && request === requestSerial) suggestion = null;
    }
  }

  $effect(() => {
    // Re-read when the thread or the list text changes.
    void dealsStore.text;
    void dealsStore.revision;
    if(previousThread!==threadId){open=false;suggestion=null;previousThread=threadId;}
    void load(threadId);
  });

  $effect(() => {
    const request=dealsStore.addRequest;
    if(mode==="tools" && request.threadId===threadId && request.serial!==handledRequest && suggestion) {handledRequest=request.serial; if(!open) toggle();}
  });
  $effect(() => {
    if(!open) return;
    const input={threadId,name,entry,scope};let cancelled=false;
    preview=null;previewError="";
    const timer=setTimeout(()=>{void dealsApi.previewScope(input).then(p=>{if(!cancelled)preview=p;}).catch(e=>{if(!cancelled)previewError=errorMessage(e);});},180);
    return ()=>{cancelled=true;clearTimeout(timer);};
  });
  function chooseScope(value:ScopeInput["scope"]) {scope=value;entry=value==="person"?(suggestion?.personEntry??""):value==="company"?(suggestion?.companyEntry??""):"";}
  async function exclude(excluded:boolean){if(saving)return;saving=true;try{await dealsStore.exclude(threadId,excluded);open=false;}catch(e){previewError=errorMessage(e);}finally{saving=false;}}
  function toggle() {
    if (!open && suggestion) {
      name = suggestion.deal ?? suggestion.name;
      chooseScope(suggestion.companyEntry ? "company" : "person");
    }
    open = !open;
  }

  async function add() {
    const n = name.trim();
    const e = entry.trim();
    if (!n || (scope!=="conversation" && !e) || saving || !preview?.count) return;
    saving = true;
    try {
      await dealsStore.applyScope({threadId,name:n,entry:e,scope});
      open = false;
      toast.show({ text: t("fork.deals.added", { name: n }) });
    } catch (err: unknown) {
      toast.show({ text: String((err as { message?: string })?.message ?? err) });
    } finally {
      saving = false;
    }
  }

  function editList() {
    open = false;
    ui.openSettings();
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && open) {
      e.preventDefault();
      e.stopImmediatePropagation();
      open = false;
      toolButton?.focus();
    }
  }
  $effect(() => {
    if (!open) return;
    window.addEventListener("keydown", onKeydown, true);
    return () => window.removeEventListener("keydown", onKeydown, true);
  });
</script>

{#if mode === "tools"}
  {#if suggestion}
    <div class="tools">
      <button
        bind:this={toolButton}
        class="tool"
        class:on={suggestion.deal !== null}
        onclick={toggle}
        title={suggestion.deal ? t("fork.deals.in", { name: suggestion.deal }) : t("fork.deals.add")}
        aria-label={suggestion.deal ? t("fork.deals.in", { name: suggestion.deal }) : t("fork.deals.add")}
        aria-expanded={open}
      >
        <svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" stroke-linecap="round">
          <rect x="1.5" y="4.5" width="13" height="9" rx="1.5" />
          <path d="M5.5 4.5V3a1 1 0 0 1 1-1h3a1 1 0 0 1 1 1v1.5M1.5 8.5h13" />
        </svg>
        <span>{suggestion.deal ? t("fork.nav.deals") : t("fork.deals.add")}</span>
      </button>

      {#if open}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="scrim" onclick={() => (open = false)}></div>
        <form class="menu" onsubmit={(e) => { e.preventDefault(); void add(); }}>
          <div class="menu-head">{t("fork.deals.add")}</div>
          <label class="field">
            <span>{t("fork.deals.name")}</span>
            <!-- svelte-ignore a11y_autofocus -->
            <input bind:value={name} autofocus spellcheck="false" />
          </label>
          <label class="field"><span>{t("fork.deals.scope")}</span><select value={scope} onchange={e=>chooseScope(e.currentTarget.value as ScopeInput["scope"])} disabled={saving}><option value="conversation">{t("fork.deals.scope_conversation")}</option><option value="person">{t("fork.deals.scope_person")}</option><option value="company" disabled={!suggestion.companyEntry}>{t("fork.deals.scope_company")}</option></select></label>
          {#if scope!=="conversation"}<label class="field">
            <span>{t("fork.deals.match")}</span>
            <input bind:value={entry} spellcheck="false" />
          </label>
          {/if}
          <p class="hint">{scope==="conversation"?t("fork.deals.conversation_only"):t("fork.deals.match_hint")}</p>
          <p class="preview" aria-live="polite">{previewError || (preview ? t("fork.deals.scope_count",{n:preview.count}) : t("fork.deals.checking_scope"))}</p>
          <div class="actions">
            <button type="button" class="link" onclick={editList}>{t("fork.deals.edit_list")}</button>
            <button type="submit" class="set" disabled={saving || !name.trim() || !preview?.count}>{t("fork.deals.add_button")}</button>
          </div>
          {#if suggestion.deal || suggestion.excluded}<button type="button" class="exclude" disabled={saving} onclick={()=>void exclude(!suggestion?.excluded)}>{t(suggestion.excluded?"fork.deals.restore_conversation":"fork.deals.exclude_conversation")}</button>{/if}
        </form>
      {/if}
    </div>
  {/if}
{:else if suggestion?.deal}
  <div class="status">
    <button class="chip" onclick={()=>dealsStore.openCompany(suggestion!.deal!)}>{t("fork.deals.status", { name: suggestion.deal })}</button>
  </div>
{/if}

<style>
  .tools {
    position: relative;
    display: flex;
    align-items: center;
  }
  /* Same look as the reading pane's .tool buttons. */
  .tool {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 5px 8px;
    border-radius: var(--radius-s);
    color: var(--deal-ink);
    font-size: 12px;
    white-space: nowrap;
    border: 1px solid color-mix(in srgb, var(--deal-ink) 24%, transparent);
    background: var(--deal-soft);
  }
  .tool:hover {
    background: var(--hover);
    color: var(--text);
  }
  .tool.on {
    color: var(--deal-ink);
  }
  .tool:focus-visible, .chip:focus-visible { outline: 2px solid var(--focus); outline-offset: 2px; }
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 40;
  }
  .menu {
    position: absolute;
    top: calc(100% + 6px);
    right: 0;
    z-index: 41;
    width: min(340px, calc(100vw - 32px));
    box-sizing: border-box;
    padding: 6px 8px 8px;
    background: var(--surface-raised);
    border: 1px solid var(--hairline-strong);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow-pop);
  }
  .menu-head {
    padding: 4px 0 8px;
    font-size: 11.5px;
    font-weight: 600;
    color: var(--text-faint);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-bottom: 8px;
    font-size: 12px;
    color: var(--text-dim);
  }
  .field input, .field select {
    min-width: 0;
    padding: 5px 7px;
    border: 1px solid var(--hairline-strong);
    border-radius: var(--radius-s);
    background: var(--surface);
    font-size: 13px;
    color: var(--text);
  }
  .hint {
    margin: -2px 0 8px;
    font-size: 11.5px;
    color: var(--text-faint);
  }
  .actions {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .set {
    padding: 5px 12px;
    border-radius: var(--radius-s);
    background: var(--primary);
    color: var(--on-primary);
    font-size: 12.5px;
    font-weight: 600;
  }
  .set:disabled {
    opacity: 0.5;
  }
  .link {
    color: var(--unread);
    font-size: 12px;
  }
  .link:hover {
    text-decoration: underline;
  }
  .status {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: -4px 0 12px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    padding: 3px 10px;
    border-radius: 999px;
    background: var(--deal-soft);
    border: 1px solid color-mix(in srgb, var(--deal-ink) 24%, transparent);
    font-size: 12.5px;
    color: var(--deal-ink);
  }
  .chip:hover { text-decoration: underline; }
  .preview { font-size: 12px; line-height: 1.4; color: var(--text); margin: 8px 0 12px; }
  .exclude { font-size: 12px; color: var(--text-dim); margin-top: 12px; text-decoration: underline; }
</style>
