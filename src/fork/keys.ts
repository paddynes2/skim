// Fork key map (PLAN.md 3.3). `App.svelte`'s `onKeydown` calls `forkKey`
// first, once the upstream guards (palette open, typing, modifiers) have run,
// so every new binding lives here and upstream's switch stays as it was.
import { mail } from "../lib/stores/mail.svelte";
import { navHooks } from "./nav";
import { prefs } from "./stores/prefs.svelte";
import { undo } from "./stores/undo.svelte";

/** Where `g` can go. Order is the order the hint shows them in. */
export const GO_TARGETS: { key: string; code: string; label: string; role?: string }[] = [
  { key: "i", code: "KeyI", label: "nav.inbox", role: "inbox" },
  { key: "s", code: "KeyS", label: "nav.starred", role: "starred" },
  { key: "t", code: "KeyT", label: "nav.sent", role: "sent" },
  { key: "d", code: "KeyD", label: "nav.drafts", role: "drafts" },
  { key: "a", code: "KeyA", label: "nav.archive", role: "archive" },
  { key: "c", code: "KeyC", label: "fork.nav.calendar" },
  { key: "o", code: "KeyO", label: "fork.nav.on_me" },
  { key: "w", code: "KeyW", label: "fork.nav.waiting" },
];

const GO_WINDOW_MS = 1000;
let goTimer: ReturnType<typeof setTimeout> | null = null;
let goArmed = $state(false);

function armGo() {
  goArmed = true;
  if (goTimer) clearTimeout(goTimer);
  goTimer = setTimeout(disarmGo, GO_WINDOW_MS);
}

function disarmGo() {
  goArmed = false;
  if (goTimer) clearTimeout(goTimer);
  goTimer = null;
}

/** Jump to a role folder in the current scope (real or unified). "archive"
 *  falls back to All Mail when the account has no archive folder (Gmail). */
export function goToRole(role: string): boolean {
  const folder =
    mail.folders.find((f) => f.role === role) ??
    (role === "archive" ? mail.folders.find((f) => f.role === "all") : undefined);
  if (!folder) return false;
  void mail.selectFolder(folder.id);
  return true;
}

export function goTo(key: string): void {
  switch (key) {
    case "c":
      navHooks.calendar?.();
      return;
    case "o":
      navHooks.court?.("on_me");
      return;
    case "w":
      navHooks.court?.("waiting");
      return;
    default: {
      const target = GO_TARGETS.find((t) => t.key === key);
      if (target?.role) goToRole(target.role);
    }
  }
}

/** Returns true when the event was consumed. */
export function forkKey(e: KeyboardEvent): boolean {
  // A pending `g`: the next key picks the destination (1 s window).
  if (goArmed) {
    disarmGo();
    const target = GO_TARGETS.find((t) => t.code === e.code);
    if (target && !e.shiftKey) {
      e.preventDefault();
      goTo(target.key);
      return true;
    }
    // Any other key cancels the sequence and is handled normally below.
  }
  if (!e.shiftKey && e.code === "KeyG") {
    e.preventDefault();
    armGo();
    return true;
  }
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
  // i: CRM sidebar (Phase 9); m: Meet now (Phase 7). No-ops until registered.
  if (!e.shiftKey && e.code === "KeyI" && navHooks.crmToggle) {
    e.preventDefault();
    navHooks.crmToggle();
    return true;
  }
  if (!e.shiftKey && e.code === "KeyM" && navHooks.meetNow) {
    e.preventDefault();
    navHooks.meetNow();
    return true;
  }
  return false;
}

export const goSequence = {
  /** True while `g` waits for its second key: the hint is shown. */
  get armed() {
    return goArmed;
  },
  cancel: disarmGo,
};

/** For the settings panel: the persisted order, applied to the open list. */
export function setListOrder(order: "date" | "unread_first"): void {
  if (prefs.listOrder !== order) void mail.setListOrder(order);
}
