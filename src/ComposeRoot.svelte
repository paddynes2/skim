<script lang="ts">
  // Root for compose windows: minimal boot (settings → theme/locale), then
  // the composer itself.
  import Composer from "./components/Composer.svelte";
  import { api } from "./lib/api";
  import { setLocale } from "./lib/i18n/index.svelte";
  import { ui } from "./lib/stores/ui.svelte";
  import { prefs } from "./fork/stores/prefs.svelte";
  import { applyZoom, zoomKey } from "./fork/zoom";
  // Fork (7.6 / 8): the compose window needs its own popover and toast host,
  // or `/slots` and Add Meet link act on a popover that only the main window has.
  import SlotsPopover from "./fork/slots/SlotsPopover.svelte";
  import Toast from "./fork/Toast.svelte";

  let { draftId }: { draftId: number } = $props();
  let ready = $state(false);

  $effect(() => {
    void (async () => {
      try {
        const settings = await api.getSettings();
        if (settings.locale) await setLocale(settings.locale as never);
        // Match the stored theme (the main window owns migration write-back).
        ui.hydrate(settings.theme);
        // Fork: same preferences (zoom, rich text) as the main window.
        prefs.hydrate(settings);
        void applyZoom(prefs.zoom);
      } catch {
        // best effort
      }
      ready = true;
    })();
  });

  // Fork (2.7): zoom keys work in this window too.
  function onKeydown(e: KeyboardEvent) {
    zoomKey(e);
  }
</script>

<svelte:window onkeydown={onKeydown} />

{#if ready}
  <Composer {draftId} />
  <SlotsPopover />
  <Toast />
{/if}
