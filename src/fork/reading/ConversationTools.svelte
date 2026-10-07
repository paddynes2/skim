<script lang="ts">
  import { t, getLocale } from "../../lib/i18n/index.svelte";
  import AttachmentChips from "../../components/AttachmentChips.svelte";
  import type { MessageMeta, RenderedBody } from "../../lib/types";
  let { messages, bodies, loading, onload, onselect }: { messages: MessageMeta[]; bodies: Record<number, RenderedBody | "loading" | "fetching" | "error">; loading: boolean; onload: () => void; onselect: (id: number) => void } = $props();
  let mode = $state<"search" | "files" | null>(null);
  let query = $state("");
  const loaded = $derived(messages.filter((m) => typeof bodies[m.id] === "object").length);
  const documents = $derived(messages.map((message) => {
    const body = bodies[message.id];
    let text = "";
    if (typeof body === "object") {
      const document = new DOMParser().parseFromString(body.html, "text/html");
      document.querySelectorAll("style,script").forEach((node) => node.remove());
      text = document.body.textContent ?? "";
    }
    return { message, body: typeof body === "object" ? body : null, text: [message.subject, message.snippet, message.from.name, message.from.addr, ...message.to.map((a) => a.addr), text].join(" ").toLocaleLowerCase() };
  }));
  const matches = $derived(query.trim() ? documents.filter((d) => d.text.includes(query.trim().toLocaleLowerCase())) : []);
  const files = $derived(documents.filter((d) => d.body?.attachments.some((a) => !a.isInline)));
  function date(unix: number) { return new Date(unix * 1000).toLocaleDateString(getLocale(), { day: "numeric", month: "short", year: "numeric" }); }
</script>
<div class="conversation-tools">
  <div class="tabs"><button class:active={mode === "search"} aria-expanded={mode === "search"} onclick={() => (mode = mode === "search" ? null : "search")}>{t("fork.reading.search_thread")}</button><button class:active={mode === "files"} aria-expanded={mode === "files"} onclick={() => (mode = mode === "files" ? null : "files")}>{t("fork.reading.thread_files")}</button></div>
  {#if mode}
    <div class="tools-body">
      <div class="coverage" role="status"><span>{t("fork.reading.loaded_coverage", { loaded, total: messages.length })}</span>{#if loaded < messages.length}<button disabled={loading} onclick={onload}>{t(loading ? "fork.reading.loading_remaining" : "fork.reading.load_remaining")}</button>{/if}</div>
      {#if mode === "search"}
        <input type="search" aria-label={t("fork.reading.search_thread")} placeholder={t("fork.reading.search_placeholder")} bind:value={query} />
        {#if query.trim()}<div class="results">{#each matches as item}<button class="result" onclick={() => onselect(item.message.id)}><strong>{item.message.from.name || item.message.from.addr}<small>{date(item.message.date)}</small></strong><span>{item.message.snippet}</span></button>{:else}<p>{t(loaded === messages.length ? "fork.reading.no_matches" : "fork.reading.no_loaded_matches")}</p>{/each}</div>{/if}
      {:else}
        {#each files as item}<div class="file-group"><button class="source" onclick={() => onselect(item.message.id)}>{item.message.from.name || item.message.from.addr} &middot; {date(item.message.date)}</button><AttachmentChips attachments={item.body!.attachments.filter((a) => !a.isInline)} /></div>{:else}<p>{t(loaded === messages.length ? "fork.reading.no_files" : "fork.reading.no_loaded_files")}</p>{/each}
      {/if}
    </div>
  {/if}
</div>
<style>
  .conversation-tools { margin: 12px 0 18px; border-bottom: 1px solid var(--hairline); padding-bottom: 10px; }
  .tabs { display: flex; gap: 16px; }.tabs button { font-size: 12px; color: var(--text-dim); padding: 5px 0; }.tabs button.active { color: var(--text); font-weight: 600; }
  .tools-body { margin-top: 12px; padding: 12px; border: 1px solid var(--hairline); border-radius: var(--radius-s); }
  .coverage { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 8px; color: var(--text-dim); font-size: 11px; margin-bottom: 10px; }.coverage button { color: var(--primary); }.coverage button:disabled { opacity: .6; }
  input { width: 100%; padding: 9px 10px; border: 1px solid var(--hairline-strong); border-radius: var(--radius-s); font-size: 13px; background: var(--surface); }
  .results { max-height: 260px; overflow: auto; }.result { display: block; text-align: left; width: 100%; padding: 10px 2px; border-bottom: 1px solid var(--hairline); }.result:hover { background: var(--hover); }.result strong { display: flex; justify-content: space-between; gap: 12px; font-size: 12px; }.result small { font-weight: 400; flex-shrink: 0; }.result span { display: block; font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-dim); margin-top: 3px; } p { color: var(--text-dim); font-size: 12px; margin-top: 8px; }
  .file-group { margin-top: 16px; }.source { color: var(--text-dim); font-size: 11px; }.source:hover { text-decoration: underline; }
</style>
