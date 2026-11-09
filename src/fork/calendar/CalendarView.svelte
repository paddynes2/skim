<script lang="ts">
  // The calendar screen (PLAN.md 7.5, Look A from D97). Rendered by App.svelte
  // in place of MessageList + ReadingPane while `ui.view === "calendar"`.
  // Layout: the rail (small month, calendar list) | one toolbar over the grid.
  // EventCalendar (@event-calendar/core, MIT) draws day, week, N-day, month
  // and schedule; YearView and SearchPanel are ours. Every colour comes from
  // ec-theme.css. An event opens as a card beside it (EventPopover); Edit and
  // new events open the full editor (EventPanel) on the right.
  //
  // Keys (capture phase on window, so App's mail bindings never see them
  // while this view is up), Google Calendar's set: 1/d day, 2/w week, 4/x
  // N days, 3/m month, 6/y year, 5/a schedule, t today, j/k next and previous,
  // c or n new event, e edit, Delete delete, g go to a date, / search,
  // r refresh, Ctrl+Z undo, Esc closes.
  import "@event-calendar/core/index.css";
  import "./ec-theme.css";
  // The `svelte` export condition of the package provides the Svelte 5
  // component + plugins (createCalendar is only in the compiled `dist/`
  // entry); options are a $state object the component reacts to.
  import { Calendar, DayGrid, Interaction, List, TimeGrid, type Calendar as EC } from "@event-calendar/core";
  import { onMount, tick } from "svelte";
  import { getLocale, t } from "../../lib/i18n/index.svelte";
  import { palette } from "../../lib/stores/palette.svelte";
  import { ui } from "../../lib/stores/ui.svelte";
  import { toast } from "../stores/toast.svelte";
  import CalendarRail from "./CalendarRail.svelte";
  import EventPanel from "./EventPanel.svelte";
  import EventPopover from "./EventPopover.svelte";
  import GuestsPrompt from "./GuestsPrompt.svelte";
  import MiniMonth from "./MiniMonth.svelte";
  import SearchPanel from "./SearchPanel.svelte";
  import YearView from "./YearView.svelte";
  import { EVENT_COLORS } from "./editor";
  import { calendarErrorText, localDate, localZone } from "./guests";
  import { eventState, isoWeek, needsReply, pastelFor, zoneClock } from "./presentation";
  import { calPrefs } from "./settings.svelte";
  import { calendar } from "./store.svelte";
  import { calUndo, deleteWithUndo } from "./undo.svelte";
  import type { CalView, EventInput, EventRow, SearchHit } from "./types";

  const VIEWS: { key: CalView; label: string; hint: string }[] = [
    { key: "day", label: "fork.cal.view_day", hint: "D" },
    { key: "week", label: "fork.cal.view_week", hint: "W" },
    { key: "custom", label: "fork.cal.view_custom", hint: "X" },
    { key: "month", label: "fork.cal.view_month", hint: "M" },
    { key: "year", label: "fork.cal.view_year", hint: "Y" },
    { key: "agenda", label: "fork.cal.view_schedule", hint: "A" },
  ];
  const ecView = (v: CalView) =>
    v === "day" ? "timeGridDay" : v === "month" ? "dayGridMonth" : v === "agenda" ? "listWeek" : "timeGridWeek";
  const timed = (v: CalView) => v === "day" || v === "week" || v === "workweek" || v === "custom";

  const PLUGINS = [TimeGrid, DayGrid, List, Interaction];
  const HOUR_PX = 56;
  let title = $state("");
  let lastRange = "";
  let visibleRange = $state<{ start: string; end: string } | null>(null);
  let gridEl: HTMLDivElement;
  let monthOpen = $state(false);
  let searchInput = $state<HTMLInputElement>();
  let searchText = $state("");
  let searchTimer: ReturnType<typeof setTimeout> | null = null;

  // The open event card: its anchor (the clicked event's box) and the grid's box.
  let anchor = $state<DOMRect | null>(null);
  let bounds = $state<DOMRect | null>(null);
  let eventOpener: HTMLElement | null = null;
  function closePanel() {
    calendar.close();
    anchor = null;
    if (eventOpener?.isConnected) eventOpener.focus({ preventScroll: true });
  }
  function openAt(id: number, el: HTMLElement | null) {
    eventOpener = el;
    anchor = el?.getBoundingClientRect() ?? null;
    bounds = gridEl?.getBoundingClientRect() ?? null;
    calendar.open(id);
  }

  const sec = (d: Date) => Math.floor(d.getTime() / 1000);
  const nowS = () => Math.floor(Date.now() / 1000);

  /** One row per meeting: the same event seen through two accounts shares its iCalUID and start (D97). */
  function dedupe(rows: EventRow[]): EventRow[] {
    const own = calendar.ownEmails;
    const seen = new Map<string, EventRow>();
    const out: EventRow[] = [];
    for (const r of rows) {
      if (!r.ical_uid) { out.push(r); continue; }
      const key = `${r.ical_uid}|${r.start_ts}`;
      const prev = seen.get(key);
      if (!prev) { seen.set(key, r); out.push(r); continue; }
      // Keep the copy on the calendar of the address that is the invited self.
      const mine = (x: EventRow) => own.includes((calendar.calendarOf(x)?.google_id ?? "").toLowerCase());
      if (!mine(prev) && mine(r)) { out[out.indexOf(prev)] = r; seen.set(key, r); }
    }
    return out;
  }

  const shownRows = $derived.by(() => {
    const hiddenCals = new Set(calPrefs.hiddenCals);
    const hiddenRows = new Set(calendar.hidden);
    return dedupe(calendar.events.filter((r) =>
      !hiddenRows.has(r.id) && !hiddenCals.has(r.calendar_id) && (calPrefs.showDeclined || r.self_response !== "declined"),
    ));
  });

  function toEc(row: EventRow): EC.EventInput {
    const cal = calendar.calendarOf(row);
    const color = EVENT_COLORS[Number(row.options?.colorId)] || cal?.color || "#7986cb";
    const declined = row.self_response === "declined";
    const tentative = row.status === "tentative" || row.self_response === "tentative";
    const short = !row.all_day && row.end_ts - row.start_ts <= 1800;
    const needs = needsReply(row, calendar.ownEmails);
    const priv = row.options?.visibility === "private" || row.options?.visibility === "confidential";
    const classNames = ["cal-ev", `cal-state-${eventState(row)}`];
    if (short) classNames.push("cal-short");
    if (declined) classNames.push("cal-declined");
    if (tentative) classNames.push("cal-tentative");
    if (needs) classNames.push("cal-needs");
    if (priv) classNames.push("cal-private");
    if (row.local_only) classNames.push("cal-local");
    if (calPrefs.dimPast && row.end_ts < nowS()) classNames.push("cal-past");
    if (row.id === calendar.selectedId) classNames.push("cal-selected");
    return {
      id: String(row.id),
      title: row.summary || t("fork.cal.untitled"),
      allDay: row.all_day,
      start: row.all_day && row.start_date ? row.start_date : new Date(row.start_ts * 1000),
      end: row.all_day && row.end_date ? row.end_date : new Date(row.end_ts * 1000),
      classNames,
      styles: [`--cal-color: ${color}`, `--cal-pastel: var(${pastelFor(color)})`],
      editable: !declined && row.status !== "cancelled" && calendar.canEdit(row),
      extendedProps: { rowId: row.id, state: eventState(row), short, needs, priv },
    };
  }

  // ---- grid callbacks ----
  function onDatesSet(info: EC.DatesSetInfo) {
    title = info.view.title;
    visibleRange = { start: localDate(info.start), end: localDate(info.end) };
    const key = `${info.startStr}|${info.endStr}`;
    if (key === lastRange) return;
    lastRange = key;
    void calendar.loadRange(info.start, info.end);
    void scrollToWork();
  }

  /** EventCalendar scrolls its body under the sticky header (the all-day row
   *  covered 08:00). Put the start of the day just below the header instead:
   *  two hours before now on a range that holds today, else an hour before
   *  the working day. */
  async function scrollToWork() {
    if (!timed(calendar.view)) return;
    await tick();
    requestAnimationFrame(() => {
      const main = gridEl?.querySelector<HTMLElement>(".ec-main");
      if (!main) return;
      const start = Number(calPrefs.visibleStart.slice(0, 2));
      const work = Number(calPrefs.workStart.slice(0, 2)) - 1;
      const today = visibleRange && localDate(new Date()) >= visibleRange.start && localDate(new Date()) < visibleRange.end;
      const now = new Date().getHours() + new Date().getMinutes() / 60 - 2;
      const target = Math.max(start, today ? Math.min(Math.max(now, work), 24) : work);
      main.scrollTop = Math.max(0, (target - start) * HOUR_PX - 16);
    });
  }

  function onSelect(info: EC.SelectInfo) {
    // With a card open, a click on the grid only closes it (Google does the same).
    if (calendar.selectedId !== null || calendar.foreign) {
      closePanel();
      return;
    }
    const allDay = info.allDay;
    let end = info.end;
    if (!allDay && end.getTime() - info.start.getTime() <= 30 * 60_000) {
      end = new Date(info.start.getTime() + calPrefs.defaultLen * 60_000);
    }
    calendar.openDraft({ start: info.start, end, allDay });
  }

  function onEventClick(info: EC.EventClickInfo) {
    const id = Number(info.event.id);
    if (Number.isFinite(id)) openAt(id, info.el);
  }

  /** Drag-move and resize both become one `patch`. Undo is a new patch back,
   *  through the guests prompt again (never a replay of the first answer). */
  async function onMoved(info: EC.EventDropInfo | EC.EventResizeInfo) {
    const id = Number(info.event.id);
    const row = calendar.events.find((r) => r.id === id);
    if (!row) return info.revert();
    const start = info.event.start;
    const end = info.event.end ?? new Date(start.getTime() + (row.end_ts - row.start_ts) * 1000);
    const input: EventInput = info.event.allDay
      ? { all_day: true, start_date: localDate(start), end_date: localDate(end) }
      : { all_day: false, start_ts: sec(start), end_ts: sec(end), time_zone: localZone() };
    const back: EventInput = row.all_day
      ? { all_day: true, start_date: row.start_date ?? undefined, end_date: row.end_date ?? undefined }
      : { all_day: false, start_ts: row.start_ts, end_ts: row.end_ts, time_zone: row.time_zone ?? localZone() };
    try {
      let text = "";
      const ok = await calendar.patch(id, input, (updates, count) => {
        text = t(count ? (updates === "all" ? "fork.cal.move_notify_queued" : "fork.cal.move_no_notify") : "fork.cal.move_saved");
      });
      if (!ok) return info.revert();
      calUndo.offer(text || t("fork.cal.move_saved"), async () => { await calendar.patch(id, back); });
    } catch (e: unknown) {
      info.revert();
      toast.show({ text: calendarErrorText(e) });
    }
  }

  /** N visible days from `from`, skipping hidden weekends (EC counts hidden days in a duration). */
  function customSpan(from: Date): number {
    if (calPrefs.showWeekends) return calPrefs.customDays;
    let span = 0, shown = 0;
    const d = new Date(from);
    while (shown < calPrefs.customDays && span < 14) {
      if (d.getDay() !== 0 && d.getDay() !== 6) shown++;
      span++;
      d.setDate(d.getDate() + 1);
    }
    return span;
  }

  const dayHead = (d: Date) => {
    const loc = getLocale();
    const wd = d.toLocaleDateString(loc, { weekday: "short" });
    return { html: `<span class="dh-num">${d.getDate()}</span><span class="dh-wd">${wd}</span>` };
  };

  // ---- options: one $state object the component follows ----
  const options: EC.Options = $state({
    view: ecView(calendar.view),
    date: calendar.date,
    events: [],
    headerToolbar: { start: "", center: "", end: "" },
    height: "100%",
    firstDay: calPrefs.weekStart,
    locale: getLocale(),
    nowIndicator: true,
    allDaySlot: true,
    scrollTime: `${calPrefs.workStart.slice(0, 2)}:00:00`,
    slotDuration: "00:30:00",
    slotHeight: HOUR_PX / 2,
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
    dayHeaderFormat: dayHead,
    views: {
      dayGridMonth: { dayHeaderFormat: { weekday: "short" } },
      // Start time only on the grid (the card shows the range), so a
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
    eventDidMount: ({ el, event, timeText }) => {
      const text = `${event.title}, ${timeText}, ${t(`fork.cal.state_${event.extendedProps.state}`)}`;
      el.title = text;
      el.setAttribute("aria-label", text);
      el.dataset.eventId = String(event.id);
    },
  });

  onMount(() => {
    void calendar.refreshStatus();
    const ro = new ResizeObserver(() => { if (anchor) bounds = gridEl?.getBoundingClientRect() ?? null; });
    if (gridEl) ro.observe(gridEl);
    return () => ro.disconnect();
  });

  $effect(() => {
    const v = calendar.view;
    options.view = ecView(v);
    options.date = calendar.date;
    options.firstDay = calPrefs.weekStart;
    options.weekNumbers = calPrefs.weekNumbers && v === "month";
    options.hiddenDays = v === "workweek" || !calPrefs.showWeekends ? [0, 6] : [];
    options.duration = v === "custom" ? { days: customSpan(calendar.date) } : v === "day" ? { days: 1 } : v === "month" ? { months: 1 } : { days: 7 };
    options.slotMinTime = `${calPrefs.visibleStart}:00`;
    options.slotMaxTime = `${calPrefs.visibleEnd}:00`;
    const zone = calPrefs.secondTz, anchorDate = calendar.date, locale = getLocale();
    options.slotLabelFormat = (time: Date) => {
      const local = document.createElement("span"); local.className = "cal-clock-local";
      local.textContent = time.toLocaleTimeString(locale, { hour: "2-digit", minute: "2-digit", hour12: false });
      if (!zone) return { domNodes: [local] };
      const second = document.createElement("span"); second.className = "cal-clock-second";
      second.textContent = zoneClock(anchorDate, time.getHours(), time.getMinutes(), zone, locale);
      second.title = `${zone} (${anchorDate.toLocaleDateString(locale)})`;
      return { domNodes: [second, local] };
    };
  });

  $effect(() => {
    void calendar.selectedId;
    void calPrefs.dimPast;
    options.events = shownRows.map(toEc);
    options.editable = calendar.connected;
  });

  // Year view and search have no grid; fetch what they show.
  $effect(() => {
    if (calendar.view !== "year") return;
    const y = calendar.date.getFullYear();
    void calendar.loadRange(new Date(y, 0, 1), new Date(y + 1, 0, 1));
  });

  // ---- search ----
  function onSearchInput(v: string) {
    searchText = v;
    if (searchTimer) clearTimeout(searchTimer);
    if (!v.trim()) { calendar.closeSearch(); return; }
    calendar.openSearch();
    searchTimer = setTimeout(() => void calendar.runSearch(v), 220);
  }
  function closeSearch() {
    searchText = "";
    calendar.closeSearch();
    searchInput?.blur();
  }
  async function pickHit(hit: SearchHit) {
    closeSearch();
    const d = hit.all_day && hit.start_date ? new Date(`${hit.start_date}T12:00:00`) : new Date(hit.start_ts * 1000);
    if (!hit.id || hit.remote) { calendar.openRow(hit); anchor = null; bounds = gridEl?.getBoundingClientRect() ?? null; return; }
    if (!timed(calendar.view) && calendar.view !== "month") calendar.setView("week");
    calendar.goto(d);
    await tick();
    setTimeout(() => {
      const el = gridEl?.querySelector<HTMLElement>(`[data-event-id="${hit.id}"]`);
      el?.scrollIntoView({ block: "center" });
      openAt(hit.id, el ?? null);
    }, 250);
  }

  // ---- keys ----
  function isTyping(): boolean {
    const a = document.activeElement as HTMLElement | null;
    return !!a && (a.tagName === "INPUT" || a.tagName === "TEXTAREA" || a.tagName === "SELECT" || a.isContentEditable);
  }

  function onKeydown(e: KeyboardEvent) {
    if (ui.view !== "calendar" || palette.open || ui.settingsOpen || ui.shortcutsOpen) return;
    if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && e.code === "KeyZ" && !isTyping()) {
      if (calUndo.run()) { e.preventDefault(); e.stopImmediatePropagation(); }
      return;
    }
    if (e.key === "Escape") {
      monthOpen = false;
      if (calendar.search.open && document.activeElement === searchInput) { closeSearch(); e.preventDefault(); e.stopImmediatePropagation(); return; }
      if (calendar.selectedId !== null || calendar.draft || calendar.foreign) {
        closePanel();
        e.preventDefault();
        e.stopImmediatePropagation();
      } else if (calendar.search.open) {
        closeSearch();
        e.preventDefault();
        e.stopImmediatePropagation();
      }
      return;
    }
    if (e.ctrlKey || e.metaKey || e.altKey || isTyping()) return;
    if (e.shiftKey) return;
    const sel = calendar.selected;
    switch (e.code) {
      case "KeyD": case "Digit1": calendar.setView("day"); break;
      case "KeyW": case "Digit2": calendar.setView("week"); break;
      case "KeyM": case "Digit3": calendar.setView("month"); break;
      case "KeyX": case "Digit4": calendar.setView("custom"); break;
      case "KeyA": case "Digit5": calendar.setView("agenda"); break;
      case "KeyY": case "Digit6": calendar.setView("year"); break;
      case "KeyT": calendar.today(); break;
      case "KeyJ": calendar.step(1); break;
      case "KeyK": calendar.step(-1); break;
      case "KeyC": case "KeyN": calendar.newEvent(); break;
      case "KeyR": void calendar.reload(); break;
      case "KeyG": monthOpen = true; break;
      case "Slash": searchInput?.focus(); break;
      case "KeyE":
        if (!sel || !calendar.canEdit(sel)) return;
        calendar.edit(sel.id);
        break;
      case "Delete": case "Backspace":
        if (!sel || !calendar.canEdit(sel) || calendar.editing) return;
        if (sel.recurring_event_id) { toast.show({ text: t("fork.cal.delete_recurring_hint") }); break; }
        void deleteWithUndo(sel, false).then((ok) => { if (ok) closePanel(); }).catch((err) => toast.show({ text: calendarErrorText(err) }));
        break;
      default:
        return;
    }
    e.preventDefault();
    e.stopImmediatePropagation();
  }

  // Keyed on the resolved row (not the id): an id chosen before the window
  // loaded resolves later, and the panel must re-init on that row.
  const panelRow = $derived(calendar.selected ?? calendar.foreign);
  const editorOpen = $derived(calendar.draft !== null || (calendar.selected !== null && calendar.editing));
  const cardOpen = $derived(panelRow !== null && !calendar.editing);
  const panelKey = $derived(calendar.selected ? `row-${calendar.selected.id}` : "draft");
  const setupText = $derived(
    !calendar.configured ? t("fork.cal.setup_unconfigured") : !calendar.connected ? t("fork.cal.setup_disconnected") : null,
  );
  const weekTag = $derived(timed(calendar.view) && calendar.view !== "day" ? t("fork.cal.week_n", { n: isoWeek(calendar.date) }) : "");
