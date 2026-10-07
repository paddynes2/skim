<script lang="ts">
  import { onMount } from "svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { t } from "../../lib/i18n/index.svelte";
  import AttachmentLinks from "./AttachmentLinks.svelte";
  import { eventState } from "./presentation";
  import { calendar } from "./store.svelte";
  import { fmtWhen, parseAttendees, calendarErrorText } from "./guests";
  import type { EventRow } from "./types";
  let { row, onclose }: { row: EventRow; onclose: () => void } = $props();
  let error = $state("");
  let discarding = $state(false);
  async function discard() { discarding=true; try { await calendar.discardSplit(row); } catch(e) { error=calendarErrorText(e); } finally { discarding=false; } }
  let heading: HTMLHeadingElement;
  onMount(() => heading?.focus({ preventScroll: true }));
  const sync = $derived(calendar.syncState(row.id));
  async function retry() { try { await calendar.retry(row.id); } catch (e) { error = calendarErrorText(e); } }
</script>
<aside class="preview" aria-label={t("fork.cal.event_preview")}>
  <header><span>{calendar.calendarOf(row)?.summary}</span><button onclick={onclose} aria-label={t("fork.cal.close")}>&times;</button></header>
  <div class="body">
    <h2 bind:this={heading} tabindex="-1">{row.summary || t("fork.cal.untitled")}</h2>
    <p class="when">{fmtWhen(row)}</p>
    <p class="event-state">{t(`fork.cal.state_${eventState(row)}`)}</p>
    {#if row.time_zone}<p class="muted">{row.time_zone}</p>{/if}
    <p class="sync" class:failed={sync?.status === "failed"}>{t(sync?.status === "failed" ? "fork.cal.sync_failed" : sync?.status === "pending" || row.local_only ? "fork.cal.sync_pending" : calendar.syncKnown(row) ? "fork.cal.synced" : "fork.cal.sync_unknown")}</p>
    {#if sync?.status === "failed"}<p class="muted">{sync.message}</p><button class="btn" onclick={retry}>{t("fork.cal.retry")}</button>{/if}
    {#if sync?.can_discard}<details><summary>{t("fork.cal.discard_split")}</summary><p>{t("fork.cal.discard_split_note")}</p><button class="btn" disabled={discarding} onclick={discard}>{t("fork.cal.discard_split_confirm")}</button></details>{/if}
    {#if row.location}<h3>{t("fork.cal.location")}</h3><p>{row.location}</p>{/if}
    {#if row.hangout_link}<button class="btn primary" onclick={() => openUrl(row.hangout_link!)}>{t("fork.cal.join")}</button>{/if}
    {#if parseAttendees(row).length}<h3>{t("fork.cal.guests")}</h3><ul>{#each parseAttendees(row) as guest}<li><span>{guest.displayName || guest.email}</span><small>{t(`fork.cal.rsvp_${guest.responseStatus ?? "needsAction"}`)}</small></li>{/each}</ul>{/if}
    {#if row.description}<h3>{t("fork.cal.description")}</h3><p class="description">{row.description}</p>{/if}
    {#if row.options?.attachments?.length}<AttachmentLinks value={row.options.attachments} />{/if}
    {#if error}<p class="failed" role="alert">{error}</p>{/if}
  </div>
  <footer><button class="btn primary" onclick={() => calendar.edit(row.id)}>{t(calendar.canEdit(row) ? "fork.cal.edit_event" : "fork.cal.event_details")}</button><button class="btn" disabled={!calendar.connected} onclick={() => calendar.duplicate(row)}>{t("fork.cal.duplicate")}</button>{#if row.html_link}<button class="google" onclick={() => openUrl(row.html_link!)}>{t("fork.cal.open_in_google")}</button>{/if}</footer>
</aside>
<style>
  .preview { width: 320px; max-width: 100%; flex-shrink: 0; border-left: 1px solid var(--hairline); display: flex; flex-direction: column; min-height: 0; background: var(--surface); }
  header { display: flex; align-items: center; justify-content: space-between; gap: 10px; padding: 14px 20px; border-bottom: 1px solid var(--hairline); font-size: 12px; color: var(--text-dim); }
  header button { width: 24px; height: 24px; font-size: 20px; }
  .body { padding: 24px 20px; overflow-y: auto; flex: 1; }
  h2:focus { outline: none; }
  h2 { margin: 0 0 14px; font-size: 22px; line-height: 1.3; overflow-wrap: anywhere; }
  p { font-size: 13px; line-height: 1.55; overflow-wrap: anywhere; margin: 5px 0; }
  .when { font-weight: 600; } .muted, small { color: var(--text-dim); } .sync { color: var(--text-dim); font-size: 11px; margin: 14px 0; } .failed { color: var(--danger); }
  h3 { font-size: 12px; margin: 24px 0 8px; color: var(--text-dim); font-weight: 600; }
  ul { list-style: none; padding: 0; display: grid; gap: 10px; } li { display: grid; gap: 3px; font-size: 13px; overflow-wrap: anywhere; } small { font-size: 11px; } .description { white-space: pre-wrap; }
  footer { padding: 16px 20px; border-top: 1px solid var(--hairline); display: flex; gap: 8px; flex-wrap: wrap; }
  .btn { padding: 7px 12px; border: 1px solid var(--hairline-strong); border-radius: var(--radius-m); font-size: 12px; } .btn:hover { background: var(--hover); } .btn.primary { background: var(--primary); color: var(--on-primary); border-color: var(--primary); } .btn:disabled { opacity: .5; } .google { padding: 6px 0; font-size: 12px; color: var(--text-dim); }
</style>
