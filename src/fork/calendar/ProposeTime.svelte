<script lang="ts">
  // Fork (v1.1.1): "Propose new time". Google's API has no attendee proposal
  // call, so this does what a person would: pick up to three times (free slots
  // of the same length, or one typed in), then open a normal draft to the
  // organiser listing them. Nothing is sent: the draft opens in the compose
  // window for Patrick to edit and send. The caller may also mark him Maybe.
  import { untrack } from "svelte";
  import { api } from "../../lib/api";
  import { t } from "../../lib/i18n/index.svelte";
  import { mail } from "../../lib/stores/mail.svelte";
  import { formatSlots } from "../slots/slots";
  import { availabilityApi } from "./api";
  import { fromDateTime, localDate, localZone } from "./guests";
  import { calendar } from "./store.svelte";
  import TimeInput from "./TimeInput.svelte";
  import { fromMinutes, toMinutes } from "./time";
  import type { Slot } from "./types";

  interface Props {
    summary: string;
    organizerEmail: string;
    organizerName?: string | null;
    startTs: number;
    endTs: number;
    /** Called once the draft is open, e.g. to mark the event Maybe. */
    onproposed?: () => void;
    oncancel: () => void;
  }
  let { summary, organizerEmail, organizerName = null, startTs, endTs, onproposed, oncancel }: Props = $props();

  const lengthMin = $derived(Math.max(15, Math.round((endTs - startTs) / 60)));
  const tz = localZone();

  let free = $state<Slot[] | null>(null);
  let freeError = $state(false);
  let picked = $state<Slot[]>([]);
  let note = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);

  // A time of his own.
  // Seeded once from the meeting: an hour later, same day.
  const seed = untrack(() => new Date(startTs * 1000));
  let ownDate = $state(localDate(seed));
  let ownStart = $state(fromMinutes(toMinutes(seed.toTimeString().slice(0, 5)) + 60));
  let ownEnd = $state("");
  $effect(() => {
    // Keep the typed slot the meeting's length unless he changes the end.
    ownEnd = fromMinutes(toMinutes(ownStart) + lengthMin);
  });

  $effect(() => {
    const len = lengthMin;
    free = null;
    freeError = false;
    availabilityApi
      .freeSlots(len, 5, tz)
      .then((s) => {
        const now = Date.now() / 1000;
        free = s.filter((x) => x.start > now && (x.start >= endTs || x.end <= startTs)).slice(0, 8);
      })
      .catch(() => {
        free = [];
        freeError = true;
      });
  });

  const MAX = 3;

  function isPicked(s: Slot): boolean {
    return picked.some((p) => p.start === s.start);
  }

  function toggle(s: Slot) {
    if (isPicked(s)) picked = picked.filter((p) => p.start !== s.start);
    else if (picked.length < MAX) picked = [...picked, s].sort((a, b) => a.start - b.start);
  }

  function addOwn() {
    const start = Math.floor(fromDateTime(ownDate, ownStart).getTime() / 1000);
    const end = Math.floor(fromDateTime(ownDate, ownEnd).getTime() / 1000);
    if (!(end > start) || picked.length >= MAX) return;
    const d = new Date(start * 1000);
    const label = new Intl.DateTimeFormat("en-GB", { weekday: "short", day: "numeric", month: "short" }).format(d);
    const slot: Slot = { start, end, day: ownDate, label, time: ownStart };
    if (!isPicked(slot)) picked = [...picked, slot].sort((a, b) => a.start - b.start);
  }

  function slotLabel(s: Slot): string {
    return `${s.label}, ${s.time}`;
  }

  // Plain words, no dashes: this text goes out under Patrick's name.
  function when(ts: number): string {
    const d = new Date(ts * 1000);
    const day = new Intl.DateTimeFormat("en-GB", { weekday: "long", day: "numeric", month: "long" }).format(d);
    const time = new Intl.DateTimeFormat("en-GB", { hour: "2-digit", minute: "2-digit", hour12: false }).format(d);
    return `${day} at ${time}`;
  }

  function firstName(): string {
    const n = (organizerName ?? "").trim();
    if (n && !n.includes("@")) return n.split(/\s+/)[0];
    return "";
  }

  function bodyText(): string {
    const hi = firstName() ? `Hi ${firstName()},` : "Hi,";
    const slots = formatSlots(picked, { senderTz: tz, recipientTz: "", bookingLink: "" });
    const ask = picked.length === 1 ? "Would this work instead?" : "Would any of these work instead?";
    const parts = [hi, "", `I can't make ${when(startTs)}. ${ask}`, "", slots];
    if (note.trim()) parts.push("", note.trim());
    return parts.join("\n");
  }

  async function draft() {
    if (busy || picked.length === 0) return;
    busy = true;
    error = null;
    try {
      const own = calendar.ownEmails.map((e) => e.toLowerCase());
      const acc = mail.accounts.find((a) => own.includes(a.email.toLowerCase()))?.id ?? (await mail.composeAccountId());
      const d = await api.createDraft(acc);
      d.to = organizerEmail;
      d.subject = `New time for ${summary || t("fork.cal.untitled")}`;
      d.body = d.body ? `${bodyText()}\n\n${d.body}` : bodyText();
      await api.updateDraft(d);
      await api.openComposeWindow(d.id);
      onproposed?.();
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : String((e as { message?: string })?.message ?? e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="propose">
  <div class="head">
    <span class="microlabel">{t("fork.cal.propose_title")}</span>
    <span class="hint">{t("fork.cal.propose_pick", { n: MAX })}</span>
  </div>

  {#if free === null}
    <div class="hint">{t("fork.cal.propose_loading")}</div>
  {:else if free.length > 0}
    <div class="chips">
      {#each free as s (s.start)}
        <button
          class="chip"
          class:on={isPicked(s)}
          disabled={!isPicked(s) && picked.length >= MAX}
          onclick={() => toggle(s)}
          aria-pressed={isPicked(s)}
        >
          {slotLabel(s)}
        </button>
      {/each}
    </div>
  {:else}
    <div class="hint">{freeError ? t("fork.cal.propose_no_free_error") : t("fork.cal.propose_no_free")}</div>
  {/if}

  <div class="own">
    <input type="date" bind:value={ownDate} aria-label={t("fork.cal.starts")} />
    <TimeInput bind:value={ownStart} label={t("fork.cal.starts")} />
    <TimeInput bind:value={ownEnd} anchor={ownStart} label={t("fork.cal.ends")} />
    <button class="btn small" onclick={addOwn} disabled={picked.length >= MAX}>{t("fork.cal.propose_add")}</button>
  </div>

  {#if picked.length > 0}
    <ul class="picked">
      {#each picked as s (s.start)}
        <li>
          <span>{slotLabel(s)}</span>
          <button class="x" onclick={() => toggle(s)} aria-label={t("fork.cal.propose_remove")}>×</button>
        </li>
      {/each}
    </ul>
  {/if}

  <textarea bind:value={note} rows="2" placeholder={t("fork.cal.propose_note")}></textarea>

  {#if error}<div class="hint danger" role="alert">{error}</div>{/if}

  <div class="actions">
    <button class="btn primary" onclick={draft} disabled={busy || picked.length === 0}>
      {t("fork.cal.propose_draft", { who: firstName() || organizerEmail })}
    </button>
    <button class="btn ghost" onclick={oncancel}>{t("fork.cal.propose_cancel")}</button>
  </div>
  <div class="hint">{t("fork.cal.propose_footnote")}</div>
</div>

<style>
  .propose {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px;
    border: 1px solid var(--hairline-strong);
    border-radius: var(--radius-m);
    background: var(--surface);
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 8px;
  }
  .hint {
    color: var(--text-faint);
    font-size: 12px;
  }
  .hint.danger {
    color: var(--danger);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip {
    padding: 4px 9px;
    border: 1px solid var(--hairline-strong);
    border-radius: 999px;
    font-size: 12.5px;
    font-variant-numeric: tabular-nums;
  }
  .chip:hover:not(:disabled) {
    background: var(--hover);
  }
  .chip.on {
    background: var(--selected);
    border-color: var(--focus);
    font-weight: 600;
  }
  .chip:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .own {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }
  .own input[type="date"] {
    padding: 5px 8px;
    border: 1px solid var(--hairline-strong);
    border-radius: var(--radius-s);
    background: var(--surface);
  }
  .picked {
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .picked li {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 4px 8px;
    background: var(--hover);
    border-radius: var(--radius-s);
    font-variant-numeric: tabular-nums;
  }
  .x {
    color: var(--text-dim);
    padding: 0 4px;
  }
  textarea {
    padding: 6px 8px;
    border: 1px solid var(--hairline-strong);
    border-radius: var(--radius-s);
    resize: vertical;
    user-select: text;
  }
  .actions {
    display: flex;
    gap: 8px;
  }
  .btn {
    padding: 6px 12px;
    border: 1px solid var(--hairline-strong);
    border-radius: var(--radius-s);
    font-size: 13px;
  }
  .btn.small {
    padding: 4px 10px;
    font-size: 12.5px;
  }
  .btn.primary {
    background: var(--primary);
    border-color: var(--primary);
    color: var(--on-primary);
    font-weight: 600;
  }
  .btn.ghost {
    border-color: transparent;
    color: var(--text-dim);
  }
  .btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
