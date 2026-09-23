<script lang="ts">
  // Invite card extras (PLAN.md 7.5): "In your calendar" when the invite's UID
  // matches a synced event, and a conflict line for anything else overlapping
  // it. Mounted by the upstream InviteCard.svelte with one line. Read-only.
  //
  // Google's event `id` and the iCalendar UID differ: an event created in
  // Google has UID `<id>@google.com`; one imported from an invite keeps the
  // sender's UID as iCalUID and gets its own id. So the match tries the UID
  // shapes first and falls back to the same start + summary.
  import type { InviteView } from "../../lib/types";
  import { t } from "../../lib/i18n/index.svelte";
  import { calendarApi } from "./api";
  import { fmtTime, fromDateTime } from "./guests";
  import { calendar } from "./store.svelte";
  import type { EventRow } from "./types";

  let { invite }: { invite: InviteView } = $props();

  let matched = $state<EventRow | null>(null);
  let conflicts = $state<EventRow[]>([]);

  function window(inv: InviteView): { from: number; to: number } | null {
    if (inv.isAllDay && inv.startDate) {
      const from = Math.floor(fromDateTime(inv.startDate, "00:00").getTime() / 1000);
      const endDate = inv.endDate ?? inv.startDate;
      const to = Math.floor(fromDateTime(endDate, "00:00").getTime() / 1000) + 86400;
      return { from, to };
    }
    if (inv.startsAt != null) return { from: inv.startsAt, to: inv.endsAt ?? inv.startsAt + 1 };
    return null;
  }

  function uidMatches(uid: string, row: EventRow): boolean {
    const g = row.google_id;
    if (!g || g.startsWith("local-")) return false;
    return uid === g || uid === `${g}@google.com` || uid.split("@")[0] === g;
  }

  async function lookup(inv: InviteView) {
    matched = null;
    conflicts = [];
    if (!calendar.connected) return;
    const w = window(inv);
    if (!w) return;
    let rows: EventRow[];
    try {
      rows = await calendarApi.events(w.from - 60, w.to + 60);
    } catch {
      return;
    }
    const live = rows.filter((r) => r.status !== "cancelled");
    const hit =
      live.find((r) => uidMatches(inv.uid, r)) ??
      live.find(
        (r) =>
          !!inv.summary &&
          r.summary.trim().toLowerCase() === inv.summary.trim().toLowerCase() &&
          (inv.isAllDay ? r.all_day && r.start_date === inv.startDate : r.start_ts === inv.startsAt),
      ) ??
      null;
    matched = hit;
    conflicts = live.filter(
      (r) => r.id !== hit?.id && r.self_response !== "declined" && r.transparency !== "transparent" && r.start_ts < w.to && r.end_ts > w.from,
    );
  }

  $effect(() => {
    void lookup(invite);
  });

  function label(r: EventRow): string {
    const title = r.summary || t("fork.cal.untitled");
    return r.all_day ? title : `${title} (${fmtTime(r.start_ts)}–${fmtTime(r.end_ts)})`;
  }
</script>

{#if matched || conflicts.length > 0}
  <div class="extras">
    {#if matched}
      <div class="line in-cal">
        <svg width="11" height="11" viewBox="0 0 10 10" aria-hidden="true"><path d="M1.5 5.5L4 8L8.5 2.5" fill="none" stroke="currentColor" stroke-width="1.4" /></svg>
        <span>{t("fork.cal.in_your_calendar")}</span>
        {#if matched.self_response && matched.self_response !== "needsAction"}
          <span class="muted">· {t(`fork.cal.rsvp_${matched.self_response}`)}</span>
        {/if}
      </div>
    {/if}
    {#if conflicts.length > 0}
      <div class="line conflict">
        <span class="warn" aria-hidden="true">⚠</span>
        <span>{t("fork.cal.conflicts_with")} {conflicts.map(label).join(", ")}</span>
      </div>
    {/if}
  </div>
{/if}

<style>
  .extras {
    display: flex;
    flex-direction: column;
    gap: 3px;
    margin-top: 6px;
    font-size: 12.5px;
  }
  .line {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--text-dim);
  }
  .in-cal {
    color: var(--success);
  }
  .muted {
    color: var(--text-faint);
  }
  .conflict {
    color: var(--danger);
  }
  .warn {
    font-size: 11px;
  }
</style>
