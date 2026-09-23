<script lang="ts">
  // Fork (PLAN.md 2.2/2.4/2.6): fixed gutter with the unread dot and the star,
  // read rows dimmed, hover actions, compact density, avatars. The checkbox
  // slot logic is upstream's, kept intact.
  import { getLocale, t } from "../lib/i18n/index.svelte";
  import { mail } from "../lib/stores/mail.svelte";
  import type { ThreadRow } from "../lib/types";
  import { act, archiveOffered } from "../fork/actions";
  import { avatarColor, initials } from "../fork/avatar";
  import { prefs } from "../fork/stores/prefs.svelte";

  let {
    thread,
    selected = false,
    checked = false,
    onselect,
    ontoggle,
  }: {
    thread: ThreadRow;
    selected?: boolean;
    checked?: boolean;
    onselect?: (id: number) => void;
    ontoggle?: (extend: boolean) => void;
  } = $props();

  // Which mailbox this row came from — shown only in the unified view, where
  // rows from every account interleave.
  const badge = $derived(mail.unified ? mail.accountBadge(thread.accountId) : null);
  const compact = $derived(prefs.density === "compact");
  const avatar = $derived(
    prefs.avatars ? { text: initials(thread.fromName, thread.fromAddr), color: avatarColor(thread.fromAddr) } : null,
  );
  const canArchive = $derived(archiveOffered(mail.selectedFolder?.role));

  function formatDate(unix: number): string {
    const locale = getLocale();
    const d = new Date(unix * 1000);
    const now = new Date();
    const sameDay = d.toDateString() === now.toDateString();
    if (sameDay)
      return d.toLocaleTimeString(locale, { hour: "numeric", minute: "2-digit" });
    const days = (now.getTime() - d.getTime()) / 86400000;
    if (days < 7)
      return d.toLocaleDateString(locale, { weekday: "short" });
    return d.toLocaleDateString(locale, { month: "short", day: "numeric" });
  }

  // State is spelled out for assistive tech, so it is never colour-only
  // (WCAG 1.4.1).
  const label = $derived.by(() => {
    const parts: string[] = [];
    if (!thread.isRead) parts.push(t("fork.row.unread"));
    if (thread.isStarred) parts.push(t("fork.row.starred"));
    parts.push(t("fork.row.from", { name: thread.fromName }));
    parts.push(thread.subject || t("fork.row.no_subject"));
    if (thread.messageCount > 1) parts.push(t("fork.row.messages", { n: thread.messageCount }));
    if (thread.hasAttachments) parts.push(t("fork.row.attachment"));
    parts.push(formatDate(thread.date));
    return parts.join(", ");
  });

  function stop(e: MouseEvent, run: () => void) {
    e.stopPropagation();
    e.preventDefault();
    run();
  }
</script>

<!-- The checkbox and the gutter/hover buttons are siblings of the row button,
     not children: a button cannot nest inside a button. -->
<div
  class="row-wrap"
  class:unread={!thread.isRead}
  class:starred={thread.isStarred}
  class:selected
  class:checked
  class:compact
  class:with-avatar={avatar !== null}
