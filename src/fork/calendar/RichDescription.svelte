<script lang="ts">
  // The event description editor (D97): bold, italic, underline, lists and
  // links, like Google Calendar's. It edits HTML and hands back only what
  // description.ts allows, so a pasted image or style never reaches Google.
  import { onMount } from "svelte";
  import { t } from "../../lib/i18n/index.svelte";
  import { safeDescription } from "./description";

  let { value = $bindable(""), readonly = false, placeholder = "" }: { value?: string; readonly?: boolean; placeholder?: string } = $props();
  let box: HTMLDivElement;
  let last = "";

  onMount(() => {
    box.innerHTML = value;
    last = value;
  });
  // A value set from outside (a scope change reloads the fields) replaces the text.
  $effect(() => {
    const v = value;
    if (box && v !== last) {
      box.innerHTML = v;
      last = v;
    }
  });
  function sync() {
    const clean = box.innerText.trim() === "" ? "" : safeDescription(box.innerHTML);
    last = clean;
    value = clean;
  }
  function cmd(name: string, arg?: string) {
    if (readonly) return;
    box.focus();
    document.execCommand(name, false, arg);
    sync();
  }
  function link() {
    const url = window.prompt(t("fork.cal.link_prompt"), "https://");
    if (url && /^(https:\/\/|mailto:)/i.test(url.trim())) cmd("createLink", url.trim());
  }
  function onpaste(e: ClipboardEvent) {
    // Paste as text with line breaks; the allow-list runs on sync anyway.
    const text = e.clipboardData?.getData("text/plain");
    if (text === undefined) return;
    e.preventDefault();
    document.execCommand("insertText", false, text);
    sync();
  }
  function onkeydown(e: KeyboardEvent) {
    if (!(e.ctrlKey || e.metaKey) || e.altKey) return;
    const k = e.key.toLowerCase();
    if (k === "b" || k === "i" || k === "u") {
      e.preventDefault();
      cmd(k === "b" ? "bold" : k === "i" ? "italic" : "underline");
    }
  }
</script>

<div class="rich" class:readonly>
  {#if !readonly}
    <div class="tools" role="toolbar" aria-label={t("fork.cal.description")}>
      <button type="button" title="Ctrl+B" aria-label={t("fork.cal.fmt_bold")} onmousedown={(e) => e.preventDefault()} onclick={() => cmd("bold")}><b>B</b></button>
      <button type="button" title="Ctrl+I" aria-label={t("fork.cal.fmt_italic")} onmousedown={(e) => e.preventDefault()} onclick={() => cmd("italic")}><i>I</i></button>
      <button type="button" title="Ctrl+U" aria-label={t("fork.cal.fmt_underline")} onmousedown={(e) => e.preventDefault()} onclick={() => cmd("underline")}><u>U</u></button>
      <button type="button" aria-label={t("fork.cal.fmt_bullets")} onmousedown={(e) => e.preventDefault()} onclick={() => cmd("insertUnorderedList")}>&bull;</button>
      <button type="button" aria-label={t("fork.cal.fmt_numbers")} onmousedown={(e) => e.preventDefault()} onclick={() => cmd("insertOrderedList")}>1.</button>
      <button type="button" aria-label={t("fork.cal.fmt_link")} onmousedown={(e) => e.preventDefault()} onclick={link}>&#128279;</button>
    </div>
  {/if}
  <div
    bind:this={box}
    class="box"
    contenteditable={!readonly}
    role="textbox"
    aria-multiline="true"
    aria-label={t("fork.cal.description")}
    data-placeholder={placeholder}
    tabindex="0"
    oninput={sync}
    {onpaste}
    {onkeydown}
  ></div>
</div>

<style>
  .rich { border: 1px solid var(--hairline-strong); border-radius: var(--radius-s); background: var(--surface-raised); }
  .rich.readonly { background: transparent; }
  .tools { display: flex; gap: 2px; padding: 3px 4px; border-bottom: 1px solid var(--hairline); }
  .tools button { min-width: 26px; height: 24px; border-radius: var(--radius-s); font-size: 12.5px; color: var(--text-dim); }
  .tools button:hover { background: var(--hover); color: var(--text); }
  .box { min-height: 84px; max-height: 260px; overflow-y: auto; padding: 8px 10px; font-size: 13px; line-height: 1.5; outline: none; user-select: text; }
  .box:empty::before { content: attr(data-placeholder); color: var(--text-faint); }
  .box :global(p) { margin: 0 0 6px; }
  .box :global(ul), .box :global(ol) { margin: 0 0 6px; padding-left: 20px; }
  .box :global(a) { color: var(--text); text-decoration: underline; }
</style>
