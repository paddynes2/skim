<script lang="ts">
  // Settings → Calendar (PLAN.md 7.7, 7.1). Mounted by SettingsFork.svelte
  // (one line, see docs/fork/pending/7-ui.md). Rows mirror SettingsFork's
  // look. The client secret is write-only: the field is a password input,
  // the stored value is shown masked (`••••ab12`), and an empty secret on
  // Save keeps the old one (the backend contract).
  import { onMount } from "svelte";
  import { t } from "../../lib/i18n/index.svelte";
  import { mail } from "../../lib/stores/mail.svelte";
  import { calendarApi, googleApi } from "./api";
  import { calendarErrorText } from "./guests";
  import { calPrefs } from "./settings.svelte";
  import { calendar } from "./store.svelte";
  import type { ClientStatus } from "./types";

  // ---- OAuth client (7.1) ----
  let client = $state<ClientStatus | null>(null);
  let clientId = $state("");
  let clientSecret = $state("");
  let clientBusy = $state(false);
  let clientError = $state<string | null>(null);
  let clientSaved = $state(false);

  async function loadClient() {
    try {
      client = await googleApi.clientGet();
      clientId = client.client_id ?? "";
    } catch {
      client = null;
    }
  }

  async function saveClient() {
    if (clientBusy) return;
    clientBusy = true;
    clientError = null;
    clientSaved = false;
    try {
      client = await googleApi.clientSet(clientId.trim(), clientSecret);
      clientSecret = "";
      clientSaved = true;
      await calendar.invalidate();
    } catch (e: unknown) {
      clientError = calendarErrorText(e);
    } finally {
      clientBusy = false;
    }
  }

  async function clearClient() {
    if (clientBusy) return;
    clientBusy = true;
    clientError = null;
    try {
      client = await googleApi.clientClear();
      clientId = client.client_id ?? "";
      clientSecret = "";
      await calendar.invalidate();
    } catch (e: unknown) {
      clientError = calendarErrorText(e);
    } finally {
      clientBusy = false;
    }
  }

  // ---- per-account connection (7.2) ----
  let busyAccount = $state<string | null>(null);
  let accountError = $state<Record<string, string>>({});

  async function connect(id: string) {
    busyAccount = id;
    accountError = { ...accountError, [id]: "" };
    try {
      await calendarApi.connect(id);
      await calendar.invalidate();
    } catch (e: unknown) {
      accountError = { ...accountError, [id]: calendarErrorText(e) };
    } finally {
      busyAccount = null;
    }
  }

  async function disconnect(id: string) {
    busyAccount = id;
    accountError = { ...accountError, [id]: "" };
    try {
      await calendarApi.disconnect(id);
      await calendar.invalidate();
    } catch (e: unknown) {
      accountError = { ...accountError, [id]: calendarErrorText(e) };
    } finally {
      busyAccount = null;
    }
  }

  async function toggleCalendar(calendarId: number, selected: boolean) {
    try {
      await calendarApi.setSelected(calendarId, selected);
      await calendar.invalidate();
    } catch (e: unknown) {
      clientError = calendarErrorText(e);
    }
  }

  // ---- preferences (7.7) ----
  const DAYS = [
    { n: 1, label: "fork.cal.day_mon" },
    { n: 2, label: "fork.cal.day_tue" },
    { n: 3, label: "fork.cal.day_wed" },
    { n: 4, label: "fork.cal.day_thu" },
    { n: 5, label: "fork.cal.day_fri" },
    { n: 6, label: "fork.cal.day_sat" },
    { n: 7, label: "fork.cal.day_sun" },
  ];
  const zones: string[] = (() => {
    try {
      return (Intl as unknown as { supportedValuesOf?: (k: string) => string[] }).supportedValuesOf?.("timeZone") ?? [];
    } catch {
      return [];
    }
  })();

  // onMount, not $effect: these loads write store state and must not become
  // dependencies of the thing that triggers them.
  onMount(() => {
    void loadClient();
    void calPrefs.load();
    void calendar.refreshStatus();
  });

  const anyConnected = $derived(calendar.connected);
</script>

