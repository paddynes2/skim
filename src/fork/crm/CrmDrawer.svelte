<script lang="ts">
  // Phase 9: the Rebound card for the sender of the focused message. A 320px
  // drawer on the right of the reading pane, read-only. Four states: connect
  // (nothing configured / signed in / chosen), loading, not in Rebound, found.
  // `i` toggles it (keys.ts -> navHooks.crmToggle -> crm.toggle()); the pref
  // remembers it across sessions.
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { t } from "../../lib/i18n/index.svelte";
  import { ui } from "../../lib/stores/ui.svelte";
  import { prefs } from "../stores/prefs.svelte";
  import { crm, crmFocus } from "./store.svelte";
  import type { CrmActivity, CrmDeal } from "./types";

  let { email = null, open = false }: { email: string | null; open: boolean } = $props();

  // The props are the source of truth for what the drawer follows; the store
  // is what the `i` key and Settings talk to. Keep them in step.
  $effect(() => {
    crmFocus.setEmail(email);
  });
  $effect(() => {
    if (open) void crm.refresh();
  });

  const found = $derived(crm.lookup);
  const person = $derived(found?.person ?? null);
  const company = $derived(found?.company ?? null);
  const hit = $derived(person !== null || company !== null);
  const openDeals = $derived((found?.deals ?? []).filter((d) => (d.status ?? "open") === "open"));
  const closedDeals = $derived((found?.deals ?? []).filter((d) => (d.status ?? "open") !== "open"));
  const activities = $derived(
    [...(found?.activities ?? [])]
      .sort((a, b) => (when(b) ?? 0) - (when(a) ?? 0))
      .slice(0, 5),
  );
  const reminders = $derived((found?.reminders ?? []).filter((r) => r.status !== "done"));

  const openLink = $derived.by(() => {
    if (person?.id) return `${crm.baseUrl}/people/${person.id}`;
    if (company?.id) return `${crm.baseUrl}/companies/${company.id}`;
    return null;
  });
  const addLink = $derived(`${crm.baseUrl}/people/new`);

  const personName = $derived(
    person?.fullName ?? [person?.firstName, person?.lastName].filter(Boolean).join(" ") ?? null,
  );
  const place = $derived([person?.city, person?.country].filter(Boolean).join(", "));

  function when(a: CrmActivity): number | null {
    const s = a.occurredAt ?? a.createdAt;
    if (!s) return null;
    const n = Date.parse(s);
    return Number.isFinite(n) ? n : null;
  }

  const thisYear = new Date().getFullYear();
  function fmtDate(iso: string | null): string {
    if (!iso) return "";
    const n = Date.parse(iso);
    if (!Number.isFinite(n)) return "";
    const d = new Date(n);
    const opts: Intl.DateTimeFormatOptions =
      d.getFullYear() === thisYear ? { day: "numeric", month: "short" } : { day: "numeric", month: "short", year: "numeric" };
    return new Intl.DateTimeFormat(undefined, opts).format(d);
  }

  /** `email_received` -> "Email received". */
  function label(type: string | null): string {
    if (!type) return t("fork.crm.activity");
    const s = type.replace(/_/g, " ");
    return s.charAt(0).toUpperCase() + s.slice(1);
  }

  /** What an activity was about, from its payload, when it says. */
  function detail(a: CrmActivity): string {
    const p = a.payload ?? {};
    for (const k of ["subject", "title", "summary", "note", "to_stage", "stage_name"]) {
      const v = p[k];
      if (typeof v === "string" && v.trim()) return v.trim();
    }
    return "";
  }

  function stage(d: CrmDeal): string {
    if (d.stageName) return d.stageName;
    if (d.status && d.status !== "open") return d.status;
    return t("fork.crm.deal_open");
  }

  function money(v: number | null): string {
    if (v === null || !Number.isFinite(v)) return "";
    return new Intl.NumberFormat(undefined, { maximumFractionDigits: 0 }).format(v);
  }

  function initial(s: string | null): string {
    const c = (s ?? "").trim().charAt(0);
    return c ? c.toUpperCase() : "?";
  }

  function close() {
    prefs.setCrmDrawer(false);
  }
</script>

