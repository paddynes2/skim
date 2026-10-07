<script lang="ts">
  // The event panel (PLAN.md 7.5): right, 360px. One component for both an
  // existing row and a quick-create draft; the parent re-keys it per event so
  // the fields initialise once. Guests reuse the upstream AddressInput. Every
  // write goes through `calendar` (store), which asks about guests before it
  // names `sendUpdates`. Delete is two clicks. "Prep" opens the Phase 11 panel
  // when the event has guests other than Patrick.
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { untrack } from "svelte";
  import AddressInput from "../../components/AddressInput.svelte";
  import { t } from "../../lib/i18n/index.svelte";
  import { prepOpen } from "../prep/open.svelte";
  import type { Draft } from "./store.svelte";
  import { calendar } from "./store.svelte";
  import {
    calendarErrorText,
    fmtWhen,
    guestsValue,
    localDate,
    localTime,
    localZone,
    otherGuests,
    parseAttendees,
    selfIsGuest,
    shiftDate,
    splitAddresses,
  } from "./guests";
  import type { EventInput, EventRow, RsvpResponse } from "./types";
  // Fork (v1.1.1): typed time field, answer bar on top, propose a new time.
  import FindTime from "./FindTime.svelte";
  import { overlappingEvents } from "./planning";
  import ProposeTime from "./ProposeTime.svelte";
  import TimeInput from "./TimeInput.svelte";
  import { shiftEnd } from "./time";
  import { calendarApi } from "./api";
  import OptionsEditor from "./OptionsEditor.svelte";
  import RepeatEditor from "./RepeatEditor.svelte";
  import { changedOptions, eventOptions, zonedParts, eventTimestamp, zonedTimestamp } from "./editor";

  interface Props {
    row: EventRow | null;
    draft: Draft | null;
    onclose: () => void;
  }
  let { row: rowProp, draft: draftProp, onclose }: Props = $props();

  // The parent re-keys this component per event, so the fields are read once
  // from the props at creation; `untrack` says so to the compiler.
  const originalRow = untrack(() => rowProp);
  let row = $state(originalRow);
  let seriesMode = $state(false);
  let followingMode = $state(false);
  let loadingSeries = $state(false);
  const draft = untrack(() => draftProp);
  const seed = originalRow ?? draft?.seed ?? null;
  const own = calendar.ownEmails;
  const isNew = originalRow === null;
  const editable = $derived(row ? calendar.canEdit(row) : calendar.connected);
  const others = $derived(row ? otherGuests(row, own) : []);
  const iAmGuest = $derived(row ? selfIsGuest(row, own) : false);

  // ---- fields ----
  const start0 = seed ? new Date(seed.start_ts * 1000) : (draft?.start ?? new Date());
  const end0 = seed ? new Date(seed.end_ts * 1000) : (draft?.end ?? new Date(start0.getTime() + 30 * 60_000));
  const allDay0 = seed ? seed.all_day : (draft?.allDay ?? false);
  let title = $state(seed?.summary ?? "");
  let allDay = $state(allDay0);
  let startDate = $state(seed?.all_day && seed.start_date ? seed.start_date : localDate(start0));
  let startTime = $state(localTime(start0));
  // The UI shows an inclusive end date; the API's end_date is exclusive.
  let endDate = $state(
    seed?.all_day && seed.end_date ? shiftDate(seed.end_date, -1) : localDate(allDay0 ? new Date(end0.getTime() - 1) : end0),
  );
  let endTime = $state(localTime(end0));
  let calendarId = $state<number>(seed?.calendar_id ?? calendar.writableCalendars.find((c) => c.is_primary)?.id ?? calendar.writableCalendars[0]?.id ?? 0);
  let guests = $state(seed ? guestsValue(seed, own) : "");
  let location = $state(seed?.location ?? "");
  let description = $state(seed?.description ?? "");
  let addMeet = $state(false);
  let zone = $state(seed?.time_zone ?? localZone());
  let options = $state(eventOptions(seed?.options, seed?.transparency));
  let optionalGuests = $state<string[]>(seed ? parseAttendees(seed).filter((a) => a.optional).map((a) => a.email.toLowerCase()) : []);
  let repeatInvalid = $state(false);
  const zones = Intl.supportedValuesOf("timeZone");
  let previousZone = untrack(() => zone);
  if (!allDay0) {
    const start = zonedParts(start0.getTime() / 1000, previousZone);
    const end = zonedParts(end0.getTime() / 1000, previousZone);
    startDate = start.date; startTime = start.time; endDate = end.date; endTime = end.time;
  }
  const guestAddresses = $derived(splitAddresses(guests));
  const remindersInvalid = $derived(options.reminders?.overrides?.some((r) => !Number.isInteger(r.minutes) || r.minutes < 0 || r.minutes > 40320) ?? false);
  async function changeScope(nextScope: "occurrence" | "following" | "series") {
    if (!originalRow || loadingSeries) return;
    if (Object.keys(buildInput()).length > 0) { error = t("fork.cal.scope_unsaved"); return; }
    loadingSeries = true; error = null;
    try {
      let next = nextScope === "series" ? await calendarApi.series(originalRow.id) : originalRow;
      if (nextScope === "following") {
        const master = await calendarApi.series(originalRow.id);
        next = { ...originalRow, options: { ...originalRow.options, recurrence: master.options?.recurrence } };
      }
      seriesMode = nextScope === "series"; followingMode = nextScope === "following"; row = next;
      title = next.summary; allDay = next.all_day;
      zone = next.time_zone ?? localZone(); previousZone = zone;
      const s = zonedParts(next.start_ts, zone), e = zonedParts(next.end_ts, zone);
      startDate = next.all_day ? next.start_date! : s.date;
      endDate = next.all_day ? shiftDate(next.end_date!, -1) : e.date;
      startTime = s.time; endTime = e.time;
      prevStart = { date: startDate, time: startTime };
      guests = guestsValue(next, own); location = next.location ?? ""; description = next.description ?? "";
      optionalGuests = parseAttendees(next).filter((a) => a.optional).map((a) => a.email.toLowerCase());
      options = eventOptions(next.options, next.transparency); repeatInvalid = false;
    } catch (e) { error = calendarErrorText(e); }
    finally { loadingSeries = false; }
  }
  function zoneChanged() {
    const s = eventTimestamp(startDate, startTime, previousZone, row?.start_ts), e = eventTimestamp(endDate, endTime, previousZone, row?.end_ts);
    try {
      if (s !== null && e !== null && !allDay) {
        const start = zonedParts(s, zone), end = zonedParts(e, zone);
        startDate = start.date; startTime = start.time; endDate = end.date; endTime = end.time;
      }
      new Intl.DateTimeFormat("en", { timeZone: zone });
      previousZone = zone; prevStart = { date: startDate, time: startTime };
    } catch { /* Validation keeps Save disabled until the zone is valid. */ }
  }

  let saving = $state(false);
  let error = $state<string | null>(null);
  let deleteStep = $state<0 | 1>(0);
  let rsvpBusy = $state(false);
  let proposing = $state(false);
  const organizer = $derived(row ? parseAttendees(row).find((a) => a.organizer) : undefined);
  const organizerEmail = $derived(row?.organizer_email ?? organizer?.email ?? null);

  // Moving the start carries the end with it, keeping the length.
  let prevStart = untrack(() => ({ date: startDate, time: startTime }));
  function startMoved() {
    const next = shiftEnd(prevStart, { date: endDate, time: endTime }, { date: startDate, time: startTime });
    endDate = next.date;
    endTime = next.time;
    prevStart = { date: startDate, time: startTime };
  }

  const calRow = $derived(row ? calendar.calendarOf(row) : calendar.calendars.find((c) => c.id === calendarId));
  const timeInvalid = $derived.by(() => {
    if (allDay) return endDate < startDate;
    const s = eventTimestamp(startDate, startTime, zone, row?.start_ts), e = eventTimestamp(endDate, endTime, zone, row?.end_ts);
    return s === null || e === null || e <= s;
  });

  const proposedStart = $derived(allDay ? zonedTimestamp(startDate, "00:00", zone) : eventTimestamp(startDate, startTime, zone, row?.start_ts));
  const proposedEnd = $derived(allDay ? zonedTimestamp(shiftDate(endDate, 1), "00:00", zone) : eventTimestamp(endDate, endTime, zone, row?.end_ts));
  let conflictRows = $state<EventRow[]>([]);
  const conflicts = $derived(overlappingEvents(conflictRows, proposedStart ?? NaN, proposedEnd ?? NaN, originalRow?.id, (date) => zonedTimestamp(date, "00:00", zone)));
  $effect(() => {
    const start = proposedStart, end = proposedEnd;
    let active = true;
    if (start !== null && end !== null && end > start) {
      const timer = setTimeout(() => { void calendarApi.events(start - 86400, end + 86400).then((rows) => { if (active) conflictRows = rows; }).catch(() => { if (active) conflictRows = []; }); }, 250);
      return () => { active = false; clearTimeout(timer); };
    }
    conflictRows = [];
  });

  function buildInput(): EventInput {
    const input: EventInput = {};
    const set = <K extends keyof EventInput>(k: K, v: EventInput[K], was: EventInput[K] | undefined) => {
      if (isNew || v !== was) input[k] = v;
    };
    set("summary", title.trim(), row?.summary);
    set("location", location.trim(), row?.location ?? "");
    set("description", description, row?.description ?? "");
    const wantGuests = splitAddresses(guests);
    const hadGuests = row ? others.map((a) => a.email.toLowerCase()) : [];
    if (isNew ? wantGuests.length > 0 : wantGuests.join(",") !== hadGuests.join(",")) input.attendees = wantGuests;
    if (allDay) {
      const s = startDate;
      const e = shiftDate(endDate, 1);
      if (isNew || !row?.all_day || s !== row.start_date || e !== row.end_date) {
        input.all_day = true;
        input.start_date = s;
        input.end_date = e;
      }
    } else {
      const s = eventTimestamp(startDate, startTime, zone, row?.start_ts)!;
      const e = eventTimestamp(endDate, endTime, zone, row?.end_ts)!;
      if (isNew || row?.all_day || s !== row?.start_ts || e !== row?.end_ts || zone !== (row?.time_zone ?? localZone())) {
        input.all_day = false;
        input.start_ts = s;
        input.end_ts = e;
        input.time_zone = zone;
      }
    }
    if (!allDay && zone !== (row?.time_zone ?? localZone())) input.time_zone = zone;
    const changed = isNew ? options : changedOptions(options, eventOptions(row?.options, row?.transparency));
    if (Object.keys(changed).length) input.options = changed;
    const hadOptional = row ? parseAttendees(row).filter((a) => a.optional).map((a) => a.email.toLowerCase()).sort() : [];
    const wantOptional = optionalGuests.filter((e) => guestAddresses.includes(e)).sort();
    if (JSON.stringify(wantOptional) !== JSON.stringify(hadOptional)) input.optional_attendees = wantOptional;
    if (addMeet && !row?.hangout_link) input.add_meet = true;
    return input;
  }

  async function save() {
    if (saving || timeInvalid || repeatInvalid || remindersInvalid || loadingSeries || !editable) return;
    error = null;
    saving = true;
    try {
      const input = buildInput();
      if (row) {
        if (Object.keys(input).length === 0) {
          onclose();
          return;
        }
        if (seriesMode) input.series = true;
        if (followingMode) input.following = true;
        if (await calendar.patch(row.id, input)) onclose();
      } else {
        if (!calendarId) throw { code: "gcal_input", message: t("fork.cal.err.no_calendar") };
        const created = await calendar.create(calendarId, input);
        if (created) onclose();
      }
    } catch (e: unknown) {
      error = calendarErrorText(e);
    } finally {
      saving = false;
    }
  }

  async function remove() {
    if (!row || saving || followingMode) return;
    if (deleteStep === 0) {
      deleteStep = 1;
      return;
    }
    error = null;
    saving = true;
    try {
      if (await calendar.remove(row.id, seriesMode)) onclose();
      else deleteStep = 0;
    } catch (e: unknown) {
      error = calendarErrorText(e);
      deleteStep = 0;
    } finally {
      saving = false;
    }
  }

  async function rsvp(response: RsvpResponse) {
    if (!row || rsvpBusy) return;
    rsvpBusy = true;
    error = null;
    try {
      await calendar.rsvp(row.id, response);
    } catch (e: unknown) {
      error = calendarErrorText(e);
    } finally {
      rsvpBusy = false;
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
      e.preventDefault();
      void save();
    }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<aside class="panel" aria-label={isNew ? t("fork.cal.new_event") : t("fork.cal.event")} onkeydown={onKeydown}>
  <div class="head">
    <span class="microlabel">{isNew ? t("fork.cal.new_event") : (calRow?.summary ?? t("fork.cal.event"))}</span>
    <button class="x" onclick={onclose} aria-label={t("fork.cal.close")} title="Esc">×</button>
  </div>

  <div class="body">
    <!-- svelte-ignore a11y_autofocus -->
    <textarea class="title" rows="2" aria-label={t("fork.cal.title_placeholder")} placeholder={t("fork.cal.title_placeholder")} bind:value={title} readonly={!editable} autofocus={isNew}></textarea>

    {#if row}
      <div class="when-line">{fmtWhen(row)}</div>
    {/if}

    {#if row && iAmGuest && !seriesMode && !followingMode}
      <div class="rsvp">
        <span class="label">{t("fork.cal.going")}</span>
        {#each ["accepted", "tentative", "declined"] as const as r (r)}
          <button class="btn small" class:active={row.self_response === r} disabled={rsvpBusy} onclick={() => rsvp(r)}>
            {t(`fork.cal.rsvp_btn_${r}`)}
          </button>
        {/each}
        {#if organizerEmail}
          <button class="btn small" class:active={proposing} onclick={() => (proposing = !proposing)}>
            {t("fork.cal.propose")}
          </button>
        {/if}
      </div>
      {#if proposing && organizerEmail}
        <ProposeTime
          summary={row.summary}
          {organizerEmail}
          organizerName={organizer?.displayName ?? null}
          startTs={row.start_ts}
          endTs={row.end_ts}
          onproposed={() => (proposing = false)}
          oncancel={() => (proposing = false)}
        />
      {/if}
    {/if}

    {#if originalRow?.recurring_event_id}
      <div class="series-scope">
        <label for="event-scope">{t("fork.cal.apply_changes")}</label>
        <select id="event-scope" value={seriesMode ? "series" : followingMode ? "following" : "occurrence"} disabled={loadingSeries || saving} onchange={(e) => { const next = e.currentTarget.value as "occurrence" | "following" | "series"; e.currentTarget.value = seriesMode ? "series" : followingMode ? "following" : "occurrence"; void changeScope(next); }}>
          <option value="occurrence">{t("fork.cal.editing_occurrence")}</option><option value="following">{t("fork.cal.edit_following")}</option><option value="series">{t("fork.cal.editing_series")}</option>
        </select>
        {#if followingMode}<span>{t("fork.cal.following_note")}</span>{/if}
      </div>
    {/if}
    <h3>{t("fork.cal.date_time")}</h3>
    <label class="row check">
      <input type="checkbox" bind:checked={allDay} disabled={!editable} />
      <span>{t("fork.cal.all_day")}</span>
    </label>

    <div class="row times" class:invalid={timeInvalid}>
      <span class="label">{t("fork.cal.starts")}</span>
      <input type="date" aria-label={t("fork.cal.starts")} bind:value={startDate} readonly={!editable} onchange={startMoved} />
      {#if !allDay}<TimeInput bind:value={startTime} readonly={!editable} label={t("fork.cal.starts")} onchange={startMoved} />{/if}
    </div>
    <div class="row times" class:invalid={timeInvalid}>
      <span class="label">{t("fork.cal.ends")}</span>
      <input type="date" aria-label={t("fork.cal.ends")} bind:value={endDate} readonly={!editable} />
      {#if !allDay}<TimeInput bind:value={endTime} anchor={endDate === startDate ? startTime : null} readonly={!editable} label={t("fork.cal.ends")} />{/if}
    </div>
    {#if timeInvalid}<div class="hint danger">{t("fork.cal.time_invalid")}</div>{/if}
    {#if !allDay}
      <label class="row"><span class="label">{t("fork.cal.time_zone")}</span><input list="event-time-zones" aria-label={t("fork.cal.time_zone")} bind:value={zone} onchange={zoneChanged} readonly={!editable} /></label>
      <datalist id="event-time-zones">{#each zones as z}<option value={z}></option>{/each}</datalist>
    {/if}
    {#key `${seriesMode}-${followingMode}`}
      {#if !originalRow?.recurring_event_id || seriesMode || followingMode}
        <RepeatEditor bind:value={options.recurrence} {startDate} {allDay} {zone} disabled={!editable || loadingSeries} bind:invalid={repeatInvalid} />
      {/if}
    {/key}
    {#if conflicts.length}<div class="conflicts" role="status"><strong>{t("fork.cal.conflicts", { count: conflicts.length })}</strong>{#each conflicts as conflict}<span>{conflict.summary || t("fork.cal.untitled")} &middot; {fmtWhen(conflict)}</span>{/each}<small>{t("fork.cal.conflicts_scope")}</small></div>{/if}
    <h3 class="section-heading">{t("fork.cal.people_place")}</h3>

    {#if isNew}
      <div class="row">
        <span class="label">{t("fork.cal.calendar")}</span>
        <select bind:value={calendarId} disabled={!editable}>
          {#each calendar.writableCalendars as c (c.id)}
            <option value={c.id}>{c.summary}</option>
          {/each}
        </select>
      </div>
    {/if}

    <div class="row guests">
      <span class="label">{t("fork.cal.guests")}</span>
      {#if editable}
        <AddressInput bind:value={guests} />
      {:else}
        <span class="ro">{guests || "–"}</span>
      {/if}
    </div>
    {#if row && others.length > 0}
      <ul class="attendees">
        {#each others as a (a.email)}
          <li>
            <span class="dot {a.responseStatus ?? 'needsAction'}" aria-hidden="true"></span>
            <span class="who">{a.displayName || a.email}</span>
            <span class="status microlabel">{t(`fork.cal.rsvp_${a.responseStatus ?? "needsAction"}`)}</span>
          </li>
        {/each}
      </ul>
    {/if}

    {#if guestAddresses.length > 0 && editable}
      <div class="guest-options">
        {#each guestAddresses as email}
          <label><span title={email}>{email}</span><input type="checkbox" checked={optionalGuests.includes(email)} onchange={(e) => { optionalGuests = e.currentTarget.checked ? [...optionalGuests, email] : optionalGuests.filter((v) => v !== email); }} />{t("fork.cal.optional_guest")}</label>
        {/each}
      </div>
    {/if}
    {#if editable && !allDay && !timeInvalid && !seriesMode}
      <FindTime accountId={calRow?.account_id ?? calendar.accountId} attendees={guestAddresses} date={startDate} {zone} duration={(proposedEnd ?? 0) - (proposedStart ?? 0)} eventId={originalRow?.id ?? null} onchoose={(start, end) => { const s = zonedParts(start, zone), e = zonedParts(end, zone); startDate = s.date; startTime = s.time; endDate = e.date; endTime = e.time; prevStart = { date: startDate, time: startTime }; }} />
    {/if}
    <div class="row">
      <span class="label">{t("fork.cal.location")}</span>
      <input bind:value={location} readonly={!editable} placeholder={editable ? t("fork.cal.location_placeholder") : ""} />
    </div>

    <div class="row meet">
      <span class="label">{t("fork.cal.meet")}</span>
      {#if row?.hangout_link}
        <button class="btn join" onclick={() => openUrl(row!.hangout_link!)}>{t("fork.cal.join")}</button>
        <span class="link" title={row.hangout_link}>{row.hangout_link.replace(/^https?:\/\//, "")}</span>
      {:else}
        <button
          type="button"
          class="switch"
          class:on={addMeet}
          role="switch"
          aria-checked={addMeet}
          aria-label={t("fork.cal.add_meet")}
          disabled={!editable}
          onclick={() => (addMeet = !addMeet)}
        >
          <span class="knob"></span>
        </button>
        <span class="sw-label">{t("fork.cal.add_meet")}</span>
      {/if}
    </div>

    <h3 class="section-heading">{t("fork.cal.description")}</h3>
    <textarea aria-label={t("fork.cal.description")} class="desc" bind:value={description} readonly={!editable} placeholder={editable ? t("fork.cal.description_placeholder") : ""} rows="4"></textarea>

    <OptionsEditor bind:value={options} disabled={!editable || loadingSeries} />
    {#if remindersInvalid}<div class="hint danger" role="alert">{t("fork.cal.reminders_invalid")}</div>{/if}
    {#if error}<div class="hint danger" role="alert">{error}</div>{/if}
  </div>

  <div class="foot">
    {#if editable}
      <button class="btn primary" onclick={save} disabled={saving || timeInvalid || repeatInvalid || remindersInvalid || loadingSeries}>
        {isNew ? t("fork.cal.create") : t("fork.cal.save")}
      </button>
    {/if}
    {#if row && others.length > 0}
      <button class="btn" onclick={() => prepOpen.open(row!.id)}>{t("fork.prep.prep")}</button>
    {/if}
    {#if row?.html_link}
      <button class="btn ghost" onclick={() => openUrl(row!.html_link!)} title={row.html_link}>{t("fork.cal.open_in_google")}</button>
    {/if}
    {#if row && editable && !followingMode}
      <button class="btn danger" class:armed={deleteStep === 1} onclick={remove} disabled={saving}>
        {deleteStep === 0 ? t("fork.cal.delete") : t("fork.cal.delete_confirm")}
      </button>
      {#if deleteStep === 1}
        <button class="btn ghost" onclick={() => (deleteStep = 0)}>{t("fork.cal.keep")}</button>
      {/if}
    {/if}
  </div>
</aside>

<style>
  .conflicts { display: grid; gap: 6px; padding: 12px; border: 1px solid var(--hairline-strong); border-left: 3px solid var(--danger); border-radius: var(--radius-s); font-size: 12px; line-height: 1.4; }
  .conflicts small { color: var(--text-dim); font-size: 11px; }
  .series-scope select { padding: 7px; border: 1px solid var(--hairline-strong); border-radius: var(--radius-s); background: var(--surface); font-size: 12px; }

  h3 { margin: 0; font-size: 13px; font-weight: 600; }
  .section-heading { margin-top: 8px; padding-top: 20px; border-top: 1px solid var(--hairline); }
  .series-scope { display: grid; gap: 7px; padding: 12px; background: var(--selected); border-radius: var(--radius-s); font-size: 12px; }
  .guest-options { display: grid; gap: 8px; }
  .guest-options label { display: flex; gap: 6px; align-items: center; color: var(--text-dim); font-size: 11px; }
  .guest-options label span { flex: 1; min-width: 0; overflow-wrap: anywhere; }
  .guests :global(.wrap) { flex: 1; min-width: 0; }
  .panel {
    width: clamp(380px, 32vw, 480px);
    max-width: 100%;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    border-left: 1px solid var(--hairline);
    background: var(--surface);
    min-height: 0;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 16px 8px;
    border-bottom: 1px solid var(--hairline);
  }
  .x {
    width: 24px;
    height: 24px;
    border-radius: var(--radius-s);
    font-size: 16px;
    color: var(--text-faint);
  }
  .x:hover {
    background: var(--hover);
    color: var(--text);
  }
  .body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 20px 24px 28px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .title {
    font-size: 21px;
    line-height: 1.35;
    resize: vertical;
    min-height: 62px;
    font-weight: 700;
    letter-spacing: -0.01em;
    padding: 4px 0;
    border-bottom: 1px solid transparent;
    user-select: text;
  }
  .title:focus {
    border-bottom-color: var(--hairline-strong);
  }
  .when-line {
    font-size: 12.5px;
    color: var(--text-dim);
    margin-top: -4px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 28px;
  }
  .row .label {
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 500;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-faint);
    flex: 0 0 64px;
  }
  .row input:not([type="checkbox"]),
  .row select,
  .desc {
    flex: 1;
    min-width: 0;
    padding: 5px 8px;
    border: 1px solid var(--hairline-strong);
    border-radius: var(--radius-s);
    background: var(--surface-raised);
    font-size: 13px;
    user-select: text;
  }
  .row input[type="date"] {
    flex: 0 1 auto;
    font-variant-numeric: tabular-nums;
  }
  .times.invalid input {
    border-color: var(--danger);
  }
  .row input[readonly],
  .desc[readonly] {
    border-color: transparent;
    background: transparent;
    padding-left: 0;
  }
  .check {
    gap: 8px;
    font-size: 13px;
    color: var(--text-dim);
  }
  .check input {
    accent-color: var(--text);
  }
  .guests {
    align-items: flex-start;
  }
  .guests .label {
    padding-top: 7px;
  }
  /* AddressInput draws a bare input; give it the panel's field frame. */
  .guests :global(.wrap input) {
    padding: 5px 8px;
    border: 1px solid var(--hairline-strong);
    border-radius: var(--radius-s);
    background: var(--surface-raised);
    font-size: 13px;
  }
  .ro {
    font-size: 13px;
    color: var(--text-dim);
    user-select: text;
  }
  .attendees {
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding-left: 0;
    font-size: 12.5px;
  }
  .attendees li {
    display: flex;
    align-items: center;
    gap: 7px;
    min-width: 0;
  }
  .who {
    overflow: hidden;
    text-overflow: ellipsis;
    overflow-wrap: anywhere;
  }
  .status {
    margin-left: auto;
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    border: 1.5px solid var(--text-faint);
    flex-shrink: 0;
  }
  .dot.accepted {
    background: var(--success);
    border-color: var(--success);
  }
  .dot.declined {
    background: var(--danger);
    border-color: var(--danger);
  }
  .dot.tentative {
    border-style: dashed;
  }
  .desc {
    flex: none;
    resize: vertical;
    font-family: var(--font-ui);
    line-height: 1.4;
  }
  .meet .link {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .sw-label {
    font-size: 13px;
    color: var(--text-dim);
  }
  .switch {
    width: 34px;
    height: 20px;
    border-radius: 999px;
    background: var(--selected);
    position: relative;
    flex-shrink: 0;
    transition: background 0.16s ease;
  }
  .switch .knob {
    position: absolute;
    top: 3px;
    left: 3px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--text-faint);
    transition:
      transform 0.16s ease,
      background 0.16s ease;
  }
  .switch.on {
    background: var(--primary);
  }
  .switch.on .knob {
    transform: translateX(14px);
    background: var(--bg);
  }
  .switch:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .rsvp {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    padding: 8px 10px;
    border: 1px solid var(--hairline);
    border-radius: var(--radius-m);
    background: var(--hover);
  }
  .rsvp .label {
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-faint);
    flex: 0 0 64px;
  }
  .hint {
    font-size: 12px;
    color: var(--text-dim);
  }
  .hint.danger {
    color: var(--danger);
  }
  .foot {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    padding: 10px 16px 12px;
    border-top: 1px solid var(--hairline);
  }
  .btn {
    padding: 6px 12px;
    border-radius: 999px;
    font-size: 12.5px;
    font-weight: 600;
    border: 1px solid var(--hairline-strong);
    color: var(--text);
  }
  .btn:hover {
    background: var(--hover);
  }
  .btn:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .btn.small {
    padding: 4px 10px;
    font-size: 12px;
  }
  .btn.active,
  .btn.primary {
    background: var(--primary);
    color: var(--on-primary);
    border-color: var(--primary);
  }
  .btn.primary:hover,
  .btn.active:hover {
    background: var(--primary);
    opacity: 0.88;
  }
  .btn.join {
    background: var(--success);
    border-color: var(--success);
    color: #fff;
  }
  .btn.ghost {
    border-color: transparent;
    color: var(--text-dim);
  }
  .btn.danger {
    color: var(--danger);
    border-color: transparent;
    margin-left: auto;
  }
  .btn.danger.armed {
    background: var(--danger);
    border-color: var(--danger);
    color: #fff;
  }
  @media (prefers-reduced-motion: reduce) {
    .switch,
    .switch .knob {
      transition: none;
    }
  }
</style>
