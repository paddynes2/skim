<script lang="ts">
  // Fork (v1.1.1): the due time on a Snoozed / Follow-ups row ("Tomorrow 08:00",
  // "No reply · since Tue"). An overlay on the upstream MessageRow, like the
  // court badge, so the row and its fixed height stay untouched.
  import { t } from "../../lib/i18n/index.svelte";
  import type { ReminderKind } from "./api";
  import { fmtDue } from "./when";

  let { kind, dueTs, now }: { kind: ReminderKind; dueTs: number; now: number } = $props();

  const overdue = $derived(dueTs <= now);
  const label = $derived.by(() => {
    const when = fmtDue(dueTs, new Date(now * 1000));
    if (kind === "snooze") return overdue ? t("fork.rem.badge_back") : when;
    return overdue ? t("fork.rem.badge_no_reply", { when }) : t("fork.rem.badge_by", { when });
  });
</script>

<span class="rem-badge" class:overdue title={label} aria-label={label}>{label}</span>

<style>
  .rem-badge {
    display: inline-flex;
    max-width: 100%;
    padding: 1px 7px;
    border-radius: 999px;
    background: var(--bg);
    box-shadow: 0 0 0 1px var(--hairline);
    font-size: 11px;
    font-weight: 600;
    color: var(--text-dim);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    font-variant-numeric: tabular-nums;
  }
  /* Colour is added to the words, never instead of them (WCAG 1.4.1). */
  .rem-badge.overdue {
    color: var(--danger);
  }
</style>
