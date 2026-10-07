<script lang="ts">
  import { tick } from "svelte";
  import { t } from "../../lib/i18n/index.svelte";
  import type { ThreadRow } from "../../lib/types";
  import { act, archiveOffered, type Action } from "../actions";
  import { mail } from "../../lib/stores/mail.svelte";
  import { dealsStore } from "../deals/store.svelte";
  let { thread, x, y, onclose }: { thread: ThreadRow; x: number; y: number; onclose: () => void } = $props();
  let menu: HTMLDivElement;
  $effect(() => { void tick().then(() => menu?.querySelector<HTMLButtonElement>("button")?.focus()); });
  $effect(() => {
    window.addEventListener("keydown", keys, true);
    return () => window.removeEventListener("keydown", keys, true);
  });
  function close() { onclose(); }
  function run(action: Action) { const target = thread; close(); void act(target, action); }
  function addDeal() { mail.selectedThreadId = thread.id; mail.selectedMessageId = thread.messageId ?? null; dealsStore.requestAdd(thread.id); close(); }
  function keys(event: KeyboardEvent) {
    // The menu owns keyboard actions until it closes. Global mail shortcuts
    // otherwise target the selected conversation, not the right-clicked one.
    event.stopImmediatePropagation();
    if (!event.ctrlKey && !event.metaKey && !event.altKey) {
      const action: Action | undefined = event.key.toLowerCase() === "s" ? "toggle_star"
        : event.key.toLowerCase() === "u" ? "toggle_read"
        : event.key === "Delete" ? "delete"
        : event.key.toLowerCase() === "e" && archiveOffered(mail.selectedFolder?.role) ? "archive" : undefined;
      if (action) { event.preventDefault(); run(action); return; }
    }
    if (event.key === "Escape") { event.preventDefault(); event.stopImmediatePropagation(); close(); }
    if (["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) {
      event.preventDefault(); event.stopPropagation();
      const buttons = [...menu.querySelectorAll<HTMLButtonElement>("button")];
      const current = buttons.indexOf(document.activeElement as HTMLButtonElement);
      const next = event.key === "Home" ? 0 : event.key === "End" ? buttons.length - 1 : (current + (event.key === "ArrowDown" ? 1 : -1) + buttons.length) % buttons.length;
      buttons[next]?.focus();
    }
    if (event.key === "Tab") close();
  }
</script>
<svelte:window onpointerdown={(event) => { if (!menu?.contains(event.target as Node)) close(); }} onblur={close} />
<div class="message-menu" role="menu" tabindex="-1" bind:this={menu} style:left={`${Math.min(x, window.innerWidth - 230)}px`} style:top={`${Math.min(y, window.innerHeight - 260)}px`}>
  <button role="menuitem" class="deal-action" onclick={addDeal}>{t("fork.deals.add")}</button>
  <button role="menuitem" onclick={() => run("toggle_star")}>{t(thread.isStarred ? "reading.unstar" : "reading.star")}<kbd>S</kbd></button>
  <button role="menuitem" onclick={() => run("toggle_read")}>{t(thread.isRead ? "reading.mark_unread" : "reading.mark_read")}<kbd>U</kbd></button>
  {#if archiveOffered(mail.selectedFolder?.role)}<button role="menuitem" onclick={() => run("archive")}>{t("reading.archive")}<kbd>E</kbd></button>{/if}
  <button role="menuitem" onclick={() => run("delete")}>{t("reading.delete")}<kbd>Del</kbd></button>
</div>
<style>
  .message-menu { position: fixed; z-index: 80; width: 220px; padding: 5px; border: 1px solid var(--hairline-strong); border-radius: var(--radius-m); background: var(--surface-raised); box-shadow: var(--shadow-pop); }
  button { display: flex; align-items: center; justify-content: space-between; width: 100%; padding: 10px; border-radius: var(--radius-s); text-align: left; font-size: 12px; color: var(--text); }
  button:hover, button:focus-visible { background: var(--hover); outline: 2px solid var(--focus); outline-offset: -2px; }
  .deal-action { color: var(--deal-ink); background: var(--deal-soft); }
  kbd { color: var(--text-dim); font-size: 10px; }
</style>
