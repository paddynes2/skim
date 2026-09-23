<script lang="ts">
  // Fork (6.3): the rich editor for the user's own words. Squire (Fastmail's
  // editor) over a contenteditable host, a short toolbar above it. Holds only
  // the words: the signature and the quoted original live below it in
  // ComposeForm, read-only, and `htmlToText` keeps `drafts.body_text` in step
  // line for line (see compose/html.ts).
  import Squire from "squire-rte";
  import { untrack } from "svelte";
  import { t } from "../lib/i18n/index.svelte";
  import { htmlToText, sanitizeToFragment } from "./compose/html";

  let {
    html = "",
    placeholder = "",
    onchange,
    onkeydown,
  }: {
    /** The words as HTML. Set by the host on load and on an AI rewrite;
     *  reported back through `onchange` on every edit. */
    html?: string;
    placeholder?: string;
    onchange?: (html: string, text: string) => void;
    onkeydown?: (e: KeyboardEvent) => void;
  } = $props();

  let host = $state<HTMLDivElement | null>(null);
  let editor: Squire | null = null;
  // Squire's path to the cursor ("BODY>DIV>B"): the toolbar's active states.
  let path = $state("");
  let empty = $state(true);
  let linkOpen = $state(false);
  let linkUrl = $state("");
  let linkInput = $state<HTMLInputElement | null>(null);
  // What the editor last reported, so a `html` prop echoing our own change
  // does not reset the cursor with a setHTML.
  let reported = "";

  $effect(() => {
    const el = host;
    if (!el) return;
    const sq = new Squire(el, {
      // Pasted markup is cleaned here (no DOMPurify on this page) and again in
      // Rust before it is sent.
      sanitizeToDOMFragment: (h: string, ed: Squire) => sanitizeToFragment(h, ed.getRoot().ownerDocument),
      toPlainText: htmlToText,
    });
    editor = sq;
    // The initial words only: a later `html` change must not rebuild the
    // editor (that would put the cursor back at the start on every key).
    const initial = untrack(() => html);
    sq.setHTML(initial || "<div><br></div>");
    reported = initial;
    empty = !sq.getRoot().textContent;
    sq.addEventListener("input", () => {
      const h = sq.getHTML();
      reported = h;
      html = h;
      empty = !sq.getRoot().textContent;
      onchange?.(h, htmlToText(h));
    });
    sq.addEventListener("pathChange", (e: Event) => {
      path = (e as CustomEvent<{ path: string }>).detail?.path ?? "";
    });
    // Google-Docs-style list keys (Squire's default puts strikethrough on 7).
    sq.setKeyHandler("Ctrl-Shift-7", (self, e) => {
      e.preventDefault();
      toggleList(self, "OL");
    });
    sq.setKeyHandler("Ctrl-Shift-8", (self, e) => {
      e.preventDefault();
      toggleList(self, "UL");
    });
    sq.setKeyHandler("Ctrl-k", (_self, e) => {
      e.preventDefault();
      openLink();
    });
    return () => {
      sq.destroy();
      editor = null;
    };
  });

  // The host replaced the words (load, AI draft): show them.
  $effect(() => {
    const h = html;
    const sq = editor;
    if (!sq || h === reported) return;
    reported = h;
    sq.setHTML(h || "<div><br></div>");
    empty = !sq.getRoot().textContent;
  });

  export function focus() {
    editor?.focus();
  }

  /** Insert plain text at the caret as one undoable edit (7.6 / 8). */
  export function insertText(text: string) {
    editor?.focus();
    editor?.insertPlainText(text, false);
  }

  /** The contenteditable, for the AI-tell underlines (6.5). */
  export function hostEl(): HTMLElement | null {
    return host;
  }

  /** Replace the words and put the cursor at the end (AI rewrite). */
  export function setHTML(h: string) {
    html = h;
    reported = h;
    editor?.setHTML(h || "<div><br></div>");
    empty = !editor?.getRoot().textContent;
  }

  function toggleList(sq: Squire, tag: "UL" | "OL") {
    const inList = new RegExp(`(?:^|>)${tag}\\b`).test(sq.getPath());
    if (inList) sq.removeList();
    else if (tag === "UL") sq.makeUnorderedList();
    else sq.makeOrderedList();
  }

  const active = (tag: string) => new RegExp(`(?:^|>)${tag}\\b`).test(path);

  function run(fn: (sq: Squire) => void) {
    const sq = editor;
    if (!sq) return;
    fn(sq);
    sq.focus();
  }

  function toggle(tag: "B" | "I" | "U") {
    run((sq) => {
      const on = sq.hasFormat(tag);
      if (tag === "B") on ? sq.removeBold() : sq.bold();
      else if (tag === "I") on ? sq.removeItalic() : sq.italic();
      else on ? sq.removeUnderline() : sq.underline();
    });
  }

  function quote() {
    run((sq) => (active("BLOCKQUOTE") ? sq.decreaseQuoteLevel() : sq.increaseQuoteLevel()));
  }

  function openLink() {
    if (active("A")) {
      run((sq) => sq.removeLink());
      return;
    }
    linkUrl = "";
    linkOpen = true;
    queueMicrotask(() => linkInput?.focus());
  }

  function applyLink() {
    const url = linkUrl.trim();
    linkOpen = false;
    if (!url) return;
    const href = /^(https?:\/\/|mailto:)/i.test(url) ? url : `https://${url}`;
    run((sq) => sq.makeLink(href));
  }

  function onLinkKey(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      applyLink();
    } else if (e.key === "Escape") {
      e.preventDefault();
      linkOpen = false;
      editor?.focus();
    }
    e.stopPropagation();
  }
