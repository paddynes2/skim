<script lang="ts">
  // /slots popover (PLAN.md Phase 8). Mounted by the shell while a composer
  // has asked for it (`slotsOpen.pending`). Duration 30/45/60, days 3/5/10,
  // recipient zone (defaults to the second zone from Settings → Calendar),
  // a live preview, Insert hands the plain list to the composer's cursor.
  import { onMount, untrack } from "svelte";
  import { t } from "../../lib/i18n/index.svelte";
  import { availabilityApi } from "../calendar/api";
  import { calendarErrorText, localZone } from "../calendar/guests";
  import { calPrefs } from "../calendar/settings.svelte";
  import { calendar } from "../calendar/store.svelte";
  import type { Slot } from "../calendar/types";
  import { slotsOpen } from "./open.svelte";
  import { formatSlots, SLOT_DAYS, SLOT_DURATIONS, validZone } from "./slots";

  let duration = $state<number>(30);
  let days = $state<number>(5);
  let recipientTz = $state("");
  let slots = $state<Slot[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let seq = 0;

  const senderTz = localZone();
  const zones: string[] = (() => {
    try {
      return (Intl as unknown as { supportedValuesOf?: (k: string) => string[] }).supportedValuesOf?.("timeZone") ?? [];
    } catch {
      return [];
    }
  })();

  const preview = $derived(
    formatSlots(slots, {
      senderTz,
      recipientTz: validZone(recipientTz) ? recipientTz : "",
      bookingLink: calPrefs.bookingLink,
      bookingLabel: t("fork.slots.or_pick"),
    }),
  );

  async function compute() {
    const my = ++seq;
    loading = true;
    error = null;
    try {
      const rows = await availabilityApi.freeSlots(duration, days, senderTz, calendar.accountId);
      if (my === seq) slots = rows;
    } catch (e: unknown) {
      if (my === seq) {
        slots = [];
        error = calendarErrorText(e);
      }
    } finally {
      if (my === seq) loading = false;
    }
  }

  onMount(() => {
    void calPrefs.load().then(() => {
      if (!recipientTz) recipientTz = calPrefs.secondTz;
    });
  });

  $effect(() => {
    // Re-run when the two inputs change; `compute` writes state of its own,
    // so it runs untracked.
    void duration;
    void days;
    untrack(() => void compute());
  });

  function insert() {
    const p = slotsOpen.pending;
    if (!p || !preview) return;
    p.insert(preview);
    slotsOpen.close();
  }

  function onKeydown(e: KeyboardEvent) {
    if (!slotsOpen.pending) return;
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopImmediatePropagation();
      slotsOpen.close();
    } else if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
      e.preventDefault();
      insert();
    }
  }
</script>

<svelte:window onkeydowncapture={onKeydown} />

