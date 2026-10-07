<script lang="ts">
  // The calendar screen (PLAN.md 7.5). Rendered by App.svelte in place of
  // MessageList + ReadingPane while `ui.view === "calendar"`. EventCalendar
  // (@event-calendar/core, MIT) draws the grid; every option it needs comes
  // from `calendar` (store.svelte.ts) and every colour from ec-theme.css.
  //
  // Keys (capture phase on window, so App's mail bindings never see them
  // while this view is up): d w m a views, t today, j/k next/prev period,
  // n new event, Esc closes the panel.
  import "@event-calendar/core/index.css";
  import "./ec-theme.css";
  // The `svelte` export condition of the package provides the Svelte 5
  // component + plugins (createCalendar is only in the compiled `dist/`
  // entry); options are a $state object the component reacts to.
  import { Calendar, DayGrid, Interaction, List, TimeGrid, type Calendar as EC } from "@event-calendar/core";
  import { onMount } from "svelte";
  import { getLocale, t } from "../../lib/i18n/index.svelte";
  import { palette } from "../../lib/stores/palette.svelte";
  import { ui } from "../../lib/stores/ui.svelte";
  import { toast } from "../stores/toast.svelte";
  import MiniMonth from "./MiniMonth.svelte";
  import EventPreview from "./EventPreview.svelte";
  import EventPanel from "./EventPanel.svelte";
  import { eventState, zoneClock } from "./presentation";
  import { EVENT_COLORS } from "./editor";
  import GuestsPrompt from "./GuestsPrompt.svelte";
  import { calendarErrorText, localDate, localZone } from "./guests";
  import { calPrefs } from "./settings.svelte";
  import { calendar } from "./store.svelte";
  import type { CalView, EventInput, EventRow } from "./types";

  const VIEWS: { key: CalView; ec: string; label: string; hint: string }[] = [
    { key: "day", ec: "timeGridDay", label: "fork.cal.view_day", hint: "D" },
    { key: "week", ec: "timeGridWeek", label: "fork.cal.view_week", hint: "W" },
    { key: "workweek", ec: "timeGridWeek", label: "fork.cal.view_workweek", hint: "" },
    { key: "month", ec: "dayGridMonth", label: "fork.cal.view_month", hint: "M" },
    { key: "agenda", ec: "listWeek", label: "fork.cal.view_agenda", hint: "A" },
  ];
  const ecView = (v: CalView) => VIEWS.find((x) => x.key === v)!.ec;

  const PLUGINS = [TimeGrid, DayGrid, List, Interaction];
  let title = $state("");
  let lastRange = "";
  let eventOpener: HTMLElement | null = null;
  function closePanel() { calendar.close(); if (eventOpener?.isConnected) eventOpener.focus({ preventScroll: true }); }
  let monthOpen = $state(false);
  let filtersOpen = $state(false);
  let visibilityBusy = $state<number | null>(null);
  async function toggleCalendar(id: number, selected: boolean) {
    visibilityBusy = id;
    try { await calendar.setVisible(id, selected); }
    catch (e) { toast.show({ text: calendarErrorText(e) }); }
    finally { visibilityBusy = null; }
  }
  async function retryFailed() {
    try { await calendar.retry(); } catch (e) { toast.show({ text: calendarErrorText(e) }); }
  }

  const sec = (d: Date) => Math.floor(d.getTime() / 1000);

  function toEc(row: EventRow): EC.EventInput {
    const cal = calendar.calendarOf(row);
    const color = EVENT_COLORS[Number(row.options?.colorId)] || cal?.color || "var(--acct-1)";
    const declined = row.self_response === "declined";
    const tentative = row.status === "tentative" || row.self_response === "tentative";
    const classNames = ["cal-ev", `cal-state-${eventState(row)}`];
    if (!row.all_day && row.end_ts - row.start_ts <= 1800) classNames.push("cal-short");
    if (declined) classNames.push("cal-declined");
    if (tentative) classNames.push("cal-tentative");
    if (row.local_only) classNames.push("cal-local");
    if (row.id === calendar.selectedId) classNames.push("cal-selected");
    return {
      id: String(row.id),
      title: row.summary || t("fork.cal.untitled"),
      allDay: row.all_day,
      start: row.all_day && row.start_date ? row.start_date : new Date(row.start_ts * 1000),
      end: row.all_day && row.end_date ? row.end_date : new Date(row.end_ts * 1000),
      classNames,
      styles: [`--cal-color: ${color}`],
      editable: !declined && row.status !== "cancelled" && calendar.canEdit(row),
      extendedProps: { rowId: row.id, state: eventState(row), short: !row.all_day && row.end_ts - row.start_ts <= 1800 },
    };
  }

  // ---- grid callbacks ----
  function onDatesSet(info: EC.DatesSetInfo) {
    title = info.view.title;
    const key = `${info.startStr}|${info.endStr}`;
    if (key === lastRange) return;
    lastRange = key;
    void calendar.loadRange(info.start, info.end);
  }

  function onSelect(info: EC.SelectInfo) {
    // A click is a one-slot selection; a drag is a range. Both quick-create.
    const allDay = info.allDay;
    let end = info.end;
    if (!allDay && end.getTime() - info.start.getTime() <= 30 * 60_000) {
      end = new Date(info.start.getTime() + calPrefs.defaultLen * 60_000);
    }
    calendar.openDraft({ start: info.start, end, allDay });
  }

  function onEventClick(info: EC.EventClickInfo) {
    const id = Number(info.event.id);
    eventOpener = info.el;
    if (Number.isFinite(id)) calendar.open(id);
  }

  /** Drag-move and resize both become one `patch`; the guests prompt (when
   *  the event has other guests) runs inside `calendar.patch`, and a cancel
   *  reverts the grid. */
  async function onMoved(info: EC.EventDropInfo | EC.EventResizeInfo) {
    const id = Number(info.event.id);
    const row = calendar.events.find((r) => r.id === id);
    if (!row) return info.revert();
    const start = info.event.start;
    const end = info.event.end ?? new Date(start.getTime() + (row.end_ts - row.start_ts) * 1000);
    const input: EventInput = info.event.allDay
      ? { all_day: true, start_date: localDate(start), end_date: localDate(end) }
      : { all_day: false, start_ts: sec(start), end_ts: sec(end), time_zone: localZone() };
    try {
      const ok = await calendar.patch(id, input, (updates, count) => {
        toast.show({ text: t(count ? updates === "all" ? "fork.cal.move_notify_queued" : "fork.cal.move_no_notify" : "fork.cal.move_saved"), ms: 6000 });
      });
      if (!ok) info.revert();
    } catch (e: unknown) {
      info.revert();
      toast.show({ text: calendarErrorText(e) });
    }
  }

  // ---- options: one $state object the component follows ----
  const options: EC.Options = $state({
      view: ecView(calendar.view),
      date: calendar.date,
      events: calendar.events.map(toEc),
      headerToolbar: { start: "", center: "", end: "" },
      height: "100%",
      firstDay: 1,
      locale: getLocale(),
      nowIndicator: true,
      allDaySlot: true,
      scrollTime: "08:00:00",
      slotDuration: "00:30:00",
      slotHeight: 36,
      slotEventOverlap: false,
      slotMinTime: `${calPrefs.visibleStart}:00`,
      slotMaxTime: `${calPrefs.visibleEnd}:00`,
      dayMaxEvents: true,
      selectable: true,
      selectMinDistance: 0,
      unselectAuto: true,
      editable: calendar.connected,
      eventStartEditable: true,
      eventDurationEditable: true,
      // "Mon 21": weekday first whatever the locale's default order is.
      dayHeaderFormat: (d: Date) => {
        const parts = new Intl.DateTimeFormat(getLocale(), { weekday: "short", day: "numeric" }).formatToParts(d);
        const of = (type: string) => parts.find((p) => p.type === type)?.value ?? "";
        return `${of("weekday")} ${of("day")}`;
      },
      // Read once at creation: the month grid's column heads are weekdays only.
      views: {
        dayGridMonth: { dayHeaderFormat: { weekday: "short" } },
        // Start time only on the grid (the panel shows the range), so a
        // 30-minute event keeps room for its title.
        timeGridWeek: { displayEventEnd: false },
        timeGridDay: { displayEventEnd: false },
      },
      eventTimeFormat: { hour: "2-digit", minute: "2-digit", hour12: false },
      slotLabelFormat: { hour: "2-digit", minute: "2-digit" },
      listDayFormat: { weekday: "short", day: "numeric", month: "short" },
      listDaySideFormat: { weekday: "long" },
      noEventsContent: t("fork.cal.no_events"),
      datesSet: onDatesSet,
      select: onSelect,
      eventClick: onEventClick,
      eventDrop: onMoved,
      eventResize: onMoved,
      eventDidMount: ({el, event, timeText}) => {
        const text = `${event.title} ? ${timeText} ? ${t(`fork.cal.state_${event.extendedProps.state}`)}`;
        el.title = text; el.setAttribute("aria-label", text);
      },
  });

  onMount(() => {
    void calendar.refreshStatus();
  });

  $effect(() => {
    options.view = ecView(calendar.view);
    options.date = calendar.date;
    options.hiddenDays = calendar.view === "workweek" ? [0, 6] : [];
    options.slotMinTime = `${calPrefs.visibleStart}:00`;
    options.slotMaxTime = `${calPrefs.visibleEnd}:00`;
    const zone = calPrefs.secondTz, anchor = calendar.date, locale = getLocale();
    options.slotLabelFormat = (time: Date) => {
      const local = document.createElement("span"); local.className = "cal-clock-local";
      local.textContent = time.toLocaleTimeString(locale, {hour:"2-digit", minute:"2-digit", hour12:false});
      if (!zone) return {domNodes:[local]};
      const second = document.createElement("span"); second.className = "cal-clock-second";
      second.textContent = zoneClock(anchor, time.getHours(), time.getMinutes(), zone, locale);
      second.title = `${zone} (${anchor.toLocaleDateString(locale)})`;
      return {domNodes:[second, local]};
    };
  });

  $effect(() => {
    options.events = calendar.events.map(toEc);
    options.editable = calendar.connected;
  });

  // ---- keys ----
  function isTyping(): boolean {
    const a = document.activeElement as HTMLElement | null;
    return !!a && (a.tagName === "INPUT" || a.tagName === "TEXTAREA" || a.tagName === "SELECT" || a.isContentEditable);
  }

  function onKeydown(e: KeyboardEvent) {
    if (ui.view !== "calendar" || palette.open || ui.settingsOpen || ui.shortcutsOpen) return;
    if (e.ctrlKey || e.metaKey || e.altKey || isTyping()) return;
    if (e.key === "Escape") {
      monthOpen = false; filtersOpen = false;
      if (calendar.selectedId !== null || calendar.draft) {
        closePanel();
        e.preventDefault();
        e.stopImmediatePropagation();
      }
      return;
    }
    if (e.shiftKey) return;
    switch (e.code) {
      case "KeyD":
        calendar.setView("day");
        break;
      case "KeyW":
        calendar.setView("week");
        break;
      case "KeyM":
        calendar.setView("month");
        break;
      case "KeyA":
        calendar.setView("agenda");
        break;
      case "KeyT":
        calendar.today();
        break;
      case "KeyJ":
        calendar.step(1);
        break;
      case "KeyK":
        calendar.step(-1);
        break;
      case "KeyN":
        calendar.newEvent();
        break;
      default:
        return;
    }
    e.preventDefault();
    e.stopImmediatePropagation();
  }

  // Keyed on the resolved row (not the id): an id chosen before the window
  // loaded resolves later, and the panel must re-init on that row.
  const panelRow = $derived(calendar.selected);
  const panelOpen = $derived(panelRow !== null || calendar.draft !== null);
  const panelKey = $derived(panelRow ? `row-${panelRow.id}` : "draft");
  const setupText = $derived(
    !calendar.configured ? t("fork.cal.setup_unconfigured") : !calendar.connected ? t("fork.cal.setup_disconnected") : null,
  );