>
  <button
    class="check"
    aria-label={t("select.toggle")}
    aria-pressed={checked}
    onclick={(e) => ontoggle?.(e.shiftKey)}
  >
    <svg width="9" height="9" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <path d="M1.5 5.2l2.4 2.4L8.5 2.6" />
    </svg>
  </button>

  <!-- Gutter: unread dot (click = toggle read) and star (click = toggle star). -->
  <div class="gutter">
    <button
      class="dot"
      aria-label={thread.isRead ? t("reading.mark_unread") : t("reading.mark_read")}
      aria-pressed={!thread.isRead}
      title={thread.isRead ? t("reading.mark_unread") : t("reading.mark_read")}
      onclick={(e) => stop(e, () => void act(thread, "toggle_read"))}
    ></button>
    <button
      class="star"
      aria-label={thread.isStarred ? t("reading.unstar") : t("reading.star")}
      aria-pressed={thread.isStarred}
      title="{thread.isStarred ? t('reading.unstar') : t('reading.star')}  S"
      onclick={(e) => stop(e, () => void act(thread, "toggle_star"))}
    >
      <svg width="12" height="12" viewBox="0 0 16 16" fill={thread.isStarred ? "currentColor" : "none"} stroke="currentColor" stroke-width="1.4" stroke-linejoin="round">
        <path d="M8 1.5l2 4.1 4.5.6-3.3 3.2.8 4.5L8 11.8l-4 2.1.8-4.5L1.5 6.2 6 5.6 8 1.5z" />
      </svg>
    </button>
  </div>

  {#if avatar}
    <span class="avatar" style:background="var(--acct-{avatar.color})" aria-hidden="true">{avatar.text}</span>
  {/if}

  <button class="row" aria-label={label} onclick={() => onselect?.(thread.id)}>
    <div class="line1">
      <span class="from">
        {#if badge}
          <span class="acct" style:background="var(--acct-{badge.color})">{badge.letter}</span>
        {/if}
        <span class="from-name">{thread.fromName}</span>
        {#if thread.messageCount > 1}<span class="mcount">{thread.messageCount}</span>{/if}
      </span>
      {#if compact}
        <span class="subject">{thread.subject}</span>
        <span class="snippet">{thread.snippet}</span>
      {/if}
      <span class="meta">
        {#if thread.hasAttachments}
          <svg class="clip" width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" aria-hidden="true">
            <path d="M10.5 5.5L6 10a1.5 1.5 0 0 0 2.1 2.1l5-5a3 3 0 0 0-4.2-4.2l-5.5 5.5" />
          </svg>
        {/if}
        <span class="date">{formatDate(thread.date)}</span>
      </span>
    </div>
    {#if !compact}
      <div class="subject">{thread.subject}</div>
      <div class="snippet">{thread.snippet}</div>
    {/if}
  </button>

  <!-- Hover actions (2.4): over the right end of line 1, replacing the date.
       Same functions as the keys; tooltips teach the key. -->
  <div class="actions" role="group" aria-label={t("fork.row.actions")}>
    {#if canArchive}
      <button
        class="action"
        title="{t('reading.archive')}  E"
        aria-label={t("reading.archive")}
        onclick={(e) => stop(e, () => void act(thread, "archive"))}
      >
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2"><path d="M2 3h12v3H2V3zm1 3v7h10V6M6.5 9h3" /></svg>
      </button>
    {/if}
    <button
      class="action"
      class:on={thread.isStarred}
      title="{thread.isStarred ? t('reading.unstar') : t('reading.star')}  S"
      aria-label={thread.isStarred ? t("reading.unstar") : t("reading.star")}
      onclick={(e) => stop(e, () => void act(thread, "toggle_star"))}
    >
      <svg width="14" height="14" viewBox="0 0 16 16" fill={thread.isStarred ? "currentColor" : "none"} stroke="currentColor" stroke-width="1.2"><path d="M8 1.5l2 4.1 4.5.6-3.3 3.2.8 4.5L8 11.8l-4 2.1.8-4.5L1.5 6.2 6 5.6 8 1.5z" /></svg>
    </button>
    <button
      class="action"
      title="{thread.isRead ? t('reading.mark_unread') : t('reading.mark_read')}  U"
      aria-label={thread.isRead ? t("reading.mark_unread") : t("reading.mark_read")}
      onclick={(e) => stop(e, () => void act(thread, "toggle_read"))}
    >
      {#if thread.isRead}
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2"><rect x="2" y="3.5" width="12" height="9" rx="1" /><path d="M2 5l6 4.5L14 5" /></svg>
      {:else}
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2"><path d="M2 6.5l6-4 6 4v6a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1v-6z" /><path d="M2 6.5l6 4.5 6-4.5" /></svg>
      {/if}
    </button>
  </div>
</div>

<style>
  /* Positioning context for the gutter, the checkbox and the hover actions:
     the row button below still draws the whole row. */
  .row-wrap {
    position: relative;
    --gutter: 20px;
    --avatar-w: 0px;
  }
  .row-wrap.with-avatar {
    --avatar-w: 36px;
  }

  .row {
    display: block;
    width: 100%;
    text-align: left;
    padding: 12px 16px 12px calc(var(--gutter) + var(--avatar-w) + 4px);
    border-bottom: 1px solid var(--hairline);
    transition: background 0.08s;
  }
  /* Hover lives on the wrapper: the buttons around the row sit outside it, and
     pointing at them must not drop the row's tint. */
  .row-wrap:hover .row {
    background: var(--hover);
  }
  /* The keyboard cursor: distinct from hover (2.3) — the selected tint plus a
     2px bar in the text colour. */
  .row-wrap.selected .row {
    background: var(--selected);
    box-shadow: inset 2px 0 0 var(--text);
  }
  .row-wrap.checked .row {
    background: var(--hover);
  }

  /* Gutter (2.2): a fixed 20px column. The dot only shows unread; the star
     shows when set, and hollow on hover so it can be set. */
  .gutter {
    position: absolute;
    top: 14px;
    left: 0;
    width: var(--gutter);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 5px;
  }
  .dot {
    width: 8px;
    height: 8px;
    margin: 4px 0;
    border-radius: 50%;
    background: var(--unread);
    visibility: hidden;
  }
  .unread .dot {
    visibility: visible;
  }
  .row-wrap:hover .dot,
  .dot:focus-visible {
    visibility: visible;
  }
  .row-wrap:hover .dot:not(.unread .dot) {
    background: transparent;
    box-shadow: inset 0 0 0 1.5px var(--text-faint);
  }
  .star {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    color: var(--text-faint);
    visibility: hidden;
  }
  .starred .star {
    color: var(--star);
    visibility: visible;
  }
  .row-wrap:hover .star,
  .star:focus-visible {
    visibility: visible;
  }
  .star:hover {
    color: var(--star);
  }

  /* Avatar (2.6): initials on a disc, hashed colour from the account palette. */
  .avatar {
    position: absolute;
    top: 12px;
    left: var(--gutter);
    width: 28px;
    height: 28px;
    border-radius: 50%;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.02em;
    color: var(--surface);
  }

  /* Parked at the end of the date line. The slot is always reserved — the date
     is simply set in from the edge — so revealing the box shifts nothing. */
  .check {
    position: absolute;
    top: 13px;
    right: 16px;
    width: 13px;
    height: 13px;
    border: 1px solid var(--text-faint);
    border-radius: 3px;
    color: transparent;
    display: flex;
    align-items: center;
    justify-content: center;
    opacity: 0;
    z-index: 2;
    transition:
      opacity 0.08s,
      border-color 0.08s;
  }
  /* Revealed only when it can be used: pointing at the row, tabbing to it, or
     once a selection exists (then every row shows one). */
  .row-wrap:hover .check,
  .check:focus-visible,
  :global(.selecting) .check {
    opacity: 1;
  }
  .row-wrap.checked .check {
    opacity: 1;
    border-color: var(--text);
    background: var(--text);
    color: var(--surface);
  }

  /* Hover actions (2.4): appear over the date on hover / at the cursor row. */
  .actions {
    position: absolute;
    top: 8px;
    right: 34px;
    display: flex;
    gap: 2px;
    padding: 1px;
    border-radius: var(--radius-s);
    background: var(--surface-raised);
    box-shadow: 0 0 0 1px var(--hairline-strong);
    opacity: 0;
    pointer-events: none;
    z-index: 2;
    transition: opacity 0.08s;
  }
  .row-wrap:hover .actions,
  .row-wrap.selected .actions,
  .actions:focus-within {
    opacity: 1;
    pointer-events: auto;
  }
  .row-wrap:hover .date,
  .row-wrap.selected .date,
  .row-wrap:hover .clip,
  .row-wrap.selected .clip {
    visibility: hidden;
  }
  .action {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 22px;
    border-radius: 4px;
    color: var(--text-dim);
  }
  .action:hover {
    background: var(--hover);
    color: var(--text);
  }
  .action.on {
    color: var(--star);
  }

  @media (prefers-reduced-motion: reduce) {
    .row,
    .check,
    .actions {
      transition: none;
    }
  }

  .line1 {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 8px;
    /* Keeps the date clear of the checkbox slot. Only this line is set in —
       the subject and snippet still run the full width of the row. */
    padding-right: 21px;
  }
  .from {
    font-size: 13.5px;
    color: var(--text-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  .from-name {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .unread .from {
    color: var(--text);
    font-weight: 700;
  }
  .mcount {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-faint);
    flex-shrink: 0;
  }
  /* The mailbox mark: the address's first letter in a colored disc. */
  .acct {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    font-family: var(--font-mono);
    font-size: 9px;
    font-weight: 600;
    line-height: 1;
    color: var(--surface);
    flex-shrink: 0;
  }
  .meta {
    display: flex;
    align-items: center;
    gap: 5px;
    flex-shrink: 0;
    color: var(--text-faint);
  }
  .date {
    font-family: var(--font-mono);
    font-size: 10.5px;
    color: var(--text-faint);
    flex-shrink: 0;
  }

  .subject {
    font-size: 13.5px;
    margin-top: 2px;
    color: var(--text-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .unread .subject {
    color: var(--text);
    font-weight: 600;
  }

  .snippet {
    font-size: 12.5px;
    color: var(--text-faint);
    margin-top: 2px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Compact (2.6): one line � sender, subject, snippet, date. ~36px rows.
     At the list's 372px the snippet is the first thing to give way. */
  .compact .row {
    padding-top: 9px;
    padding-bottom: 9px;
  }
  .compact .line1 {
    align-items: center;
    gap: 8px;
  }
  .compact .from {
    flex: 0 0 108px;
    font-size: 13px;
  }
  .compact .subject {
    margin: 0;
    flex: 1 1 auto;
    min-width: 0;
    font-size: 13px;
  }
  .compact .snippet {
    margin: 0;
    flex: 0 1 30%;
    min-width: 0;
    font-size: 12px;
  }
  .compact .gutter {
    top: 10px;
    flex-direction: row;
    gap: 2px;
    width: calc(var(--gutter) + 14px);
  }
  .compact .dot {
    margin: 0;
  }
  .compact .row {
    padding-left: calc(var(--gutter) + 14px + var(--avatar-w) + 2px);
  }
  .compact.with-avatar {
    --avatar-w: 30px;
  }
  .compact.with-avatar .avatar {
    top: 6px;
    width: 24px;
    height: 24px;
    font-size: 9px;
    left: calc(var(--gutter) + 16px);
  }
  .compact .check {
    top: 11px;
  }
  .compact .actions {
    top: 5px;
  }
</style>