{#if slotsOpen.pending}
  <div class="scrim" role="presentation" onmousedown={() => slotsOpen.close()}>
    <div class="pop" role="dialog" aria-modal="true" aria-label={t("fork.slots.title")} tabindex="-1" onmousedown={(e) => e.stopPropagation()}>
      <div class="head">
        <span class="microlabel">{t("fork.slots.title")}</span>
        <button class="x" onclick={() => slotsOpen.close()} aria-label={t("fork.cal.close")}>×</button>
      </div>

      <div class="row">
        <span class="label">{t("fork.slots.duration")}</span>
        <div class="chips" role="radiogroup" aria-label={t("fork.slots.duration")}>
          {#each SLOT_DURATIONS as n (n)}
            <button class="chip" class:active={duration === n} role="radio" aria-checked={duration === n} onclick={() => (duration = n)}>
              {t("fork.cal.minutes", { n })}
            </button>
          {/each}
        </div>
      </div>
      <div class="row">
        <span class="label">{t("fork.slots.days")}</span>
        <div class="chips" role="radiogroup" aria-label={t("fork.slots.days")}>
          {#each SLOT_DAYS as n (n)}
            <button class="chip" class:active={days === n} role="radio" aria-checked={days === n} onclick={() => (days = n)}>
              {t("fork.slots.days_n", { n })}
            </button>
          {/each}
        </div>
      </div>
      <div class="row">
        <label class="label" for="slots-tz">{t("fork.slots.recipient_tz")}</label>
        <input id="slots-tz" class="text" list="slots-zones" bind:value={recipientTz} placeholder={t("fork.slots.tz_placeholder")} spellcheck="false" />
        <datalist id="slots-zones">
          {#each zones as z (z)}<option value={z}></option>{/each}
        </datalist>
      </div>
      {#if recipientTz && !validZone(recipientTz)}
        <div class="hint danger">{t("fork.slots.tz_unknown")}</div>
      {/if}

      <div class="preview-wrap">
        <span class="microlabel">{t("fork.slots.preview")}</span>
        {#if loading}
          <div class="hint">{t("fork.slots.loading")}</div>
        {:else if error}
          <div class="hint danger">{error}</div>
        {:else if !preview}
          <div class="hint">{t("fork.slots.none")}</div>
        {:else}
          <pre class="preview">{preview}</pre>
        {/if}
      </div>

      <div class="actions">
        <button class="btn primary" onclick={insert} disabled={!preview || loading}>{t("fork.slots.insert")}</button>
        <button class="btn ghost" onclick={() => slotsOpen.close()}>{t("fork.cal.cancel")}</button>
        <kbd>Ctrl Enter</kbd>
      </div>
    </div>
  </div>
{/if}

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 380;
    display: grid;
    place-items: center;
    background: rgba(0, 0, 0, 0.18);
  }
  .pop {
    width: 480px;
    max-width: calc(100vw - 32px);
    max-height: calc(100vh - 64px);
    overflow-y: auto;
    padding: 14px 18px 14px;
    background: var(--surface-raised);
    border: 1px solid var(--hairline-strong);
    border-radius: var(--radius-l);
    box-shadow: var(--shadow-pop);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .x {
    width: 24px;
    height: 24px;
    border-radius: var(--radius-s);
    font-size: 16px;
    color: var(--text-faint);
  }
  .x:hover {
    background: var(--hover);
    color: var(--text);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .label {
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 500;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-faint);
    flex: 0 0 96px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .chip {
    padding: 5px 11px;
    border-radius: 999px;
    font-size: 12.5px;
    color: var(--text-dim);
    border: 1px solid transparent;
  }
  .chip:hover {
    background: var(--hover);
    color: var(--text);
  }
  .chip.active {
    background: var(--text);
    color: var(--bg);
    font-weight: 600;
  }
  .text {
    flex: 1;
    min-width: 0;
    padding: 5px 8px;
    border: 1px solid var(--hairline-strong);
    border-radius: var(--radius-s);
    background: var(--surface);
    font-family: var(--font-mono);
    font-size: 12px;
    user-select: text;
  }
  .preview-wrap {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-top: 4px;
  }
  .preview {
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 1.5;
    white-space: pre-wrap;
    padding: 10px 12px;
    border: 1px solid var(--hairline);
    border-radius: var(--radius-m);
    background: var(--surface);
    user-select: text;
    color: var(--text);
  }
  .hint {
    font-size: 12.5px;
    color: var(--text-dim);
  }
  .hint.danger {
    color: var(--danger);
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 4px;
  }
  .btn {
    padding: 7px 13px;
    border-radius: 999px;
    font-size: 13px;
    font-weight: 600;
    border: 1px solid var(--hairline-strong);
    color: var(--text);
  }
  .btn:hover {
    background: var(--hover);
  }
  .btn:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .btn.primary {
    background: var(--text);
    color: var(--bg);
    border-color: var(--text);
  }
  .btn.primary:hover {
    background: var(--text);
    opacity: 0.88;
  }
  .btn.ghost {
    border-color: transparent;
    color: var(--text-dim);
  }
  kbd {
    margin-left: auto;
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-faint);
    border: 1px solid var(--hairline-strong);
    border-radius: 4px;
    padding: 1px 5px;
  }
</style>
