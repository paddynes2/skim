// Webview zoom (PLAN.md 2.7). One place applies the level so the main,
// compose and chat windows all read the same setting at their root.
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { prefs, ZOOM_MAX, ZOOM_MIN, ZOOM_STEP } from "./stores/prefs.svelte";

let applied = 1;

/** Apply `level` to this window's webview. Best effort: the demo harness and
 *  a webview without the permission simply keep 100%. */
export async function applyZoom(level: number): Promise<void> {
  const v = Math.round(Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, level)) * 100) / 100;
  if (v === applied) return;
  try {
    await getCurrentWebview().setZoom(v);
    applied = v;
  } catch {
    // no permission / not in Tauri: nothing to apply
  }
}

/** Ctrl+= / Ctrl+- / Ctrl+0 (any window). Returns true when handled. */
export function zoomKey(e: KeyboardEvent): boolean {
  if (!(e.ctrlKey || e.metaKey) || e.altKey) return false;
  let next: number | null = null;
  if (e.code === "Equal" || e.code === "NumpadAdd") next = prefs.zoom + ZOOM_STEP;
  else if (e.code === "Minus" || e.code === "NumpadSubtract") next = prefs.zoom - ZOOM_STEP;
  else if (e.code === "Digit0" || e.code === "Numpad0") next = 1;
  if (next === null) return false;
  e.preventDefault();
  void applyZoom(prefs.setZoom(next));
  return true;
}
