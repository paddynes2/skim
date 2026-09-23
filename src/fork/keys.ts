// Fork key map (PLAN.md 3.3). `App.svelte`'s `onKeydown` calls `forkKey`
// first, once the upstream guards (palette open, typing, modifiers) have run,
// so every new binding lives here and upstream's switch stays as it was.
import { mail } from "../lib/stores/mail.svelte";
import { prefs } from "./stores/prefs.svelte";
import { undo } from "./stores/undo.svelte";

/** Returns true when the event was consumed. */
export function forkKey(e: KeyboardEvent): boolean {
  // z: undo the last action (3.2). Ctrl+Z is handled in App before the Ctrl guard.
  if (!e.shiftKey && e.code === "KeyZ") {
    e.preventDefault();
    void undo.undo();
    return true;
  }
  // Shift+U / Shift+S toggle the list filter (3.1). Checked before upstream's
  // plain U / S, which would otherwise mark read / star.
  if (e.shiftKey && e.code === "KeyU") {
    e.preventDefault();
    void mail.setListFilter(mail.listFilter === "unread" ? "all" : "unread");
    return true;
  }
  if (e.shiftKey && e.code === "KeyS") {
    e.preventDefault();
    void mail.setListFilter(mail.listFilter === "starred" ? "all" : "starred");
    return true;
  }
  return false;
}

/** For the settings panel: the persisted order, applied to the open list. */
export function setListOrder(order: "date" | "unread_first"): void {
  if (prefs.listOrder !== order) void mail.setListOrder(order);
}
