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
    fromDateTime,
    guestsValue,
    localDate,
    localTime,
    localZone,
    otherGuests,
    selfIsGuest,
    shiftDate,
    splitAddresses,
  } from "./guests";
  import type { EventInput, EventRow, RsvpResponse } from "./types";

  interface Props {
    row: EventRow | null;
    draft: Draft | null;
    onclose: () => void;
  }
  let { row: rowProp, draft: draftProp, onclose }: Props = $props();

  // The parent re-keys this component per event, so the fields are read once
  // from the props at creation; `untrack` says so to the compiler.
  const row = untrack(() => rowProp);
  const draft = untrack(() => draftProp);
  const own = calendar.ownEmails;
  const isNew = row === null;
  const editable = row ? calendar.canEdit(row) : calendar.connected;
  const others = row ? otherGuests(row, own) : [];
  const iAmGuest = row ? selfIsGuest(row, own) : false;

  // ---- fields ----
  const start0 = row ? new Date(row.start_ts * 1000) : (draft?.start ?? new Date());
  const end0 = row ? new Date(row.end_ts * 1000) : (draft?.end ?? new Date(start0.getTime() + 30 * 60_000));
  const allDay0 = row ? row.all_day : (draft?.allDay ?? false);
  let title = $state(row?.summary ?? "");
  let allDay = $state(allDay0);
  let startDate = $state(row?.all_day && row.start_date ? row.start_date : localDate(start0));
  let startTime = $state(localTime(start0));
  // The UI shows an inclusive end date; the API's end_date is exclusive.
  let endDate = $state(
    row?.all_day && row.end_date ? shiftDate(row.end_date, -1) : localDate(allDay0 ? new Date(end0.getTime() - 1) : end0),
  );
  let endTime = $state(localTime(end0));
  let calendarId = $state<number>(row?.calendar_id ?? calendar.writableCalendars.find((c) => c.is_primary)?.id ?? calendar.writableCalendars[0]?.id ?? 0);
  let guests = $state(row ? guestsValue(row, own) : "");
  let location = $state(row?.location ?? "");
  let description = $state(row?.description ?? "");
  let addMeet = $state(false);

  let saving = $state(false);
  let error = $state<string | null>(null);
  let deleteStep = $state<0 | 1>(0);
  let rsvpBusy = $state(false);

  const calRow = $derived(row ? calendar.calendarOf(row) : calendar.calendars.find((c) => c.id === calendarId));
  const timeInvalid = $derived.by(() => {
    if (allDay) return endDate < startDate;
    return fromDateTime(endDate, endTime).getTime() <= fromDateTime(startDate, startTime).getTime();
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
      const s = Math.floor(fromDateTime(startDate, startTime).getTime() / 1000);
      const e = Math.floor(fromDateTime(endDate, endTime).getTime() / 1000);
      if (isNew || row?.all_day || s !== row?.start_ts || e !== row?.end_ts) {
        input.all_day = false;
        input.start_ts = s;
        input.end_ts = e;
        input.time_zone = localZone();
      }
    }
    if (addMeet && !row?.hangout_link) input.add_meet = true;
    return input;
  }

  async function save() {
    if (saving || timeInvalid || !editable) return;
    error = null;
    saving = true;
    try {
      const input = buildInput();
      if (row) {
        if (Object.keys(input).length === 0) {
          onclose();
          return;
        }
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
    if (!row || saving) return;
    if (deleteStep === 0) {
      deleteStep = 1;
      return;
    }
    error = null;
    saving = true;
    try {
      if (await calendar.remove(row.id)) onclose();
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
    <input class="title" placeholder={t("fork.cal.title_placeholder")} bind:value={title} readonly={!editable} autofocus={isNew} />

    {#if row}
      <div class="when-line">{fmtWhen(row)}</div>
    {/if}

    <label class="row check">
      <input type="checkbox" bind:checked={allDay} disabled={!editable} />
      <span>{t("fork.cal.all_day")}</span>
    </label>

    <div class="row times" class:invalid={timeInvalid}>
      <span class="label">{t("fork.cal.starts")}</span>
      <input type="date" bind:value={startDate} readonly={!editable} />
      {#if !allDay}<input type="time" step="300" bind:value={startTime} readonly={!editable} />{/if}
    </div>
    <div class="row times" class:invalid={timeInvalid}>
      <span class="label">{t("fork.cal.ends")}</span>
      <input type="date" bind:value={endDate} readonly={!editable} />
      {#if !allDay}<input type="time" step="300" bind:value={endTime} readonly={!editable} />{/if}
    </div>
    {#if timeInvalid}<div class="hint danger">{t("fork.cal.ends_before_starts")}</div>{/if}

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

    <textarea class="desc" bind:value={description} readonly={!editable} placeholder={editable ? t("fork.cal.description_placeholder") : ""} rows="4"></textarea>

    {#if row && iAmGuest}
      <div class="rsvp">
        <span class="label">{t("fork.cal.going")}</span>
        {#each ["accepted", "declined", "tentative"] as const as r (r)}
          <button class="btn small" class:active={row.self_response === r} disabled={rsvpBusy} onclick={() => rsvp(r)}>
            {t(`fork.cal.rsvp_btn_${r}`)}
          </button>
        {/each}
      </div>
    {/if}

    {#if error}<div class="hint danger" role="alert">{error}</div>{/if}
  </div>

  <div class="foot">
    {#if editable}
      <button class="btn primary" onclick={save} disabled={saving || timeInvalid}>
        {isNew ? t("fork.cal.create") : t("fork.cal.save")}
      </button>
    {/if}
    {#if row && others.length > 0}
      <button class="btn" onclick={() => prepOpen.open(row!.id)}>{t("fork.prep.prep")}</button>
    {/if}
    {#if row?.html_link}
      <button class="btn ghost" onclick={() => openUrl(row!.html_link!)} title={row.html_link}>{t("fork.cal.open_in_google")}</button>
    {/if}
    {#if row && editable}
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
  .panel {
    width: 360px;
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
    padding: 12px 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .title {
    font-size: 17px;
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
  .row input[type="date"],
  .row input[type="time"] {
    flex: 0 1 auto;
    font-family: var(--font-mono);
    font-size: 12px;
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
    padding-left: 72px;
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
    white-space: nowrap;
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
    background: var(--text);
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
    align-items: center;
    gap: 6px;
    padding-top: 4px;
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
    background: var(--text);
    color: var(--bg);
    border-color: var(--text);
  }
  .btn.primary:hover,
  .btn.active:hover {
    background: var(--text);
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