<section class="cal-settings">
  <div class="microlabel">{t("fork.cal.settings_title")}</div>

  <!-- 7.1 client -->
  <div class="group">
    <div class="field">
      <label class="label" for="gcal-client-id">{t("fork.cal.client_id")}</label>
      <input id="gcal-client-id" class="text mono" bind:value={clientId} spellcheck="false" autocomplete="off" placeholder="…apps.googleusercontent.com" />
    </div>
    <div class="field">
      <label class="label" for="gcal-client-secret">{t("fork.cal.client_secret")}</label>
      <input
        id="gcal-client-secret"
        class="text mono"
        type="password"
        bind:value={clientSecret}
        autocomplete="off"
        placeholder={client?.secret_masked ?? ""}
      />
      <button class="chip" onclick={saveClient} disabled={clientBusy || !clientId.trim()}>{t("fork.cal.save")}</button>
      {#if client?.source === "stored"}
        <button class="chip" onclick={clearClient} disabled={clientBusy}>{t("fork.cal.clear")}</button>
      {/if}
    </div>
    <div class="hint">
      {#if client?.configured}
        {client.source === "built_in" ? t("fork.cal.client_built_in") : t("fork.cal.client_stored")}
        {#if clientSaved}<span class="ok">· {t("fork.cal.saved")}</span>{/if}
      {:else}
        {t("fork.cal.client_help")}
      {/if}
    </div>
    {#if clientError}<div class="hint danger">{clientError}</div>{/if}
  </div>

  <!-- 7.2 accounts -->
  <div class="group">
    {#each mail.accounts as a (a.id)}
      {@const st = calendar.statuses[a.id]}
      <div class="field account">
        <span class="label">{t("fork.cal.account")}</span>
        <span class="addr">{a.email}</span>
        {#if st?.connected}
          <span class="state ok">{t("fork.cal.connected")}</span>
          {#if st.pending_ops > 0}<span class="state">{t("fork.cal.pending_ops", { n: st.pending_ops })}</span>{/if}
          <button class="chip" onclick={() => disconnect(a.id)} disabled={busyAccount === a.id}>{t("fork.cal.disconnect")}</button>
        {:else}
          <span class="state">{t("fork.cal.not_connected")}</span>
          <button class="chip active" onclick={() => connect(a.id)} disabled={busyAccount === a.id || !(st?.configured ?? client?.configured)}>
            {busyAccount === a.id ? t("fork.cal.connecting") : t("fork.cal.connect")}
          </button>
        {/if}
      </div>
      {#if accountError[a.id]}<div class="hint danger">{accountError[a.id]}</div>{/if}
    {/each}
    {#if !client?.configured}
      <div class="hint">{t("fork.cal.connect_needs_client")}</div>
    {/if}
  </div>

  <!-- calendars shown -->
  {#if anyConnected && calendar.calendars.length > 0}
    <div class="group">
      <div class="field top">
        <span class="label">{t("fork.cal.calendars_shown")}</span>
        <ul class="cals">
          {#each calendar.calendars as c (c.id)}
            <li>
              <label class="cal">
                <input type="checkbox" checked={c.selected} onchange={(e) => toggleCalendar(c.id, (e.currentTarget as HTMLInputElement).checked)} />
                <span class="swatch" style:background={c.color ?? "var(--acct-1)"}></span>
                <span class="name">{c.summary}</span>
                {#if c.is_primary}<span class="microlabel">{t("fork.cal.primary")}</span>{/if}
                {#if c.access_role === "reader" || c.access_role === "freeBusyReader"}<span class="microlabel">{t("fork.cal.read_only")}</span>{/if}
              </label>
            </li>
          {/each}
        </ul>
      </div>
    </div>
  {/if}

  <!-- 7.7 preferences -->
  <div class="group">
    <div class="field">
      <span class="label">{t("fork.cal.default_len")}</span>
      <div class="chips" role="radiogroup" aria-label={t("fork.cal.default_len")}>
        {#each [15, 30, 45, 60] as n (n)}
          <button class="chip" class:active={calPrefs.defaultLen === n} role="radio" aria-checked={calPrefs.defaultLen === n} onclick={() => calPrefs.setDefaultLen(n)}>
            {t("fork.cal.minutes", { n })}
          </button>
        {/each}
      </div>
    </div>
    <div class="field">
      <label class="label" for="gcal-work-start">{t("fork.cal.working_hours")}</label>
      <input id="gcal-work-start" class="text mono short" type="time" value={calPrefs.workStart} onchange={(e) => calPrefs.setWorkStart((e.currentTarget as HTMLInputElement).value)} />
      <span class="dash">–</span>
      <input class="text mono short" type="time" aria-label={t("fork.cal.work_end")} value={calPrefs.workEnd} onchange={(e) => calPrefs.setWorkEnd((e.currentTarget as HTMLInputElement).value)} />
    </div>
    <div class="field">
      <span class="label">{t("fork.cal.working_days")}</span>
      <div class="chips">
        {#each DAYS as d (d.n)}
          <button class="chip day" class:active={calPrefs.workDaySet.has(d.n)} aria-pressed={calPrefs.workDaySet.has(d.n)} onclick={() => calPrefs.toggleWorkDay(d.n)}>
            {t(d.label)}
          </button>
        {/each}
      </div>
    </div>
    <div class="field">
      <label class="label" for="gcal-second-tz">{t("fork.cal.second_tz")}</label>
      <input
        id="gcal-second-tz"
        class="text mono"
        list="gcal-zones"
        value={calPrefs.secondTz}
        placeholder="Europe/London"
        spellcheck="false"
        onchange={(e) => calPrefs.setSecondTz((e.currentTarget as HTMLInputElement).value)}
      />
      <datalist id="gcal-zones">
        {#each zones as z (z)}<option value={z}></option>{/each}
      </datalist>
    </div>
    <div class="field">
      <label class="label" for="gcal-booking">{t("fork.cal.booking_link")}</label>
      <input
        id="gcal-booking"
        class="text mono"
        value={calPrefs.bookingLink}
        placeholder="https://"
        spellcheck="false"
        onchange={(e) => calPrefs.setBookingLink((e.currentTarget as HTMLInputElement).value)}
      />
    </div>
  </div>
</section>

<style>
  .cal-settings {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .group {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding-bottom: 8px;
    border-bottom: 1px solid var(--hairline);
  }
  .group:last-child {
    border-bottom: none;
  }
  .field {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 3px 0;
    min-width: 0;
  }
  .field.top {
    align-items: flex-start;
  }
  .label {
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 500;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-faint);
    flex: 0 0 120px;
  }
  .text {
    flex: 1;
    min-width: 0;
    padding: 5px 8px;
    border: 1px solid var(--hairline-strong);
    border-radius: var(--radius-s);
    background: var(--surface-raised);
    font-size: 13px;
    user-select: text;
  }
  .text.mono {
    font-family: var(--font-mono);
    font-size: 12px;
  }
  .text.short {
    flex: 0 0 auto;
    width: 96px;
  }
  .dash {
    color: var(--text-faint);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .chip {
    padding: 5px 11px;
    border-radius: 999px;
    font-size: 12.5px;
    color: var(--text-dim);
    border: 1px solid transparent;
    white-space: nowrap;
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
  .chip:disabled {
    opacity: 0.4;
    cursor: default;
    background: none;
  }
  .chip.day {
    padding: 4px 9px;
    font-family: var(--font-mono);
    font-size: 11px;
  }
  .addr {
    font-size: 13px;
  }
  .state {
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-faint);
  }
  .state.ok,
  .ok {
    color: var(--success);
  }
  .account .chip {
    margin-left: auto;
  }
  .hint {
    font-size: 12px;
    color: var(--text-dim);
    padding-left: 130px;
  }
  .hint.danger {
    color: var(--danger);
  }
  .cals {
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 4px;
    flex: 1;
  }
  .cal {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    cursor: pointer;
  }
  .cal input {
    accent-color: var(--text);
  }
  .swatch {
    width: 10px;
    height: 10px;
    border-radius: 3px;
    flex-shrink: 0;
  }
  .cal .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
