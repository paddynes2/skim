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
  import EventPanel from "./EventPanel.svelte";
  import GuestsPrompt from "./GuestsPrompt.svelte";
  import { calendarErrorText, localDate, localZone } from "./guests";
  import { calPrefs } from "./settings.svelte";
  import { calendar } from "./store.svelte";
  import type { CalView, EventInput, EventRow } from "./types";

  const VIEWS: { key: CalView; ec: string; label: string; hint: string }[] = [
    { key: "day", ec: "timeGridDay", label: "fork.cal.view_day", hint: "D" },
    { key: "week", ec: "timeGridWeek", label: "fork.cal.view_week", hint: "W" },
    { key: "month", ec: "dayGridMonth", label: "fork.cal.view_month", hint: "M" },
    { key: "agenda", ec: "listWeek", label: "fork.cal.view_agenda", hint: "A" },
  ];
  const ecView = (v: CalView) => VIEWS.find((x) => x.key === v)!.ec;

  const PLUGINS = [TimeGrid, DayGrid, List, Interaction];
  let title = $state("");
  let lastRange = "";

  const sec = (d: Date) => Math.floor(d.getTime() / 1000);

  function toEc(row: EventRow): EC.EventInput {
    const cal = calendar.calendarOf(row);
    const color = cal?.color || "var(--acct-1)";
    const declined = row.self_response === "declined";
    const tentative = row.status === "tentative" || row.self_response === "tentative";
    const classNames = ["cal-ev"];
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
      extendedProps: { rowId: row.id },
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
      const ok = await calendar.patch(id, input);
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
      slotHeight: 26,
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
      eventTimeFormat: { hour: "2-digit", minute: "2-digit" },
      slotLabelFormat: { hour: "2-digit", minute: "2-digit" },
      listDayFormat: { weekday: "short", day: "numeric", month: "short" },
      listDaySideFormat: { weekday: "long" },
      noEventsContent: t("fork.cal.no_events"),
      datesSet: onDatesSet,
      select: onSelect,
      eventClick: onEventClick,
      eventDrop: onMoved,
      eventResize: onMoved,
  });

  onMount(() => {
    void calendar.refreshStatus();
  });

  $effect(() => {
    options.view = ecView(calendar.view);
    options.date = calendar.date;
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
      if (calendar.selectedId !== null || calendar.draft) {
        calendar.close();
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
        <h1 class="title">{title}</h1>
        {#if calendar.loading}<span class="spinner" aria-hidden="true"></span>{/if}
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

    {#if setupText}
      <div class="setup">
        <span class="microlabel">{t("fork.nav.calendar")}</span>
        <p>{setupText}</p>
        <button class="chip active" onclick={() => ui.openSettings()}>{t("fork.cal.open_settings")}</button>
      </div>
    {/if}
    {#if calendar.error}
      <div class="error">{calendar.error}</div>
    {/if}

    <div class="grid">
      <Calendar plugins={PLUGINS} {options} />
    </div>
  </div>

  {#if panelOpen}
    {#key panelKey}
      <EventPanel row={panelRow} draft={calendar.draft} onclose={() => calendar.close()} />
    {/key}
  {/if}
</section>

<GuestsPrompt />

<style>
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
    background: var(--text);
    color: var(--bg);
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
    background: var(--text);
    color: var(--bg);
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
  @media (prefers-reduced-motion: reduce) {
    .spinner {
      animation: none;
    }
  }
</style>
