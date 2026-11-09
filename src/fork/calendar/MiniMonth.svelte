<script lang="ts">
  // The month picker: a popup (titlebar date) or, with `inline`, the rail's
  // small month (Look A, D97): today ringed, the visible range shaded.
  import { untrack } from "svelte";
  import { t, getLocale } from "../../lib/i18n/index.svelte";
  import { monthDays } from "./planning";
  import { localDate } from "./guests";
  let { selected, onselect, inline = false, range = null, weekStart = 1 }: {
    selected: Date;
    onselect: (d: Date) => void;
    inline?: boolean;
    /** The days on screen, [start, end) as local dates "YYYY-MM-DD". */
    range?: { start: string; end: string } | null;
    weekStart?: number;
  } = $props();
  let month = $state(untrack(() => new Date(selected.getFullYear(), selected.getMonth(), 1, 12)));
  // Follow the grid: when the selected date leaves the month on show, move to it.
  $effect(() => {
    const s = selected;
    untrack(() => {
      if (s.getFullYear() !== month.getFullYear() || s.getMonth() !== month.getMonth()) month = new Date(s.getFullYear(), s.getMonth(), 1, 12);
    });
  });
  const days = $derived(monthDays(month, weekStart));
  const todayKey = $derived(localDate(new Date()));
  function step(n: number) { month = new Date(month.getFullYear(), month.getMonth() + n, 1, 12); }
  const inRange = (d: Date) => !!range && localDate(d) >= range.start && localDate(d) < range.end;
</script>
<div class="mini-month" class:inline>
  <header>
    <strong>{month.toLocaleDateString(getLocale(), { month: "long", year: "numeric" })}</strong>
    <span class="arrows"><button onclick={() => step(-1)} aria-label={t("fork.cal.prev_month")}>&lsaquo;</button><button onclick={() => step(1)} aria-label={t("fork.cal.next_month")}>&rsaquo;</button></span>
  </header>
  <div class="days">
    {#each days.slice(0, 7) as day}<span class="weekday">{day.toLocaleDateString(getLocale(), { weekday: "narrow" })}</span>{/each}
    {#each days as day, i}
      {@const key = localDate(day)}
      <button
        class:muted={day.getMonth() !== month.getMonth()}
        class:chosen={!inline && key === localDate(selected)}
        class:band={inline && inRange(day)}
        class:band-start={inline && inRange(day) && (i % 7 === 0 || !inRange(days[i - 1]))}
        class:band-end={inline && inRange(day) && (i % 7 === 6 || !inRange(days[i + 1]))}
        class:today={key === todayKey}
        aria-label={day.toLocaleDateString(getLocale(), { dateStyle: "full" })}
        aria-pressed={key === localDate(selected)}
        onclick={() => onselect(day)}><span>{day.getDate()}</span></button>
    {/each}
  </div>
</div>
<style>
  .mini-month { width: 244px; padding: 12px; background: var(--surface-raised); border: 1px solid var(--hairline-strong); border-radius: var(--radius-m); box-shadow: var(--shadow-pop); }
  .mini-month.inline { width: auto; padding: 0; background: transparent; border: 0; box-shadow: none; }
  header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 10px; font-size: 13px; }
  .inline header { padding: 0 2px; }
  .arrows { display: flex; gap: 2px; }
  header button { font-size: 20px; width: 26px; height: 26px; border-radius: var(--radius-s); color: var(--text-dim); }
  .days { display: grid; grid-template-columns: repeat(7, 1fr); row-gap: 2px; text-align: center; }
  .days button, .weekday { min-height: 28px; display: grid; place-items: center; font-size: 12px; font-variant-numeric: tabular-nums; }
  .days button { border-radius: var(--radius-s); }
  .days button span { width: 24px; height: 24px; display: grid; place-items: center; border-radius: 50%; }
  .weekday { color: var(--text-faint); font-size: 11px; }
  button:hover { background: var(--hover); }
  .muted { color: var(--text-faint); }
  .today span { box-shadow: inset 0 0 0 1.5px var(--text); font-weight: 700; }
  .chosen { background: var(--primary); color: var(--on-primary); }
  .band { background: var(--selected); border-radius: 0; }
  .band-start { border-top-left-radius: 99px; border-bottom-left-radius: 99px; }
  .band-end { border-top-right-radius: 99px; border-bottom-right-radius: 99px; }
  :global(:root[data-theme="pastel-light"]) .band { background: var(--p-peri); }
</style>
