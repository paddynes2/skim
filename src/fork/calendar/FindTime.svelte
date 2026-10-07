<script lang="ts">
  import { t, getLocale } from "../../lib/i18n/index.svelte";
  import { calendarApi } from "./api";
  import { calPrefs } from "./settings.svelte";
  import { calendarErrorText, shiftDate } from "./guests";
  import { zonedTimestamp } from "./editor";
  import { availableCandidates } from "./planning";
  import type { AvailabilityResult } from "./types";
  let { accountId, attendees, date, zone, duration, eventId, onchoose }: { accountId: string | null; attendees: string[]; date: string; zone: string; duration: number; eventId: number | null; onchoose: (start: number, end: number) => void } = $props();
  let loading = $state(false);
  let error = $state("");
  let result = $state<AvailabilityResult | null>(null);
  let suggestions = $state<{ start: number; end: number }[]>([]);
  let searchedKey = $state("");
  const key = $derived(JSON.stringify([accountId, attendees, date, zone, duration, eventId]));
  const current = $derived(searchedKey === key);
  const unknown = $derived(result?.calendars.filter((c) => c.status === "unknown") ?? []);
  async function search() {
    if (!accountId || !Number.isFinite(duration) || duration <= 0) return;
    loading = true; error = ""; result = null; suggestions = [];
    const requestKey = key;
    try {
      const from = zonedTimestamp(date, "00:00", zone), to = zonedTimestamp(shiftDate(date, 7), "00:00", zone);
      if (from === null || to === null) throw new Error(t("fork.cal.time_invalid"));
      const response = await calendarApi.availability(accountId, from, to, attendees, eventId);
      if (key !== requestKey) return;
      result = response;
      const candidates = [];
      const startMinutes = Number(calPrefs.workStart.slice(0, 2)) * 60 + Number(calPrefs.workStart.slice(3));
      const endMinutes = Number(calPrefs.workEnd.slice(0, 2)) * 60 + Number(calPrefs.workEnd.slice(3));
      for (let day = 0; day < 7; day++) {
        const dayDate = shiftDate(date, day);
        const dow = new Date(`${dayDate}T12:00:00Z`).getUTCDay() || 7;
        if (!calPrefs.workDaySet.has(dow)) continue;
        for (let minute = startMinutes; minute + duration / 60 <= endMinutes; minute += 30) {
          const time = `${String(Math.floor(minute / 60)).padStart(2, "0")}:${String(minute % 60).padStart(2, "0")}`;
          const start = zonedTimestamp(dayDate, time, zone);
          if (start !== null && start >= Date.now() / 1000) candidates.push({ start, end: start + duration });
        }
      }
      suggestions = availableCandidates(response, candidates).slice(0, 8);
      searchedKey = requestKey;
    } catch (e) { error = calendarErrorText(e); }
    finally { loading = false; }
  }
  function label(start: number) { return new Date(start * 1000).toLocaleString(getLocale(), { timeZone: zone, weekday: "short", month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" }); }
</script>
<div class="finder">
  <button type="button" class="find" disabled={loading || !accountId || duration <= 0 || !Number.isFinite(duration)} onclick={search}>{t(loading ? "fork.cal.checking_availability" : "fork.cal.find_time")}</button>
  {#if current && result}
    <p class="scope">{t("fork.cal.find_time_scope")} {zone}</p>
    {#if unknown.length}<div class="unknown" role="status"><strong>{t("fork.cal.availability_unknown")}</strong>{#each unknown as guest}<span>{guest.email}{guest.reason ? `: ${guest.reason}` : ""}</span>{/each}</div>{/if}
    {#if suggestions.length}<p>{t(unknown.length ? "fork.cal.available_known" : "fork.cal.available_all")}</p><div class="slots">{#each suggestions as slot}<button type="button" onclick={() => onchoose(slot.start, slot.end)}>{label(slot.start)}</button>{/each}</div>{:else}<p>{t("fork.cal.no_common_slots")}</p>{/if}
  {:else if result}<p>{t("fork.cal.availability_changed")}</p>{/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
</div>
<style>
  .finder { padding: 12px; border: 1px solid var(--hairline); border-radius: var(--radius-m); font-size: 12px; }
  .find { color: var(--primary); font-weight: 600; padding: 2px 0; }.find:disabled { opacity: .5; }
  p { margin: 8px 0; line-height: 1.45; color: var(--text-dim); }.scope { font-size: 11px; }
  .unknown { display: grid; gap: 4px; margin: 12px 0; line-height: 1.4; overflow-wrap: anywhere; }.unknown strong { font-weight: 600; }
  .slots { display: flex; flex-wrap: wrap; gap: 6px; }.slots button { padding: 6px 9px; border: 1px solid var(--hairline-strong); border-radius: var(--radius-s); }.slots button:hover { background: var(--hover); }.error { color: var(--danger); }
</style>