</script>

<svelte:window onkeydowncapture={onKeydown} />

<section class="fork-cal" class:with-panel={panelOpen} aria-label={t("fork.nav.calendar")}>
  <div class="main">
    <header class="bar">
      <div class="left">
        <button class="chip" onclick={() => calendar.today()} title="T">{t("fork.cal.today")}</button>
        <div class="nav">
          <button class="arrow" onclick={() => calendar.step(-1)} aria-label={t("fork.cal.prev")} title="K">
            <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M10 4L6 8l4 4" /></svg>
          </button>
          <button class="arrow" onclick={() => calendar.step(1)} aria-label={t("fork.cal.next")} title="J">
            <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M6 4l4 4-4 4" /></svg>
          </button>
        </div>
        <div class="month-anchor"><button class="title month-toggle" onclick={() => (monthOpen = !monthOpen)} aria-expanded={monthOpen} aria-label={t("fork.cal.choose_date")}>{title}<span aria-hidden="true"> &#9662;</span></button>
          {#if monthOpen}<div class="month-popup"><MiniMonth selected={calendar.date} onselect={(d) => { calendar.goto(d); monthOpen = false; }} /></div>{/if}
        </div>
        {#if calendar.loading}<span class="spinner" aria-hidden="true"></span><span class="loading-label" role="status">{t("fork.cal.refreshing")}</span>{/if}
      </div>
      <div class="right">
        <div class="views" role="radiogroup" aria-label={t("fork.cal.views")}>
          {#each VIEWS as v (v.key)}
            <button
              class="chip"
              class:active={calendar.view === v.key}
              role="radio"
              aria-checked={calendar.view === v.key}
              title={v.hint}
              onclick={() => calendar.setView(v.key)}
            >
              {t(v.label)}
            </button>
          {/each}
        </div>
        <button class="new" onclick={() => calendar.newEvent()} disabled={!calendar.connected} title="N">
          <span class="plus">+</span>
          {t("fork.cal.new_event")}
        </button>
      </div>
    </header>

    <div class="calendar-controls">
      <button class="control" class:active={filtersOpen} aria-expanded={filtersOpen} onclick={() => (filtersOpen = !filtersOpen)}>{t("fork.cal.calendars")} <span>{calendar.calendars.filter((c) => c.selected).length}</span></button>
      {#if calendar.view === "day" || calendar.view === "week" || calendar.view === "workweek"}
        <label>{t("fork.cal.visible_hours")}<select aria-label={t("fork.cal.visible_from")} value={calPrefs.visibleStart} onchange={(e) => calPrefs.setVisibleHours(e.currentTarget.value, calPrefs.visibleEnd)}>{#each Array.from({length: 24}, (_, n) => `${String(n).padStart(2, "0")}:00`) as hour}<option value={hour} disabled={hour >= calPrefs.visibleEnd}>{hour}</option>{/each}</select></label>
        <span aria-hidden="true">&ndash;</span><select aria-label={t("fork.cal.visible_to")} value={calPrefs.visibleEnd} onchange={(e) => calPrefs.setVisibleHours(calPrefs.visibleStart, e.currentTarget.value)}>{#each Array.from({length: 24}, (_, n) => `${String(n + 1).padStart(2, "0")}:00`) as hour}<option value={hour} disabled={hour <= calPrefs.visibleStart}>{hour}</option>{/each}</select>
      {/if}
      <div class="zone"><span>{localZone()}</span><select aria-label={t("fork.cal.second_column")} value={calPrefs.secondTz} onchange={(e) => calPrefs.setSecondTz(e.currentTarget.value)}><option value="">{t("fork.cal.second_column_off")}</option>{#each Intl.supportedValuesOf("timeZone") as zone}<option value={zone}>{zone}</option>{/each}</select></div>
    </div>
    {#if filtersOpen}<div class="calendar-filters">{#each calendar.calendars as cal}<label><input type="checkbox" checked={cal.selected} disabled={visibilityBusy !== null} onchange={(e) => toggleCalendar(cal.id, e.currentTarget.checked)} /><span class="color" style:background={cal.color || "var(--primary)"}></span>{cal.summary}</label>{/each}</div>{/if}
    {#if calendar.failedEvents.length}<div class="sync-alert" role="status"><span>{t("fork.cal.failed_changes", { count: calendar.failedEvents.length })}</span><button onclick={retryFailed}>{t("fork.cal.retry")}</button></div>{/if}
    {#if setupText}
      <div class="setup">
        <span class="microlabel">{t("fork.nav.calendar")}</span>
        <p>{setupText}</p>
        <button class="chip active" onclick={() => ui.openSettings()}>{t("fork.cal.open_settings")}</button>
      </div>
    {/if}
    {#if calendar.error}
      <div class="error" role="alert">{calendar.error} <button onclick={() => calendar.reload()}>{t("fork.cal.retry")}</button></div>
    {/if}

    {#if calendar.events.filter((e) => e.all_day).length > 4}<div class="all-day-hint">{t("fork.cal.all_day_scroll", {count: calendar.events.filter((e) => e.all_day).length})}</div>{/if}
    {#if calPrefs.secondTz && ["day", "week", "workweek"].includes(calendar.view)}<div class="zone-legend" title={t("fork.cal.zone_anchor", {date: calendar.date.toLocaleDateString(getLocale())})}><span>{calPrefs.secondTz.split("/").at(-1)?.replaceAll("_", " ")}</span><span>{localZone().split("/").at(-1)?.replaceAll("_", " ")}</span></div>{/if}
    <div class="grid" class:second-clock={!!calPrefs.secondTz}>
      <Calendar plugins={PLUGINS} {options}>
        {#snippet eventContent({event, timeText})}
          <span class="cal-event-content">{#if !event.extendedProps.short && !event.allDay}<span class="cal-event-time">{timeText}</span>{/if}<span class="cal-event-title"><span class="cal-state-mark" aria-hidden="true">{event.extendedProps.state === "accepted" ? "\u2713 " : event.extendedProps.state === "tentative" ? "? " : event.extendedProps.state === "declined" ? "\u00d7 " : event.extendedProps.state === "cancelled" ? `${t("fork.cal.state_cancelled")}: ` : ""}</span>{event.title}</span></span>
        {/snippet}
      </Calendar>
    </div>
  </div>

  {#if panelOpen}
    {#key panelKey}
      {#if panelRow && !calendar.editing}
        <EventPreview row={panelRow} onclose={closePanel} />
      {:else}
        <EventPanel row={panelRow} draft={calendar.draft} onclose={closePanel} />
      {/if}
    {/key}
  {/if}
</section>

<GuestsPrompt />

<style>
  .loading-label { color: var(--text-dim); font-size: 11px; }
  .month-anchor { position: relative; min-width: 0; }
  .month-toggle { padding: 5px 4px; border-radius: var(--radius-s); }
  .month-toggle:hover { background: var(--hover); }
  .month-popup { position: absolute; top: calc(100% + 8px); left: 0; z-index: 20; }
  .calendar-controls { display: flex; align-items: center; gap: 8px; padding: 8px 16px; font-size: 12px; color: var(--text-dim); flex-wrap: wrap; }
  .calendar-controls label { display: flex; align-items: center; gap: 8px; margin-left: 6px; }
  .calendar-controls select { padding: 4px 6px; border: 1px solid var(--hairline); border-radius: var(--radius-s); background: var(--surface); color: var(--text); font-size: 12px; }
  .control { padding: 5px 9px; border: 1px solid var(--hairline); border-radius: var(--radius-s); }.control:hover, .control.active { background: var(--hover); } .control span { margin-left: 6px; color: var(--text); }
  .zone { margin-left: auto; font-size: 11px; display:flex; align-items:center; flex-wrap:wrap; gap:8px; }.zone select { max-width:180px; }
  .zone-legend { display:flex; gap:12px; font-size:10px; color:var(--text-dim); padding:0 10px 4px; }.zone-legend span { max-width:90px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .all-day-hint { padding:3px 16px 7px; font-size:11px; color:var(--text-dim); }
  .cal-event-content { display:block; width:100%; min-width:0; }.cal-state-mark { font-weight:700; }.cal-event-time { display:block; font-size:10px; color:var(--text-dim); line-height:1.2; margin-bottom:2px; }.cal-event-title { display:block; font-weight:600; overflow:hidden; text-overflow:ellipsis; overflow-wrap:break-word; }

  .calendar-filters { display: flex; flex-wrap: wrap; gap: 12px 20px; padding: 6px 20px 14px; border-bottom: 1px solid var(--hairline); }
  .calendar-filters label { display: flex; align-items: center; gap: 7px; font-size: 12px; }.calendar-filters input { accent-color: var(--primary); }.color { width: 8px; height: 8px; border-radius: 50%; }
  .sync-alert { display: flex; justify-content: space-between; gap: 12px; margin: 4px 16px 10px; font-size: 12px; color: var(--danger); }.sync-alert button, .error button { text-decoration: underline; }

  .fork-cal {
    flex: 1;
    min-width: 0;
    display: flex;
    background: var(--surface);
  }
  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px 12px;
    padding: 10px 16px;
    border-bottom: 1px solid var(--hairline);
    flex-shrink: 0;
  }
  .left,
  .right {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .left {
    flex: 1 1 auto;
  }
  .right {
    margin-left: auto;
    flex-shrink: 0;
  }
  .nav {
    display: flex;
    gap: 2px;
  }
  .arrow {
    width: 26px;
    height: 26px;
    display: grid;
    place-items: center;
    border-radius: var(--radius-s);
    color: var(--text-dim);
  }
  .arrow:hover {
    background: var(--hover);
    color: var(--text);
  }
  .title {
    font-size: 15px;
    font-weight: 700;
    letter-spacing: -0.01em;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
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
  .chip.active {
    background: var(--primary);
    color: var(--on-primary);
    font-weight: 600;
  }
  .views {
    display: flex;
    gap: 2px;
    padding: 2px;
    border: 1px solid var(--hairline-strong);
    border-radius: 999px;
  }
  .new {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border-radius: var(--radius-m);
    background: var(--primary);
    color: var(--on-primary);
    font-weight: 600;
    font-size: 13px;
  }
  .new:hover {
    opacity: 0.88;
  }
  .new:disabled {
    opacity: 0.35;
    cursor: default;
  }
  .plus {
    font-size: 15px;
    line-height: 1;
  }
  .grid {
    flex: 1;
    min-height: 0;
    padding: 0 8px 8px;
  }
  .setup {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
    margin: 16px;
    padding: 14px 16px;
    border: 1px dashed var(--hairline-strong);
    border-radius: var(--radius-m);
    max-width: 520px;
  }
  .setup p {
    font-size: 13.5px;
    color: var(--text-dim);
  }
  .error {
    margin: 8px 16px 0;
    font-size: 12.5px;
    color: var(--danger);
  }
  .spinner {
    width: 10px;
    height: 10px;
    border: 1.5px solid var(--text-faint);
    border-top-color: var(--text);
    border-radius: 50%;
    animation: spin 0.9s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (max-width: 1100px) {
    .fork-cal.with-panel .main { display: none; }
    .fork-cal.with-panel :global(.panel), .fork-cal.with-panel :global(.preview) { width: 100%; border-left: 0; }
  }
  @media (prefers-reduced-motion: reduce) {
    .spinner {
      animation: none;
    }
  }
</style>
