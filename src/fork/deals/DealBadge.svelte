<script lang="ts">
  import { initials } from "../avatar";

  let { deal, domain }: { deal: string; domain: string | null } = $props();
  const source = $derived(domain
    ? `https://www.google.com/s2/favicons?domain=${encodeURIComponent(domain)}&sz=64`
    : null);
  let failedSource = $state<string | null>(null);
  let loadedSource = $state<string | null>(null);
</script>

<span class="deal-badge" title={deal} role="img" aria-label={deal}>
  {#if source && failedSource !== source}
    <img src={source} alt="" width="24" height="24" referrerpolicy="no-referrer"
      class:loaded={loadedSource === source} onload={() => { loadedSource = source; }}
      onerror={() => { failedSource = source; }} />
  {/if}
  {#if !source || failedSource === source || loadedSource !== source}
    <span class="initials" aria-hidden="true">{initials(deal, "")}</span>
  {/if}
</span>

<style>
  .deal-badge {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    flex-shrink: 0;
  }
  img {
    position: absolute;
    opacity: 0;
    width: 24px;
    height: 24px;
    object-fit: contain;
    border-radius: 4px;
  }
  img.loaded { opacity: 1; }
  .initials {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: 4px;
    background: var(--bg);
    box-shadow: 0 0 0 1px var(--hairline);
    color: var(--text-dim);
    font-size: 10px;
    font-weight: 600;
  }
</style>
