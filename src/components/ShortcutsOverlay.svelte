<script lang="ts">
  import { backdropClose } from "../lib/backdrop";
  import { t } from "../lib/i18n/index.svelte";
  import { mail } from "../lib/stores/mail.svelte";
  import { ui } from "../lib/stores/ui.svelte";

  interface Row {
    label: string;
    keys: string[];
  }
  interface Group {
    title: string;
    rows: Row[];
  }

  // Key captions are the physical keycaps (Latin) — they match e.code and the
  // engraving on any keyboard regardless of layout, so they are NOT localized.
  const groups = $derived<Group[]>([
    {
      title: t("shortcuts.nav"),
      rows: [
        { label: t("shortcuts.next"), keys: ["J"] },
        { label: t("shortcuts.prev"), keys: ["K"] },
        { label: t("shortcuts.select"), keys: ["X"] },
        { label: t("shortcuts.select_all"), keys: ["Ctrl A"] },
        { label: t("shortcuts.clear"), keys: ["Esc"] },
        // Fork (3.1)
        { label: t("fork.shortcuts.filter_unread"), keys: ["Shift U"] },
        { label: t("fork.shortcuts.filter_starred"), keys: ["Shift S"] },
        { label: t("fork.shortcuts.goto"), keys: ["G  I/S/T/D/A/C/O/W"] },
        // Fork (4.3)
        { label: t("fork.shortcuts.search_list"), keys: ["Shift Enter"] },
        { label: t("fork.shortcuts.search_close"), keys: ["Esc"] },
        // Fork (9)
        { label: t("fork.nav.crm"), keys: ["I"] },
        // Fork (7.6)
        { label: t("fork.nav.meet_now"), keys: ["M"] },
      ],
    },
    {
      title: t("shortcuts.actions"),
      rows: [
        { label: t("reading.archive"), keys: ["E"] },
        { label: t("reading.delete"), keys: ["Del"] },
        { label: t("reading.spam"), keys: ["!"] },
        { label: t("reading.move"), keys: ["V"] },
        { label: t("reading.star"), keys: ["S"] },
        { label: t("reading.toggle_read"), keys: ["U"] },
        { label: t("reading.reply"), keys: ["R"] },
        { label: t("reading.reply_all"), keys: ["A"] },
        { label: t("reading.forward"), keys: ["F"] },
        // Fork (3.2)
        { label: t("fork.shortcuts.undo"), keys: ["Z", "Ctrl Z"] },
        // Fork (5, 6): composer keys.
        { label: t("fork.shortcuts.send"), keys: ["Ctrl Enter"] },
        { label: t("fork.shortcuts.smell_open"), keys: ["Alt Enter"] },
      ],
    },
    {
      // Fork (7.5): only while the calendar view is open.
      title: t("fork.nav.calendar"),
      rows: [
        { label: t("fork.shortcuts.cal_day"), keys: ["D"] },
        { label: t("fork.shortcuts.cal_week"), keys: ["W"] },
        { label: t("fork.shortcuts.cal_month"), keys: ["M"] },
        { label: t("fork.shortcuts.cal_agenda"), keys: ["A"] },
        { label: t("fork.shortcuts.cal_today"), keys: ["T"] },
        { label: t("fork.shortcuts.cal_next"), keys: ["J"] },
        { label: t("fork.shortcuts.cal_prev"), keys: ["K"] },
        { label: t("fork.shortcuts.cal_new"), keys: ["N"] },
      ],
    },
    {
      // Brand name, matching the AI action in ReadingPane — not localized.
      title: "Skim AI",
      rows: [
        { label: t("ai.ask"), keys: ["Q"] },
        { label: t("ai.translate"), keys: ["T"] },
        // The three below only do anything while a chat is open.
        { label: t("ai.expand"), keys: ["Ctrl E"] },
        { label: t("ai.open_window"), keys: ["Ctrl Shift E"] },
        { label: t("ai.close_chat"), keys: ["Esc"] },
      ],
    },
    {
      title: t("shortcuts.global"),
      rows: [
        { label: t("nav.search"), keys: ["Ctrl K", "/"] },
        { label: t("nav.compose"), keys: ["Ctrl N"] },
        // Only meaningful (and only active) with several mailboxes connected.
        ...(mail.accounts.length > 1
          ? [{ label: t("accounts.switch"), keys: ["Ctrl 1…9"] }]
          : []),
        { label: t("palette.toggle_sidebar"), keys: ["."] },
        { label: t("shortcuts.title"), keys: ["?"] },
        // Fork (2.7)
        { label: t("fork.shortcuts.zoom_in"), keys: ["Ctrl +"] },
        { label: t("fork.shortcuts.zoom_out"), keys: ["Ctrl -"] },
        { label: t("fork.shortcuts.zoom_reset"), keys: ["Ctrl 0"] },
      ],
    },
  ]);

  function onWindowKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      ui.closeShortcuts();
    }
  }
</script>

<svelte:window onkeydown={onWindowKeydown} />

<div class="overlay" use:backdropClose={() => ui.closeShortcuts()}>
  <div class="panel">
    <div class="head">
      <span class="title">{t("shortcuts.title")}</span>
      <kbd>ESC</kbd>
    </div>
    <div class="groups">
      {#each groups as group (group.title)}
        <div class="group">
          <div class="microlabel section">{group.title}</div>
          {#each group.rows as row (row.label)}
            <div class="row">
              <span class="label">{row.label}</span>
              <span class="keys">
                {#each row.keys as key, i (key)}
                  {#if i > 0}<span class="or">·</span>{/if}
                  <kbd>{key}</kbd>
                {/each}
              </span>
            </div>
          {/each}
        </div>
      {/each}
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.35);
    display: flex;
    justify-content: center;
    padding-top: 12vh;
    z-index: 100;
  }
  .panel {
    width: 520px;
    max-width: calc(100vw - 48px);
    max-height: 70vh;
    background: var(--surface-raised);
    border: 1px solid var(--hairline-strong);
    border-radius: var(--radius-l);
    box-shadow: var(--shadow-pop);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    height: fit-content;
  }

  .head {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 14px 16px;
    border-bottom: 1px solid var(--hairline);
  }
  .title {
    flex: 1;
    font-size: 14px;
    font-weight: 700;
  }

  .groups {
    overflow-y: auto;
    padding: 8px;
  }
  .section {
    padding: 10px 10px 4px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 10px;
    border-radius: var(--radius-s);
    font-size: 13.5px;
  }
  .label {
    flex: 1;
  }
  .keys {
    display: flex;
    align-items: center;
    gap: 5px;
    flex-shrink: 0;
  }
  .or {
    color: var(--text-faint);
    font-size: 11px;
  }

  kbd {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-faint);
    border: 1px solid var(--hairline-strong);
    border-radius: 4px;
    padding: 2px 6px;
  }
</style>
