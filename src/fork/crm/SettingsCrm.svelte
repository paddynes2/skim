<script lang="ts">
  // Phase 9: Settings → CRM. The three connection values are prefilled from
  // the build (or whatever was saved); Patrick types his Rebound email and
  // password once. The password goes to Rust for one token request and is
  // never kept; the anon key is shown masked and only sent when retyped.
  // Styles mirror SettingsFork.svelte / Settings.svelte (tokens only).
  import { onMount } from "svelte";
  import { t } from "../../lib/i18n/index.svelte";
  import { crmApi } from "./api";
  import { crm } from "./store.svelte";
  import type { CrmStatus } from "./types";

  let status = $state<CrmStatus | null>(null);
  let baseUrl = $state("");
  let supabaseUrl = $state("");
  let anonKey = $state("");
  let email = $state("");
  let password = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);
  let configDirty = $state(false);

  function adopt(s: CrmStatus) {
    status = s;
    baseUrl = s.baseUrl ?? "";
    supabaseUrl = s.supabaseUrl ?? "";
    anonKey = "";
    configDirty = false;
    if (s.email && !email) email = s.email;
    crm.invalidate(s);
  }

  async function run(action: () => Promise<CrmStatus>) {
    busy = true;
    error = null;
    try {
      adopt(await action());
    } catch (e) {
      error = e && typeof e === "object" && "message" in e ? String((e as { message: unknown }).message) : String(e);
    } finally {
      busy = false;
    }
  }

  onMount(() => {
    void run(() => crmApi.status());
  });

  const saveConfig = () => run(() => crmApi.setConfig(baseUrl, supabaseUrl, anonKey === "" ? null : anonKey));
  const connect = () =>
    run(async () => {
      const s = await crmApi.connect(email.trim(), password);
      password = "";
      return s;
    });
  const disconnect = () => run(() => crmApi.disconnect());
  const pick = (id: string) => run(() => crmApi.pickWorkspace(id));

  const connected = $derived(status?.connected === true);
  const needsPick = $derived(connected && !status?.workspaceId && (status?.workspaces?.length ?? 0) > 0);
  const chosen = $derived(status?.workspaces?.find((w) => w.id === status?.workspaceId) ?? null);
</script>

<section class="crm-settings">
  <div class="microlabel">{t("fork.crm.settings")}</div>

  <div class="fields">
    <label class="field">
      <span class="microlabel">{t("fork.crm.base_url")}</span>
      <input value={baseUrl} oninput={(e) => { baseUrl = e.currentTarget.value; configDirty = true; }} placeholder="https://rebound.patricknesbitt.ai" spellcheck="false" autocomplete="off" />
    </label>
    <label class="field">
      <span class="microlabel">{t("fork.crm.supabase_url")}</span>
      <input value={supabaseUrl} oninput={(e) => { supabaseUrl = e.currentTarget.value; configDirty = true; }} placeholder="https://xxxx.supabase.co" spellcheck="false" autocomplete="off" />
    </label>
    <label class="field">
      <span class="microlabel">{t("fork.crm.anon_key")}</span>
      <input
        type="password"
        value={anonKey}
        oninput={(e) => { anonKey = e.currentTarget.value; configDirty = true; }}
        placeholder={status?.anonKeyMasked ?? t("fork.crm.anon_key_ph")}
        spellcheck="false"
        autocomplete="off"
      />
      <span class="dim hint">{t("fork.crm.config_hint")}</span>
    </label>
    <div class="row">
      <button class="ghost" onclick={saveConfig} disabled={busy || !configDirty}>{t("fork.crm.save")}</button>
      {#if status && !status.configured}
        <span class="dim">{t("fork.crm.not_configured")}</span>
      {/if}
    </div>
  </div>

  {#if status?.configured}
    {#if !connected}
      <form class="fields" onsubmit={(e) => { e.preventDefault(); void connect(); }}>
        <label class="field">
          <span class="microlabel">{t("fork.crm.login_email")}</span>
          <input type="email" bind:value={email} autocomplete="username" spellcheck="false" />
        </label>
        <label class="field">
          <span class="microlabel">{t("fork.crm.login_password")}</span>
          <input type="password" bind:value={password} autocomplete="current-password" />
          <span class="dim hint">{t("fork.crm.password_hint")}</span>
        </label>
        <div class="row">
          <button type="submit" class="ghost" disabled={busy || !email.trim() || !password}>
            {busy ? t("fork.crm.connecting") : t("fork.crm.connect")}
          </button>
        </div>
      </form>
    {:else}
      <div class="fields">
        <div class="row">
          <span class="grow">
            <span class="ok">●</span>
            {t("fork.crm.connected_as", { email: status?.email ?? email ?? "" })}
          </span>
          <button class="ghost" onclick={disconnect} disabled={busy}>{t("fork.crm.disconnect")}</button>
        </div>
        {#if needsPick}
          <div class="field">
            <span class="microlabel">{t("fork.crm.pick_workspace")}</span>
            <div class="chips" role="radiogroup" aria-label={t("fork.crm.pick_workspace")}>
              {#each status?.workspaces ?? [] as w (w.id)}
                <button class="chip" role="radio" aria-checked="false" onclick={() => void pick(w.id)} disabled={busy}>
                  {w.name ?? w.slug ?? w.id}
                </button>
              {/each}
            </div>
          </div>
        {:else if status?.workspaceId}
          <div class="row">
            <span class="microlabel">{t("fork.crm.workspace")}</span>
            <span class="mono">{chosen?.name ?? chosen?.slug ?? status.workspaceId}</span>
            {#if (status.workspaces?.length ?? 0) > 1}
              <div class="chips">
                {#each status.workspaces ?? [] as w (w.id)}
                  <button class="chip" class:active={w.id === status?.workspaceId} onclick={() => void pick(w.id)} disabled={busy}>
                    {w.name ?? w.slug ?? w.id}
                  </button>
                {/each}
              </div>
            {/if}
          </div>
        {:else if status?.error}
          <span class="warn">{status.error}</span>
        {/if}
      </div>
    {/if}
  {/if}

  {#if error}
    <div class="warn">{error}</div>
  {/if}
  <span class="dim hint">{t("fork.crm.readonly_hint")}</span>
</section>

<style>
  .crm-settings {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .microlabel {
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 500;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-faint);
  }
  .fields {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .field input {
    padding: 8px 10px;
    border: 1px solid var(--hairline-strong);
    border-radius: var(--radius-s);
    font-size: 13px;
    user-select: text;
    font-family: inherit;
  }
  .field input:focus {
    border-color: var(--text-faint);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 12.5px;
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .ok {
    color: var(--success);
    margin-right: 4px;
    font-size: 10px;
  }
  .mono {
    font-family: var(--font-mono);
    font-size: 11.5px;
  }
  .dim {
    color: var(--text-faint);
    font-size: 12px;
  }
  .hint {
    font-size: 11.5px;
  }
  .warn {
    color: var(--danger);
    font-size: 12.5px;
    line-height: 1.45;
  }
  .ghost {
    padding: 6px 12px;
    border-radius: var(--radius-s);
    border: 1px solid var(--hairline-strong);
    font-size: 12.5px;
    color: var(--text-dim);
    flex-shrink: 0;
  }
  .ghost:hover:not(:disabled) {
    background: var(--hover);
    color: var(--text);
  }
  .ghost:disabled {
    opacity: 0.5;
    cursor: default;
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
  }
  .chip:hover:not(:disabled) {
    background: var(--hover);
    color: var(--text);
  }
  .chip.active {
    background: var(--text);
    color: var(--bg);
    font-weight: 600;
  }
</style>
