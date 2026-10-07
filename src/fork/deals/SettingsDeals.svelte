<script lang="ts">
  // Fork (v1.1.3): the Deals list, one deal per line. Mounted from
  // SettingsFork.svelte; styles mirror the court section's. Saved when the box
  // loses focus, so a half-typed line never reaches the view.
  import { t } from "../../lib/i18n/index.svelte";
  import { dealsApi, type ParsedDeals } from "./api";
  import { dealsStore } from "./store.svelte";

  let text = $state("");
  let saved = $state(false);
  let ready = $state(false);

  $effect(() => {
    void dealsStore.load().then(() => {
      text = dealsStore.text;
      ready = true;
    });
  });

  async function save() {
    if (!ready || text === dealsStore.text) return;
    await dealsStore.save(text);
    saved = true;
    setTimeout(() => (saved = false), 1500);
  }

  // How the box reads, as he types: the deals it found and any entry it
  // will not use, with the reason. Read by the same parser the view uses.
  let parsed = $state<ParsedDeals | null>(null);
  $effect(() => {
    const current = text;
    if (!ready) return;
    const timer = setTimeout(() => {
      void dealsApi
        .preview(current)
        .then((p) => {
          if (current === text) parsed = p;
        })
        .catch(() => {});
    }, 250);
    return () => clearTimeout(timer);
  });
</script>

<section class="deals">
  <div class="head">
    <span class="microlabel">{t("fork.deals.settings")}</span>
    {#if saved}<span class="saved">{t("fork.deals.settings_saved")}</span>{/if}
  </div>
  <p class="note">{t("fork.deals.settings_note")}</p>
  <textarea
    bind:value={text}
    onblur={() => void save()}
    rows="8"
    spellcheck="false"
    aria-label={t("fork.deals.settings")}
    placeholder={"Acme: acme.com\nJo Bloggs: jo.bloggs@gmail.com"}
    disabled={!ready}
  ></textarea>
  {#if parsed && (parsed.deals.length > 0 || parsed.ignored.length > 0)}
    <div class="read-as" aria-live="polite">
      {#if parsed.deals.length > 0}
        <p>{t("fork.deals.read_as", { n: parsed.deals.length, names: parsed.deals.map((d) => d.name).join(", ") })}</p>
      {/if}
      {#each parsed.ignored as i (i.entry)}
        <p class="ignored">{t(`fork.deals.ignored_${i.why}`, { entry: i.entry })}</p>
      {/each}
    </div>
  {/if}
</section>

<style>
  .deals {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
  }
  .saved {
    font-size: 11.5px;
    color: var(--text-faint);
  }
  .note {
    font-size: 12px;
    color: var(--text-faint);
    margin: -4px 0 2px;
    line-height: 1.4;
  }
  textarea {
    width: 100%;
    min-height: 140px;
    padding: 8px 10px;
    border: 1px solid var(--hairline-strong);
    border-radius: var(--radius-s);
    background: var(--surface);
    color: var(--text);
    font-family: var(--font-mono);
    font-size: 12.5px;
    line-height: 1.6;
    resize: vertical;
  }
  textarea:focus-visible {
    outline: none;
    border-color: var(--focus);
  }
  .read-as {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 12px;
    color: var(--text-dim);
    line-height: 1.4;
  }
  .read-as p {
    margin: 0;
  }
  .read-as .ignored {
    color: var(--danger);
  }
</style>
