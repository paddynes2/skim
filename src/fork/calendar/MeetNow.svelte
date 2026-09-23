<script lang="ts">
  // Titlebar "Meet now" (PLAN.md 7.6): video icon, key `m` (keys.ts ->
  // navHooks.meetNow), palette row. The work is in calendar.meetNow(): a fresh
  // Meet copied + opened when Google is connected, meet.new otherwise.
  import { t } from "../../lib/i18n/index.svelte";
  import { calendar } from "./store.svelte";

  let busy = $state(false);

  async function click() {
    if (busy) return;
    busy = true;
    try {
      await calendar.meetNow();
    } finally {
      busy = false;
    }
  }
</script>

<button class="meet" class:busy onclick={click} disabled={busy} title={`${t("fork.nav.meet_now")} (M)`} aria-label={t("fork.nav.meet_now")}>
  <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" aria-hidden="true">
    <rect x="1.5" y="4" width="9" height="8" rx="1.5" />
    <path d="M10.5 7.2l4-2.2v6l-4-2.2" />
  </svg>
  <span class="label">{t("fork.nav.meet_now")}</span>
</button>

<style>
  .meet {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 24px;
    padding: 0 10px;
    border-radius: 999px;
    border: 1px solid var(--hairline-strong);
    color: var(--text-dim);
    font-size: 12px;
    font-weight: 600;
    background: var(--surface);
  }
  .meet:hover {
    background: var(--hover);
    color: var(--text);
  }
  .meet.busy {
    opacity: 0.6;
    cursor: progress;
  }
</style>
