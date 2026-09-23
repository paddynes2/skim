<script lang="ts">
  // Fork (6.4): the "Scheduled" virtual folder (id -910). Every send on a
  // hold, soonest first, with Edit (cancel the hold, reopen the draft) and
  // Send now. Self-contained: it reads `fork_scheduled_list` and refreshes on
  // `fork:scheduled` / `drafts:updated`, so the mail store never sees it.
  // App.svelte shows it in place of the message list + reading pane when
  // `mail.selectedFolderId === SCHEDULED_FOLDER_ID`.
  import { listen } from "@tauri-apps/api/event";
  import { api, errorMessage } from "../../lib/api";
  import { t } from "../../lib/i18n/index.svelte";
  import { forkComposeApi, type ScheduledSend } from "../compose/api";
  import { formatWhen } from "../compose/when";

  let rows = $state<ScheduledSend[]>([]);
  let loaded = $state(false);
  let error = $state("");
  // Re-rendered every half minute so "Today 14:00" turns into "Sending…".
  let tick = $state(0);

  async function refresh() {
    try {
      rows = await forkComposeApi.scheduledList();
      error = "";
    } catch (e) {
      error = errorMessage(e);
    }
    loaded = true;
  }

  $effect(() => {
    void refresh();
    const offs: (() => void)[] = [];
    let gone = false;
    for (const name of ["fork:scheduled", "drafts:updated", "mail:sent"]) {
      void listen(name, () => void refresh()).then((un) => {
        if (gone) un();
        else offs.push(un);
      });
    }
    const timer = setInterval(() => (tick += 1), 30_000);
    return () => {
      gone = true;
      offs.forEach((un) => un());
      clearInterval(timer);
    };
  });

  function whenLabel(r: ScheduledSend): string {
    void tick;
    const now = new Date();
    const when = new Date(r.notBefore * 1000);
    if (when <= now) return t("fork.compose.sending");
    return r.label ?? formatWhen(when, now);
  }

  async function edit(r: ScheduledSend) {
    try {
      const draftId = await forkComposeApi.cancelScheduled(r.opId);
      rows = rows.filter((x) => x.opId !== r.opId);
      await api.openComposeWindow(draftId);
    } catch (e) {
      error = errorMessage(e);
    }
  }

  async function sendNow(r: ScheduledSend) {
    try {
      await forkComposeApi.sendNow(r.opId);
      await refresh();
    } catch (e) {
      error = errorMessage(e);
    }
  }
</script>

<section class="scheduled">
  <header class="head">
    <h1>{t("fork.scheduled.title")}</h1>
    <span class="count">{rows.length}</span>
  </header>
  {#if error}
    <div class="error">{error}</div>
  {/if}
  {#if loaded && rows.length === 0}
    <div class="empty">{t("fork.scheduled.empty")}</div>
  {:else}
    <ul class="list">
      {#each rows as r (r.opId)}
        <li class="row">
          <div class="when">{whenLabel(r)}</div>
          <div class="main">
            <div class="line1">
              <span class="to">{r.to || t("fork.scheduled.no_recipient")}</span>
            </div>
            <div class="line2">
              <span class="subject">{r.subject || t("fork.row.no_subject")}</span>
              {#if r.snippet}<span class="snippet">— {r.snippet}</span>{/if}
            </div>
          </div>
          <div class="actions">
            <button class="btn" onclick={() => edit(r)} title={t("fork.scheduled.edit_hint")}>{t("fork.scheduled.edit")}</button>
            <button class="btn primary" onclick={() => sendNow(r)}>{t("fork.scheduled.send_now")}</button>
          </div>
        </li>
      {/each}
    </ul>
  {/if}
  <footer class="note">{t("fork.scheduled.note")}</footer>
</section>

<style>
  .scheduled {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    background: var(--surface);
  }
  .head {
    display: flex;
    align-items: baseline;
    gap: 10px;
    padding: 18px 20px 12px;
    border-bottom: 1px solid var(--hairline);
  }
  h1 {
    font-size: 17px;
    font-weight: 700;
    margin: 0;
  }
  .count {
    color: var(--text-faint);
    font-family: var(--font-mono);
    font-size: 11px;
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    flex: 1;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 12px 20px;
    border-bottom: 1px solid var(--hairline);
  }
  .row:hover {
    background: var(--hover);
  }
  .when {
    width: 112px;
    flex-shrink: 0;
    font-family: var(--font-mono);
    font-size: 11.5px;
    color: var(--text-dim);
  }
  .main {
    flex: 1;
    min-width: 0;
  }
  .line1 {
    font-weight: 600;
    font-size: 13.5px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .line2 {
    font-size: 13px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .snippet {
    color: var(--text-faint);
  }
  .actions {
    display: flex;
    gap: 6px;
    flex-shrink: 0;
  }
  .btn {
    padding: 5px 12px;
    border-radius: var(--radius-s);
    border: 1px solid var(--hairline-strong);
    font-size: 12.5px;
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
  }
  .empty {
    padding: 40px 20px;
    text-align: center;
    color: var(--text-faint);
    font-size: 13.5px;
    flex: 1;
  }
  .error {
    padding: 8px 20px;
    color: var(--danger);
    font-size: 12.5px;
  }
  .note {
    padding: 10px 20px;
    border-top: 1px solid var(--hairline);
    color: var(--text-faint);
    font-size: 12px;
  }
</style>
