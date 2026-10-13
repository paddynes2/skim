<script lang="ts">
  // Priority settings (v1.1.11, D64): on/off, the TypeSafe key, the
  // threshold and the "never in Priority" list. Mounted from SettingsFork.
  // Styles mirror SettingsCourt's rows.
  import { onMount } from "svelte";
  import { t } from "../../lib/i18n/index.svelte";
  import { api } from "../../lib/api";
  import { priorityApi, type PriorityStatus } from "./api";

  let status = $state<PriorityStatus | null>(null);
  let key = $state("");
  let exclude = $state("");
  let about = $state("");
  let threshold = $state(0.35);
  let busy = $state(false);
  let keyMsg = $state("");
  let keyErr = $state("");
  let saved = $state(false);
  const dirty = $derived(status !== null && (exclude !== status.exclude || threshold !== status.threshold || about !== status.about));

  async function load() {
    try {
      const s = await priorityApi.status();
      status = s;
      exclude = s.exclude;
      threshold = s.threshold;
      about = s.about;
    } catch {
      status = null;
    }
  }
  onMount(() => {
    void load();
  });

  async function toggle() {
    if (!status) return;
    await api.setSetting("fork_priority", status.enabled ? "off" : "on").catch(() => {});
    await priorityApi.recompute().catch(() => {});
    await load();
  }

  async function saveKey(remove = false) {
    busy = true;
    keyErr = "";
    keyMsg = "";
    try {
      await priorityApi.setKey(remove ? "" : key);
      key = "";
      keyMsg = remove ? t("fork.priority.key_removed") : t("fork.priority.key_saved");
      await load();
    } catch (e) {
      keyErr = String(e);
    } finally {
      busy = false;
    }
  }

  async function save() {
    if (!dirty || busy) return;
    busy = true;
    try {
      await api.setSetting("fork_priority_threshold", String(threshold));
      await api.setSetting("fork_priority_exclude", exclude);
      await api.setSetting("fork_priority_about", about.trim());
      await priorityApi.recompute();
      saved = true;
      await load();
    } finally {
      busy = false;
    }
  }
</script>

<section class="priority">
  <div class="microlabel">{t("fork.priority.settings")}</div>
  {#if status}
    <div class="toggle-row">
      <span class="microlabel">{t("fork.priority.settings_on")}</span>
      <button type="button" class="switch" class:on={status.enabled} role="switch" aria-checked={status.enabled} aria-label={t("fork.priority.settings_on")} onclick={() => void toggle()}>
        <span class="knob"></span>
      </button>
    </div>
    <p class="note">{t("fork.priority.note")}</p>

    <div class="field">
      <label class="label" for="fork-priority-key">{t("fork.priority.key")}</label>
      <input
        id="fork-priority-key"
        class="wide"
        type="password"
        autocomplete="off"
        spellcheck="false"
        bind:value={key}
        placeholder={status.keyPresent ? t("fork.priority.key_present") : t("fork.priority.key_missing")}
        disabled={busy}
      />
      <button class="chip" onclick={() => void saveKey()} disabled={busy || key.trim() === ""}>{t("fork.priority.key_save")}</button>
      {#if status.keyPresent}<button class="chip" onclick={() => void saveKey(true)} disabled={busy}>{t("fork.priority.key_remove")}</button>{/if}
    </div>
    {#if keyMsg}<p class="note" role="status">{keyMsg}</p>{/if}
    {#if keyErr}<p class="note error" role="alert">{keyErr}</p>{/if}

    <div class="field">
      <label class="label" for="fork-priority-threshold">{t("fork.priority.threshold")}</label>
      <input id="fork-priority-threshold" type="number" min="0.05" max="0.95" step="0.05" bind:value={threshold} oninput={() => (saved = false)} disabled={busy} />
      <span class="unit">{t("fork.priority.threshold_unit")}</span>
    </div>

    <label class="stack" for="fork-priority-about">
      <span class="label">{t("fork.priority.about")}</span>
      <input id="fork-priority-about" class="wide" type="text" spellcheck="false" bind:value={about} oninput={() => (saved = false)} disabled={busy} placeholder={t("fork.priority.about_placeholder")} />
      <span class="note">{t("fork.priority.about_note")}</span>
    </label>

    <label class="stack" for="fork-priority-exclude">
      <span class="label">{t("fork.priority.exclude")}</span>
      <textarea id="fork-priority-exclude" rows="4" spellcheck="false" bind:value={exclude} oninput={() => (saved = false)} disabled={busy} placeholder={"mail.example.com\nalerts@example.com"}></textarea>
      <span class="note">{t("fork.priority.exclude_note")}</span>
    </label>
    <div class="actions">
      <span class="note" role="status">
        {t("fork.priority.counts", { count: status.count, scored: status.scoredToday, cap: status.cap })}
        {#if saved}{t("fork.priority.saved")}{/if}
      </span>
      <button class="chip save" onclick={() => void save()} disabled={!dirty || busy}>{t("fork.priority.save")}</button>
    </div>
    {#if status.lastError}<p class="note error" role="alert">{t("fork.priority.paused", { error: status.lastError })}</p>{/if}
  {:else}
    <p class="note">{t("fork.priority.not_loaded")}</p>
  {/if}
</section>

<style>
  .priority {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .note {
    font-size: 12px;
    color: var(--text-faint);
    margin: 0;
    line-height: 1.4;
  }
  .error {
    color: var(--danger);
  }
  .field {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 3px 0;
  }
  .label {
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 500;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-faint);
    flex: 0 0 120px;
  }
  .stack {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  input,
  textarea {
    padding: 4px 8px;
    border-radius: var(--radius-s);
    border: 1px solid var(--hairline-strong);
    background: var(--surface);
    color: var(--text);
    font: inherit;
    font-size: 12.5px;
  }
  input[type="number"] {
    width: 72px;
  }
  input.wide {
    flex: 1;
    min-width: 0;
  }
  textarea {
    resize: vertical;
    font-family: var(--font-mono);
    font-size: 12px;
  }
  input:disabled,
  textarea:disabled {
    opacity: 0.4;
  }
  .unit {
    font-size: 12px;
    color: var(--text-dim);
  }
  .actions {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  .chip {
    padding: 5px 11px;
    border-radius: 999px;
    font-size: 12.5px;
    color: var(--text-dim);
    border: 1px solid var(--hairline-strong);
  }
  .chip:hover:not(:disabled) {
    background: var(--hover);
    color: var(--text);
  }
  .chip:disabled {
    opacity: 0.5;
  }
  .chip.save:not(:disabled) {
    background: var(--primary);
    color: var(--on-primary);
    border-color: transparent;
  }
  input:focus-visible,
  textarea:focus-visible,
  button:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
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
    background: var(--primary);
  }
  .switch.on .knob {
    transform: translateX(14px);
    background: var(--bg);
  }
  @media (prefers-reduced-motion: reduce) {
    .switch,
    .switch .knob {
      transition: none;
    }
  }
</style>
