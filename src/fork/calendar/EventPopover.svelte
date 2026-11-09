<script lang="ts">
  // The event card beside the event (Look A, D97). Replaces the side preview:
  // actions on top (Edit, Email guests, Duplicate, Delete, Open in Google),
  // then the title, time, Join, guests, description and "Going?".
  // Placed with position: fixed next to the clicked event, inside `bounds`.
  import { onMount, tick } from "svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { api } from "../../lib/api";
  import { getLocale, t } from "../../lib/i18n/index.svelte";
  import { mail } from "../../lib/stores/mail.svelte";
  import { toast } from "../stores/toast.svelte";
  import AttachmentLinks from "./AttachmentLinks.svelte";
  import { EVENT_COLORS } from "./editor";
  import { calendarErrorText, parseAttendees, selfIsGuest } from "./guests";
  import { safeDescription } from "./description";
  import { eventState, initials, joinLink, needsReply } from "./presentation";
  import { calendar } from "./store.svelte";
  import { calUndo, deleteWithUndo } from "./undo.svelte";
  import type { EventRow, RsvpResponse } from "./types";

  let { row, anchor, bounds, onclose }: { row: EventRow; anchor: DOMRect | null; bounds: DOMRect | null; onclose: () => void } = $props();

  const WIDTH = 340;
  let card: HTMLElement;
  let pos = $state({ left: 0, top: 0, arrow: 24, side: "right" as "right" | "left" | "none", ready: false });

  async function place() {
    await tick();
    if (!card) return;
    const b = bounds ?? new DOMRect(0, 0, window.innerWidth, window.innerHeight);
    const h = card.offsetHeight;
    if (!anchor) {
      pos = { left: b.left + (b.width - WIDTH) / 2, top: Math.max(b.top + 16, b.top + (b.height - h) / 3), arrow: 0, side: "none", ready: true };
      return;
    }
    const gap = 12;
    const roomRight = b.right - anchor.right - gap;
    const side = roomRight >= WIDTH + 8 ? "right" : anchor.left - b.left - gap >= WIDTH + 8 ? "left" : "none";
    const left = side === "right" ? anchor.right + gap : side === "left" ? anchor.left - gap - WIDTH : Math.min(Math.max(b.left + 8, anchor.left), b.right - WIDTH - 8);
    const mid = anchor.top + Math.min(anchor.height, 60) / 2;
    const top = side === "none" ? Math.min(anchor.bottom + 8, b.bottom - h - 8) : Math.min(Math.max(b.top + 8, mid - 40), b.bottom - h - 8);
    pos = { left, top: Math.max(b.top + 8, top), arrow: Math.min(Math.max(16, mid - Math.max(b.top + 8, top)), h - 16), side, ready: true };
  }
  onMount(() => {
    void place();
    heading?.focus({ preventScroll: true });
  });
  $effect(() => {
    void anchor; void row.id;
    void place();
  });

  let heading = $state<HTMLHeadingElement>();
  let error = $state("");
  const foreign = $derived(!row.id);
  const cal = $derived(calendar.calendarOf(row));
  const color = $derived(EVENT_COLORS[Number(row.options?.colorId)] || cal?.color || "var(--acct-1)");
  const join = $derived(joinLink(row));
  const guests = $derived(parseAttendees(row).filter((g) => !g.email.endsWith("resource.calendar.google.com")));
  const iAmGuest = $derived(!foreign && selfIsGuest(row, calendar.ownEmails) && row.status !== "cancelled");
  const canEdit = $derived(!foreign && calendar.canEdit(row));
  const recurring = $derived(!!row.recurring_event_id);
  const evState = $derived(eventState(row));
  const others = $derived(guests.filter((g) => !calendar.ownEmails.includes(g.email.toLowerCase())));

  const when = $derived.by(() => {
    const loc = getLocale();
    if (row.all_day && row.start_date) {
      const s = new Date(`${row.start_date}T12:00:00`);
      const e = row.end_date ? new Date(`${row.end_date}T12:00:00`) : s;
      e.setDate(e.getDate() - 1);
      const f = (d: Date) => d.toLocaleDateString(loc, { weekday: "long", day: "numeric", month: "long" });
      return e > s ? t("fork.cal.range", { from: f(s), to: f(e) }) : f(s);
    }
    const s = new Date(row.start_ts * 1000), e = new Date(row.end_ts * 1000);
    const day = (d: Date) => d.toLocaleDateString(loc, { weekday: "long", day: "numeric", month: "long" });
    const hm = (d: Date) => d.toLocaleTimeString(loc, { hour: "2-digit", minute: "2-digit", hour12: false });
    return s.toDateString() === e.toDateString()
      ? `${day(s)}, ${t("fork.cal.range", { from: hm(s), to: hm(e) })}`
      : t("fork.cal.range", { from: `${day(s)} ${hm(s)}`, to: `${day(e)} ${hm(e)}` });
  });

  const tally = $derived.by(() => {
    if (!guests.length) return "";
    const yes = guests.filter((g) => g.responseStatus === "accepted").length;
    const no = guests.filter((g) => g.responseStatus === "declined").length;
    const maybe = guests.filter((g) => g.responseStatus === "tentative").length;
    const waiting = guests.length - yes - no - maybe;
    if (yes === guests.length) return t("fork.cal.tally_all_yes");
    return [yes && t("fork.cal.tally_yes", { n: yes }), maybe && t("fork.cal.tally_maybe", { n: maybe }), no && t("fork.cal.tally_no", { n: no }), waiting && t("fork.cal.tally_waiting", { n: waiting })].filter(Boolean).join(", ");
  });

  // ---- answer ----
  let note = $state("");
  let noteOpen = $state(false);
  let scope = $state<"one" | "all">("one");
  let busy = $state(false);
  async function answer(r: RsvpResponse) {
    if (busy) return;
    busy = true; error = "";
    const before = row.self_response;
    try {
      const ok = await calendar.rsvp(row.id, r, { comment: noteOpen ? note.trim() : null, series: recurring && scope === "all" });
      if (ok) {
        noteOpen = false;
        if (before && before !== r) calUndo.offer(t("fork.cal.answered", { answer: t(`fork.cal.rsvp_${r}`) }), async () => { await calendar.rsvp(row.id, before as RsvpResponse, { series: recurring && scope === "all" }); });
      }
    } catch (e) { error = calendarErrorText(e); } finally { busy = false; }
  }

  // ---- actions ----
  let confirmDelete = $state(false);
  async function del(series: boolean) {
    confirmDelete = false;
    try { if (await deleteWithUndo(row, series)) onclose(); } catch (e) { error = calendarErrorText(e); }
  }
  async function emailGuests() {
    if (!others.length) return;
    try {
      const own = calendar.ownEmails.map((e) => e.toLowerCase());
      const acc = mail.accounts.find((a) => own.includes(a.email.toLowerCase()) && a.email.toLowerCase() === (cal?.google_id ?? "").toLowerCase())?.id ?? cal?.account_id ?? (await mail.composeAccountId());
      const d = await api.createDraft(acc);
      d.to = others.map((g) => (g.displayName ? `${g.displayName} <${g.email}>` : g.email)).join(", ");
      d.subject = row.summary || t("fork.cal.untitled");
      await api.updateDraft(d);
      await api.openComposeWindow(d.id);
    } catch (e: unknown) {
      toast.show({ text: e instanceof Error ? e.message : String((e as { message?: string })?.message ?? e) });
    }
  }
  const mark = (s?: string) => (s === "accepted" ? "yes" : s === "declined" ? "no" : s === "tentative" ? "maybe" : "wait");
  function openLink(e: MouseEvent) {
    const a = (e.target as HTMLElement)?.closest?.("a");
    if (!a) return;
    e.preventDefault();
    const href = a.getAttribute("href") ?? "";
    if (/^(https:\/\/|mailto:)/i.test(href)) void openUrl(href);
  }
