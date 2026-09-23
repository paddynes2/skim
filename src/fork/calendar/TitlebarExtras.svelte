<script lang="ts">
  // The two titlebar mounts of Phase 7 (next-event chip, Meet now), drawn as
  // one fixed strip over the titlebar's empty middle so App.svelte mounts it
  // next to <Titlebar /> without an upstream edit to Titlebar.svelte. Not a
  // drag region: both children are buttons.
  import MeetNow from "./MeetNow.svelte";
  import NextEventChip from "./NextEventChip.svelte";
</script>

<div class="extras">
  <NextEventChip />
  <MeetNow />
</div>

<style>
  .extras {
    position: fixed;
    top: 0;
    right: 150px;
    height: var(--titlebar-h);
    display: flex;
    align-items: center;
    gap: 8px;
    z-index: 25;
  }
</style>