</script>

<svelte:window onkeydowncapture={onKeydown} />

<section class="fork-cal look-a" class:with-panel={editorOpen} aria-label={t("fork.nav.calendar")}>
  {#if calPrefs.railOpen && !editorOpen}<CalendarRail range={visibleRange} />{/if}
  <div class="main">
    <header class="bar">
      <button class="icon" onclick={() => calPrefs.setRailOpen(!calPrefs.railOpen)} aria-label={t("fork.cal.toggle_rail")} aria-pressed={calPrefs.railOpen} title={t("fork.cal.toggle_rail")}>
        <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M2.5 4h11M2.5 8h11M2.5 12h11" /></svg>
      </button>
      <button class="outline" onclick={() => calendar.today()} title="T">{t("fork.cal.today")}</button>
      <div class="nav">
        <button class="icon" onclick={() => calendar.step(-1)} aria-label={t("fork.cal.prev")} title="K">
          <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M10 4L6 8l4 4" /></svg>
        </button>
        <button class="icon" onclick={() => calendar.step(1)} aria-label={t("fork.cal.next")} title="J">
          <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M6 4l4 4-4 4" /></svg>
        </button>
      </div>
      <div class="month-anchor">
        <button class="title" onclick={() => (monthOpen = !monthOpen)} aria-expanded={monthOpen} aria-label={t("fork.cal.choose_date")} title="G">{calendar.view === "year" ? calendar.date.getFullYear() : title}</button>
        {#if weekTag}<span class="week-tag">{weekTag}</span>{/if}
        {#if monthOpen}<div class="month-popup"><MiniMonth selected={calendar.date} weekStart={calPrefs.weekStart} onselect={(d) => { calendar.goto(d); monthOpen = false; }} /></div>{/if}
      </div>
      {#if calendar.loading}<span class="spinner" role="status" aria-label={t("fork.cal.refreshing")}></span>{/if}
      <div class="spacer"></div>
      <div class="views" role="radiogroup" aria-label={t("fork.cal.views")}>
        {#each VIEWS as v (v.key)}
          {@const active = calendar.view === v.key || (v.key === "week" && calendar.view === "workweek")}
          <button class:active role="radio" aria-checked={active} title={v.hint} onclick={() => calendar.setView(v.key)}>
            {v.key === "custom" ? t("fork.cal.view_n_days", { n: calPrefs.customDays }) : t(v.label)}
          </button>
        {/each}
      </div>
      <label class="search" class:active={calendar.search.open}>
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"><circle cx="7" cy="7" r="4.5" /><path d="m10.5 10.5 3 3" /></svg>
        <input bind:this={searchInput} type="search" placeholder={t("fork.cal.search_events")} value={searchText} oninput={(e) => onSearchInput(e.currentTarget.value)} aria-label={t("fork.cal.search_events")} />
        <kbd>/</kbd>
      </label>
      <button class="new" onclick={() => calendar.newEvent()} disabled={!calendar.connected} title="C">
        <span class="plus">+</span><span class="label">{t("fork.cal.new_event")}</span>
      </button>
    </header>

    {#if calendar.failedEvents.length}<div class="sync-alert" role="status"><span>{t("fork.cal.failed_changes", { count: calendar.failedEvents.length })}</span><button onclick={() => calendar.retry().catch((e) => toast.show({ text: calendarErrorText(e) }))}>{t("fork.cal.retry")}</button></div>{/if}
    {#if setupText}
      <div class="setup">
        <p>{setupText}</p>
        <button class="outline" onclick={() => ui.openSettings()}>{t("fork.cal.open_settings")}</button>
      </div>
    {/if}
    {#if calendar.error}
      <div class="error" role="alert">{calendar.error} <button onclick={() => calendar.reload()}>{t("fork.cal.retry")}</button></div>
    {/if}
    {#if calPrefs.secondTz && timed(calendar.view) && !calendar.search.open}<div class="zone-legend" title={t("fork.cal.zone_anchor", { date: calendar.date.toLocaleDateString(getLocale()) })}><span>{calPrefs.secondTz.split("/").at(-1)?.replaceAll("_", " ")}</span><span>{localZone().split("/").at(-1)?.replaceAll("_", " ")}</span></div>{/if}

    <div class="grid" class:second-clock={!!calPrefs.secondTz} class:hidden-grid={calendar.view === "year" || calendar.search.open} bind:this={gridEl}>
      <Calendar plugins={PLUGINS} {options}>
        {#snippet eventContent({ event, timeText })}
          <span class="cal-event-content">
            <span class="cal-event-title">{#if event.extendedProps.priv}<svg class="lock" viewBox="0 0 12 12" aria-hidden="true"><rect x="2.5" y="5.5" width="7" height="5" rx="1" /><path d="M4 5.5V4a2 2 0 0 1 4 0v1.5" /></svg>{/if}<span class="cal-state-mark" aria-hidden="true">{event.extendedProps.state === "declined" ? "× " : event.extendedProps.state === "cancelled" ? `${t("fork.cal.state_cancelled")}: ` : ""}</span>{event.title}</span>
            {#if !event.allDay && !event.extendedProps.short}<span class="cal-event-time">{timeText}</span>{/if}
            {#if event.extendedProps.needs}<span class="cal-reply">{t("fork.cal.reply_pill")}</span>{/if}
          </span>
        {/snippet}
      </Calendar>
    </div>
    {#if calendar.view === "year" && !calendar.search.open}
      <div class="overlay"><YearView year={calendar.date.getFullYear()} events={shownRows} weekStart={calPrefs.weekStart} onday={(d) => { calendar.goto(d); calendar.setView("day"); }} /></div>
    {/if}
    {#if calendar.search.open}
      <div class="overlay"><SearchPanel onpick={pickHit} /></div>
    {/if}
  </div>

  {#if cardOpen && panelRow}
    {#key panelRow.id || "foreign"}
      <EventPopover row={panelRow} {anchor} {bounds} onclose={closePanel} />
    {/key}
  {/if}
  {#if editorOpen}
    {#key panelKey}
      <EventPanel row={calendar.selected} draft={calendar.draft} onclose={closePanel} />
    {/key}
  {/if}
</section>

<GuestsPrompt />

<style>
  .fork-cal { flex: 1; min-width: 0; display: flex; background: var(--surface); position: relative; }
  .main { flex: 1; min-width: 0; display: flex; flex-direction: column; position: relative; container-type: inline-size; container-name: calmain; }
  .bar { display: flex; align-items: center; gap: 8px; padding: 10px 14px 10px 10px; border-bottom: 1px solid var(--hairline); flex-shrink: 0; min-width: 0; }
  .spacer { flex: 1; min-width: 8px; }
  .icon { width: 30px; height: 30px; display: grid; place-items: center; border-radius: var(--radius-s); color: var(--text-dim); flex-shrink: 0; }
  .icon:hover { background: var(--hover); color: var(--text); }
  .icon[aria-pressed="false"] { color: var(--text-faint); }
  .outline { padding: 5px 13px; border: 1px solid var(--hairline-strong); border-radius: 99px; font-size: 13px; background: var(--surface-raised); flex-shrink: 0; }
  .outline:hover { background: var(--hover); }
  .nav { display: flex; gap: 0; flex-shrink: 0; }
  .month-anchor { position: relative; display: flex; align-items: baseline; gap: 8px; min-width: 0; flex: 0 1 auto; }
  .title { font-size: 17px; font-weight: 700; letter-spacing: -0.015em; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; padding: 3px 4px; border-radius: var(--radius-s); }
  .title:hover { background: var(--hover); }
  .week-tag { font-size: 12px; color: var(--text-faint); white-space: nowrap; }
  .month-popup { position: absolute; top: calc(100% + 8px); left: 0; z-index: 30; }
  .views { display: flex; padding: 2px; border: 1px solid var(--hairline-strong); border-radius: 99px; flex-shrink: 0; background: var(--surface-raised); }
  .views button { padding: 4px 11px; border-radius: 99px; font-size: 12.5px; color: var(--text-dim); white-space: nowrap; }
  .views button:hover { color: var(--text); background: var(--hover); }
  .views button.active { background: var(--text); color: var(--bg); font-weight: 600; }
  .search { display: flex; align-items: center; gap: 7px; padding: 5px 10px; width: 190px; border: 1px solid var(--hairline-strong); border-radius: var(--radius-m); background: var(--surface-raised); color: var(--text-faint); flex-shrink: 1; min-width: 120px; }
  .search.active, .search:focus-within { border-color: var(--text); color: var(--text); }
  .search input { flex: 1; min-width: 0; border: 0; outline: 0; background: transparent; color: var(--text); font-size: 12.5px; }
  .search input::-webkit-search-cancel-button { display: none; }
  .search kbd { font-family: var(--font-mono); font-size: 10.5px; padding: 0 5px; border: 1px solid var(--hairline-strong); border-radius: 4px; color: var(--text-faint); }
  .new { display: inline-flex; align-items: center; gap: 6px; padding: 6px 13px; border-radius: var(--radius-m); background: var(--primary); color: var(--on-primary); font-weight: 600; font-size: 13px; flex-shrink: 0; }
  .new:hover { filter: brightness(0.97); }
  .new:disabled { opacity: 0.35; cursor: default; }
  .plus { font-size: 15px; line-height: 1; }
  .grid { flex: 1; min-height: 0; padding: 0 10px 8px 0; }
  .grid.hidden-grid { visibility: hidden; }
  .overlay { position: absolute; inset: 52px 0 0 0; background: var(--surface); }
  .setup { display: flex; align-items: center; gap: 12px; margin: 10px 16px 0; padding: 10px 14px; border: 1px dashed var(--hairline-strong); border-radius: var(--radius-m); }
  .setup p { flex: 1; margin: 0; font-size: 13px; color: var(--text-dim); }
  .error, .sync-alert { margin: 8px 16px 0; font-size: 12.5px; color: var(--danger); display: flex; gap: 10px; }
  .error button, .sync-alert button { text-decoration: underline; }
  .zone-legend { display: flex; gap: 12px; font-size: 10px; color: var(--text-dim); padding: 4px 10px 0; }
  .zone-legend span { max-width: 90px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .spinner { width: 11px; height: 11px; border: 1.5px solid var(--text-faint); border-top-color: var(--text); border-radius: 50%; animation: spin 0.9s linear infinite; flex-shrink: 0; }
  @keyframes spin { to { transform: rotate(360deg); } }
  :global(:root[data-theme="pastel-light"]) .fork-cal .new { background: var(--p-mint); color: var(--ink); border: 1px solid var(--ink); }
  :global(:root[data-theme="pastel-light"]) .fork-cal .outline, :global(:root[data-theme="pastel-light"]) .fork-cal .views, :global(:root[data-theme="pastel-light"]) .fork-cal .search { border-color: var(--ink); background: #fff; }
  /* The bar follows the width it has, which shrinks when the editor is open. */
  @container calmain (max-width: 1060px) { .search { width: 150px; } .week-tag { display: none; } }
  @container calmain (max-width: 940px) { .search { width: 34px; min-width: 34px; padding: 5px 9px; } .search input, .search kbd { display: none; } .search:focus-within { width: 160px; } .search:focus-within input { display: block; } }
  @container calmain (max-width: 820px) { .views button { padding: 4px 8px; } .new .label { display: none; } .bar > .icon:first-child { display: none; } }
  @container calmain (max-width: 700px) { .views { display: none; } }
  @media (max-width: 1100px) {
    .fork-cal.with-panel .main { display: none; }
    .fork-cal.with-panel :global(.panel) { width: 100%; border-left: 0; }
  }
  @media (prefers-reduced-motion: reduce) { .spinner { animation: none; } }
</style>
