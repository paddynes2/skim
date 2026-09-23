<script lang="ts">
  // Fork (6.4): the caret half of the split Send button. Three presets, then
  // "Pick…" with a date + time and a free-text line ("3d", "tomorrow 9am",
  // "fri 14:00"). Reports the chosen moment; the composer queues the send.
  import { t } from "../../lib/i18n/index.svelte";
  import { formatWhen, parseWhen, presets, type WhenPick } from "./when";

  let { onpick, disabled = false }: { onpick: (pick: WhenPick) => void; disabled?: boolean } = $props();

  let open = $state(false);
  let picking = $state(false);
  let date = $state("");
  let time = $state("");
  let text = $state("");
  let textEl = $state<HTMLInputElement | null>(null);
  let root = $state<HTMLDivElement | null>(null);

  const now = () => new Date();
  const options = $derived(open ? presets(now()) : []);

  /** What the free text or the date + time resolve to, for the preview line. */
  const resolved = $derived.by(() => {
    if (!picking) return null;
    const n = now();
    if (text.trim()) return parseWhen(text, n);
    if (date && time) {
      const d = new Date(`${date}T${time}:00`);
      return Number.isFinite(d.getTime()) && d > n ? d : null;
    }
    return null;
  });

  function pad(v: number) {
    return v < 10 ? `0${v}` : String(v);
  }

  function toggle() {
    if (disabled) return;
    open = !open;
    picking = false;
    if (open) {
      const n = now();
      date = `${n.getFullYear()}-${pad(n.getMonth() + 1)}-${pad(n.getDate())}`;
      time = "08:00";
      text = "";
    }
  }

  function choose(when: Date) {
    open = false;
    picking = false;
    onpick({ at: Math.floor(when.getTime() / 1000), label: formatWhen(when, now()) });
  }

  function startPick() {
    picking = true;
    queueMicrotask(() => textEl?.focus());
  }

  function confirmPick() {
    const when = resolved;
    if (when) choose(when);
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      open = false;
    } else if (e.key === "Enter" && picking) {
      e.preventDefault();
      e.stopPropagation();
      confirmPick();
    }
  }

  function onWindowPointer(e: PointerEvent) {
    if (open && root && !root.contains(e.target as Node)) open = false;
  }
</script>

<svelte:window onpointerdown={onWindowPointer} />

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="send-later" bind:this={root} onkeydown={onKey}>
  <button
    type="button"
    class="caret"
    {disabled}
    onclick={toggle}
    aria-haspopup="menu"
    aria-expanded={open}
    title={t("fork.compose.send_later")}
    aria-label={t("fork.compose.send_later")}
  >
    <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.3"><path d="M2 3.5l3 3 3-3" /></svg>
  </button>
  {#if open}
    <div class="menu" role="menu">
      <div class="heading microlabel">{t("fork.compose.send_later")}</div>
      {#each options as o (o.key)}
        <button type="button" class="item" role="menuitem" onclick={() => choose(o.when)}>
          <span>{t(`fork.compose.when_${o.key}`)}</span>
          <span class="at">{formatWhen(o.when, now())}</span>
        </button>
      {/each}
      {#if !picking}
        <button type="button" class="item" role="menuitem" onclick={startPick}>{t("fork.compose.when_pick")}</button>
      {:else}
        <div class="pick">
          <input class="free" bind:this={textEl} bind:value={text} placeholder={t("fork.compose.when_placeholder")} spellcheck="false" />
          <div class="row">
            <input type="date" bind:value={date} disabled={!!text.trim()} />
            <input type="time" bind:value={time} disabled={!!text.trim()} />
          </div>
          <div class="row">
            <span class="preview" class:bad={resolved === null}>
              {resolved ? formatWhen(resolved, now()) : t("fork.compose.when_invalid")}
            </span>
            <button type="button" class="go" disabled={resolved === null} onclick={confirmPick}>{t("fork.compose.schedule")}</button>
          </div>
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .send-later {
    position: relative;
    display: flex;
  }
  .caret {
    width: 26px;
    height: 34px;
    display: grid;
    place-items: center;
    border-radius: 0 var(--radius-m) var(--radius-m) 0;
    background: var(--text);
    color: var(--bg);
    border-left: 1px solid color-mix(in srgb, var(--bg) 30%, transparent);
  }
  .caret:hover:not(:disabled) {
    opacity: 0.88;
  }
  .caret:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .menu {
    position: absolute;
    left: 0;
    bottom: calc(100% + 6px);
    min-width: 232px;
    padding: 6px;
    border: 1px solid var(--hairline-strong);
    border-radius: var(--radius-m);
    background: var(--surface);
    box-shadow: var(--shadow-pop);
    z-index: 30;
  }
  .heading {
    padding: 4px 8px 6px;
  }
  .item {
    width: 100%;
    display: flex;
    justify-content: space-between;
    gap: 16px;
    padding: 7px 8px;
    border-radius: var(--radius-s);
    font-size: 13px;
    color: var(--text);
    text-align: left;
  }
  .item:hover {
    background: var(--hover);
  }
  .at {
    white-space: nowrap;
    color: var(--text-faint);
    font-family: var(--font-mono);
    font-size: 11px;
  }
  .pick {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 6px 4px 2px;
    border-top: 1px solid var(--hairline);
    margin-top: 4px;
  }
  .pick input {
    font-size: 12.5px;
    padding: 5px 8px;
    border: 1px solid var(--hairline-strong);
    border-radius: var(--radius-s);
    background: var(--bg);
    color: var(--text);
    user-select: text;
    color-scheme: inherit;
  }
  .pick input:disabled {
    opacity: 0.5;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .row input {
    flex: 1;
    min-width: 0;
  }
  .preview {
    flex: 1;
    font-size: 12px;
    color: var(--text-dim);
  }
  .preview.bad {
    color: var(--text-faint);
  }
  .go {
    padding: 5px 12px;
    border-radius: var(--radius-s);
    background: var(--text);
    color: var(--bg);
    font-size: 12.5px;
    font-weight: 600;
  }
  .go:disabled {
    opacity: 0.45;
    cursor: default;
  }
</style>
