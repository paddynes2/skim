<script lang="ts">
  import { initials } from "../avatar";

  let { deal, domain, onclick }: { deal: string; domain: string | null; onclick?: () => void } = $props();
  const source = $derived(domain
    ? `https://www.google.com/s2/favicons?domain=${encodeURIComponent(domain)}&sz=64`
    : null);
  let failedSource = $state<string | null>(null);
  let loadedSource = $state<string | null>(null);
</script>

{#snippet artwork()}
  {#if source && failedSource !== source}
    <img src={source} alt="" width="24" height="24" referrerpolicy="no-referrer"
      class:loaded={loadedSource === source} onload={() => { loadedSource = source; }}
      onerror={() => { failedSource = source; }} />
  {/if}
  {#if !source || failedSource === source || loadedSource !== source}
    <span class="initials" aria-hidden="true">{initials(deal, "")}</span>
  {/if}
{/snippet}

{#if onclick}<button class="deal-badge" title={deal} aria-label={deal} {onclick}>{@render artwork()}</button>{:else}<span class="deal-badge" title={deal} role="img" aria-label={deal}>{@render artwork()}</span>{/if}

<style>
  .deal-badge {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--deal-logo-size, 28px);
    height: var(--deal-logo-size, 28px);
    flex-shrink: 0;
    box-sizing: border-box;
    border-radius: 6px;
    background: var(--deal-soft);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--deal-ink) 20%, transparent);
    overflow: hidden;
  }
  img {
    position: absolute;
    opacity: 0;
    width: calc(100% - 6px);
    height: calc(100% - 6px);
    object-fit: contain;
    border-radius: 4px;
  }
  img.loaded { opacity: 1; background: #fff; }
  .deal-badge:has(img.loaded) { background: #fff; }
  button.deal-badge:hover { box-shadow: inset 0 0 0 1px var(--deal-ink); }
  button.deal-badge:focus-visible { outline: 2px solid var(--focus); outline-offset: 2px; }
  .initials {
    display: grid;
    place-items: center;
    width: 100%;
    height: 100%;
    border-radius: 4px;
    color: var(--deal-ink);
    font-size: 10px;
    font-weight: 600;
  }
</style>
