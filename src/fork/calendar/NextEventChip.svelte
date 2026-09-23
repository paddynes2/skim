<script lang="ts">
  // Titlebar chip (PLAN.md 7.5): "14:00 Call with X · in 25m · Join" while an
  // event starts within the hour; nothing otherwise. Click opens the event in
  // the calendar view; Join opens the Meet link in the browser.
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { t } from "../../lib/i18n/index.svelte";
  import { ui } from "../../lib/stores/ui.svelte";
  import { fmtIn, fmtTime } from "./guests";
  import { calendar } from "./store.svelte";

  const ev = $derived(calendar.upcoming);

  function open() {
    if (!ev) return;
    ui.showCalendar();
    calendar.goto(new Date(ev.start_ts * 1000));
    calendar.open(ev.id);
  }
</script>

{#if ev}
  <div class="chip" data-testid="next-event">
    <button class="main" onclick={open} title={t("fork.cal.chip_open")}>
      <span class="time">{fmtTime(ev.start_ts)}</span>
      <span class="title">{ev.summary || t("fork.cal.untitled")}</span>
      <span class="sep">·</span>
      <span class="in">{fmtIn(ev.start_ts, calendar.now)}</span>
    </button>
    {#if ev.hangout_link}
      <button class="join" onclick={() => openUrl(ev.hangout_link!)}>{t("fork.cal.join")}</button>
    {/if}
  </div>
{/if}

<style>
  .chip {
    display: inline-flex;
    align-items: center;
    height: 24px;
    border: 1px solid var(--hairline-strong);
    border-radius: 999px;
    background: var(--surface);
    font-size: 12px;
    color: var(--text-dim);
    overflow: hidden;
    max-width: 380px;
  }
  .main {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 0 10px;
    height: 100%;
    min-width: 0;
    color: inherit;
  }
  .main:hover {
    background: var(--hover);
    color: var(--text);
  }
  .time {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text);
  }
  .title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 200px;
    color: var(--text);
    font-weight: 600;
  }
  .sep {
    color: var(--text-faint);
  }
  .in {
    white-space: nowrap;
  }
  .join {
    height: 100%;
    padding: 0 10px;
    border-left: 1px solid var(--hairline-strong);
    font-weight: 700;
    font-size: 11.5px;
    color: var(--text);
  }
  .join:hover {
    background: var(--text);
    color: var(--bg);
  }
</style>
