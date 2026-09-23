// Fork rows for the command palette (PLAN.md 3.3): every new key is
// discoverable there with its hint. Spliced into upstream's list by one call.
import { t } from "../lib/i18n/index.svelte";
import { mail } from "../lib/stores/mail.svelte";
import { GO_TARGETS } from "./keys";
import { navHooks } from "./nav";
import { prefs } from "./stores/prefs.svelte";
import { undo } from "./stores/undo.svelte";
import { applyZoom } from "./zoom";

export interface ForkCommand {
  id: string;
  label: string;
  hint?: string;
  run: () => void | Promise<void>;
}

/** The key hint for "Go to <folder>" rows, so upstream's goto rows teach `g`. */
export function gotoHint(role: string | null): string | undefined {
  const g = GO_TARGETS.find((x) => x.role === role);
  return g ? `G ${g.key.toUpperCase()}` : undefined;
}

export function forkCommands(): ForkCommand[] {
  const list: ForkCommand[] = [];
  if (undo.canUndo) {
    list.push({ id: "fork-undo", label: t("fork.shortcuts.undo"), hint: "Z", run: () => void undo.undo() });
  }
  list.push(
    {
      id: "fork-filter-unread",
      label: t("fork.shortcuts.filter_unread"),
      hint: "Shift U",
      run: () => void mail.setListFilter(mail.listFilter === "unread" ? "all" : "unread"),
    },
    {
      id: "fork-filter-starred",
      label: t("fork.shortcuts.filter_starred"),
      hint: "Shift S",
      run: () => void mail.setListFilter(mail.listFilter === "starred" ? "all" : "starred"),
    },
    {
      id: "fork-zoom-in",
      label: t("fork.shortcuts.zoom_in"),
      hint: "Ctrl +",
      run: () => void applyZoom(prefs.setZoom(prefs.zoom + 0.1)),
    },
    {
      id: "fork-zoom-out",
      label: t("fork.shortcuts.zoom_out"),
      hint: "Ctrl -",
      run: () => void applyZoom(prefs.setZoom(prefs.zoom - 0.1)),
    },
    {
      id: "fork-zoom-reset",
      label: t("fork.shortcuts.zoom_reset"),
      hint: "Ctrl 0",
      run: () => void applyZoom(prefs.setZoom(1)),
    },
  );
  if (navHooks.calendar)
    list.push({ id: "fork-calendar", label: t("palette.goto", { folder: t("fork.nav.calendar") }), hint: "G C", run: () => navHooks.calendar?.() });
  if (navHooks.court) {
    list.push(
      { id: "fork-on-me", label: t("palette.goto", { folder: t("fork.nav.on_me") }), hint: "G O", run: () => navHooks.court?.("on_me") },
      { id: "fork-waiting", label: t("palette.goto", { folder: t("fork.nav.waiting") }), hint: "G W", run: () => navHooks.court?.("waiting") },
    );
  }
  if (navHooks.crmToggle)
    list.push({ id: "fork-crm", label: t("fork.nav.crm"), hint: "I", run: () => navHooks.crmToggle?.() });
  if (navHooks.meetNow)
    list.push({ id: "fork-meet", label: t("fork.nav.meet_now"), hint: "M", run: () => navHooks.meetNow?.() });
  return list;
}
