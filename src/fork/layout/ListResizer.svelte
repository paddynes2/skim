<script lang="ts">
  import { prefs } from "../stores/prefs.svelte";
  import { t } from "../../lib/i18n/index.svelte";
  let { max = 640 }: { max?: number } = $props();
  let dragging = $state(false);
  let origin = 0, width = 0;
  function start(e: PointerEvent) {
    if (e.button !== 0) return;
    origin = e.clientX; width = Math.min(prefs.listWidth, max); dragging = true;
    e.currentTarget instanceof HTMLElement && e.currentTarget.setPointerCapture(e.pointerId);
    e.preventDefault();
  }
  function move(e: PointerEvent) { if (dragging) prefs.setListWidth(Math.min(max, width + e.clientX - origin), false); }
  function finish() { if (dragging) { dragging = false; prefs.setListWidth(prefs.listWidth); } }
  function key(e: KeyboardEvent) {
    if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(e.key)) return;
    e.preventDefault();
    prefs.setListWidth(e.key === "Home" ? 300 : e.key === "End" ? max : Math.min(max, Math.min(prefs.listWidth, max) + (e.key === "ArrowLeft" ? -20 : 20)));
  }
</script>
<div class="resizer" class:dragging role="slider" tabindex="0" aria-label={t("fork.list.resize")} aria-orientation="horizontal" aria-valuemin="300" aria-valuemax={max} aria-valuenow={Math.min(prefs.listWidth, max)} onpointerdown={start} onpointermove={move} onpointerup={finish} onpointercancel={finish} onlostpointercapture={finish} onkeydown={key} ondblclick={() => prefs.setListWidth(420)}></div>
<style>
  .resizer{position:absolute;right:-4px;top:0;bottom:0;width:8px;z-index:8;cursor:col-resize;touch-action:none}
  .resizer:hover,.resizer.dragging,.resizer:focus-visible{background:var(--primary);opacity:.65;outline:none}
</style>
