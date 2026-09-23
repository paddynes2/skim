<script lang="ts">
  // Fork (6.5.4): the suggestion popover for one underline. Shows the rule's
  // reason and static suggestion; Ignore (this occurrence), Ignore rule (a
  // setting), and ✦ Rewrite (AI, violet: the one place the accent is allowed).
  // Rewrite asks `fork_smell_rewrite` for three options, already fact-guarded
  // in Rust and re-checked here with the TS mirror; Accept replaces the
  // sentence through the mount as one undoable edit.
  import { t } from "../../lib/i18n/index.svelte";
  import type { Span } from "./scan";
  import type { SmellMount } from "./highlight";
  import { smellApi, type SmellSettings, ignoreRule } from "./api";
  import { guard } from "./factguard";

  interface Props {
    span: Span;
    rect: DOMRect;
    mount: SmellMount;
    settings: SmellSettings;
    /** AI configured (key present). Rewrite is hidden without one. */
    aiAvailable: boolean;
    /** Optional: the demo / a test hands in the rewrite call. */
    rewrite?: typeof smellApi.rewrite;
    onclose: () => void;
    /** After Ignore (the occurrence): the caller drops the span until the next edit. */
    onignore: (span: Span) => void;
  }
  let { span, rect, mount, settings, aiAvailable, rewrite = smellApi.rewrite, onclose, onignore }: Props = $props();

  let busy = $state(false);
  let error = $state<string | null>(null);
  let options = $state<string[]>([]);
  let rejected = $state(0);
  let asked = $state(false);
  let requestId = "";

  const ctx = $derived(mount.context(span));
  const title = $derived(span.category.replace(/_/g, " "));

  // Keep the popover inside the window: below the underline, flipped above
  // when there is no room, clamped to the right edge.
  const pos = $derived.by(() => {
    const w = 360;
    const h = 220;
    const vw = typeof window === "undefined" ? 1440 : window.innerWidth;
    const vh = typeof window === "undefined" ? 900 : window.innerHeight;
    const left = Math.max(8, Math.min(rect.left, vw - w - 8));
    const top = rect.bottom + h + 8 > vh ? Math.max(8, rect.top - h - 6) : rect.bottom + 6;
    return { left, top, w };
  });

  async function doRewrite() {
    if (busy) return;
    busy = true;
    error = null;
    asked = true;
    requestId = `smell-${Date.now()}`;
    try {
      const res = await rewrite(
        { sentence: ctx.sentence, before: ctx.before, after: ctx.after, reason: span.suggestion },
        requestId,
      );
      // The Rust guard already filtered; the mirror is the second lock.
      options = res.options.filter((o) => guard(ctx.sentence, o)).slice(0, 3);
      rejected = res.rejected + (res.options.length - options.length);
    } catch (e) {
      error = (e as { message?: string })?.message ?? String(e);
    } finally {
      busy = false;
    }
  }

  function accept(option: string) {
    mount.replace(ctx.start, ctx.end, option);
    onclose();
  }

  async function ignoreThisRule() {
    await ignoreRule(settings, span.rule);
    mount.rescan();
    onclose();
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.stopPropagation();
      if (busy) void smellApi.cancel(requestId).catch(() => {});
      onclose();
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<div
  class="smell-pop"
  class:hard={span.severity === "hard"}
  role="dialog"
  aria-label={t("fork.smell.popover_title")}
  style="left: {pos.left}px; top: {pos.top}px; width: {pos.w}px"
  tabindex="-1"
>
  <div class="head">
    <span class="sev" class:hard={span.severity === "hard"}>{span.severity === "hard" ? t("fork.smell.must_fix") : t("fork.smell.tell")}</span>
    <span class="cat">{title}</span>
    <button class="x" onclick={onclose} aria-label={t("fork.smell.close")}>✕</button>
  </div>
  <p class="reason">{span.suggestion}</p>
  <p class="matched">“{span.text}”</p>

  {#if asked}
    <div class="options">
      {#if busy}
        <p class="wait">{t("fork.smell.rewriting")}</p>
      {:else if error}
        <p class="err">{error}</p>
      {:else if options.length === 0}
        <p class="wait">{t("fork.smell.no_options")}</p>
      {:else}
        {#each options as o, i (i)}
          <button class="opt" onclick={() => accept(o)}>
            <span class="n">{i + 1}</span>
            <span class="txt">{o}</span>
          </button>
        {/each}
      {/if}
      {#if rejected > 0 && !busy}
        <p class="note">{t("fork.smell.rejected", { n: rejected })}</p>
      {/if}
    </div>
  {/if}

  <div class="actions">
    <button class="ghost" onclick={() => { onignore(span); onclose(); }}>{t("fork.smell.ignore")}</button>
    <button class="ghost" onclick={ignoreThisRule}>{t("fork.smell.ignore_rule")}</button>
    {#if aiAvailable && span.severity !== "hard"}
      <button class="ai" onclick={doRewrite} disabled={busy}>✦ {t("fork.smell.rewrite")}</button>
    {/if}
  </div>
</div>

<style>
  .smell-pop {
    position: fixed;
    z-index: 60;
    background: var(--surface-raised);
    color: var(--text);
    border: 1px solid var(--hairline);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow-pop);
    padding: 10px 12px 10px;
    font-size: 13px;
    line-height: 1.4;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 6px;
  }
  .sev {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.02em;
    text-transform: uppercase;
    color: var(--smell-warn, #b7791f);
  }
  .sev.hard {
    color: var(--danger);
  }
  .cat {
    color: var(--text-dim);
    font-size: 12px;
  }
  .x {
    margin-left: auto;
    background: none;
    border: 0;
    color: var(--text-dim);
    cursor: pointer;
    font-size: 12px;
    padding: 2px 4px;
  }
  .reason {
    margin: 0 0 4px;
  }
  .matched {
    margin: 0 0 8px;
    color: var(--text-dim);
    font-style: italic;
  }
  .options {
    display: grid;
    gap: 6px;
    margin: 6px 0 8px;
  }
  .opt {
    display: flex;
    gap: 8px;
    text-align: left;
    background: var(--hover);
    border: 1px solid var(--hairline);
    border-radius: 8px;
    color: var(--text);
    padding: 6px 8px;
    cursor: pointer;
    font: inherit;
  }
  .opt:hover {
    border-color: var(--accent);
  }
  .opt .n {
    color: var(--accent);
    font-weight: 600;
    min-width: 1ch;
  }
  .wait,
  .note,
  .err {
    margin: 0;
    color: var(--text-dim);
    font-size: 12px;
  }
  .err {
    color: var(--danger);
  }
  .actions {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .ghost {
    background: none;
    border: 1px solid var(--hairline);
    border-radius: 6px;
    color: var(--text);
    padding: 4px 8px;
    cursor: pointer;
    font: inherit;
    font-size: 12px;
  }
  .ai {
    margin-left: auto;
    background: var(--accent-soft);
    border: 1px solid var(--accent-dim);
    border-radius: 6px;
    color: var(--accent);
    padding: 4px 10px;
    cursor: pointer;
    font: inherit;
    font-size: 12px;
    font-weight: 600;
  }
  .ai:disabled {
    opacity: 0.6;
    cursor: default;
  }
</style>
