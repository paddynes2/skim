<script lang="ts">
  import { untrack } from "svelte";
  import { t } from "../../lib/i18n/index.svelte";
  import { makeRecurrence, WEEKDAYS, type RepeatRule } from "./editor";
  let { value = $bindable<string[]>([]), startDate, allDay, zone, disabled = false, invalid = $bindable(false) }:
    { value?: string[]; startDate: string; allDay: boolean; zone: string; disabled?: boolean; invalid?: boolean } = $props();
  const original = untrack(() => [...value]);
  let mode = $state(original.length ? "keep" : "none");
  const summary = $derived.by(() => {
    const line = value.find((s) => s.startsWith("RRULE:"));
    if (!line) return t("fork.cal.repeat_custom");
    const fields: Record<string, string> = Object.fromEntries(line.slice(6).split(";").map((s) => s.split("=")));
    const unit = fields.FREQ?.toLowerCase();
    if (!["daily", "weekly", "monthly", "yearly"].includes(unit)) return t("fork.cal.repeat_custom");
    let text = t(`fork.cal.repeat_${unit}`);
    if (Number(fields.INTERVAL) > 1) text = t("fork.cal.repeat_interval_summary", { n: fields.INTERVAL, unit: t(`fork.cal.repeat_unit_${unit}`) });
    if (fields.BYDAY) text += ": " + fields.BYDAY.split(",").map((d) => {
      const day = t(`fork.cal.day_${d.slice(-2)}`);
      const order = d.slice(0, -2);
      return order === "-1" ? t("fork.cal.repeat_last_day", { day }) : order ? t(`fork.cal.repeat_nth_${order}`, { day }) : day;
    }).join(", ");
    if (fields.COUNT) text += " / " + t("fork.cal.repeat_count_summary", { n: fields.COUNT });
    if (fields.UNTIL) text += " / " + t("fork.cal.repeat_until_summary", { date: fields.UNTIL.slice(0, 8).replace(/(....)(..)(..)/, "$1-$2-$3") });
    return text;
  });
  let frequency = $state<RepeatRule["frequency"]>("WEEKLY");
  let interval = $state(1);
  let days = $state<string[]>([]);
  let monthly = $state<RepeatRule["monthly"]>("date");
  let end = $state<RepeatRule["end"]>("never");
  let until = $state("");
  let count = $state(10);
  function changed() {
    if (mode === "keep") { value = [...original]; invalid = false; return; }
    if (mode === "none") { value = []; invalid = false; return; }
    const day = WEEKDAYS[new Date(`${startDate}T12:00:00Z`).getUTCDay()];
    const f = mode === "custom" ? frequency : mode === "weekdays" ? "WEEKLY" : mode.toUpperCase() as RepeatRule["frequency"];
    const result = makeRecurrence({ frequency: f, interval: mode === "custom" ? interval : 1,
      days: mode === "weekdays" ? ["MO", "TU", "WE", "TH", "FR"] : mode === "custom" ? days : [day],
      monthly, end, until, count }, startDate, allDay, zone);
    invalid = result === null;
    if (result) value = result;
  }
  function selectMode() {
    if (!days.length) days = [WEEKDAYS[new Date(`${startDate}T12:00:00Z`).getUTCDay()]];
    if (!until) until = startDate;
    changed();
  }
  $effect(() => { void startDate; void allDay; void zone; if (mode !== "keep") changed(); });
</script>
<div class="repeat">
  <label class="field">
    <span>{t("fork.cal.repeat")}</span>
    <select bind:value={mode} onchange={selectMode} {disabled}>
      {#if original.length}<option value="keep">{t("fork.cal.repeat_existing")}</option>{/if}
      {#each ["none", "daily", "weekdays", "weekly", "monthly", "yearly", "custom"] as choice}
        <option value={choice}>{t(`fork.cal.repeat_${choice}`)}</option>
      {/each}
    </select>
  </label>
  {#if mode === "keep" && value.length}
    <p class="rule">{summary}</p>
  {/if}
  {#if mode === "custom"}
    <div class="pair">
      <label class="field"><span>{t("fork.cal.repeat_every")}</span><input type="number" min="1" max="999" bind:value={interval} onchange={changed} {disabled} /></label>
      <label class="field"><span>{t("fork.cal.repeat_unit")}</span><select bind:value={frequency} onchange={changed} {disabled}>
        {#each ["DAILY", "WEEKLY", "MONTHLY", "YEARLY"] as f}<option value={f}>{t(`fork.cal.repeat_${f.toLowerCase()}`)}</option>{/each}
      </select></label>
    </div>
    {#if frequency === "WEEKLY"}
      <div class="weekdays" aria-label={t("fork.cal.repeat_days")}>
        {#each ["MO", "TU", "WE", "TH", "FR", "SA", "SU"] as d}
          <button type="button" aria-pressed={days.includes(d)} {disabled} onclick={() => { days = days.includes(d) ? days.filter((v) => v !== d) : [...days, d]; changed(); }}>{t(`fork.cal.day_${d}`)}</button>
        {/each}
      </div>
    {/if}
    {#if frequency === "MONTHLY"}
      <label class="field"><span>{t("fork.cal.repeat_on")}</span><select bind:value={monthly} onchange={changed} {disabled}>
        <option value="date">{t("fork.cal.repeat_month_date")}</option><option value="weekday">{t("fork.cal.repeat_month_weekday")}</option>
      </select></label>
    {/if}
  {/if}
  {#if mode !== "keep" && mode !== "none"}
    <div class="pair">
      <label class="field"><span>{t("fork.cal.repeat_ends")}</span><select bind:value={end} onchange={changed} {disabled}>
        <option value="never">{t("fork.cal.repeat_never")}</option><option value="date">{t("fork.cal.repeat_until")}</option><option value="count">{t("fork.cal.repeat_count")}</option>
      </select></label>
      {#if end === "date"}<label class="field"><span>{t("fork.cal.repeat_until")}</span><input type="date" min={startDate} bind:value={until} onchange={changed} {disabled} /></label>{/if}
      {#if end === "count"}<label class="field"><span>{t("fork.cal.occurrences")}</span><input type="number" min="1" max="9999" bind:value={count} onchange={changed} {disabled} /></label>{/if}
    </div>
  {/if}
  {#if invalid}<p class="error" role="alert">{t("fork.cal.repeat_invalid")}</p>{/if}
</div>
<style>
.repeat { display: grid; gap: 12px; }
.field { display: grid; gap: 6px; min-width: 0; font-size: 12px; color: var(--text-dim); }
input, select { width: 100%; min-width: 0; padding: 8px; border: 1px solid var(--hairline-strong); border-radius: var(--radius-s); background: var(--surface-raised); color: var(--text); font: inherit; }
.pair { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
.weekdays { display: flex; gap: 4px; }
.weekdays button { flex: 1; padding: 7px 2px; font-size: 11px; border: 1px solid var(--hairline); border-radius: var(--radius-s); }
.weekdays button[aria-pressed="true"] { background: var(--selected); border-color: var(--primary); }
.rule { font-size: 11px; color: var(--text-dim); overflow-wrap: anywhere; }
.error { font-size: 12px; color: var(--danger); }
</style>
