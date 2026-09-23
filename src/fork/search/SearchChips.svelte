<script lang="ts">
  // Fork (4.3): the list header while the list shows search results. The
  // title is the free text, each operator is a chip with a ✕, and the close
  // button (or Esc) returns to the folder the search started from.
  import { t } from "../../lib/i18n/index.svelte";
  import { chipsOf, textOf, type SearchChip } from "./query";

  interface Props {
    query: string;
    /** Total rows shown so far, for the count under the title. */
    count: number;
    onremove: (chip: SearchChip) => void;
    onclose: () => void;
  }
  let { query, count, onremove, onclose }: Props = $props();

  const chips = $derived(chipsOf(query));
  const text = $derived(textOf(query));

  function label(chip: SearchChip): string {
    const key = chip.key === "not" ? "" : `${chip.key}:`;
    return `${chip.negated ? "-" : ""}${key}${chip.value}`;
  }
</script>

<div class="search-head">
  <div class="search-title">
    <h1>
      {#if text}
        {t("fork.search.title", { q: text })}
      {:else}
        {t("fork.search.title_filters")}
      {/if}
    </h1>
    <button class="close" onclick={onclose} title="{t('fork.search.close')}  Esc" aria-label={t("fork.search.close")}>
      <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round">
        <path d="M2.5 2.5l7 7M9.5 2.5l-7 7" />
      </svg>
    </button>
  </div>
  {#if chips.length > 0}
    <div class="search-chips" role="group" aria-label={t("fork.search.filters")}>
      {#each chips as chip, i (`${i}:${chip.token}`)}
        <span class="schip" class:neg={chip.negated}>
          <span class="schip-text">{label(chip)}</span>
          <button
            class="schip-x"
            onclick={() => onremove(chip)}
            title={t("fork.search.remove", { f: label(chip) })}
            aria-label={t("fork.search.remove", { f: label(chip) })}>✕</button>
        </span>
      {/each}
    </div>
  {/if}
  <span class="microlabel count">{t("fork.search.count", { n: count })}</span>
</div>

<style>
  .search-head {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
    flex: 1;
  }
  .search-title {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  h1 {
    font-size: 17px;
    font-weight: 800;
    letter-spacing: -0.02em;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
    flex: 1;
  }
  .close {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border-radius: var(--radius-s);
    color: var(--text-faint);
    flex-shrink: 0;
  }
  .close:hover {
    background: var(--hover);
    color: var(--text);
  }
  .search-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  /* Same mono microlabel voice as the All / Unread / Starred chips (3.1); a
     removable chip carries its own ✕ so the whole pill is not the target. */
  .schip {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    padding: 2px 4px 2px 8px;
    border-radius: 999px;
    background: var(--hover);
    box-shadow: 0 0 0 1px var(--hairline);
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 500;
    letter-spacing: 0.04em;
    color: var(--text);
    white-space: nowrap;
    max-width: 100%;
  }
  .schip.neg {
    text-decoration: line-through;
    text-decoration-color: var(--text-faint);
  }
  .schip-text {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .schip-x {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    border-radius: 999px;
    color: var(--text-faint);
    font-size: 9px;
    line-height: 1;
  }
  .schip-x:hover {
    background: var(--surface-raised);
    color: var(--text);
  }
  .count {
    color: var(--text-faint);
  }
</style>
