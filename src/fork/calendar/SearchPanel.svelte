<script lang="ts">
  // Event search results (D97), shown in place of the grid while a query is
  // open: upcoming first, then past (newest first), grouped by day. Cached
  // events open on the grid; a hit Google found outside the synced window
  // opens read-only.
  import { getLocale, t } from "../../lib/i18n/index.svelte";
  import { EVENT_COLORS } from "./editor";
  import { localDate } from "./guests";
  import { calendar } from "./store.svelte";
  import type { SearchHit } from "./types";

  let { onpick }: { onpick: (hit: SearchHit) => void } = $props();

  const now = Math.floor(Date.now() / 1000);
  const groups = $derived.by(() => {
    const hits = calendar.search.result?.hits ?? [];
    const upcoming = hits.filter((h) => h.end_ts >= now).sort((a, b) => a.start_ts - b.start_ts);
    const past = hits.filter((h) => h.end_ts < now).sort((a, b) => b.start_ts - a.start_ts);
    const byDay = (list: SearchHit[]) => {
      const out: { key: string; date: Date; hits: SearchHit[] }[] = [];
      for (const h of list) {
        const d = h.all_day && h.start_date ? new Date(`${h.start_date}T12:00:00`) : new Date(h.start_ts * 1000);
        const key = localDate(d);
        const last = out.at(-1);
        if (last?.key === key) last.hits.push(h);
        else out.push({ key, date: d, hits: [h] });
      }
      return out;
    };
    return { upcoming: byDay(upcoming), past: byDay(past) };
  });
  const color = (h: SearchHit) => EVENT_COLORS[Number(h.options?.colorId)] || calendar.calendarOf(h)?.color || "var(--acct-1)";
  const time = (h: SearchHit) => (h.all_day ? t("fork.cal.all_day") : new Date(h.start_ts * 1000).toLocaleTimeString(getLocale(), { hour: "2-digit", minute: "2-digit", hour12: false }));
</script>

<div class="results" aria-live="polite">
  {#if calendar.search.busy && !calendar.search.result}
    <p class="note">{t("fork.cal.searching")}</p>
  {:else if calendar.search.result}
    {#if calendar.search.result.remote_error}<p class="note">{t("fork.cal.search_local_only")}</p>{/if}
    {#if !calendar.search.result.hits.length}
      <p class="note">{t("fork.cal.search_none", { q: calendar.search.query })}</p>
    {/if}
    {#each [["fork.cal.upcoming", groups.upcoming], ["fork.cal.past", groups.past]] as const as [label, list] (label)}
      {#if list.length}
        <h3>{t(label)}</h3>
        {#each list as g (g.key)}
          <div class="day">
            <div class="date"><strong>{g.date.getDate()}</strong><span>{g.date.toLocaleDateString(getLocale(), { month: "short", year: g.date.getFullYear() === new Date().getFullYear() ? undefined : "numeric", weekday: "short" })}</span></div>
            <ul>
              {#each g.hits as h (`${h.calendar_id}:${h.google_id}`)}
                <li><button onclick={() => onpick(h)}><span class="dot" style:background={color(h)}></span><span class="t">{time(h)}</span><span class="s">{h.summary || t("fork.cal.untitled")}</span>{#if h.location}<span class="l">{h.location}</span>{/if}</button></li>
              {/each}
            </ul>
          </div>
        {/each}
      {/if}
    {/each}
  {:else}
    <p class="note">{t("fork.cal.search_hint")}</p>
  {/if}
</div>

<style>
  .results { height: 100%; overflow-y: auto; padding: 8px 24px 28px; }
  .note { color: var(--text-dim); font-size: 13px; margin: 18px 0; }
  h3 { font-size: 11px; font-weight: 600; letter-spacing: 0.06em; text-transform: uppercase; color: var(--text-faint); margin: 22px 0 8px; }
  .day { display: grid; grid-template-columns: 92px 1fr; gap: 12px; padding: 8px 0; border-top: 1px solid var(--hairline); }
  .date { display: flex; align-items: baseline; gap: 6px; }
  .date strong { font-size: 20px; font-weight: 650; font-variant-numeric: tabular-nums; }
  .date span { font-size: 11.5px; color: var(--text-dim); }
  ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 2px; }
  li button { width: 100%; display: grid; grid-template-columns: 10px 64px minmax(0, 1fr) minmax(0, 0.6fr); align-items: center; gap: 10px; padding: 7px 8px; border-radius: var(--radius-s); text-align: left; font-size: 13px; }
  li button:hover { background: var(--hover); }
  .dot { width: 10px; height: 10px; border-radius: 50%; }
  .t { font-family: var(--font-mono); font-size: 11.5px; color: var(--text-dim); }
  .s { font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .l { color: var(--text-faint); font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
