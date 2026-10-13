// The `+` / `-` keys and menu items (v1.1.12, D65): put the sender of a
// conversation on the always or never-in-Priority list, and say so.
import { t } from "../../lib/i18n/index.svelte";
import type { ThreadRow } from "../../lib/types";
import { toast } from "../stores/toast.svelte";
import { priorityApi } from "./api";

export async function trainPriority(row: ThreadRow, important: boolean): Promise<void> {
  const sender = row.fromAddr.trim().toLowerCase();
  if (!sender.includes("@")) return;
  try {
    await priorityApi.train(sender, important);
    toast.show({
      text: t(important ? "fork.priority.trained_always" : "fork.priority.trained_never", { sender }),
      action: { label: t("fork.priority.trained_undo"), run: () => void priorityApi.train(sender, null) },
      ms: 6000,
    });
  } catch (e) {
    toast.show({ text: String((e as { message?: string })?.message ?? e), ms: 6000 });
  }
}
