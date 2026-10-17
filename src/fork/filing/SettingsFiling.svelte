<script lang="ts">
  // Filing settings (v1.1.24, D72): the mode, the folder lines Jev reads, the
  // Never-file list, the bar, and what would move right now. Mounted from
  // SettingsFork under Priority; styles mirror SettingsPriority.
  import { onMount } from "svelte";
  import { t, getLocale } from "../../lib/i18n/index.svelte";
  import { api } from "../../lib/api";
  import { filingApi, type FilingMode, type FilingPreview, type FilingStatus } from "./api";

  let status = $state<FilingStatus | null>(null);
  let preview = $state<FilingPreview[]>([]);
  let mode = $state<FilingMode>("off");
  let rules = $state("");
  let never = $state("");
  let threshold = $state(0.7);
  let busy = $state(false);
  let saved = $state(false);
  const dirty = $derived(
    status !== null &&
      (mode !== status.mode || rules !== status.rules || never !== status.never || threshold !== status.threshold),
  );

  async function load() {
    try {
      const s = await filingApi.status();
      status = s;
      mode = s.mode;
      rules = s.rules;
      never = s.never;
      threshold = s.threshold;
    } catch {
      status = null;
    }
    preview = await filingApi.preview().catch(() => []);
  }
  onMount(() => {
    void load();
  });

  async function save() {
    if (!dirty || busy) return;
    busy = true;
    try {
      await api.setSetting("fork_filing", rules);
      await api.setSetting("fork_filing_never", never);
      await api.setSetting("fork_filing_threshold", String(threshold));
      await api.setSetting("fork_filing_mode", mode);
      await api.setSetting("fork_filing_paused", "");
      saved = true;
      await load();
    } finally {
      busy = false;
    }
  }

  function day(unix: number) {
    return new Date(unix * 1000).toLocaleDateString(getLocale(), { day: "numeric", month: "short" });
  }
</script>

<section class="filing">
  <div class="microlabel">{t("fork.filing.settings")}</div>
  {#if status}
    <div class="field">
      <span class="label">{t("fork.filing.mode")}</span>
      <div class="modes" role="radiogroup" aria-label={t("fork.filing.mode")}>
        {#each ["off", "preview", "on"] as const as m}
          <button type="button" role="radio" aria-checked={mode === m} class="chip" class:active={mode === m} onclick={() => { mode = m; saved = false; }} disabled={busy}>{t(`fork.filing.mode_${m}`)}</button>
        {/each}
      </div>
    </div>
    <p class="note">{t("fork.filing.note")}</p>
    {#if status.paused}<p class="note error" role="alert">{status.paused}</p>{/if}

    <label class="stack" for="fork-filing-rules">
      <span class="label">{t("fork.filing.rules")}</span>
      <textarea id="fork-filing-rules" rows="4" spellcheck="false" bind:value={rules} oninput={() => (saved = false)} disabled={busy} placeholder={"Receipts: order confirmations, invoices and payment receipts from shops and services"}></textarea>
      <span class="note">{t("fork.filing.rules_note")}</span>
    </label>
    {#each status.ignored as line}<p class="note error">{t("fork.filing.ignored", { line })}</p>{/each}

    <label class="stack" for="fork-filing-never">
      <span class="label">{t("fork.filing.never")}</span>
      <textarea id="fork-filing-never" rows="2" spellcheck="false" bind:value={never} oninput={() => (saved = false)} disabled={busy} placeholder={"alerts.example.com\nperson@example.com"}></textarea>
      <span class="note">{t("fork.filing.never_note")}</span>
    </label>

    <div class="field">
      <label class="label" for="fork-filing-threshold">{t("fork.filing.threshold")}</label>
      <input id="fork-filing-threshold" type="number" min="0.5" max="0.95" step="0.05" bind:value={threshold} oninput={() => (saved = false)} disabled={busy} />
      <span class="unit">{t("fork.filing.threshold_unit")}</span>
    </div>

    <div class="actions">
      <span class="note" role="status">
        {t("fork.filing.counts", { scored: status.scoredToday, cap: status.cap, filed: status.filedToday })}
        {#if saved}{t("fork.filing.saved")}{/if}
      </span>
      <button class="chip save" onclick={() => void save()} disabled={!dirty || busy}>{t("fork.filing.save")}</button>
    </div>

    {#if status.mode !== "off"}
      <div class="label">{t("fork.filing.preview_heading", { n: preview.length })}</div>
      {#if preview.length === 0}
        <p class="note">{t("fork.filing.preview_empty")}</p>
      {:else}
        <ul class="preview">
          {#each preview as p (p.threadId)}
            <li><span class="folder">{p.folder}</span><span class="who">{p.fromName || p.fromAddr}</span><span class="subject">{p.subject}</span><span class="meta">{day(p.date)} &middot; {p.score.toFixed(2)}</span></li>
          {/each}
        </ul>
      {/if}
    {/if}
  {:else}
    <p class="note">{t("fork.filing.not_loaded")}</p>
  {/if}
</section>

<style>
  .filing {
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
  .modes {
    display: flex;
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
  .chip.active {
    background: var(--selected);
    color: var(--text);
  }
  .chip.save:not(:disabled) {
    background: var(--primary);
    color: var(--on-primary);
    border-color: transparent;
  }
  .preview {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 280px;
    overflow: auto;
    border: 1px solid var(--hairline);
    border-radius: var(--radius-s);
  }
  .preview li {
    display: grid;
    grid-template-columns: 96px 150px minmax(0, 1fr) auto;
    gap: 10px;
    padding: 6px 10px;
    border-bottom: 1px solid var(--hairline);
    font-size: 12px;
  }
  .preview li:last-child {
    border-bottom: 0;
  }
  .folder,
  .meta {
    color: var(--text-dim);
    white-space: nowrap;
  }
  .who,
  .subject {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  input:focus-visible,
  textarea:focus-visible,
  button:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
  }
</style>
