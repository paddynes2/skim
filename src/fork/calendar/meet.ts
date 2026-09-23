// Compose toolbar "Add Meet link" (PLAN.md 7.6). The composer (Phase 6,
// src/fork/compose/) calls `addMeetLink(insert)` from its toolbar button; the
// insert callback writes at the cursor. Connected: a fresh Meet space, inserted
// and toasted. Not connected: meet.new opens in the browser and nothing is
// inserted, since there is no link to insert until the user creates one.
import { openUrl } from "@tauri-apps/plugin-opener";
import { t } from "../../lib/i18n/index.svelte";
import { toast } from "../stores/toast.svelte";
import { calendarErrorText } from "./guests";
import { calendar } from "./store.svelte";

export async function addMeetLink(insert: (text: string) => void): Promise<void> {
  try {
    const link = await calendar.meetLink();
    if (!link) {
      void openUrl("https://meet.new");
      toast.show({ text: t("fork.cal.meet_not_connected"), ms: 6000 });
      return;
    }
    insert(link);
    toast.show({ text: t("fork.cal.meet_inserted"), ms: 4000 });
  } catch (e: unknown) {
    toast.show({ text: calendarErrorText(e) });
  }
}
