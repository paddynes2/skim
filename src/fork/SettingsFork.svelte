<script lang="ts">
  // Fork settings (PLAN.md 2.6, 2.7, 2.5): list density, avatars, zoom and
  // what happens after archive. Rendered inside Settings.svelte after the
  // upstream toggles; visually the same rows (styles mirrored, not shared,
  // so the upstream file keeps a one-line touch).
  import { t } from "../lib/i18n/index.svelte";
  import { prefs, ZOOM_MAX, ZOOM_MIN, ZOOM_STEP } from "./stores/prefs.svelte";
  import { applyZoom } from "./zoom";
  import { setListOrder } from "./keys";

  const zoomPct = $derived(Math.round(prefs.zoom * 100));

  function setZoom(v: number) {
    void applyZoom(prefs.setZoom(v));
  }
</script>

{#snippet toggleRow(label: string, on: boolean, toggle: () => void)}
  <div class="toggle-row">
    <span class="microlabel">{label}</span>
    <button type="button" class="switch" class:on role="switch" aria-checked={on} aria-label={label} onclick={toggle}>
      <span class="knob"></span>
    </button>
  </div>
{/snippet}

<section class="fork-list">
  <div class="microlabel">{t("fork.settings.list")}</div>

  <div class="field">
    <span class="label">{t("fork.settings.density")}</span>
    <div class="chips" role="radiogroup" aria-label={t("fork.settings.density")}>
      <button class="chip" class:active={prefs.density === "comfortable"} role="radio" aria-checked={prefs.density === "comfortable"} onclick={() => prefs.setDensity("comfortable")}>
        {t("fork.settings.density_comfortable")}
      </button>
      <button class="chip" class:active={prefs.density === "compact"} role="radio" aria-checked={prefs.density === "compact"} onclick={() => prefs.setDensity("compact")}>
        {t("fork.settings.density_compact")}
      </button>
    </div>
  </div>

  {@render toggleRow(t("fork.settings.avatars"), prefs.avatars, () => prefs.setAvatars(!prefs.avatars))}

  <div class="field">
    <span class="label">{t("fork.settings.list_order")}</span>
    <div class="chips" role="radiogroup" aria-label={t("fork.settings.list_order")}>
      {#each ["date", "unread_first"] as const as order (order)}
        <button class="chip" class:active={prefs.listOrder === order} role="radio" aria-checked={prefs.listOrder === order} onclick={() => setListOrder(order)}>
          {t(`fork.settings.list_order_${order}`)}
        </button>
      {/each}
    </div>
  </div>

  <div class="field">
    <span class="label">{t("fork.settings.after_archive")}</span>
    <div class="chips" role="radiogroup" aria-label={t("fork.settings.after_archive")}>
      {#each ["next", "previous", "list"] as const as mode (mode)}
        <button class="chip" class:active={prefs.afterArchive === mode} role="radio" aria-checked={prefs.afterArchive === mode} onclick={() => prefs.setAfterArchive(mode)}>
          {t(`fork.settings.after_archive_${mode}`)}
        </button>
      {/each}
    </div>
  </div>

  <div class="field zoom">
    <label class="label" for="fork-zoom">{t("fork.settings.zoom")}</label>
    <input
      id="fork-zoom"
      type="range"
      min={ZOOM_MIN}
      max={ZOOM_MAX}
      step={ZOOM_STEP}
      value={prefs.zoom}
      oninput={(e) => setZoom(Number((e.currentTarget as HTMLInputElement).value))}
    />
    <span class="pct" aria-live="polite">{zoomPct}%</span>
    <button class="chip" onclick={() => setZoom(1)} disabled={prefs.zoom === 1}>{t("fork.settings.zoom_reset")}</button>
    <span class="hint"><kbd>Ctrl +</kbd> <kbd>Ctrl -</kbd> <kbd>Ctrl 0</kbd></span>
  </div>
</section>

<style>
  .fork-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .field {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 3px 0;
  }
  .field .label {
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 500;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-faint);
    flex: 0 0 120px;
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
  .chip:disabled {
    opacity: 0.4;
    cursor: default;
    background: none;
  }
  .toggle-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 5px 0;
  }
  .switch {
    width: 34px;
    height: 20px;
    border-radius: 999px;
    background: var(--selected);
    position: relative;
    flex-shrink: 0;
    transition: background 0.16s ease;
  }
  .switch .knob {
    position: absolute;
    top: 3px;
    left: 3px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--text-faint);
    transition:
      transform 0.16s ease,
      background 0.16s ease;
  }
  .switch:hover .knob {
    background: var(--text-dim);
  }
  .switch.on {
    background: var(--text);
  }
  .switch.on .knob {
    transform: translateX(14px);
    background: var(--bg);
  }
  .zoom input[type="range"] {
    width: 140px;
    accent-color: var(--text);
  }
  .pct {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-dim);
    min-width: 36px;
  }
  .hint {
    display: flex;
    gap: 4px;
    margin-left: auto;
  }
  .hint kbd {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-faint);
    border: 1px solid var(--hairline-strong);
    border-radius: 4px;
    padding: 1px 5px;
  }
  @media (prefers-reduced-motion: reduce) {
    .switch,
    .switch .knob {
      transition: none;
    }
  }
</style>
