<script lang="ts">
  // The small hint shown while `g` waits for its second key (PLAN.md 3.3).
  import { t } from "../lib/i18n/index.svelte";
  import { mail } from "../lib/stores/mail.svelte";
  import { GO_TARGETS, goSequence, goTo } from "./keys";
  import { navHooks } from "./nav";

  // Only destinations that exist right now: role folders the scope has,
  // and the views whose hooks a later phase registered.
  const targets = $derived(
    GO_TARGETS.filter((g) => {
      if (g.role)
        return mail.folders.some((f) => f.role === g.role || (g.role === "archive" && f.role === "all"));
      if (g.key === "c") return !!navHooks.calendar;
      return !!navHooks.court;
    }),
  );
</script>

{#if goSequence.armed}
  <div class="go-hint" role="status" aria-live="polite">
    <span class="lead"><kbd>g</kbd> {t("fork.go.then")}</span>
    {#each targets as g (g.key)}
      <button class="go" onclick={() => { goSequence.cancel(); goTo(g.key); }}>
        <kbd>{g.key}</kbd><span>{t(g.label)}</span>
      </button>
    {/each}
  </div>
{/if}

<style>
  .go-hint {
    position: fixed;
    left: 50%;
    bottom: 72px;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 6px 8px;
    border-radius: var(--radius-m);
    background: var(--surface-raised);
    box-shadow: var(--shadow-pop), 0 0 0 1px var(--hairline-strong);
    font-size: 12px;
    z-index: 190;
  }
  .lead {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 0 6px;
    color: var(--text-dim);
  }
  .go {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 3px 8px;
    border-radius: var(--radius-s);
    color: var(--text);
  }
  .go:hover {
    background: var(--hover);
  }
  kbd {
    font-family: var(--font-mono);
    font-size: 10.5px;
    padding: 1px 5px;
    border: 1px solid var(--hairline-strong);
    border-radius: 4px;
    color: var(--text);
    background: var(--surface);
  }
</style>
