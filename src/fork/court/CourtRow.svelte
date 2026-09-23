<script lang="ts">
  // The age + reason badge on a court row (PLAN.md Phase 10): "3d · draft
  // started", amber past the amber setting, red past the red one. Rendered by
  // MessageList as an overlay on the upstream MessageRow, so the row itself
  // (and its fixed height, which the windowing arithmetic relies on) is
  // untouched.
  import { t } from "../../lib/i18n/index.svelte";
  import { ageDays, ageLabel, ageTone } from "./nudge";
  import { courtPrefs } from "./prefs.svelte";

  let { since, reason, now }: { since: number; reason: string | null; now: number } = $props();

  const days = $derived(ageDays(since, now));
  const age = $derived(ageLabel(since, now));
  const tone = $derived(ageTone(days, courtPrefs.amberDays, courtPrefs.redDays));

  // The rule reasons are stored as English literals (pending/10.md); the AI's
  // free text is shown as-is.
  const REASON_KEYS: Record<string, string> = {
    "draft started": "fork.court.reason_draft",
    filed: "fork.court.reason_filed",
    "bulk: list-unsubscribe": "fork.court.reason_bulk",
    "bulk: automated sender": "fork.court.reason_bulk",
  };
  const why = $derived.by(() => {
    if (!reason) return null;
    const key = REASON_KEYS[reason.trim().toLowerCase()];
    return key ? t(key) : reason;
  });

  const label = $derived(
    why
      ? t("fork.court.row_label_reason", { age: t("fork.court.age", { n: days }), reason: why })
      : t("fork.court.row_label", { age: t("fork.court.age", { n: days }) }),
  );
</script>

<span class="court-badge" class:amber={tone === "amber"} class:red={tone === "red"} title={label} aria-label={label}>
  <span class="age">{age}</span>
  {#if why}<span class="sep" aria-hidden="true">·</span><span class="why">{why}</span>{/if}
</span>

<style>
  .court-badge {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    max-width: 100%;
    padding: 1px 7px;
    border-radius: 999px;
    background: var(--bg);
    box-shadow: 0 0 0 1px var(--hairline);
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 0.04em;
    color: var(--text-faint);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .age {
    font-weight: 600;
  }
  .why {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  /* Colour is added on top of the age text, never instead of it (WCAG 1.4.1). */
  /* The amber account swatch is the one amber every theme already carries. */
  .court-badge.amber .age {
    color: var(--acct-2);
  }
  .court-badge.red .age {
    color: var(--danger);
  }
</style>
