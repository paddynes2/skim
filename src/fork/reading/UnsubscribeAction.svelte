<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { api, errorMessage } from "../../lib/api";
  import { t } from "../../lib/i18n/index.svelte";
  import type { MessageMeta } from "../../lib/types";
  import { unsubscribeLink } from "./unsubscribe";

  let { message }: { message: MessageMeta } = $props();
  let busy = $state(false);
  let result = $state<"" | "submitted" | "opened" | "missing" | "blocked">("");
  let error = $state("");
  let flagged = $state<string | null>(null);

  async function openPage(url: string) {
    await openUrl(url);
    result = "opened";
    flagged = null;
  }

  async function unsubscribe() {
    if (busy) return;
    busy = true; error = ""; flagged = null;
    try {
      if (message.canUnsubscribe) {
        try {
          result = await api.unsubscribe(message.id);
          return;
        } catch (e) {
          // A malformed header can still have a usable footer. Other errors
          // must stay visible, because a queued request must not be repeated.
          if ((e as { code?: string })?.code !== "unsubscribe") throw e;
        }
      }
      const body = await api.getMessageBody(message.id, false, false);
      const url = unsubscribeLink(body.html);
      if (!url) { result = "missing"; return; }
      if (body.security?.links.some((link) => link.href === url)) {
        flagged = url;
        return;
      }
      await openPage(url);
    } catch (e) { error = errorMessage(e); }
    finally { busy = false; }
  }

  async function block() {
    if (busy) return;
    busy = true; error = "";
    try {
      await invoke("fork_block_sender", { messageId: message.id });
      result = "blocked";
    } catch (e) { error = errorMessage(e); }
    finally { busy = false; }
  }

  async function openFlagged() {
    if (!flagged || busy) return;
    busy = true; error = "";
    try { await openPage(flagged); }
    catch (e) { error = errorMessage(e); }
    finally { busy = false; }
  }
</script>

<div class="unsubscribe">
  <button class="chip" disabled={busy || result === "submitted" || result === "blocked"} onclick={unsubscribe}>
    {t(busy ? "fork.unsubscribe.working" : result === "submitted" ? "fork.unsubscribe.queued" : result === "blocked" ? "fork.unsubscribe.blocked" : "reading.unsubscribe")}
  </button>
  {#if result === "opened"}<span role="status">{t("fork.unsubscribe.opened")}</span>{/if}
  {#if flagged}
    <span role="alert">{t("fork.unsubscribe.flagged", { host: new URL(flagged).host })}</span>
    <button class="chip" disabled={busy} onclick={openFlagged}>{t("security.open_anyway")}</button>
    <button class="chip" onclick={() => flagged = null}>{t("settings.cancel")}</button>
  {/if}
  {#if result === "missing"}
    <span role="status">{t("fork.unsubscribe.missing", { addr: message.from.addr })}</span>
    <button class="chip" disabled={busy} onclick={block}>{t("fork.unsubscribe.block")}</button>
    <button class="chip" onclick={() => result = ""}>{t("settings.cancel")}</button>
  {/if}
  {#if error}<span class="error" role="alert">{error}</span>{/if}
</div>

<style>
  .unsubscribe { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; min-width: 0; }
  span { color: var(--text-dim); font-size: 12px; overflow-wrap: anywhere; }
  .error { color: var(--danger); }
  .chip { border: 1px solid var(--hairline); background: transparent; color: var(--text-dim); border-radius: var(--radius-s); padding: 2px 9px; font-size: 12px; line-height: 1.4; cursor: pointer; }
  .chip:hover:not(:disabled) { color: var(--text); background: var(--hover); }
  .chip:disabled { cursor: default; }
</style>