</script>

<div
  bind:this={card}
  class="popover side-{pos.side}"
  class:ready={pos.ready}
  role="dialog"
  aria-label={row.summary || t("fork.cal.untitled")}
  style:left="{pos.left}px"
  style:top="{pos.top}px"
  style:--arrow="{pos.arrow}px"
  style:--cal-color={color}
>
  <div class="tools">
    {#if canEdit}<button title={t("fork.cal.edit_event")} aria-label={t("fork.cal.edit_event")} onclick={() => calendar.edit(row.id)}><svg viewBox="0 0 16 16"><path d="M3 11.5V13h1.5l7-7L10 4.5l-7 7ZM11 3.5 12.5 5l1-1L12 2.5l-1 1Z" /></svg></button>{/if}
    {#if !foreign && others.length}<button title={t("fork.cal.email_guests")} aria-label={t("fork.cal.email_guests")} onclick={emailGuests}><svg viewBox="0 0 16 16"><path d="M2.5 4h11v8h-11zM2.5 4.5 8 9l5.5-4.5" /></svg></button>{/if}
    {#if !foreign}<button title={t("fork.cal.duplicate")} aria-label={t("fork.cal.duplicate")} disabled={!calendar.connected} onclick={() => calendar.duplicate(row)}><svg viewBox="0 0 16 16"><path d="M5.5 5.5h7v7h-7zM3.5 10.5v-7h7" /></svg></button>{/if}
    {#if canEdit}<button title={t("fork.cal.delete")} aria-label={t("fork.cal.delete")} onclick={() => (confirmDelete = !confirmDelete)}><svg viewBox="0 0 16 16"><path d="M3 4.5h10M6.5 4.5V3h3v1.5M4.5 4.5l.5 8.5h6l.5-8.5" /></svg></button>{/if}
    {#if row.html_link}<button title={t("fork.cal.open_in_google")} aria-label={t("fork.cal.open_in_google")} onclick={() => openUrl(row.html_link!)}><svg viewBox="0 0 16 16"><path d="M9 3h4v4M13 3 7.5 8.5M11.5 9.5V13H3V4.5h3.5" /></svg></button>{/if}
    <span class="sep"></span>
    <button title={t("fork.cal.close")} aria-label={t("fork.cal.close")} onclick={onclose}><svg viewBox="0 0 16 16"><path d="m4 4 8 8M12 4l-8 8" /></svg></button>
  </div>

  {#if confirmDelete}
    <div class="confirm" role="alert">
      <span>{t("fork.cal.delete_confirm")}</span>
      {#if recurring}
        <button class="pill" onclick={() => del(false)}>{t("fork.cal.this_event")}</button>
        <button class="pill" onclick={() => del(true)}>{t("fork.cal.all_events")}</button>
      {:else}
        <button class="pill danger" onclick={() => del(false)}>{t("fork.cal.delete")}</button>
      {/if}
      <button class="pill" onclick={() => (confirmDelete = false)}>{t("fork.cal.keep")}</button>
    </div>
  {/if}

  <div class="head">
    <span class="swatch" aria-hidden="true"></span>
    <div>
      <h2 bind:this={heading} tabindex="-1" class:struck={evState === "cancelled" || evState === "declined"}>{row.summary || t("fork.cal.untitled")}</h2>
      <p class="when">{when}</p>
      {#if recurring}<p class="meta">{t("fork.cal.repeats")}</p>{/if}
      {#if foreign}<p class="meta">{t("fork.cal.outside_sync")}</p>{/if}
    </div>
  </div>

  {#if join}
    <button class="join" onclick={() => openUrl(join.url)}><svg viewBox="0 0 16 16" aria-hidden="true"><path d="M2.5 5h7.5v6H2.5zM10 7.2l3.5-2v5.6l-3.5-2" /></svg>{t(`fork.cal.join_${join.kind}`)}</button>
  {/if}

  {#if row.location && !(join && (row.location.includes(join.url) || /^(google( meet)?|microsoft teams meeting|zoom)$/i.test(row.location.trim())))}
    <div class="row"><svg viewBox="0 0 16 16" aria-hidden="true"><path d="M8 14s4.5-4.2 4.5-7.5a4.5 4.5 0 0 0-9 0C3.5 9.8 8 14 8 14Z" /><circle cx="8" cy="6.5" r="1.5" /></svg><p class="text">{row.location}</p></div>
  {/if}

  {#if guests.length}
    <div class="row top"><svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="6" cy="5.5" r="2.3" /><path d="M2 13c.4-2.4 2-3.6 4-3.6s3.6 1.2 4 3.6M10.5 3.5a2.2 2.2 0 0 1 0 4.2M12 9.6c1.2.5 1.9 1.6 2.1 3.4" /></svg>
      <div class="guests">
        <p><strong>{t("fork.cal.guest_count", { n: guests.length })}</strong> <span class="meta">{tally}</span></p>
        <ul>
          {#each guests.slice(0, 12) as g (g.email)}
            <li title={g.email}>
              <span class="avatar">{initials(g.displayName || g.email)}<i class="mark {mark(g.responseStatus)}" aria-label={t(`fork.cal.rsvp_${g.responseStatus ?? "needsAction"}`)}></i></span>
              <span class="who">{g.displayName || g.email}{#if calendar.ownEmails.includes(g.email.toLowerCase())} <em>{t("fork.cal.you")}</em>{/if}{#if g.organizer} <em>{t("fork.cal.organiser")}</em>{/if}{#if g.optional} <em>{t("fork.cal.optional_guest")}</em>{/if}</span>
            </li>
          {/each}
        </ul>
        {#if guests.length > 12}<p class="meta">{t("fork.cal.more_guests", { n: guests.length - 12 })}</p>{/if}
      </div>
    </div>
  {/if}

  {#if row.description}
    <div class="row top"><svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3 4h10M3 7h10M3 10h7" /></svg>
      <!-- Sanitised by description.ts: no images, links only to https or mailto, opened outside Skim. -->
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class="text desc" onclick={openLink}>{@html safeDescription(row.description)}</div>
    </div>
  {/if}
  {#if row.options?.attachments?.length}<div class="row"><AttachmentLinks value={row.options.attachments} /></div>{/if}

  {#if cal}<div class="row"><svg viewBox="0 0 16 16" aria-hidden="true"><path d="M2.5 3.5h11v10h-11zM2.5 6.5h11M5.5 2v3M10.5 2v3" /></svg><p class="text meta">{cal.summary}</p></div>{/if}

  {#if error}<p class="error" role="alert">{error}</p>{/if}

  {#if iAmGuest}
    <footer>
      <div class="going">
        <span>{t("fork.cal.going")}</span>
        {#each ["accepted", "tentative", "declined"] as const as r (r)}
          <button class="pill" class:on={row.self_response === r} aria-pressed={row.self_response === r} disabled={busy || !calendar.connected} onclick={() => answer(r)}>{t(`fork.cal.rsvp_btn_${r}`)}</button>
        {/each}
      </div>
      <div class="going-extra">
        {#if recurring}
          <select aria-label={t("fork.cal.answer_scope")} bind:value={scope}><option value="one">{t("fork.cal.this_event")}</option><option value="all">{t("fork.cal.all_events")}</option></select>
        {/if}
        {#if noteOpen}
          <input class="note" placeholder={t("fork.cal.note_placeholder")} bind:value={note} maxlength="500" />
        {:else}
          <button class="link" onclick={() => (noteOpen = true)}>{t("fork.cal.add_note")}</button>
        {/if}
        {#if needsReply(row, calendar.ownEmails)}<span class="reply-pill">{t("fork.cal.reply_pill")}</span>{/if}
      </div>
    </footer>
  {/if}
</div>

<style>
  .popover { position: fixed; z-index: 40; width: 340px; max-height: calc(100vh - 32px); overflow-y: auto; background: var(--surface-raised); border: 1px solid var(--hairline-strong); border-radius: var(--radius-l); box-shadow: var(--shadow-pop), 0 18px 48px -18px rgba(0, 0, 0, 0.28); padding: 10px 16px 14px; visibility: hidden; font-size: 13px; }
  .popover.ready { visibility: visible; }
  .tools { display: flex; justify-content: flex-end; align-items: center; gap: 2px; margin: 0 -8px 2px 0; }
  .tools button { width: 30px; height: 30px; display: grid; place-items: center; border-radius: var(--radius-s); color: var(--text-dim); }
  .tools button:hover { background: var(--hover); color: var(--text); }
  .tools button:disabled { opacity: 0.4; }
  .tools .sep { width: 1px; height: 16px; background: var(--hairline); margin: 0 4px; }
  svg { width: 16px; height: 16px; fill: none; stroke: currentColor; stroke-width: 1.4; stroke-linecap: round; stroke-linejoin: round; flex-shrink: 0; }
  .confirm { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; padding: 8px 10px; margin: 0 0 8px; border-radius: var(--radius-m); background: var(--hover); font-size: 12.5px; }
  .confirm span { flex: 1 1 100%; }
  .head { display: grid; grid-template-columns: 14px 1fr; gap: 12px; align-items: start; margin-bottom: 12px; }
  .swatch { width: 14px; height: 14px; border-radius: 4px; background: var(--cal-color); margin-top: 5px; }
  h2 { margin: 0; font-size: 18px; line-height: 1.3; font-weight: 650; letter-spacing: -0.01em; overflow-wrap: anywhere; }
  h2:focus { outline: none; }
  .struck { text-decoration: line-through; color: var(--text-dim); }
  .when { margin: 4px 0 0; color: var(--text-dim); }
  .meta { color: var(--text-faint); font-size: 12px; margin: 2px 0 0; }
  .join { display: inline-flex; align-items: center; gap: 8px; margin: 0 0 12px 26px; padding: 7px 14px; border-radius: var(--radius-m); background: var(--primary); color: var(--on-primary); font-weight: 600; font-size: 13px; }
  .join:hover { filter: brightness(0.97); }
  .row { display: grid; grid-template-columns: 16px 1fr; gap: 10px; align-items: center; margin: 0 0 10px; color: var(--text-dim); }
  .row.top { align-items: start; }
  .row.top svg { margin-top: 2px; }
  .text { margin: 0; color: var(--text); overflow-wrap: anywhere; line-height: 1.5; }
  .desc { max-height: 180px; overflow-y: auto; }
  .desc :global(p) { margin: 0 0 6px; }
  .desc :global(ul), .desc :global(ol) { margin: 0 0 6px; padding-left: 18px; }
  .desc :global(a) { color: var(--text); text-decoration: underline; text-underline-offset: 2px; overflow-wrap: anywhere; cursor: pointer; }
  .guests p { margin: 0 0 8px; }
  .guests ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 7px; }
  .guests li { display: flex; align-items: center; gap: 9px; min-width: 0; color: var(--text); }
  .avatar { position: relative; flex-shrink: 0; width: 26px; height: 26px; border-radius: 50%; display: grid; place-items: center; font-size: 10.5px; font-weight: 650; background: var(--selected); color: var(--text); }
  .mark { position: absolute; right: -3px; bottom: -3px; width: 12px; height: 12px; border-radius: 50%; border: 2px solid var(--surface-raised); }
  .mark.yes { background: var(--success, #16a34a); }
  .mark.no { background: var(--danger); }
  .mark.maybe { background: var(--text-faint); }
  .mark.wait { display: none; }
  .who { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .who em { font-style: normal; color: var(--text-faint); font-size: 11.5px; margin-left: 4px; }
  .link { color: var(--text-dim); text-decoration: underline; text-underline-offset: 2px; font-size: inherit; text-align: left; overflow-wrap: anywhere; }
  .error { color: var(--danger); font-size: 12px; }
  footer { margin: 12px -16px -14px; padding: 10px 16px 12px; border-top: 1px solid var(--hairline); display: grid; gap: 8px; }
  .going { display: flex; align-items: center; gap: 6px; }
  .going > span { flex: 1; font-weight: 600; font-size: 12.5px; }
  .going-extra { display: flex; align-items: center; gap: 8px; font-size: 12px; }
  .going-extra select, .note { padding: 4px 6px; border: 1px solid var(--hairline-strong); border-radius: var(--radius-s); background: var(--surface); color: var(--text); font-size: 12px; }
  .note { flex: 1; min-width: 0; }
  .pill { padding: 4px 12px; border-radius: 99px; border: 1px solid var(--hairline-strong); font-size: 12.5px; background: var(--surface-raised); }
  .pill:hover { background: var(--hover); }
  .pill.on { background: var(--text); color: var(--bg); border-color: var(--text); }
  .pill.danger { color: var(--danger); }
  .pill:disabled { opacity: 0.5; }
  .reply-pill { margin-left: auto; padding: 1px 8px; border-radius: 99px; font-size: 11px; background: var(--p-amber, #fde68a); color: #16181d; border: 1px solid currentColor; }
  :global(:root[data-theme="pastel-light"]) .popover { border-color: var(--ink); }
  :global(:root[data-theme="pastel-light"]) .join { background: var(--p-mint); color: var(--ink); border: 1px solid var(--ink); }
  :global(:root[data-theme="pastel-light"]) .avatar { background: var(--p-peri); box-shadow: inset 0 0 0 1px var(--ink); }
  @media (prefers-reduced-motion: no-preference) { .popover.ready { animation: pop-in 120ms ease-out; } @keyframes pop-in { from { opacity: 0; transform: translateY(4px); } } }
</style>
