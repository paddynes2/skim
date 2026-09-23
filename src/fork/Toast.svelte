<script lang="ts">
  // Bottom-centre toast (PLAN.md 3.2): "Archived · Undo (Z)". One at a time,
  // polite live region, pauses while hovered or focused.
  import { toast } from "./stores/toast.svelte";

  const t = $derived(toast.current);
</script>

<div class="toast-host" aria-live="polite" aria-atomic="true">
  {#if t}
    <div
      class="toast"
      role="status"
      onpointerenter={() => toast.hold()}
      onpointerleave={() => toast.release()}
      onfocusin={() => toast.hold()}
      onfocusout={() => toast.release()}
    >
      <span class="text">{t.text}</span>
      {#if t.action}
        <button class="action" onclick={() => toast.act()}>
          {t.action.label}
          {#if t.action.key}<kbd>{t.action.key}</kbd>{/if}
        </button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .toast-host {
    position: fixed;
    left: 0;
    right: 0;
    bottom: 18px;
    display: flex;
    justify-content: center;
    pointer-events: none;
    z-index: 200;
  }
  .toast {
    pointer-events: auto;
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 9px 10px 9px 14px;
    border-radius: var(--radius-m);
    background: var(--text);
    color: var(--bg);
    font-size: 13px;
    box-shadow: var(--shadow-pop);
    animation: rise 0.14s ease-out;
  }
  .text {
    white-space: nowrap;
  }
  .action {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 3px 8px;
    border-radius: var(--radius-s);
    font-weight: 600;
    color: var(--bg);
    background: rgba(255, 255, 255, 0.14);
  }
  :global(:root[data-theme$="-light"]) .action {
    background: rgba(255, 255, 255, 0.16);
  }
  .action:hover {
    background: rgba(255, 255, 255, 0.26);
  }
  .action kbd {
    font-family: var(--font-mono);
    font-size: 10px;
    padding: 0 4px;
    border: 1px solid currentColor;
    border-radius: 3px;
    opacity: 0.8;
  }
  @keyframes rise {
    from {
      transform: translateY(6px);
      opacity: 0;
    }
    to {
      transform: none;
      opacity: 1;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .toast {
      animation: none;
    }
  }
</style>
