<script lang="ts">
  import { folderLabel } from "../lib/folders";
  import { t } from "../lib/i18n/index.svelte";
  import { ai } from "../lib/stores/ai.svelte";
  import { aiSessions } from "../lib/stores/aiSession.svelte";
  import { mail, rowKey } from "../lib/stores/mail.svelte";
  import { ui } from "../lib/stores/ui.svelte";
  import MessageRow from "./MessageRow.svelte";
  import SelectionBar from "./SelectionBar.svelte";
  import { prefs } from "../fork/stores/prefs.svelte";
  import SearchChips from "../fork/search/SearchChips.svelte";
  // Fork (10): the court views' header and per-row age + reason badge.
  import CourtRow from "../fork/court/CourtRow.svelte";
  import type { CourtRow as CourtRowShape } from "../fork/court/types";

  // "now" for the row ages, refreshed each minute so a row that crosses a
  // day boundary recolours without a reload.
  let nowSecs = $state(Math.floor(Date.now() / 1000));
  $effect(() => {
    const timer = setInterval(() => (nowSecs = Math.floor(Date.now() / 1000)), 60_000);
    return () => clearInterval(timer);
  });
  const courtOf = (row: unknown): CourtRowShape | null => {
    const r = row as Partial<CourtRowShape>;
    return typeof r.since === "number" ? (row as CourtRowShape) : null;
  };

  const title = $derived.by(() => {
    const f = mail.selectedFolder;
    return f ? folderLabel(f) : t("nav.inbox");
  });

  const unread = $derived(mail.selectedFolder?.unreadCount ?? 0);

  // Only the user's own folders can be renamed or deleted — provider folders
  // (Inbox, Trash, Spam…) are not ours to touch. Hidden in the unified view too:
  // a virtual folder stands for one real folder per account, and renaming all of
  // them at once is not what anyone means by "rename this".
  const editableFolder = $derived(
    !mail.unified && mail.selectedFolder?.role === null ? mail.selectedFolder : null,
  );

  function openEditor() {
    const f = editableFolder;
    if (f) ui.openFolderEditor({ id: f.id, name: f.displayName });
  }

  // AI Recap is an inbox catch-up: only there, only with unread mail.
  const recapAvailable = $derived(
    mail.selectedFolder?.role === "inbox" && unread > 0 && ai.keyPresent,
  );

  function openRecap() {
    // This folder's digest is already open in a window of its own — show it
    // there rather than scanning the same mail a second time.
    const detached = aiSessions.recapFor(mail.selectedFolderId);
    if (detached?.detached) {
      void aiSessions.detach(detached);
      return;
    }
    // The digest occupies the reading pane — clear the selection.
    mail.selectedThreadId = null;
    ui.openRecap();
  }

  let rowsEl: HTMLDivElement | undefined = $state();

  // Windowed rendering: only the rows near the viewport get live components,
  // so a folder with thousands of messages doesn't hold thousands of DOM
  // nodes. Rows are fixed-height (three nowrap lines), which keeps the
  // arithmetic exact; the real height is measured off the first rendered row.
  const OVERSCAN = 8;
  let scrollTop = $state(0);
  let viewH = $state(0);
  let rowH = $state(76);

  const start = $derived(Math.max(0, Math.floor(scrollTop / rowH) - OVERSCAN));
  const end = $derived(
    Math.min(mail.threads.length, Math.ceil((scrollTop + viewH) / rowH) + OVERSCAN),
  );
  const visible = $derived(mail.threads.slice(start, end));

  // Measure the row height for the windowing arithmetic. Rows are NOT perfectly
  // uniform — one with an empty snippet is a line shorter — so as different rows
  // scroll into the measured slot, a two-way `!= rowH` update would flip rowH
  // between e.g. 68 and 76 forever: that feeds `visible`, which re-runs this
  // effect, which re-measures… Svelte caps that as `effect_update_depth_exceeded`
  // and then stops applying updates — the whole UI freezes (list stops loading,
  // clicks stop opening) until a full restart. So only ever ratchet *up* to the
  // tallest row seen; it converges in a couple of steps and can't oscillate.
  // Overestimating a little is harmless — overscan keeps the viewport full.
  $effect(() => {
    void visible;
    const el = rowsEl?.querySelector<HTMLElement>("button.row");
    if (el && el.offsetHeight > rowH) rowH = el.offsetHeight;
  });

  // Fork (2.6): a density change is a new baseline, set explicitly (never
  // measured two-way, see above). The ratchet then only grows from here.
  $effect(() => {
    rowH = prefs.density === "compact" ? 36 : 76;
  });

  // A new folder (or grouping mode) is a new list — start it at the top.
  $effect(() => {
    void mail.selectedFolderId;
    void mail.groupThreads;
    if (rowsEl) rowsEl.scrollTop = 0;
    scrollTop = 0;
  });

  function onScroll() {
    if (!rowsEl) return;
    scrollTop = rowsEl.scrollTop;
    if (rowsEl.scrollTop + rowsEl.clientHeight > rowsEl.scrollHeight - 400) {
      if (mail.threads.length >= 100) void mail.loadMoreThreads();
    }
  }
