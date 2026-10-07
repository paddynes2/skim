<script lang="ts">
  // Fork (v1.1.3): Deals for the open thread. `mode="tools"` renders the
  // toolbar button and its "Add to Deals" panel; `mode="status"` renders the
  // line under the subject naming the deal the thread belongs to. Both read
  // the same suggestion for `threadId`, and re-read it when the list changes.
  import { t } from "../../lib/i18n/index.svelte";
  import { ui } from "../../lib/stores/ui.svelte";
  import { toast } from "../stores/toast.svelte";
  import { type DealSuggestion, dealsApi } from "./api";
  import { dealsStore } from "./store.svelte";

  let { threadId, mode }: { threadId: number; mode: "tools" | "status" } = $props();

  let suggestion = $state<DealSuggestion | null>(null);
  let open = $state(false);
  let name = $state("");
  let entry = $state("");
  let saving = $state(false);

  async function load(id: number) {
    try {
      const s = await dealsApi.suggest(id);
      if (id === threadId) suggestion = s;
    } catch {
      suggestion = null;
    }
  }

  $effect(() => {
    // Re-read when the thread or the list text changes.
    void dealsStore.text;
    void load(threadId);
  });

  function toggle() {
    if (!open && suggestion) {
      name = suggestion.deal ?? suggestion.name;
      entry = suggestion.entry;
    }
    open = !open;
  }

  async function add() {
    const n = name.trim();
    const e = entry.trim();
    if (!n || !e || saving) return;
    saving = true;
    try {
      await dealsStore.add(n, e);
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
      e.stopPropagation();
      open = false;
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

{#if mode === "tools"}
  {#if suggestion}
    <div class="tools">
      <button
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
          <label class="field">
            <span>{t("fork.deals.match")}</span>
            <input bind:value={entry} spellcheck="false" />
          </label>
          <p class="hint">{t("fork.deals.match_hint")}</p>
          <div class="actions">
            <button type="button" class="link" onclick={editList}>{t("fork.deals.edit_list")}</button>
            <button type="submit" class="set" disabled={saving || !name.trim() || !entry.trim()}>{t("fork.deals.add_button")}</button>
          </div>
        </form>
      {/if}
    </div>
  {/if}
{:else if suggestion?.deal}
  <div class="status">
    <span class="chip">{t("fork.deals.status", { name: suggestion.deal })}</span>
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
    color: var(--text-dim);
  }
  .tool:hover {
    background: var(--hover);
    color: var(--text);
  }
  .tool.on {
    color: var(--unread);
  }
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
    width: 300px;
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
  .field input {
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
    background: var(--hover);
    border: 1px solid var(--hairline);
    font-size: 12.5px;
    color: var(--text-dim);
  }
</style>