{#if open}
  <aside class="crm" aria-label={t("fork.crm.title")}>
    <header class="head">
      <span class="microlabel">{t("fork.crm.title")}</span>
      <button class="close" onclick={close} aria-label={t("fork.crm.close")} title="{t('fork.crm.close')}  I">×</button>
    </header>

    {#if crm.needsSetup}
      <div class="state">
        <p class="dim">{t("fork.crm.connect_line")}</p>
        <button class="ghost" onclick={() => ui.openSettings()}>{t("fork.crm.connect_button")}</button>
      </div>
    {:else if !email}
      <div class="state">
        <p class="dim">{t("fork.crm.no_message")}</p>
      </div>
    {:else if crm.loading && !found}
      <div class="state skeleton" aria-busy="true" aria-label={t("fork.crm.loading")}>
        <span class="bar w60"></span>
        <span class="bar w40"></span>
        <span class="bar w80"></span>
      </div>
    {:else if crm.error}
      <div class="state">
        <p class="warn">{t("fork.crm.error")}</p>
        <p class="dim mono">{crm.error.message}</p>
        <button class="ghost" onclick={() => void crm.refresh()}>{t("fork.crm.retry")}</button>
      </div>
    {:else if !hit}
      <div class="state">
        <p class="empty">{t("fork.crm.not_found")}</p>
        <p class="dim mono addr">{email}</p>
        <button class="linkish" onclick={() => void openUrl(addLink)}>{t("fork.crm.add")} ↗</button>
      </div>
    {:else}
      <div class="body">
        {#if person}
          <section class="card">
            <div class="who">
              <span class="avatar" aria-hidden="true">{initial(personName ?? person.primaryEmail)}</span>
              <div class="names">
                <div class="name">{personName || person.primaryEmail || t("fork.crm.unnamed")}</div>
                {#if company?.name}
                  <div class="dim">{company.name}</div>
                {/if}
              </div>
            </div>
            <dl class="facts">
              {#if person.primaryEmail}
                <dt>{t("fork.crm.email")}</dt>
                <dd class="mono">{person.primaryEmail}</dd>
              {/if}
              {#if person.primaryPhone}
                <dt>{t("fork.crm.phone")}</dt>
                <dd class="mono">{person.primaryPhone}</dd>
              {/if}
              {#if place}
                <dt>{t("fork.crm.location")}</dt>
                <dd>{place}</dd>
              {/if}
              {#if person.linkedinUrl}
                <dt>LinkedIn</dt>
                <dd><button class="linkish" onclick={() => void openUrl(person.linkedinUrl!)}>{t("fork.crm.profile")} ↗</button></dd>
              {/if}
            </dl>
            {#if person.tags?.length}
              <div class="tags">
                {#each person.tags as tag (tag)}<span class="tag">{tag}</span>{/each}
              </div>
            {/if}
          </section>
        {/if}

        {#if company}
          <section class="card">
            <div class="microlabel">{t("fork.crm.company")}</div>
            <div class="name">{company.name ?? company.domain ?? t("fork.crm.unnamed")}</div>
            <dl class="facts">
              {#if company.domain}
                <dt>{t("fork.crm.domain")}</dt>
                <dd class="mono">{company.domain}</dd>
              {/if}
              {#if company.industry}
                <dt>{t("fork.crm.industry")}</dt>
                <dd>{company.industry}</dd>
              {/if}
              {#if company.employeeCountRange}
                <dt>{t("fork.crm.size")}</dt>
                <dd>{company.employeeCountRange}</dd>
              {/if}
            </dl>
          </section>
        {/if}

        <section class="card">
          <div class="microlabel">{t("fork.crm.deals")} {#if openDeals.length}<span class="count">{openDeals.length}</span>{/if}</div>
          {#if openDeals.length === 0}
            <p class="dim">{t("fork.crm.no_deals")}</p>
          {:else}
            <ul class="list">
              {#each openDeals as d (d.id ?? d.name)}
                <li class="deal">
                  <span class="deal-name">{d.name ?? t("fork.crm.unnamed")}</span>
                  <span class="deal-meta">
                    <span class="stage">{stage(d)}</span>
                    {#if d.value !== null}<span class="mono">{money(d.value)}</span>{/if}
                  </span>
                </li>
              {/each}
            </ul>
          {/if}
          {#if closedDeals.length}
            <p class="dim small">{t("fork.crm.closed_deals", { n: closedDeals.length })}</p>
          {/if}
        </section>

        {#if reminders.length}
          <section class="card">
            <div class="microlabel">{t("fork.crm.reminders")}</div>
            <ul class="list">
              {#each reminders as r (r.id ?? r.title)}
                <li class="row">
                  <span class="grow">{r.title ?? t("fork.crm.unnamed")}</span>
                  <span class="dim mono">{fmtDate(r.snoozedUntil ?? r.remindAt)}</span>
                </li>
              {/each}
            </ul>
          </section>
        {/if}

        <section class="card">
          <div class="microlabel">{t("fork.crm.activity")}</div>
          {#if activities.length === 0}
            <p class="dim">{t("fork.crm.no_activity")}</p>
          {:else}
            <ul class="list">
              {#each activities as a (a.id ?? when(a))}
                <li class="row">
                  <span class="grow">
                    <span>{label(a.activityType)}</span>
                    {#if detail(a)}<span class="dim detail">{detail(a)}</span>{/if}
                  </span>
                  <span class="dim mono">{fmtDate(a.occurredAt ?? a.createdAt)}</span>
                </li>
              {/each}
            </ul>
          {/if}
        </section>

        {#if openLink}
          <button class="ghost open" onclick={() => void openUrl(openLink)}>{t("fork.crm.open")} ↗</button>
        {/if}
      </div>
    {/if}
  </aside>
{/if}

<style>
  .crm {
    flex: 0 0 320px;
    width: 320px;
    min-height: 0;
    display: flex;
    flex-direction: column;
    border-left: 1px solid var(--hairline);
    background: var(--surface);
    overflow: hidden;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 14px 8px;
    flex-shrink: 0;
  }
  .microlabel {
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 500;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-faint);
  }
  .count {
    margin-left: 4px;
    color: var(--text-dim);
  }
  .close {
    width: 24px;
    height: 24px;
    border-radius: var(--radius-s);
    color: var(--text-faint);
    font-size: 16px;
    line-height: 1;
  }
  .close:hover {
    background: var(--hover);
    color: var(--text);
  }
  .state,
  .body {
    padding: 4px 14px 14px;
    overflow-y: auto;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .state {
    align-items: flex-start;
  }
  .state p {
    margin: 0;
    font-size: 13px;
    line-height: 1.45;
  }
  .empty {
    font-weight: 600;
  }
  .dim {
    color: var(--text-dim);
    font-size: 12.5px;
  }
  .small {
    font-size: 11.5px;
  }
  .mono {
    font-family: var(--font-mono);
    font-size: 11.5px;
  }
  .addr {
    word-break: break-all;
  }
  .warn {
    color: var(--danger);
    font-size: 12.5px;
  }
  .card {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding-bottom: 12px;
    border-bottom: 1px solid var(--hairline);
  }
  .card:last-of-type {
    border-bottom: none;
  }
  .who {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .avatar {
    width: 32px;
    height: 32px;
    border-radius: 50%;
    background: var(--selected);
    color: var(--text-dim);
    display: grid;
    place-items: center;
    font-weight: 600;
    font-size: 13px;
    flex-shrink: 0;
  }
  .names {
    min-width: 0;
  }
  .name {
    font-weight: 600;
    font-size: 13.5px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .facts {
    display: grid;
    grid-template-columns: 72px 1fr;
    gap: 4px 8px;
    margin: 0;
    font-size: 12.5px;
  }
  .facts dt {
    color: var(--text-faint);
  }
  .facts dd {
    margin: 0;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .tag {
    padding: 2px 8px;
    border-radius: 999px;
    border: 1px solid var(--hairline-strong);
    font-size: 11px;
    color: var(--text-dim);
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 12.5px;
  }
  .deal {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .deal-name {
    font-weight: 500;
  }
  .deal-meta {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    color: var(--text-dim);
  }
  .stage {
    padding: 1px 7px;
    border-radius: 999px;
    background: var(--selected);
    font-size: 11px;
  }
  .row {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }
  .grow {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .detail {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 11.5px;
  }
  .ghost {
    padding: 6px 12px;
    border-radius: var(--radius-s);
    border: 1px solid var(--hairline-strong);
    font-size: 12.5px;
    color: var(--text-dim);
    align-self: flex-start;
  }
  .ghost:hover {
    background: var(--hover);
    color: var(--text);
  }
  .open {
    align-self: stretch;
    text-align: center;
  }
  .linkish {
    padding: 0;
    font-size: 12.5px;
    color: var(--text-dim);
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .linkish:hover {
    color: var(--text);
  }
  .skeleton .bar {
    display: block;
    height: 10px;
    border-radius: 999px;
    background: var(--selected);
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
</style>
