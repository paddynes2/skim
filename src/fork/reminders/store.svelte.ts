// Fork (v1.1.1): Snooze / Follow-ups lifecycle. Registers the nav hook (so
// `g z` / `g f`, the palette and the sidebar open the views), keeps the sidebar
// counts, and shows a toast when a reminder falls due. The views themselves
// are the mail store's virtual folders -922 / -923. A due reminder also pins
// its thread to the top of the list (Rust, `fork::list::apply`), so a closed
// window still shows it on return.
import { listen } from "@tauri-apps/api/event";
import { t } from "../../lib/i18n/index.svelte";
import { mail } from "../../lib/stores/mail.svelte";
import { ui } from "../../lib/stores/ui.svelte";
import { navHooks } from "../nav";
import { toast } from "../stores/toast.svelte";
import { type DueNote, REMINDERS_UPDATED, type ReminderKind, remindersApi } from "./api";
import { reminderCounts } from "./counts.svelte";

let started = false;

function announce(due: DueNote[]) {
  if (due.length === 0) return;
  const first = due[0];
  const key = first.kind === "snooze" ? "fork.rem.toast_snooze" : "fork.rem.toast_followup";
  const text =
    due.length === 1
      ? t(key, { subject: first.subject || t("fork.rem.no_subject") })
      : t("fork.rem.toast_many", { n: due.length });
  toast.show({
    text,
    ms: 12_000,
    action: {
      label: t("fork.rem.toast_open"),
      run: () => remindersStore.open(first.kind),
    },
  });
}

export const remindersStore = {
  open(kind: ReminderKind) {
    ui.showMail();
    void mail.selectReminders(kind);
  },
  async set(threadId: number, kind: ReminderKind, due: Date) {
    await remindersApi.set(threadId, kind, Math.floor(due.getTime() / 1000));
    void reminderCounts.refresh();
  },
  async clear(threadId: number, kind: ReminderKind) {
    await remindersApi.clear(threadId, kind);
    void reminderCounts.refresh();
  },
  start(): () => void {
    if (started) return () => {};
    started = true;
    navHooks.reminders = (kind) => remindersStore.open(kind);
    void reminderCounts.refresh();
    let unlisten: (() => void) | null = null;
    let stopped = false;
    void listen<{ due?: DueNote[] }>(REMINDERS_UPDATED, (e) => {
      announce(e.payload?.due ?? []);
    }).then((u) => {
      if (stopped) u();
      else unlisten = u;
    });
    return () => {
      stopped = true;
      unlisten?.();
      delete navHooks.reminders;
      started = false;
    };
  },
};
