<script lang="ts">
  // Phase 11: the prep panel for one calendar event. Per external guest: the
  // Rebound card (or "Not in Rebound"), their last five threads (click opens
  // the thread in the reading pane), and one violet "✦ Brief me" that streams
  // a short brief over all of it. Read-only everywhere: the panel never
  // writes mail, calendar or CRM.
  //
  // Mounted by the shell while `prepOpen.eventId` is set (the reminder toast's
  // Prep action, or the calendar event panel's Prep button, both set it).
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { t } from "../../lib/i18n/index.svelte";
  import { mdLite } from "../../lib/md";
  import { ai } from "../../lib/stores/ai.svelte";
  import { mail } from "../../lib/stores/mail.svelte";
  import { crmApi } from "../crm/api";
  import { crm } from "../crm/store.svelte";
  import { CRM_SETUP_CODES, type CrmDeal, type CrmLookup } from "../crm/types";
  import { prepApi } from "./api";
  import type { PrepGuest, PrepPayload, PrepThread } from "./types";

  interface Props {
    eventId: number;
    onclose?: () => void;
  }
  let { eventId, onclose }: Props = $props();

  let payload = $state<PrepPayload | null>(null);
  let loading = $state(false);
  let loadError = $state<string | null>(null);
  /** Cards the panel fetched itself, by guest email (Rust's `crm` wins). */
  let cards = $state<Record<string, CrmLookup | null>>({});

  type BriefStatus = "idle" | "thinking" | "streaming" | "done" | "error";
  let brief = $state("");
  let briefStatus = $state<BriefStatus>("idle");
  let briefError = $state<string | null>(null);
  let cancelBrief: (() => void) | null = null;

  const guests = $derived(payload?.guests ?? []);

  async function load(id: number) {
    loading = true;
    loadError = null;
    payload = null;
    cards = {};
    resetBrief();
    try {
      const p = await prepApi.prep(id);
      if (id !== eventId) return;
      payload = p;
      void fillCards(p.guests);
    } catch (e: unknown) {
      if (id !== eventId) return;
      loadError = messageOf(e);
    } finally {
      if (id === eventId) loading = false;
    }
  }

  /** Until `fork::prep` has a CRM seam, ask the CRM per guest here. Setup
   *  errors (not configured / connected) mean "no card", silently. */
  async function fillCards(list: PrepGuest[]) {
    await Promise.all(
      list
        .filter((g) => g.crm === null)
        .map(async (g) => {
          try {
            const found = await crmApi.lookup(g.email);
            cards[g.email] = found.person || found.company ? found : null;
          } catch (e: unknown) {
            const code = (e as { code?: string } | null)?.code ?? "";
            if (!CRM_SETUP_CODES.has(code)) console.warn("prep: crm lookup failed", g.email, e);
            cards[g.email] = null;
          }
        }),
    );
  }

  function cardOf(g: PrepGuest): CrmLookup | null {
    return g.crm ?? cards[g.email] ?? null;
  }

  function messageOf(e: unknown): string {
    if (typeof e === "string") return e;
    const m = (e as { message?: string } | null)?.message;
    return m ?? String(e);
  }

  $effect(() => {
    void load(eventId);
    return () => {
      cancelBrief?.();
      cancelBrief = null;
    };
  });

  function resetBrief() {
    cancelBrief?.();
    cancelBrief = null;
    brief = "";
    briefStatus = "idle";
    briefError = null;
  }

  function briefMe() {
    if (briefStatus === "thinking" || briefStatus === "streaming") return;
    resetBrief();
    briefStatus = "thinking";
    cancelBrief = prepApi.brief(eventId, {
      delta: (text) => {
        briefStatus = "streaming";
        brief += text;
      },
      // Reasoning only says the model is alive; "thinking" already shows that.
      reasoning: () => {},
      done: () => {
        briefStatus = "done";
        cancelBrief = null;
      },
      error: (_code, message) => {
        briefStatus = "error";
        briefError = message;
        cancelBrief = null;
      },
    });
  }

  function openThread(th: PrepThread) {
    void mail.openLocation(th.folderId, th.id, th.hitMessageId);
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && onclose) {
      e.preventDefault();
      onclose();
    }
  }

  const thisYear = new Date().getFullYear();
  function fmtDate(secs: number): string {
    const d = new Date(secs * 1000);
    const opts: Intl.DateTimeFormatOptions =
      d.getFullYear() === thisYear
        ? { day: "numeric", month: "short" }
        : { day: "numeric", month: "short", year: "numeric" };
    return new Intl.DateTimeFormat(undefined, opts).format(d);
  }
  function fmtTime(secs: number): string {
    return new Intl.DateTimeFormat(undefined, { hour: "2-digit", minute: "2-digit" }).format(
      new Date(secs * 1000),
    );
  }
  function personName(c: CrmLookup): string {
    const p = c.person;
    return (
      p?.fullName ??
      [p?.firstName, p?.lastName].filter(Boolean).join(" ") ??
      p?.primaryEmail ??
      ""
    );
  }
  function openDeals(c: CrmLookup): CrmDeal[] {
    return c.deals.filter((d) => (d.status ?? "open") === "open");
  }
  function stage(d: CrmDeal): string {
    return d.stageName ?? d.status ?? t("fork.crm.deal_open");
  }
  function money(v: number | null): string {
    if (v === null || !Number.isFinite(v)) return "";
    return new Intl.NumberFormat(undefined, { maximumFractionDigits: 0 }).format(v);
  }
  function lastActivity(c: CrmLookup): string {
    const stamps = c.activities
      .map((a) => Date.parse(a.occurredAt ?? a.createdAt ?? ""))
      .filter((n) => Number.isFinite(n));
    if (stamps.length === 0) return "";
    return fmtDate(Math.max(...stamps) / 1000);
  }
  function crmLink(c: CrmLookup, base: string | null): string | null {
    if (!base) return null;
    if (c.person?.id) return `${base}/people/${c.person.id}`;
    if (c.company?.id) return `${base}/companies/${c.company.id}`;
    return null;
  }
  function initial(s: string): string {
    const c = s.trim().charAt(0);
    return c ? c.toUpperCase() : "?";
  }
