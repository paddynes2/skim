<script lang="ts">
  import { highlightParts } from "./passages";
  let {text,query=""}:{text:string;query?:string}=$props();
  const parts=$derived(highlightParts(text,query));
</script>
{#each parts as part}{#if part.match}<mark>{part.text}</mark>{:else}{part.text}{/if}{/each}
<style>mark{background:color-mix(in srgb,var(--primary) 22%,transparent);color:var(--text);border-radius:2px;padding:0 1px}</style>
