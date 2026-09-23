<script lang="ts">
  // Court settings (PLAN.md Phase 10): the AI pass and its day cap, the row
  // colour thresholds, the daily nudge time. Mounted from SettingsFork.svelte
  // (one line, pending/10-ui.md). Styles mirror SettingsFork's rows.
  import { t } from "../../lib/i18n/index.svelte";
  import { showCourtNudge } from "./CourtNudge";
  import { courtPrefs } from "./prefs.svelte";

  void courtPrefs.load();

  const nudgeOn = $derived(courtPrefs.nudge.toLowerCase() !== "off");
  // The time the input shows while the nudge is off (so switching it back on
  // lands on a sensible hour, not an empty box).
  let lastTime = $state("09:00");
  $effect(() => {
    if (nudgeOn) lastTime = courtPrefs.nudge;
  });

  function toggleNudge() {
    courtPrefs.setNudge(nudgeOn ? "off" : lastTime);
  }

  function num(e: Event): number {
    return Number((e.currentTarget as HTMLInputElement).value);
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

<section class="court">
  <div class="microlabel">{t("fork.court.settings")}</div>

  {@render toggleRow(t("fork.court.settings_ai"), courtPrefs.ai, () => courtPrefs.setAi(!courtPrefs.ai))}
  <p class="note">{t("fork.court.settings_ai_note")}</p>

  <div class="field">
    <label class="label" for="fork-court-cap">{t("fork.court.settings_ai_cap")}</label>
    <input
      id="fork-court-cap"
      type="number"
      min="0"
      step="10"
      value={courtPrefs.aiCap}
      disabled={!courtPrefs.ai}
      onchange={(e) => courtPrefs.setAiCap(num(e))}
    />
    <span class="unit">{t("fork.court.settings_ai_cap_unit")}</span>
  </div>

  <div class="field">
    <span class="label">{t("fork.court.settings_colours")}</span>
    <label class="inline">
      <span class="swatch amber"></span>
      <input type="number" min="0" value={courtPrefs.amberDays} aria-label={t("fork.court.settings_amber")} onchange={(e) => courtPrefs.setAmberDays(num(e))} />
      <span class="unit">{t("fork.court.settings_days")}</span>
    </label>
    <label class="inline">
      <span class="swatch red"></span>
      <input type="number" min="0" value={courtPrefs.redDays} aria-label={t("fork.court.settings_red")} onchange={(e) => courtPrefs.setRedDays(num(e))} />
      <span class="unit">{t("fork.court.settings_days")}</span>
    </label>
  </div>

  {@render toggleRow(t("fork.court.settings_nudge"), nudgeOn, toggleNudge)}
  <div class="field">
    <label class="label" for="fork-court-nudge">{t("fork.court.settings_nudge_at")}</label>
    <input
      id="fork-court-nudge"
      type="time"
      value={nudgeOn ? courtPrefs.nudge : lastTime}
      disabled={!nudgeOn}
      onchange={(e) => {
        const v = (e.currentTarget as HTMLInputElement).value;
        if (v) courtPrefs.setNudge(v);
      }}
    />
    <button class="chip" onclick={() => void showCourtNudge()}>{t("fork.court.settings_nudge_test")}</button>
  </div>
</section>

<style>
  .court {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .note {
    font-size: 12px;
    color: var(--text-faint);
    margin: -4px 0 2px;
    line-height: 1.4;
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
  .inline {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  input[type="number"],
  input[type="time"] {
    width: 72px;
    padding: 4px 8px;
    border-radius: var(--radius-s);
    border: 1px solid var(--hairline-strong);
    background: var(--surface);
    color: var(--text);
    font: inherit;
    font-size: 12.5px;
  }
  input[type="time"] {
    width: auto;
  }
  input:disabled {
    opacity: 0.4;
  }
  .unit {
    font-size: 12px;
    color: var(--text-dim);
  }
  .swatch {
    width: 10px;
    height: 10px;
    border-radius: 50%;
  }
  .swatch.amber {
    background: var(--acct-2);
  }
  .swatch.red {
    background: var(--danger);
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
  @media (prefers-reduced-motion: reduce) {
    .switch,
    .switch .knob {
      transition: none;
    }
  }
</style>
