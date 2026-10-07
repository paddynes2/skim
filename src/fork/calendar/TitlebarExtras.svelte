<script lang="ts">
  // These controls participate in title bar layout so they cannot cover the account or window buttons.
  import MeetNow from "./MeetNow.svelte";
  import NextEventChip from "./NextEventChip.svelte";
</script>

<div class="extras">
  <NextEventChip />
  <MeetNow />
</div>

<style>
  .extras {
    height: var(--titlebar-h);
    display: flex;
    align-items: center;
    gap: 8px;
    z-index: 25;
    min-width: 0;
    max-width: min(550px, 60vw);
  }
</style>