</script>

<section class="list">
  <header class="head">
    {#if mail.selecting}
      <!-- The header becomes the action bar rather than a second strip
           appearing: same height, no rows covered, no chrome when idle. -->
      <SelectionBar />
    {:else if mail.searching}
      <!-- Fork (4.3): search mode — the query as the title, its operators as
           removable chips, ✕ / Esc back to the folder the search came from. -->
      <SearchChips
        query={mail.searchQuery ?? ""}
        count={mail.threads.length}
        onremove={(chip) => void mail.removeSearchToken(chip.token)}
        onclose={() => void mail.exitSearch()}
      />
    {:else if mail.courtView}
      <!-- Fork (10): a court view. The title names it; the sub-line says the
           order (oldest first) and how many, since the filter chips do not
           apply here (the view is its own filter). -->
      <div class="court-head">
        <h1>{t(mail.courtView === "on_me" ? "fork.nav.on_me" : "fork.nav.waiting")}</h1>
        <span class="court-sub">
          {t(mail.courtView === "on_me" ? "fork.court.sub_on_me" : "fork.court.sub_waiting", { n: mail.threads.length })}
        </span>
      </div>
      {#if mail.threads.length > 0}
        <span class="nav-hint" title="{t('shortcuts.next')} · {t('shortcuts.prev')}">
          <svg width="11" height="11" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.3">
            <path d="M3.5 5L6 2.5 8.5 5" />
            <path d="M3.5 7L6 9.5 8.5 7" />
          </svg>
          <kbd>J</kbd><kbd>K</kbd>
        </span>
      {/if}
    {:else}
      <h1>{title}</h1>
      {#if editableFolder}
        <button class="edit" onclick={openEditor} title={t("folder.edit")} aria-label={t("folder.edit")}>
          <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round">
            <path d="M11.2 2.6l2.2 2.2M9.9 3.9l2.2 2.2-6.4 6.4-2.9.7.7-2.9 6.4-6.4z" />
          </svg>
        </button>
      {/if}
      <div class="head-right">
        {#if recapAvailable}
          <button class="recap-chip" onclick={openRecap}>✦ {t("ai.recap")}</button>
        {/if}
        <!-- Fork (3.1): All / Unread / Starred chips. The unread count lives
             on its chip (click filters); the count is the folder's, not the
             page's. -->
        <div class="chips" role="group" aria-label={t("fork.list.filter")}>
          <button class="chip" class:active={mail.listFilter === "all"} aria-pressed={mail.listFilter === "all"} onclick={() => void mail.setListFilter("all")}>
            {t("fork.list.all")}
          </button>
          <button
            class="chip"
            class:active={mail.listFilter === "unread"}
            aria-pressed={mail.listFilter === "unread"}
            title="{t('fork.list.unread')}  Shift U"
            onclick={() => void mail.setListFilter(mail.listFilter === "unread" ? "all" : "unread")}
          >
            {t("fork.list.unread")}{#if unread > 0}<span class="n">{unread}</span>{/if}
          </button>
          <button
            class="chip"
            class:active={mail.listFilter === "starred"}
            aria-pressed={mail.listFilter === "starred"}
            title="{t('fork.list.starred')}  Shift S"
            onclick={() => void mail.setListFilter(mail.listFilter === "starred" ? "all" : "starred")}
          >
            {t("fork.list.starred")}
          </button>
        </div>
        {#if mail.threads.length > 0}
          <span class="nav-hint" title="{t('shortcuts.next')} · {t('shortcuts.prev')}">
            <svg width="11" height="11" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.3">
              <path d="M3.5 5L6 2.5 8.5 5" />
              <path d="M3.5 7L6 9.5 8.5 7" />
            </svg>
            <kbd>J</kbd><kbd>K</kbd>
          </span>
        {/if}
      </div>
    {/if}
  </header>
  <div
    class="rows"
    class:selecting={mail.selecting}
    bind:this={rowsEl}
    bind:clientHeight={viewH}
    onscroll={onScroll}
  >
    {#if mail.threads.length === 0 && !mail.threadsLoading}
      <div class="empty">
        {#if mail.loadFailed}
          <button class="retry" onclick={() => void mail.retryLoad()}>{t("list.load_failed")}</button>
        {:else}
          {mail.syncState === "syncing" ? t("sync.syncing") : t("list.empty")}
        {/if}
      </div>
    {:else}
      <div class="spacer" style="height: {start * rowH}px"></div>
      {#each visible as thread (thread.messageId ?? thread.id)}
        {@const court = mail.courtView ? courtOf(thread) : null}
        <!-- Fork (10): in a court view the row gets its age + reason as an
             overlay in a wrapper of its own, so MessageRow (and the fixed
             height the windowing measures) stays exactly upstream's. -->
        <div class="court-wrap" class:court={court !== null} class:compact={prefs.density === "compact"}>
          <MessageRow
            {thread}
            selected={mail.groupThreads
              ? mail.selectedThreadId === thread.id
              : mail.selectedMessageId === thread.messageId}
            checked={mail.isSelected(rowKey(thread))}
            onselect={() => {
              mail.selectedThreadId = thread.id;
              mail.selectedMessageId = thread.messageId ?? null;
            }}
            ontoggle={(extend) => mail.toggleRow(rowKey(thread), extend)}
          />
          {#if court}
            <div class="court-badge-slot">
              <CourtRow since={court.since} reason={court.reason} now={nowSecs} />
            </div>
          {/if}
        </div>
      {/each}
      <div class="spacer" style="height: {(mail.threads.length - end) * rowH}px"></div>
    {/if}
  </div>
</section>

<style>
  .list {
    width: var(--list-w);
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    border-right: 1px solid var(--hairline);
    background: var(--bg);
    min-width: 0;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 18px 16px 12px;
    /* Pinned so the strip's contents can change without moving the list under
       the pointer — the selection bar replaces the title, and the AI Recap chip
       comes and goes with the unread count. */
    min-height: 57px;
  }
  .head-right {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }
  /* Fork (3.1): filter chips, mono microlabel voice, one active at a time. */
  .chips {
    display: flex;
    gap: 2px;
    padding: 2px;
    border-radius: 999px;
    background: var(--hover);
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 8px;
    border-radius: 999px;
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 500;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-faint);
    white-space: nowrap;
  }
  .chip:hover {
    color: var(--text);
  }
  .chip.active {
    background: var(--surface-raised);
    color: var(--text);
    box-shadow: 0 0 0 1px var(--hairline-strong);
  }
  .chip .n {
    color: var(--unread);
    font-weight: 600;
  }
  /* Sits right after the folder name, quiet until the header is hovered —
     it is a rare action and shouldn't compete with the title. */
  .edit {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    margin-right: auto;
    border-radius: var(--radius-s);
    color: var(--text-faint);
    opacity: 0;
    transition: opacity 0.12s ease;
  }
  .head:hover .edit,
  .edit:focus-visible {
    opacity: 1;
  }
  .edit:hover {
    background: var(--hover);
    color: var(--text);
  }
  @media (prefers-reduced-motion: reduce) {
    .edit {
      transition: none;
    }
  }
  .nav-hint {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    color: var(--text-faint);
    flex-shrink: 0;
  }
  .nav-hint kbd {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-faint);
  }
  /* Violet — an AI moment */
  .recap-chip {
    padding: 4px 11px;
    border-radius: 999px;
    border: 1px solid var(--accent-dim);
    color: var(--accent);
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
  }
  .recap-chip:hover {
    background: var(--accent-soft);
  }
  /* Quiet-zine character moment: the chip sits slightly askew, like a stamp. */
  :global(:root[data-theme="warm-light"]) .recap-chip,
  :global(:root[data-theme="warm-dark"]) .recap-chip {
    border-radius: 3px;
    background: var(--surface);
    transform: rotate(-1.5deg);
  }
  @media (prefers-reduced-motion: reduce) {
    :global(:root[data-theme="warm-light"]) .recap-chip,
    :global(:root[data-theme="warm-dark"]) .recap-chip {
      transform: none;
    }
  }
  h1 {
    font-size: 17px;
    font-weight: 800;
    letter-spacing: -0.02em;
  }
  /* Fork (10): court header (title + order/count line) and the row overlay. */
  .court-head {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    margin-right: auto;
  }
  .court-sub {
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-faint);
    white-space: nowrap;
  }
  .court-wrap {
    position: relative;
  }
  /* Bottom-right of the row, over the tail of the snippet line; the hover
     actions live on line 1, so the two never overlap. */
  .court-badge-slot {
    position: absolute;
    right: 14px;
    bottom: 9px;
    width: 168px;
    display: flex;
    justify-content: flex-end;
    pointer-events: none;
    z-index: 1;
  }
  /* Compact rows are one line: sit left of the date instead. */
  .court-wrap.compact .court-badge-slot {
    bottom: auto;
    top: 50%;
    transform: translateY(-50%);
    right: 92px;
    width: 40px;
  }
  /* Reserve the badge's room so it never sits on top of text. */
  .court-wrap.court :global(.snippet) {
    padding-right: 176px;
  }
  .court-wrap.court.compact :global(.snippet) {
    padding-right: 48px;
  }
  /* One line has room for the age only; the reason stays in the tooltip. */
  .court-wrap.compact :global(.court-badge .sep),
  .court-wrap.compact :global(.court-badge .why) {
    display: none;
  }
  .rows {
    overflow-y: auto;
    flex: 1;
  }
  .empty {
    padding: 48px 16px;
    text-align: center;
    color: var(--text-faint);
    font-size: 13px;
  }
  /* Reads as the same line of text the empty state shows, only clickable —
     the failure is the message, the retry is not a separate control. */
  .retry {
    font: inherit;
    color: inherit;
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    text-decoration: underline;
    text-underline-offset: 3px;
  }
  .retry:hover {
    color: var(--text-dim);
  }
</style>
