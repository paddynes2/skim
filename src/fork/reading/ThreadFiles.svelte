<script lang="ts">
  // v1.1.24 (D72): every file in the conversation, pinned to the top of the
  // reading pane however far down it is scrolled. Reads the attachment rows
  // straight from the cache (`fork_thread_files`), so a file in a message
  // whose body is not open yet is still listed. One line; "All files" opens
  // the full chips with Save all and a jump to each file's message.
  import { invoke } from "@tauri-apps/api/core";
  import { t, getLocale } from "../../lib/i18n/index.svelte";
  import { api, errorMessage } from "../../lib/api";
  import AttachmentChips from "../../components/AttachmentChips.svelte";
  import type { AttachmentMeta, MessageMeta } from "../../lib/types";

  let { messages, refreshKey, loading, onselect, onload }: {
    messages: MessageMeta[];
    /** Changes when bodies land, so newly parsed files show. */
    refreshKey: number;
    loading: boolean;
    onselect: (id: number) => void;
    onload: () => void;
  } = $props();

  let files = $state<AttachmentMeta[]>([]);
  let pending = $state<number[]>([]);
  let open = $state(false);
  let error = $state("");

  // Every message: `fork_thread_files` decides which carry files.
  const ids = $derived(messages.map((m) => m.id));
  // Copies of one file sent again down the thread show once in the line.
  const shown = $derived.by(() => {
    const seen = new Set<string>();
    return files.filter((f) => {
      const key = `${f.filename ?? ""}:${f.size}`;
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    });
  });

  $effect(() => {
    const messageIds = ids;
    void refreshKey;
    let stale = false;
    void invoke<{ files: AttachmentMeta[]; pending: number[] }>("fork_thread_files", { messageIds })
      .then((r) => {
        if (stale) return;
        files = r.files;
        pending = r.pending;
      })
      .catch(() => {});
    return () => {
      stale = true;
    };
  });

  function from(file: AttachmentMeta) {
    const m = messages.find((m) => m.id === file.messageId);
    if (!m) return "";
    const date = new Date(m.date * 1000).toLocaleDateString(getLocale(), { day: "numeric", month: "short" });
    return `${m.from.name || m.from.addr}, ${date}`;
  }

  async function openFile(file: AttachmentMeta) {
    error = "";
    try {
      await api.openAttachment(file.id);
    } catch (e) {
      error = errorMessage(e);
    }
  }
</script>

{#if shown.length > 0 || pending.length > 0}
  <div class="thread-files" class:open>
    <div class="line">
      <span class="label">{t("fork.threadfiles.label")}{#if shown.length > 0}&nbsp;{shown.length}{/if}</span>
      <div class="pills">
        {#each shown as f (f.id)}
          <button class="pill" onclick={() => void openFile(f)} title={t("fork.threadfiles.open_title", { from: from(f) })}>
            <span class="name">{f.filename ?? t("fork.files.attachment")}</span>
          </button>
        {/each}
        {#if pending.length > 0}
          <button class="pending" disabled={loading} onclick={onload}>{t(loading ? "fork.reading.loading_remaining" : "fork.threadfiles.pending", { n: pending.length })}</button>
        {/if}
      </div>
      {#if files.length > 0}
        <button class="toggle" aria-expanded={open} onclick={() => (open = !open)}>{t(open ? "fork.threadfiles.hide" : "fork.threadfiles.all")}</button>
      {/if}
    </div>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    {#if open}
      <div class="all">
        <AttachmentChips attachments={files} onsource={(f) => { open = false; onselect(f.messageId); }} sourceLabel={from} />
      </div>
    {/if}
  </div>
{/if}

<style>
  .thread-files {
    position: sticky;
    top: 0;
    z-index: 3;
    margin: 0 -36px 10px;
    padding: 7px 36px;
    background: var(--bg);
    border-bottom: 1px solid var(--hairline);
  }
  .line {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }
  .label {
    flex-shrink: 0;
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 500;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-faint);
  }
  .pills {
    display: flex;
    gap: 6px;
    flex: 1;
    min-width: 0;
    overflow-x: auto;
    scrollbar-width: none;
  }
  .pills::-webkit-scrollbar {
    display: none;
  }
  .pill {
    flex-shrink: 0;
    max-width: 240px;
    padding: 4px 10px;
    border: 1px solid var(--hairline-strong);
    border-radius: 999px;
    font-size: 12px;
    color: var(--text);
  }
  .pill:hover {
    background: var(--hover);
  }
  .name {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pending {
    flex-shrink: 0;
    font-size: 12px;
    color: var(--primary);
    white-space: nowrap;
  }
  .pending:disabled {
    opacity: 0.6;
  }
  .toggle {
    flex-shrink: 0;
    font-size: 12px;
    color: var(--text-dim);
  }
  .toggle:hover {
    color: var(--text);
  }
  .all {
    max-height: 50vh;
    overflow: auto;
  }
  .error {
    margin: 6px 0 0;
    font-size: 12px;
    color: var(--danger);
  }
  button:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
  }
</style>
