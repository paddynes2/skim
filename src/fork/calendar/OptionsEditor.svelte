<script lang="ts">
  import { t } from "../../lib/i18n/index.svelte";
  import type { EventOptions } from "./types";
  import { EVENT_COLORS } from "./editor";
  let { value = $bindable<EventOptions>(), disabled = false }: { value: EventOptions; disabled?: boolean } = $props();
  const reminders = $derived(value.reminders ?? { useDefault: true, overrides: [] });
  function useDefaults(useDefault: boolean) { value.reminders = { useDefault, overrides: useDefault ? [] : [{ method: "popup", minutes: 10 }] }; }
</script>
<section>
  <h3>{t("fork.cal.notifications")}</h3>
  <label class="check"><input type="checkbox" checked={reminders.useDefault} onchange={(e) => useDefaults(e.currentTarget.checked)} {disabled} />{t("fork.cal.default_reminders")}</label>
  {#if !reminders.useDefault}
    {#each reminders.overrides ?? [] as reminder, i}
      <div class="reminder">
        <select aria-label={t("fork.cal.reminder_method")} bind:value={reminder.method} {disabled}><option value="popup">{t("fork.cal.notification")}</option><option value="email">{t("fork.cal.reminder_email")}</option></select>
        <input type="number" min="0" max="40320" aria-label={t("fork.cal.minutes_before")} bind:value={reminder.minutes} {disabled} />
        <span>{t("fork.cal.minutes_before")}</span>
        <button type="button" aria-label={t("fork.cal.remove_reminder")} {disabled} onclick={() => { value.reminders = { useDefault: false, overrides: reminders.overrides?.filter((_, n) => n !== i) }; }}>&times;</button>
      </div>
    {/each}
    {#if (reminders.overrides?.length ?? 0) < 5}<button class="text-button" type="button" {disabled} onclick={() => { value.reminders = { useDefault: false, overrides: [...(reminders.overrides ?? []), { method: "popup", minutes: 10 }] }; }}>+ {t("fork.cal.add_reminder")}</button>{/if}
  {/if}
</section>
<section>
  <h3>{t("fork.cal.event_settings")}</h3>
  <div class="pair">
    <label class="field"><span>{t("fork.cal.availability")}</span><select bind:value={value.transparency} {disabled}><option value="opaque">{t("fork.cal.busy")}</option><option value="transparent">{t("fork.cal.free")}</option></select></label>
    <label class="field"><span>{t("fork.cal.visibility")}</span><select bind:value={value.visibility} {disabled}>
      {#each ["default", "public", "private", "confidential"] as v}<option value={v}>{t(`fork.cal.visibility_${v}`)}</option>{/each}
    </select></label>
  </div>
  <div class="field"><span>{t("fork.cal.colour")}</span>
    <div class="colours" role="group" aria-label={t("fork.cal.colour")}>
      {#each EVENT_COLORS as colour, i}
        <button type="button" style:--swatch={colour || "var(--text-dim)"} aria-label={t(`fork.cal.colour_${i}`)} title={t(`fork.cal.colour_${i}`)} aria-pressed={value.colorId === (i ? String(i) : "")} {disabled} onclick={() => { value.colorId = i ? String(i) : ""; }}><span></span></button>
      {/each}
    </div>
  </div>
</section>
<section>
  <h3>{t("fork.cal.guest_permissions")}</h3>
  <label class="check"><input type="checkbox" bind:checked={value.guestsCanModify} {disabled} />{t("fork.cal.guests_modify")}</label>
  <label class="check"><input type="checkbox" bind:checked={value.guestsCanInviteOthers} {disabled} />{t("fork.cal.guests_invite")}</label>
  <label class="check"><input type="checkbox" bind:checked={value.guestsCanSeeOtherGuests} {disabled} />{t("fork.cal.guests_see")}</label>
</section>
<style>
section { display: grid; gap: 12px; padding-top: 20px; border-top: 1px solid var(--hairline); }
h3 { font-size: 13px; font-weight: 600; color: var(--text); margin: 0; }
.field { display: grid; gap: 7px; min-width: 0; font-size: 12px; color: var(--text-dim); }
.check { display: flex; align-items: center; gap: 8px; font-size: 12px; color: var(--text-dim); }
input[type="checkbox"] { accent-color: var(--primary); }
input[type="number"], select { width: 100%; min-width: 0; padding: 8px; border: 1px solid var(--hairline-strong); border-radius: var(--radius-s); background: var(--surface-raised); color: var(--text); font: inherit; font-size: 12px; }
.pair { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
.reminder { display: grid; grid-template-columns: minmax(90px, 1fr) 64px auto 24px; align-items: center; gap: 7px; font-size: 11px; color: var(--text-dim); }
.text-button { width: fit-content; font-size: 12px; color: var(--text); padding: 4px 0; }
.colours { display: flex; gap: 3px; flex-wrap: wrap; }
.colours button { width: 29px; height: 29px; display: grid; place-items: center; border: 1px solid transparent; border-radius: 50%; }
.colours button span { width: 18px; height: 18px; background: var(--swatch); border-radius: 50%; }
.colours button[aria-pressed="true"] { border-color: var(--text); }
</style>
