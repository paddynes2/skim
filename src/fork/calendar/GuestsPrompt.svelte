<script lang="ts">
  // "Send invites/updates to guests?" (PLAN.md 7.4). Three answers, none of
  // them the default: Send (all), Don't send (none), Cancel (null, the change
  // is dropped). Enter does nothing here on purpose; Esc cancels.
  import { t } from "../../lib/i18n/index.svelte";
  import { guestsPrompt } from "./prompt.svelte";

  const p = $derived(guestsPrompt.pending);

  const title = $derived.by(() => {
    switch (p?.kind) {
      case "create":
        return t("fork.cal.guests_create");
      case "delete":
        return t("fork.cal.guests_delete");
      case "rsvp":
        return t("fork.cal.guests_rsvp");
      default:
        return t("fork.cal.guests_update");
    }
  });
  const sendLabel = $derived(p?.kind === "rsvp" ? t("fork.cal.guests_notify") : t("fork.cal.guests_send"));
  const skipLabel = $derived(p?.kind === "rsvp" ? t("fork.cal.guests_no_notify") : t("fork.cal.guests_no_send"));

  function onKeydown(e: KeyboardEvent) {
    if (!p) return;
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopImmediatePropagation();
      guestsPrompt.answer(null);
    }
  }
</script>

<svelte:window onkeydowncapture={onKeydown} />

{#if p}
  <div class="scrim" role="presentation" onmousedown={() => guestsPrompt.answer(null)}>
    <div
      class="dialog"
      role="dialog"
      aria-modal="true"
      aria-labelledby="guests-title"
      tabindex="-1"
      onmousedown={(e) => e.stopPropagation()}
    >
      <div class="microlabel">{t("fork.cal.guests_label")}</div>
      <h2 id="guests-title">{title}</h2>
      <p class="sub">{t("fork.cal.guests_n", { n: p.n })}</p>
      <div class="actions">
        <button class="btn primary" onclick={() => guestsPrompt.answer("all")}>{sendLabel}</button>
        <button class="btn" onclick={() => guestsPrompt.answer("none")}>{skipLabel}</button>
        <button class="btn ghost" onclick={() => guestsPrompt.answer(null)}>{t("fork.cal.cancel")}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 400;
    display: grid;
    place-items: center;
    background: rgba(0, 0, 0, 0.25);
  }
  .dialog {
    width: 380px;
    max-width: calc(100vw - 32px);
    padding: 18px 20px 16px;
    background: var(--surface-raised);
    border: 1px solid var(--hairline-strong);
    border-radius: var(--radius-l);
    box-shadow: var(--shadow-pop);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  h2 {
    font-size: 15px;
    font-weight: 700;
    letter-spacing: -0.01em;
  }
  .sub {
    font-size: 13px;
    color: var(--text-dim);
  }
  .actions {
    display: flex;
    gap: 6px;
    margin-top: 8px;
    flex-wrap: wrap;
  }
  .btn {
    padding: 7px 13px;
    border-radius: 999px;
    font-size: 13px;
    font-weight: 600;
    border: 1px solid var(--hairline-strong);
    color: var(--text);
  }
  .btn:hover {
    background: var(--hover);
  }
  .btn.primary {
    background: var(--text);
    color: var(--bg);
    border-color: var(--text);
  }
  .btn.primary:hover {
    opacity: 0.88;
    background: var(--text);
  }
  .btn.ghost {
    border-color: transparent;
    color: var(--text-dim);
    margin-left: auto;
  }
</style>
