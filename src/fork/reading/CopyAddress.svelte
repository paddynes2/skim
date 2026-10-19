<script lang="ts">
  // Fork (D75): one click on an email address copies it.
  import { t } from "../../lib/i18n/index.svelte";
  import { toast } from "../stores/toast.svelte";

  let { addr, brackets = false }: { addr: string; brackets?: boolean } = $props();

  async function copy(e: MouseEvent) {
    e.stopPropagation();
    try {
      await navigator.clipboard.writeText(addr);
      toast.show({ text: t("fork.copy.done", { addr }) });
    } catch {
      // Clipboard refused: the address is still selectable by hand.
    }
  }
</script>

<button type="button" class="copy-addr" onclick={copy} title={t("fork.copy.address")}>{brackets ? `<${addr}>` : addr}</button>

<style>
  .copy-addr {
    all: unset;
    color: inherit;
    font: inherit;
    cursor: copy;
    border-radius: 3px;
    overflow-wrap: anywhere;
  }
  .copy-addr:hover {
    text-decoration: underline;
    text-decoration-color: var(--text-faint);
  }
  .copy-addr:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 1px;
  }
</style>