</script>

<div class="rich">
  <div class="toolbar" role="toolbar" aria-label={t("fork.compose.formatting")}>
    <button type="button" class="tb" class:on={active("B")} onmousedown={(e) => e.preventDefault()} onclick={() => toggle("B")} title="{t('fork.compose.bold')} (Ctrl B)" aria-label={t("fork.compose.bold")}><b>B</b></button>
    <button type="button" class="tb" class:on={active("I")} onmousedown={(e) => e.preventDefault()} onclick={() => toggle("I")} title="{t('fork.compose.italic')} (Ctrl I)" aria-label={t("fork.compose.italic")}><i>I</i></button>
    <button type="button" class="tb" class:on={active("U")} onmousedown={(e) => e.preventDefault()} onclick={() => toggle("U")} title="{t('fork.compose.underline')} (Ctrl U)" aria-label={t("fork.compose.underline")}><u>U</u></button>
    <span class="sep"></span>
    <button type="button" class="tb" class:on={active("A")} onmousedown={(e) => e.preventDefault()} onclick={openLink} title="{t('fork.compose.link')} (Ctrl K)" aria-label={t("fork.compose.link")}>
      <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3"><path d="M6.5 9.5a3 3 0 0 0 4.2 0l2-2a3 3 0 0 0-4.2-4.2l-1 1" /><path d="M9.5 6.5a3 3 0 0 0-4.2 0l-2 2a3 3 0 0 0 4.2 4.2l1-1" /></svg>
    </button>
    <button type="button" class="tb" class:on={active("UL")} onmousedown={(e) => e.preventDefault()} onclick={() => run((sq) => toggleList(sq, "UL"))} title="{t('fork.compose.bullets')} (Ctrl Shift 8)" aria-label={t("fork.compose.bullets")}>
      <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3"><path d="M6 4h8M6 8h8M6 12h8" /><circle cx="2.8" cy="4" r="0.9" fill="currentColor" stroke="none" /><circle cx="2.8" cy="8" r="0.9" fill="currentColor" stroke="none" /><circle cx="2.8" cy="12" r="0.9" fill="currentColor" stroke="none" /></svg>
    </button>
    <button type="button" class="tb" class:on={active("OL")} onmousedown={(e) => e.preventDefault()} onclick={() => run((sq) => toggleList(sq, "OL"))} title="{t('fork.compose.numbers')} (Ctrl Shift 7)" aria-label={t("fork.compose.numbers")}>
      <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3"><path d="M6 4h8M6 8h8M6 12h8" /><text x="1.2" y="5.6" font-size="4.6" fill="currentColor" stroke="none" font-family="var(--font-mono)">1</text><text x="1.2" y="9.6" font-size="4.6" fill="currentColor" stroke="none" font-family="var(--font-mono)">2</text><text x="1.2" y="13.6" font-size="4.6" fill="currentColor" stroke="none" font-family="var(--font-mono)">3</text></svg>
    </button>
    <button type="button" class="tb" class:on={active("BLOCKQUOTE")} onmousedown={(e) => e.preventDefault()} onclick={quote} title={t("fork.compose.quote")} aria-label={t("fork.compose.quote")}>
      <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3"><path d="M3 3v10" /><path d="M6.5 5h7M6.5 8h7M6.5 11h5" /></svg>
    </button>
    <span class="sep"></span>
    <button type="button" class="tb" onmousedown={(e) => e.preventDefault()} onclick={() => run((sq) => sq.removeAllFormatting())} title={t("fork.compose.clear_format")} aria-label={t("fork.compose.clear_format")}>
      <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3"><path d="M4 3h9M8.5 3l-2 8" /><path d="M3 13l3-3M6 13l-3-3" /></svg>
    </button>
    {#if linkOpen}
      <input
        class="link-url"
        bind:this={linkInput}
        bind:value={linkUrl}
        placeholder={t("fork.compose.link_placeholder")}
        spellcheck="false"
        onkeydown={onLinkKey}
        onblur={applyLink}
      />
    {/if}
  </div>
  <div class="host-wrap">
    <div class="host" bind:this={host} onkeydown={onkeydown} spellcheck="true" role="textbox" aria-multiline="true" tabindex="0"></div>
    {#if empty && placeholder}
      <div class="placeholder" aria-hidden="true">{placeholder}</div>
    {/if}
  </div>
</div>

<style>
  .rich {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 6px 16px 0;
  }
  .tb {
    width: 28px;
    height: 26px;
    display: grid;
    place-items: center;
    border-radius: var(--radius-s);
    color: var(--text-dim);
    font-size: 13px;
  }
  .tb:hover {
    background: var(--hover);
    color: var(--text);
  }
  .tb.on {
    background: var(--hover);
    color: var(--text);
    box-shadow: inset 0 0 0 1px var(--hairline-strong);
  }
  .sep {
    width: 1px;
    height: 16px;
    background: var(--hairline);
    margin: 0 4px;
  }
  .link-url {
    flex: 1;
    min-width: 0;
    margin-left: 8px;
    padding: 3px 8px;
    font-size: 12.5px;
    border: 1px solid var(--hairline-strong);
    border-radius: var(--radius-s);
    background: var(--bg);
    color: var(--text);
    user-select: text;
  }
  .host-wrap {
    position: relative;
    flex: 1;
    min-height: 120px;
    overflow-y: auto;
  }
  .host {
    min-height: 100%;
    padding: 14px 20px;
    font-size: 14px;
    line-height: 1.6;
    outline: none;
    user-select: text;
    cursor: text;
    overflow-wrap: anywhere;
  }
  .host :global(blockquote) {
    margin: 0 0 0 0.2em;
    padding-left: 0.8em;
    border-left: 2px solid var(--hairline-strong);
    color: var(--text-dim);
  }
  .host :global(a) {
    color: var(--link, var(--text));
    text-decoration: underline;
  }
  .host :global(ul),
  .host :global(ol) {
    padding-left: 1.4em;
    margin: 0;
  }
  .placeholder {
    position: absolute;
    top: 14px;
    left: 20px;
    color: var(--text-faint);
    font-size: 14px;
    line-height: 1.6;
    pointer-events: none;
  }
</style>
