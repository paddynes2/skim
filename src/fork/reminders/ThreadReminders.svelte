<script lang="ts">
  // Fork (v1.1.1): Snooze and Follow-up for the open thread. `mode="tools"`
  // renders the two toolbar buttons and their menu (also opened by H / B);
  // `mode="status"` renders the line under the subject that says what is set,
  // with Change / Remove. Both read the same reminders for `threadId`.
  import { listen } from "@tauri-apps/api/event";
  import { t } from "../../lib/i18n/index.svelte";
  import { leaveList } from "../actions";
  import { navHooks } from "../nav";
  import { toast } from "../stores/toast.svelte";
  import { REMINDERS_UPDATED, type Reminder, type ReminderKind, remindersApi } from "./api";
  import { remindersStore } from "./store.svelte";
  import { fmtDue, followupPresets, snoozePresets } from "./when";

  let { threadId, mode }: { threadId: number; mode: "tools" | "status" } = $props();

  let reminders = $state<Reminder[]>([]);
  let menu = $state<ReminderKind | null>(null);
  let customDate = $state("");
  let customTime = $state("08:00");

  async function load(id: number) {
    try {
      const r = await remindersApi.get(id);
      if (id === threadId) reminders = r;
    } catch {
      reminders = [];
    }
  }

  $effect(() => {
    void load(threadId);
    // Opening a thread un-pins its due reminders.
    if (mode === "status") void remindersApi.seen(threadId).catch(() => {});
  });

  $effect(() => {
    let off: (() => void) | null = null;
    let gone = false;
    void listen(REMINDERS_UPDATED, () => void load(threadId)).then((u) => {
      if (gone) u();
      else off = u;
    });
    return () => {
      gone = true;
      off?.();
    };
  });

  // H / B open the menu; only the toolbar instance registers.
  $effect(() => {
    if (mode !== "tools") return;
    navHooks.reminderMenu = (kind) => openMenu(kind);
    return () => {
      delete navHooks.reminderMenu;
    };
  });

  const snooze = $derived(reminders.find((r) => r.kind === "snooze") ?? null);
  const followup = $derived(reminders.find((r) => r.kind === "followup") ?? null);

  const presets = $derived(menu === "snooze" ? snoozePresets(new Date()) : followupPresets(new Date()));

  function openMenu(kind: ReminderKind) {
    const d = new Date();
    d.setDate(d.getDate() + 1);
    customDate = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
    menu = menu === kind ? null : kind;
  }

  async function pick(kind: ReminderKind, at: Date) {
    menu = null;
    if (at.getTime() <= Date.now()) return;
    try {
      await remindersStore.set(threadId, kind, at);
      const when = fmtDue(Math.floor(at.getTime() / 1000), new Date());
      if (kind === "snooze") {
        leaveList(threadId);
        toast.show({ text: t("fork.rem.snoozed_until", { when }) });
      } else {
        toast.show({ text: t("fork.rem.followup_set", { when }) });
      }
    } catch (e: unknown) {
      toast.show({ text: String((e as { message?: string })?.message ?? e) });
    }
  }

  function pickCustom(kind: ReminderKind) {
    const [y, mo, d] = customDate.split("-").map(Number);
    const [h, mi] = customTime.split(":").map(Number);
    if (!y || Number.isNaN(h)) return;
    void pick(kind, new Date(y, mo - 1, d, h, mi || 0));
  }

  async function remove(kind: ReminderKind) {
    await remindersStore.clear(threadId, kind);
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && menu) {
      e.stopPropagation();
      menu = null;
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

{#if mode === "tools"}
  <div class="tools">
    <button
      class="tool"
      class:on={snooze !== null}
      onclick={() => openMenu("snooze")}
      title={`${t("fork.rem.snooze")}  H`}
      aria-expanded={menu === "snooze"}
    >
      <svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" stroke-linecap="round">
        <circle cx="8" cy="8.5" r="5.5" />
        <path d="M8 5.5v3l2 1.2M3 2.5l-1.5 1.5M13 2.5l1.5 1.5" />
      </svg>
      <kbd>H</kbd>
    </button>
    <button
      class="tool"
      class:on={followup !== null}
      onclick={() => openMenu("followup")}
      title={`${t("fork.rem.followup")}  B`}
      aria-expanded={menu === "followup"}
    >
      <svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" stroke-linecap="round">
        <path d="M6 4L2.5 7.5 6 11" />
        <path d="M2.5 7.5h7a4 4 0 0 1 4 4v1" />
      </svg>
      <kbd>B</kbd>
    </button>

    {#if menu}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="scrim" onclick={() => (menu = null)}></div>
      <div class="menu" role="menu">
        <div class="menu-head">{menu === "snooze" ? t("fork.rem.snooze_until") : t("fork.rem.followup_if")}</div>
        {#each presets as p (p.id)}
          <button class="opt" role="menuitem" onclick={() => pick(menu!, p.at)}>
            <span>{t(p.label)}</span>
            <span class="when">{fmtDue(Math.floor(p.at.getTime() / 1000), new Date())}</span>
          </button>
        {/each}
        <div class="custom">
          <input type="date" bind:value={customDate} aria-label={t("fork.rem.pick_date")} />
          <input type="time" bind:value={customTime} step="900" aria-label={t("fork.rem.pick_time")} />
          <button class="set" onclick={() => pickCustom(menu!)}>{t("fork.rem.set")}</button>
        </div>
      </div>
    {/if}
  </div>
{:else if snooze || followup}
  <div class="status">
    {#if snooze}
      <span class="chip">
        {t("fork.rem.status_snoozed", { when: fmtDue(snooze.dueTs, new Date()) })}
        <button class="link" onclick={() => navHooks.reminderMenu?.("snooze")}>{t("fork.rem.change")}</button>
        <button class="link" onclick={() => remove("snooze")}>{t("fork.rem.remove")}</button>
      </span>
    {/if}
    {#if followup}
      <span class="chip" class:overdue={followup.dueTs * 1000 <= Date.now()}>
        {followup.dueTs * 1000 <= Date.now()
          ? t("fork.rem.status_no_reply", { when: fmtDue(followup.dueTs, new Date()) })
          : t("fork.rem.status_followup", { when: fmtDue(followup.dueTs, new Date()) })}
        <button class="link" onclick={() => remove("followup")}>{t("fork.rem.remove")}</button>
      </span>
    {/if}
  </div>
{/if}

<style>
  .tools {
    position: relative;
    display: flex;
    align-items: center;
    gap: 2px;
  }
  /* Same look as the reading pane's .tool buttons. */
  .tool {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 5px 8px;
    border-radius: var(--radius-s);
    color: var(--text-dim);
  }
  .tool:hover {
    background: var(--hover);
    color: var(--text);
  }
  .tool.on {
    color: var(--unread);
  }
  .tool kbd {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-faint);
  }
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 40;
  }
  .menu {
    position: absolute;
    top: calc(100% + 6px);
    right: 0;
    z-index: 41;
    width: 300px;
    padding: 6px;
    background: var(--surface-raised);
    border: 1px solid var(--hairline-strong);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow-pop);
  }
  .menu-head {
    padding: 4px 8px 6px;
    font-size: 11.5px;
    font-weight: 600;
    color: var(--text-faint);
  }
  .opt {
    display: flex;
    justify-content: space-between;
    gap: 10px;
    width: 100%;
    padding: 6px 8px;
    border-radius: var(--radius-s);
    text-align: left;
  }
  .opt:hover {
    background: var(--hover);
  }
  .when {
    color: var(--text-faint);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
  .custom {
    display: flex;
    gap: 6px;
    align-items: center;
    padding: 8px 8px 4px;
    margin-top: 4px;
    border-top: 1px solid var(--hairline);
  }
  .custom input {
    min-width: 0;
    padding: 4px 6px;
    border: 1px solid var(--hairline-strong);
    border-radius: var(--radius-s);
    background: var(--surface);
    font-size: 12.5px;
  }
  .custom input[type="date"] {
    flex: 1;
  }
  .set {
    padding: 4px 10px;
    border-radius: var(--radius-s);
    background: var(--primary);
    color: var(--on-primary);
    font-size: 12.5px;
    font-weight: 600;
  }
  .status {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: -4px 0 12px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 3px 10px;
    border-radius: 999px;
    background: var(--hover);
    border: 1px solid var(--hairline);
    font-size: 12.5px;
    color: var(--text-dim);
  }
  .chip.overdue {
    color: var(--danger);
  }
  .link {
    color: var(--unread);
    font-size: 12px;
  }
  .link:hover {
    text-decoration: underline;
  }
</style>
