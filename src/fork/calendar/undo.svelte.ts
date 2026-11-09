// Undo for calendar changes (D97): the toast's Undo button and Ctrl+Z.
//
// A delete is held: the event leaves the grid at once, but nothing is queued
// until the Undo time is up (or another toast replaces this one). Undo then
// just shows the event again; Google and the guests never hear of it. The
// guest prompt is answered at the click, so the held delete sends exactly what
// was chosen there.
//
// Every other undo (a move, a resize, an answer, a create) is a new change and
// goes through the guest prompt again, like any change: nothing is replayed
// with an earlier answer.
import { t } from "../../lib/i18n/index.svelte";
import { toast } from "../stores/toast.svelte";
import { calendarErrorText } from "./guests";
import { calendar } from "./store.svelte";
import type { EventRow } from "./types";

const HOLD_MS = 8000;

let last: { undo: () => Promise<void> | void } | null = null;

async function runLast(): Promise<void> {
  const entry = last;
  last = null;
  if (!entry) return;
  try {
    await entry.undo();
  } catch (e: unknown) {
    toast.show({ text: calendarErrorText(e) });
  }
}

export const calUndo = {
  /** Offer Undo for a change that already happened; `undo` makes the reverse change. */
  offer(text: string, undo: () => Promise<void> | void, ms = HOLD_MS) {
    const entry = { undo };
    last = entry;
    toast.show({
      text,
      ms,
      action: { label: t("fork.cal.undo"), key: "Ctrl+Z", run: () => void runLast() },
      onExpire: () => {
        if (last === entry) last = null;
      },
    });
  },
  /** Ctrl+Z in the calendar. False when there is nothing to undo. */
  run(): boolean {
    if (!last) return false;
    toast.act();
    return true;
  },
  get available(): boolean {
    return last !== null;
  },
};

/** Delete with a hold. Resolves false when the guest prompt was cancelled. */
export async function deleteWithUndo(row: EventRow, series = false): Promise<boolean> {
  const su = await calendar.askDelete(row.id, series);
  if (su === null) return false;
  calendar.hide(row.id);
  let undone = false;
  const entry = {
    undo: () => {
      undone = true;
      calendar.unhide(row.id);
    },
  };
  last = entry;
  toast.show({
    text: t(series ? "fork.cal.deleted_series" : "fork.cal.deleted"),
    ms: HOLD_MS,
    action: { label: t("fork.cal.undo"), key: "Ctrl+Z", run: () => void runLast() },
    onExpire: () => {
      if (last === entry) last = null;
      if (undone) return;
      calendar
        .remove(row.id, series, su)
        .catch((e: unknown) => toast.show({ text: calendarErrorText(e) }))
        .finally(() => calendar.unhide(row.id));
    },
  });
  return true;
}
