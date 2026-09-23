<script lang="ts">
  // Fork (6.5.3): the one-line health bar under the editor. Only the parts
  // that fired; nothing at all when the draft is clean. Click jumps to the
  // first span of that part.
  import { t } from "../../lib/i18n/index.svelte";
  import { healthParts } from "./health";
  import type { ScanResult, Span } from "./scan";

  interface Props {
    result: ScanResult;
    onjump: (span: Span) => void;
  }
  let { result, onjump }: Props = $props();
  const parts = $derived(healthParts(result, t));
</script>

{#if parts.length > 0}
  <div class="smell-health" role="status">
    {#each parts as p, i (p.kind)}
      {#if i > 0}<span class="dot">·</span>{/if}
      {#if p.jumpTo}
        <button class="part {p.kind}" onclick={() => onjump(p.jumpTo!)}>{p.text}</button>
      {:else}
        <span class="part {p.kind}">{p.text}</span>
      {/if}
    {/each}
  </div>
{/if}

<style>
  .smell-health {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 0 2px;
    font-size: 12px;
    color: var(--text-dim);
    white-space: nowrap;
    overflow: hidden;
  }
  .dot {
    color: var(--text-faint);
  }
  .part {
    background: none;
    border: 0;
    padding: 0;
    font: inherit;
    color: inherit;
    cursor: default;
  }
  button.part {
    cursor: pointer;
  }
  button.part:hover {
    text-decoration: underline;
  }
  .part.hard {
    color: var(--danger);
  }
  .part.tells,
  .part.contrast {
    color: var(--smell-warn, #b7791f);
  }
</style>
