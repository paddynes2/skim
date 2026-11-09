<script lang="ts">
  // Time insights (D97), Google Calendar's "Time insights" in the rail: hours
  // in meetings in the week on screen against the week before, a bar per day,
  // and the people met most. A meeting is a timed event with at least one other
  // guest that the user has not declined and that is not free time,
  // out-of-office, focus time or a working location. Read from the local cache.
  import { getLocale, t } from "../../lib/i18n/index.svelte";
  import { calendarApi } from "./api";
  import { otherGuests } from "./guests";
  import { calPrefs } from "./settings.svelte";
  import { calendar } from "./store.svelte";
  import type { EventRow } from "./types";

  let open = $state(true);
  let rows = $state<EventRow[]>([]);

  const weekStart = $derived.by(() => {
    const d = new Date(calendar.date);
    d.setHours(0, 0, 0, 0);
    d.setDate(d.getDate() - ((d.getDay() - calPrefs.weekStart + 7) % 7));
    return d;
  });

  $effect(() => {
    const from = weekStart.getTime() / 1000 - 7 * 86400;
    const to = weekStart.getTime() / 1000 + 7 * 86400;
    void calendar.events.length; // refresh after a sync
    let live = true;
    calendarApi.events(from, to).then((r) => { if (live) rows = r; }).catch(() => { if (live) rows = []; });
    return () => { live = false; };
  });

  function isMeeting(r: EventRow): boolean {
    if (r.all_day || r.status === "cancelled" || r.self_response === "declined") return false;
    if (r.transparency === "transparent" || r.options?.transparency === "transparent") return false;
    if (r.event_type && r.event_type !== "default") return false;
    if (calPrefs.hiddenCals.includes(r.calendar_id)) return false;
    return otherGuests(r, calendar.ownEmails).length > 0;
  }

  const stats = $derived.by(() => {
    const start = weekStart.getTime() / 1000;
    const seen = new Set<string>();
    const days = Array.from({ length: 7 }, () => 0);
    let now = 0, before = 0;
    const people = new Map<string, { name: string; n: number }>();
    for (const r of rows) {
      if (!isMeeting(r)) continue;
      const key = r.ical_uid ? `${r.ical_uid}|${r.start_ts}` : `${r.calendar_id}|${r.google_id}`;
      if (seen.has(key)) continue;
      seen.add(key);
      const h = (r.end_ts - r.start_ts) / 3600;
      if (r.start_ts >= start && r.start_ts < start + 7 * 86400) {
        now += h;
        days[Math.floor((r.start_ts - start) / 86400)] += h;
        for (const g of otherGuests(r, calendar.ownEmails)) {
          const p = people.get(g.email) ?? { name: g.displayName || g.email.split("@")[0], n: 0 };
          p.n++;
          people.set(g.email, p);
        }
      } else if (r.start_ts >= start - 7 * 86400 && r.start_ts < start) before += h;
    }
    const max = Math.max(1, ...days);
    const busiest = days.indexOf(Math.max(...days));
    return { now, before, days, max, busiest: days[busiest] > 0 ? busiest : -1, people: [...people.values()].sort((a, b) => b.n - a.n).slice(0, 3) };
  });
  const hrs = (h: number) => (Math.round(h * 10) / 10).toLocaleString(getLocale());
  const dayName = (i: number) => new Date(weekStart.getTime() + i * 86400_000).toLocaleDateString(getLocale(), { weekday: "narrow" });
  const dayLong = (i: number) => new Date(weekStart.getTime() + i * 86400_000).toLocaleDateString(getLocale(), { weekday: "long" });
</script>

<section class="insights">
  <button class="head" aria-expanded={open} onclick={() => (open = !open)}>
    <span>{t("fork.cal.insights")}</span><span class="chev" aria-hidden="true">{open ? "−" : "+"}</span>
  </button>
  {#if open}
    <p class="big"><strong>{hrs(stats.now)} h</strong> {t("fork.cal.in_meetings")}</p>
    <p class="meta">{t("fork.cal.last_week_h", { h: hrs(stats.before) })}</p>
    <div class="bars" role="img" aria-label={t("fork.cal.insights_bars", { h: hrs(stats.now) })}>
      {#each stats.days as h, i}
        <div class="bar" title={`${dayLong(i)}: ${hrs(h)} h`}><span style:height="{(h / stats.max) * 100}%" class:top={i === stats.busiest}></span><em>{dayName(i)}</em></div>
      {/each}
    </div>
    {#if stats.busiest >= 0}<p class="meta">{t("fork.cal.busiest", { day: dayLong(stats.busiest) })}</p>{/if}
    {#if stats.people.length}
      <p class="meta label">{t("fork.cal.met_most")}</p>
      <ul>{#each stats.people as p (p.name)}<li><span>{p.name}</span><em>{p.n}</em></li>{/each}</ul>
    {/if}
  {/if}
</section>

<style>
  .insights { margin-top: auto; padding-top: 12px; border-top: 1px solid var(--hairline); }
  .head { width: 100%; display: flex; justify-content: space-between; align-items: center; font-size: 12.5px; font-weight: 600; padding: 2px 2px 8px; }
  .chev { color: var(--text-faint); }
  p { margin: 0 2px; font-size: 12px; }
  .big { font-size: 12.5px; color: var(--text-dim); }
  .big strong { font-size: 18px; color: var(--text); font-weight: 650; margin-right: 2px; }
  .meta { color: var(--text-faint); margin-top: 2px; }
  .label { margin-top: 10px; }
  .bars { display: grid; grid-template-columns: repeat(7, 1fr); gap: 5px; height: 64px; margin: 10px 2px 6px; align-items: end; }
  .bar { height: 100%; display: flex; flex-direction: column; justify-content: flex-end; align-items: center; gap: 3px; }
  .bar span { width: 100%; min-height: 2px; border-radius: 3px 3px 1px 1px; background: var(--hairline-strong); opacity: 0.45; }
  .bar span.top { opacity: 1; background: var(--text); }
  .bar em { font-style: normal; font-size: 10px; color: var(--text-faint); }
  ul { list-style: none; margin: 4px 0 0; padding: 0 2px; display: grid; gap: 3px; }
  li { display: flex; justify-content: space-between; font-size: 12px; }
  li span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  li em { font-style: normal; color: var(--text-faint); font-family: var(--font-mono); font-size: 11px; }
  :global(:root[data-theme="pastel-light"]) .bar span { background: var(--p-peri); opacity: 1; box-shadow: inset 0 0 0 1px var(--ink); }
  :global(:root[data-theme="pastel-light"]) .bar span.top { background: var(--p-mint-2); }
</style>
