<script lang="ts">
  // Fork (v1.1.3): the deal a Deals row belongs to ("IQ-EQ"). An overlay on
  // the upstream MessageRow, like the court and reminder badges, so the row and
  // its fixed height stay untouched.
  let { deal }: { deal: string } = $props();
</script>

<span class="deal-badge" title={deal}>{deal}</span>

<style>
  .deal-badge {
    display: inline-flex;
    max-width: 100%;
    padding: 1px 7px;
    border-radius: 999px;
    background: var(--bg);
    box-shadow: 0 0 0 1px var(--hairline);
    font-size: 11px;
    font-weight: 600;
    color: var(--text-dim);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
