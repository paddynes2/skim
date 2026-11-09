<script lang="ts">
  // The calendar rail (Look A, D97): the small month and the calendar list.
  // A tick here hides a calendar from the grid only. It keeps syncing, keeps
  // its meeting alerts and still counts as busy time. A calendar that is not
  // synced at all (unticked under Settings, Calendars shown) is greyed, and
  // ticking it starts syncing it again.
  import { getLocale, t } from "../../lib/i18n/index.svelte";
  import { mail } from "../../lib/stores/mail.svelte";
  import { toast } from "../stores/toast.svelte";
  import MiniMonth from "./MiniMonth.svelte";
  import { calendarErrorText } from "./guests";
  import { calPrefs } from "./settings.svelte";
  import { calendar } from "./store.svelte";
  import type { CalendarRow } from "./types";

  let { range }: { range: { start: string; end: string } | null } = $props();

  const groups = $derived.by(() => {
    const byAccount = new Map<string, CalendarRow[]>();
    for (const c of calendar.calendars) {
      const list = byAccount.get(c.account_id) ?? [];
      list.push(c);
      byAccount.set(c.account_id, list);
    }
    return [...byAccount.entries()].map(([id, cals]) => ({
      id,
      email: mail.accounts.find((a) => a.id === id)?.email ?? "",
      cals: cals.sort((a, b) => Number(b.is_primary) - Number(a.is_primary) || Number(b.selected) - Number(a.selected) || a.summary.localeCompare(b.summary, getLocale())),
    }));
  });

  let menuFor = $state<number | null>(null);
  let busy = $state<number | null>(null);

  const shown = (c: CalendarRow) => c.selected && !calPrefs.hiddenCals.includes(c.id);

  async function toggle(c: CalendarRow) {
    if (!c.selected) {
      busy = c.id;
      try { await calendar.setVisible(c.id, true); } catch (e) { toast.show({ text: calendarErrorText(e) }); } finally { busy = null; }
      calPrefs.setHiddenCals(calPrefs.hiddenCals.filter((x) => x !== c.id));
      return;
    }
    const hidden = calPrefs.hiddenCals.includes(c.id);
    calPrefs.setHiddenCals(hidden ? calPrefs.hiddenCals.filter((x) => x !== c.id) : [...calPrefs.hiddenCals, c.id]);
  }
  function only(c: CalendarRow) {
    menuFor = null;
    calPrefs.setHiddenCals(calendar.calendars.filter((x) => x.id !== c.id && x.selected).map((x) => x.id));
  }
  function showAll() {
    menuFor = null;
    calPrefs.setHiddenCals([]);
  }
</script>

<svelte:window onclick={(e) => { if (menuFor !== null && !(e.target as HTMLElement)?.closest?.(".cal-menu, .more")) menuFor = null; }} />

<aside class="rail" aria-label={t("fork.cal.rail")}>
  <MiniMonth inline selected={calendar.date} {range} weekStart={calPrefs.weekStart} onselect={(d) => calendar.goto(d)} />

  <section class="list">
    <h2>{t("fork.cal.calendars")}</h2>
    {#each groups as g (g.id)}
      {#if groups.length > 1}<p class="account">{g.email}</p>{/if}
      <ul>
        {#each g.cals as c (c.id)}
          <li class:off={!shown(c)} class:unsynced={!c.selected}>
            <label title={c.selected ? c.summary : t("fork.cal.not_synced")}>
              <input type="checkbox" checked={shown(c)} disabled={busy !== null} onchange={() => toggle(c)} style:--cal-color={c.color || "var(--primary)"} />
              <span class="name">{c.summary}</span>
            </label>
            {#if c.selected}
              <button class="more" aria-label={t("fork.cal.calendar_options", { name: c.summary })} aria-expanded={menuFor === c.id} onclick={() => (menuFor = menuFor === c.id ? null : c.id)}>&middot;&middot;&middot;</button>
              {#if menuFor === c.id}
                <div class="cal-menu" role="menu">
                  <button role="menuitem" onclick={() => only(c)}>{t("fork.cal.show_only_this")}</button>
                  <button role="menuitem" onclick={showAll}>{t("fork.cal.show_all")}</button>
                </div>
              {/if}
            {/if}
          </li>
        {/each}
      </ul>
    {/each}
  </section>
</aside>

<style>
  .rail { width: 232px; flex-shrink: 0; display: flex; flex-direction: column; gap: 22px; padding: 16px 14px; border-right: 1px solid var(--hairline); overflow-y: auto; background: var(--bg); }
  h2 { font-size: 12.5px; font-weight: 600; margin: 0 0 8px 2px; color: var(--text); }
  .account { font-family: var(--font-mono); font-size: 10.5px; color: var(--text-faint); margin: 10px 2px 4px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 1px; }
  li { position: relative; display: flex; align-items: center; border-radius: var(--radius-s); }
  li:hover { background: var(--hover); }
  label { flex: 1; min-width: 0; display: flex; align-items: center; gap: 9px; padding: 5px 4px; font-size: 12.5px; cursor: pointer; }
  .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  input { appearance: none; flex-shrink: 0; width: 15px; height: 15px; margin: 0; border-radius: 4px; border: 1.5px solid var(--cal-color); background: var(--cal-color); display: grid; place-items: center; cursor: pointer; }
  input::after { content: ""; width: 7px; height: 4px; border-left: 1.8px solid #fff; border-bottom: 1.8px solid #fff; transform: translateY(-1px) rotate(-45deg); }
  input:not(:checked) { background: transparent; }
  input:not(:checked)::after { display: none; }
  input:focus-visible { outline: 2px solid var(--focus); outline-offset: 2px; }
  .off .name { color: var(--text-dim); }
  .unsynced .name { color: var(--text-faint); }
  .unsynced input { border-color: var(--hairline-strong); opacity: 0.5; }
  .more { width: 24px; height: 24px; border-radius: var(--radius-s); color: var(--text-faint); font-size: 11px; letter-spacing: 1px; opacity: 0; }
  li:hover .more, .more:focus-visible, .more[aria-expanded="true"] { opacity: 1; }
  .more:hover { background: var(--hover); color: var(--text); }
  .cal-menu { position: absolute; right: 0; top: 100%; z-index: 30; min-width: 160px; padding: 4px; background: var(--surface-raised); border: 1px solid var(--hairline-strong); border-radius: var(--radius-m); box-shadow: var(--shadow-pop); display: grid; }
  .cal-menu button { text-align: left; padding: 7px 10px; font-size: 12.5px; border-radius: var(--radius-s); }
  .cal-menu button:hover { background: var(--hover); }
  :global(:root[data-theme="pastel-light"]) .rail { background: var(--bg); }
</style>
