<script lang="ts">
  // Fork (6.5.5): the one-line prompt Send stops on when the draft still has
  // hard findings and `fork_smell_block_hard` is on: "2 must-fix items"
  // with Fix (jump to the first) / Send anyway. Warn findings never block.
  import { t } from "../../lib/i18n/index.svelte";
  import { sendPrompt } from "./health";
  import { mustFix, type ScanResult } from "./scan";

  interface Props {
    result: ScanResult;
    onfix: () => void;
    onsend: () => void;
  }
  let { result, onfix, onsend }: Props = $props();
  const line = $derived(sendPrompt(result, t));
  const first = $derived(mustFix(result)[0]?.suggestion ?? "");
</script>

{#if line}
  <div class="smell-send" role="alertdialog" aria-label={line}>
    <span class="msg">{line}</span>
    <span class="hint">{first}</span>
    <button class="fix" onclick={onfix}>{t("fork.smell.fix")}</button>
    <button class="anyway" onclick={onsend}>{t("fork.smell.send_anyway")}</button>
  </div>
{/if}

<style>
  .smell-send {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 10px;
    border: 1px solid var(--danger);
    border-radius: var(--radius-m);
    background: var(--surface-raised);
    font-size: 12px;
    color: var(--text);
  }
  .msg {
    color: var(--danger);
    font-weight: 600;
    white-space: nowrap;
  }
  .hint {
    color: var(--text-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
  }
  .fix,
  .anyway {
    font: inherit;
    font-size: 12px;
    border-radius: 6px;
    padding: 3px 9px;
    cursor: pointer;
  }
  .fix {
    background: var(--danger);
    color: var(--on-accent, #fff);
    border: 1px solid var(--danger);
  }
  .anyway {
    background: none;
    color: var(--text);
    border: 1px solid var(--hairline);
  }
</style>
