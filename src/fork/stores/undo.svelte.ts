// Undo (PLAN.md 3.2, D3). Two layers for removals:
//   1. an 8 s client-side hold: the rows leave the list at once, the API call
//      waits; `z` cancels the timer and puts the rows back at their index.
//   2. after the hold: a `fork_restore` op finds the mail by Message-ID where
//      it went and puts it back.
// Flags (star / read) undo by the inverse call. Stack depth 20 per session.
import { api } from "../../lib/api";
import { t } from "../../lib/i18n/index.svelte";
import { mail } from "../../lib/stores/mail.svelte";
import type { ThreadRow } from "../../lib/types";
import { forkApi, type RemovalSnapshot } from "../api";
import { toast } from "./toast.svelte";
import { keyOf, pushBounded } from "../undo-filter";

export const GRACE_MS = 8000;
export const STACK_DEPTH = 20;

export type RemovalKind = "archive" | "delete" | "spam" | "move";

interface HeldRemoval {
  id: number;
  kind: RemovalKind;
  /** Rows as they were, with the index each sat at. */
  rows: { row: ThreadRow; index: number }[];
  /** Message ids the API call will act on. */
  ids: number[];
  /** Resolved before the rows left; empty until the IPC answers. */
  snapshots: RemovalSnapshot[];
  destImapName?: string;
  /** The API call, run once the hold ends. */
  fire: () => void;
  timer: ReturnType<typeof setTimeout>;
  /** Which row was open, so undo can reopen it. */
  reopen: { threadId: number; messageId: number | null } | null;
}

type Entry =
  | { kind: "held"; held: HeldRemoval }
  | { kind: "restore"; removal: RemovalKind; snapshots: RemovalSnapshot[]; destImapName?: string; label: string }
  | { kind: "flag"; label: string; undo: () => void };

let stack = $state<Entry[]>([]);
const pending = $state(new Set<number>());
let seq = 0;

function pendingKeys(): ReadonlySet<number> {
  return pending;
}

function labelFor(kind: RemovalKind, n: number): string {
  const key =
    kind === "archive"
      ? "fork.undo.archived"
      : kind === "delete"
        ? "fork.undo.deleted"
        : kind === "spam"
          ? "fork.undo.spammed"
          : "fork.undo.moved";
  return t(key, { n });
}

/** Hold a removal for the grace window. The caller has already dropped the
 *  rows from the list; this remembers where they were and fires `fire` once
 *  the window closes (or right away on flush). */
function hold(
  kind: RemovalKind,
  rows: { row: ThreadRow; index: number }[],
  ids: number[],
  fire: () => void,
  opts: { destImapName?: string; reopen?: HeldRemoval["reopen"] } = {},
): void {
  const id = ++seq;
  for (const { row } of rows) pending.add(keyOf(row));
  const held: HeldRemoval = {
    id,
    kind,
    rows,
    ids,
    snapshots: [],
    destImapName: opts.destImapName,
    fire,
    reopen: opts.reopen ?? null,
    timer: setTimeout(() => commit(id), GRACE_MS),
  };
  // Snapshot while the local rows still exist (they go when `fire` runs).
  void forkApi
    .removalSnapshot(ids)
    .then((s) => {
      held.snapshots = s;
    })
    .catch(() => {});
  stack = pushBounded(stack, { kind: "held", held }, STACK_DEPTH);
  toast.show({
    text: labelFor(kind, rows.length),
    ms: GRACE_MS,
    action: { label: t("fork.undo.undo"), key: "Z", run: () => undoHeld(id) },
    // The toast going away (replaced, dismissed, expired) does not end the
    // hold: the timer does. A replaced toast still leaves its removal held.
  });
}

/** The window closed: make the call, keep a server-side restore entry. */
function commit(id: number) {
  const i = stack.findIndex((e) => e.kind === "held" && e.held.id === id);
  if (i === -1) return;
  const entry = stack[i];
  if (entry.kind !== "held") return;
  const held = entry.held;
  clearTimeout(held.timer);
  for (const { row } of held.rows) pending.delete(keyOf(row));
  held.fire();
  const next = [...stack];
  next[i] = {
    kind: "restore",
    removal: held.kind,
    snapshots: held.snapshots,
    destImapName: held.destImapName,
    label: labelFor(held.kind, held.rows.length),
  };
  stack = next;
  if (toast.current && toast.current.action?.key === "Z") {
    // Keep the toast honest once the hold is over: undo now means restore.
    toast.dismiss();
  }
}

function undoHeld(id: number) {
  const i = stack.findIndex((e) => e.kind === "held" && e.held.id === id);
  if (i === -1) return;
  const entry = stack[i];
  if (entry.kind !== "held") return;
  const held = entry.held;
  clearTimeout(held.timer);
  for (const { row } of held.rows) pending.delete(keyOf(row));
  // Restore in index order so each lands where it was.
  for (const { row, index } of [...held.rows].sort((a, b) => a.index - b.index)) {
    mail.insertThreadRow(row, index);
  }
  if (held.reopen) {
    mail.selectedThreadId = held.reopen.threadId;
    mail.selectedMessageId = held.reopen.messageId;
  }
  stack = stack.filter((_, j) => j !== i);
  toast.dismiss();
}

async function undoRestore(entry: Extract<Entry, { kind: "restore" }>) {
  const usable = entry.snapshots.filter((s) => s.messageIds.length > 0);
  if (usable.length === 0) {
    toast.show({ text: t("fork.undo.cannot_restore"), ms: 6000 });
    return;
  }
  try {
    for (const snapshot of usable) {
      await forkApi.restore(snapshot, entry.removal, entry.destImapName ?? null);
    }
    toast.show({ text: t("fork.undo.restoring"), ms: 4000 });
  } catch {
    toast.show({ text: t("fork.undo.cannot_restore"), ms: 6000 });
  }
}

export const undo = {
  get pendingKeys() {
    return pendingKeys();
  },
  get canUndo() {
    return stack.length > 0;
  },
  hold,
  /** Record a reversible flag change. */
  pushFlag(label: string, revert: () => void) {
    stack = pushBounded(stack, { kind: "flag", label, undo: revert }, STACK_DEPTH);
  },
  /** `z` / Ctrl+Z / the toast: undo the most recent entry. */
  async undo() {
    const entry = stack[stack.length - 1];
    if (!entry) return;
    if (entry.kind === "held") {
      undoHeld(entry.held.id);
      return;
    }
    stack = stack.slice(0, -1);
    if (entry.kind === "flag") {
      entry.undo();
      toast.show({ text: t("fork.undo.undone", { what: entry.label }), ms: 3000 });
      return;
    }
    await undoRestore(entry);
  },
  /** Window closing or app quitting: the held removals were intended. */
  flushAll() {
    for (const e of [...stack]) if (e.kind === "held") commit(e.held.id);
  },
};

// Keep `api` referenced for the flag revert callers that build closures over it.
void api;
