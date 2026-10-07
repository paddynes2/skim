<script lang="ts">
  import { untrack } from "svelte";
  import { t, getLocale } from "../../lib/i18n/index.svelte";
  import { monthDays } from "./planning";
  import { localDate } from "./guests";
  let { selected, onselect }: { selected: Date; onselect: (d: Date) => void } = $props();
  let month = $state(untrack(() => new Date(selected.getFullYear(), selected.getMonth(), 1, 12)));
  const days = $derived(monthDays(month));
  function step(n: number) { month = new Date(month.getFullYear(), month.getMonth() + n, 1, 12); }
</script>
<div class="mini-month">
  <header><button onclick={() => step(-1)} aria-label={t("fork.cal.prev_month")}>&lsaquo;</button><strong>{month.toLocaleDateString(getLocale(), { month: "long", year: "numeric" })}</strong><button onclick={() => step(1)} aria-label={t("fork.cal.next_month")}>&rsaquo;</button></header>
  <div class="days">
    {#each days.slice(0, 7) as day}<span class="weekday">{day.toLocaleDateString(getLocale(), { weekday: "narrow" })}</span>{/each}
    {#each days as day}
      <button class:muted={day.getMonth() !== month.getMonth()} class:chosen={localDate(day) === localDate(selected)} class:today={localDate(day) === localDate(new Date())} aria-label={day.toLocaleDateString(getLocale(), { dateStyle: "full" })} aria-pressed={localDate(day) === localDate(selected)} onclick={() => onselect(day)}>{day.getDate()}</button>
    {/each}
  </div>
</div>
<style>
  .mini-month { width: 244px; padding: 12px; background: var(--surface-raised); border: 1px solid var(--hairline-strong); border-radius: var(--radius-m); box-shadow: var(--shadow-pop); }
  header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 12px; font-size: 13px; }
  header button { font-size: 22px; width: 26px; height: 26px; }
  .days { display: grid; grid-template-columns: repeat(7, 1fr); gap: 3px; text-align: center; }
  .days button, .weekday { min-height: 28px; display: grid; place-items: center; font-size: 12px; border-radius: var(--radius-s); }
  .weekday { color: var(--text-dim); }
  button:hover { background: var(--hover); }
  .muted { color: var(--text-faint); }
  .today { border: 1px solid var(--primary); }
  .chosen { background: var(--primary); color: var(--on-primary); }
</style>