</script>

<svelte:window onkeydown={onKeydown} />

<aside class="prep" aria-label={t("fork.prep.title")}>
  <header class="head">
    <div class="head-text">
      <span class="microlabel">{t("fork.prep.title")}</span>
      {#if payload}
        <h2 class="event">{payload.summary || t("fork.prep.untitled")}</h2>
        <span class="dim">{fmtDate(payload.startTs)} · {fmtTime(payload.startTs)}–{fmtTime(payload.endTs)}</span>
      {/if}
    </div>
    {#if onclose}
      <button class="close" onclick={onclose} aria-label={t("fork.prep.close")} title="{t('fork.prep.close')}  Esc">×</button>
    {/if}
  </header>

  {#if loading}
    <div class="state skeleton" aria-busy="true" aria-label={t("fork.prep.loading")}>
      <span class="bar w60"></span>
      <span class="bar w40"></span>
      <span class="bar w80"></span>
    </div>
  {:else if loadError}
    <div class="state">
      <p class="warn">{t("fork.prep.error")}</p>
      <p class="dim mono">{loadError}</p>
      <button class="ghost" onclick={() => void load(eventId)}>{t("fork.prep.retry")}</button>
    </div>
  {:else if payload && guests.length === 0}
    <div class="state">
      <p class="empty">{t("fork.prep.no_guests")}</p>
    </div>
  {:else if payload}
    <div class="body">
      {#if ai.keyPresent}
        <section class="brief" class:live={briefStatus !== "idle"}>
          <div class="brief-row">
            <button
              class="ai-btn"
              onclick={briefMe}
              disabled={briefStatus === "thinking" || briefStatus === "streaming"}
            >
              ✦ {briefStatus === "done" || briefStatus === "error" ? t("fork.prep.brief_again") : t("fork.prep.brief")}
            </button>
            {#if briefStatus === "thinking"}
              <span class="dim spark-note"><span class="spark">✦</span>{t("fork.prep.brief_thinking")}</span>
            {:else if briefStatus === "streaming"}
              <span class="dim spark-note"><span class="spark">✦</span>{t("fork.prep.brief_writing")}</span>
            {/if}
          </div>
          {#if brief}
            <div class="brief-text md-body">{@html mdLite(brief)}</div>
          {/if}
          {#if briefStatus === "error"}
            <p class="warn">{t("fork.prep.brief_failed")}</p>
            <p class="dim mono">{briefError}</p>
          {/if}
        </section>
      {/if}

      {#each guests as g (g.email)}
        {@const card = cardOf(g)}
        <section class="guest">
          <div class="who">
            <span class="avatar" aria-hidden="true">{initial(g.name ?? g.email)}</span>
            <div class="names">
              <div class="name">{g.name ?? g.email}</div>
              {#if g.name}<div class="dim mono addr">{g.email}</div>{/if}
            </div>
          </div>

          {#if card}
            <div class="card">
              <div class="card-head">
                <span class="microlabel">Rebound</span>
                {#if crmLink(card, crm.baseUrl)}
                  <button class="linkish" onclick={() => void openUrl(crmLink(card, crm.baseUrl)!)}>{t("fork.prep.open_in_rebound")} ↗</button>
                {/if}
              </div>
              {#if card.person}
                <div class="line">
                  <span class="strong">{personName(card)}</span>
                  {#if card.company?.name}<span class="dim"> · {card.company.name}</span>{/if}
                </div>
              {:else if card.company}
                <div class="line"><span class="strong">{card.company.name ?? card.company.domain}</span></div>
              {/if}
              {#if card.company?.industry}
                <div class="line dim">{card.company.industry}{#if card.company.employeeCountRange} · {card.company.employeeCountRange}{/if}</div>
              {/if}
              {#if openDeals(card).length > 0}
                <ul class="deals">
                  {#each openDeals(card) as d (d.id ?? d.name)}
                    <li>
                      <span class="strong">{d.name ?? t("fork.prep.deal_unnamed")}</span>
                      <span class="dim"> · {stage(d)}</span>
                      {#if d.value !== null}<span class="dim mono"> · {money(d.value)}</span>{/if}
                    </li>
                  {/each}
                </ul>
              {/if}
              {#if lastActivity(card)}
                <div class="line dim">{t("fork.prep.last_activity", { date: lastActivity(card) })}</div>
              {/if}
              {#if card.person?.tags?.length}
                <div class="tags">
                  {#each card.person.tags as tag (tag)}<span class="tag">{tag}</span>{/each}
                </div>
              {/if}
              {#if card.person?.linkedinUrl}
                <button class="linkish" onclick={() => void openUrl(card.person!.linkedinUrl!)}>LinkedIn ↗</button>
              {/if}
            </div>
          {:else}
            <div class="card empty-card">
              <span class="dim">{t("fork.crm.not_found")}</span>
            </div>
          {/if}

          {#if g.threads.length === 0}
            <p class="dim no-threads">{t("fork.prep.no_threads")}</p>
          {:else}
            <ul class="threads" aria-label={t("fork.prep.threads")}>
              {#each g.threads as th (th.id)}
                <li>
                  <button class="thread" onclick={() => openThread(th)} title={th.subject}>
                    <span class="t-top">
                      <span class="t-subject" class:unread={!th.isRead}>{th.subject || t("fork.prep.no_subject")}</span>
                      <span class="t-date dim">{fmtDate(th.date)}</span>
                    </span>
                    <span class="t-snippet dim">{th.snippet}</span>
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        </section>
      {/each}
    </div>
  {/if}
</aside>

<style>
  .prep {
    position: fixed;
    top: var(--titlebar-h);
    right: 0;
    bottom: 0;
    width: 380px;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border-left: 1px solid var(--hairline);
    z-index: 150;
    box-shadow: var(--shadow-pop);
  }
  .head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 8px;
    padding: 14px 16px 10px;
    border-bottom: 1px solid var(--hairline);
  }
  .head-text {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }
  .event {
    font-size: 15px;
    font-weight: 700;
    letter-spacing: -0.01em;
    margin: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .close {
    font-size: 18px;
    line-height: 1;
    padding: 2px 6px;
    border-radius: var(--radius-s);
    color: var(--text-faint);
  }
  .close:hover {
    background: var(--hover);
    color: var(--text);
  }
  .body {
    flex: 1;
    overflow-y: auto;
    padding: 12px 16px 24px;
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  .state {
    padding: 24px 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    font-size: 13px;
  }
  .skeleton .bar {
    display: block;
    height: 10px;
    border-radius: 5px;
    background: var(--hover);
  }
  .w60 {
    width: 60%;
  }
  .w40 {
    width: 40%;
  }
  .w80 {
    width: 80%;
  }
  .dim {
    color: var(--text-faint);
  }
  .mono {
    font-family: var(--font-mono);
    font-size: 11px;
  }
  .warn {
    color: var(--danger);
    font-weight: 600;
    margin: 0;
  }
  .empty {
    margin: 0;
    color: var(--text-dim);
  }
  .ghost {
    align-self: flex-start;
    padding: 5px 10px;
    border-radius: var(--radius-s);
    border: 1px solid var(--hairline-strong);
    font-size: 12px;
  }
  .ghost:hover {
    background: var(--hover);
  }
  .linkish {
    color: var(--text-dim);
    font-size: 12px;
    text-decoration: underline;
    text-underline-offset: 2px;
    align-self: flex-start;
  }

  /* Brief: the only violet in the panel. */
  .brief {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .brief-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .ai-btn {
    padding: 7px 14px;
    border-radius: var(--radius-m);
    border: 1px solid var(--accent-dim);
    color: var(--accent);
    font-size: 13px;
    font-weight: 600;
    white-space: nowrap;
  }
  .ai-btn:hover:not(:disabled) {
    background: var(--accent-soft);
  }
  .ai-btn:disabled {
    opacity: 0.6;
  }
  .spark-note {
    font-size: 12px;
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .spark {
    color: var(--accent);
  }
  .brief-text {
    padding: 10px 12px;
    border-radius: var(--radius-m);
    background: var(--accent-soft);
    font-size: 13px;
    line-height: 1.5;
  }

  .guest {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding-top: 14px;
    border-top: 1px solid var(--hairline);
  }
  .who {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .avatar {
    width: 30px;
    height: 30px;
    border-radius: 50%;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: var(--active);
    color: var(--text-dim);
    font-size: 13px;
    font-weight: 700;
    flex: none;
  }
  .names {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .name {
    font-size: 14px;
    font-weight: 700;
    letter-spacing: -0.01em;
  }
  .addr {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .card {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 10px 12px;
    border-radius: var(--radius-m);
    border: 1px solid var(--hairline);
    background: var(--bg);
    font-size: 13px;
  }
  .empty-card {
    font-size: 12px;
  }
  .card-head {
    display: flex;
    justify-content: space-between;
    margin-bottom: 2px;
  }
  .line {
    line-height: 1.4;
  }
  .strong {
    font-weight: 600;
  }
  .deals {
    list-style: none;
    margin: 2px 0 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-top: 2px;
  }
  .tag {
    font-family: var(--font-mono);
    font-size: 10px;
    padding: 1px 6px;
    border-radius: 999px;
    border: 1px solid var(--hairline-strong);
    color: var(--text-dim);
  }
  .no-threads {
    margin: 0;
    font-size: 12px;
  }
  .threads {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
  }
  .thread {
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 7px 8px;
    margin: 0 -8px;
    border-radius: var(--radius-s);
    text-align: left;
  }
  .thread:hover {
    background: var(--hover);
  }
  .t-top {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    min-width: 0;
  }
  .t-subject {
    font-size: 13px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .t-subject.unread {
    font-weight: 700;
    color: var(--unread);
  }
  .t-date {
    font-size: 11px;
    flex: none;
  }
  .t-snippet {
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
