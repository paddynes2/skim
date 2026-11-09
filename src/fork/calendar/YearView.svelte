<script lang="ts">
  // Year view (D97): twelve month grids. A dot marks a day with a timed or
  // all-day event in the synced window (60 days back, 180 ahead); a click
  // opens that day.
  import { getLocale, t } from "../../lib/i18n/index.svelte";
  import { localDate } from "./guests";
  import { monthDays } from "./planning";
  import type { EventRow } from "./types";

  let { year, events, weekStart = 1, onday }: { year: number; events: EventRow[]; weekStart?: number; onday: (d: Date) => void } = $props();

  const months = $derived(Array.from({ length: 12 }, (_, m) => new Date(year, m, 1, 12)));
  const busy = $derived.by(() => {
    const set = new Set<string>();
    for (const e of events) {
      if (e.status === "cancelled") continue;
      if (e.all_day && e.start_date) { set.add(e.start_date); continue; }
      set.add(localDate(new Date(e.start_ts * 1000)));
    }
    return set;
  });
  const todayKey = localDate(new Date());
</script>

<div class="year" aria-label={t("fork.cal.view_year")}>
  {#each months as month (month.getMonth())}
    {@const days = monthDays(month, weekStart)}
    <section class="month">
      <h3>{month.toLocaleDateString(getLocale(), { month: "long" })}</h3>
      <div class="grid">
        {#each days.slice(0, 7) as d}<span class="wd">{d.toLocaleDateString(getLocale(), { weekday: "narrow" })}</span>{/each}
        {#each days as d}
          {@const key = localDate(d)}
          {#if d.getMonth() === month.getMonth()}
            <button class:today={key === todayKey} class:busy={busy.has(key)} aria-label={d.toLocaleDateString(getLocale(), { dateStyle: "full" })} onclick={() => onday(d)}><span>{d.getDate()}</span></button>
          {:else}
            <span class="pad"></span>
          {/if}
        {/each}
      </div>
    </section>
  {/each}
</div>

<style>
  .year { height: 100%; overflow-y: auto; display: grid; grid-template-columns: repeat(auto-fill, minmax(232px, 1fr)); gap: 28px 32px; padding: 18px 24px 28px; align-content: start; }
  h3 { margin: 0 0 8px 4px; font-size: 14px; font-weight: 650; }
  .grid { display: grid; grid-template-columns: repeat(7, 1fr); row-gap: 2px; text-align: center; }
  .wd { font-size: 11px; color: var(--text-faint); padding-bottom: 4px; }
  button, .pad { height: 30px; display: grid; place-items: center; position: relative; font-size: 12px; font-variant-numeric: tabular-nums; }
  button { border-radius: var(--radius-s); }
  button span { width: 26px; height: 26px; display: grid; place-items: center; border-radius: 50%; }
  button:hover span { background: var(--hover); }
  .busy::after { content: ""; position: absolute; bottom: 1px; width: 4px; height: 4px; border-radius: 50%; background: var(--text-dim); }
  .today span { background: var(--text); color: var(--bg); font-weight: 700; }
  .today:hover span { background: var(--text); }
</style>
