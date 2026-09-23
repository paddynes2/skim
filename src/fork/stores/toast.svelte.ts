// One toast at a time, bottom-centre (PLAN.md 3.2). Stays for `ms`, pauses
// on hover, never auto-dismisses while it has focus (Material rule). The
// component is src/fork/Toast.svelte.

export interface ToastAction {
  label: string;
  /** Key caption shown next to the label, e.g. "Z". */
  key?: string;
  run: () => void;
}

export interface ToastSpec {
  text: string;
  action?: ToastAction;
  /** Lifetime; 0 means until dismissed. */
  ms?: number;
  /** Called when the toast goes away without its action having run. */
  onExpire?: () => void;
}

interface Live extends ToastSpec {
  id: number;
  remaining: number;
  startedAt: number;
  timer: ReturnType<typeof setTimeout> | null;
  held: boolean;
}

let seq = 0;
let current = $state<Live | null>(null);

function clearTimer(t: Live) {
  if (t.timer) clearTimeout(t.timer);
  t.timer = null;
}

function arm(t: Live) {
  clearTimer(t);
  if (!t.ms || t.remaining <= 0) return;
  t.startedAt = Date.now();
  t.timer = setTimeout(() => expire(t.id), t.remaining);
}

function expire(id: number) {
  const t = current;
  if (!t || t.id !== id) return;
  clearTimer(t);
  current = null;
  t.onExpire?.();
}

export const toast = {
  get current() {
    return current;
  },
  /** Show a toast, replacing (and expiring) the one before it. */
  show(spec: ToastSpec): number {
    if (current) {
      const prev = current;
      clearTimer(prev);
      current = null;
      prev.onExpire?.();
    }
    const live: Live = {
      ...spec,
      id: ++seq,
      remaining: spec.ms ?? 8000,
      startedAt: Date.now(),
      timer: null,
      held: false,
    };
    current = live;
    arm(live);
    return live.id;
  },
  /** Run the action (if any) and dismiss. */
  act() {
    const t = current;
    if (!t) return;
    clearTimer(t);
    current = null;
    t.action?.run();
  },
  /** Dismiss without the action; `onExpire` still runs (the intent stands). */
  dismiss(id?: number) {
    if (id !== undefined && current?.id !== id) return;
    if (current) expire(current.id);
  },
  /** Pointer over / focus in: freeze the clock. */
  hold() {
    const t = current;
    if (!t || t.held) return;
    t.held = true;
    if (t.timer) {
      t.remaining -= Date.now() - t.startedAt;
      clearTimer(t);
    }
  },
  /** Pointer out / focus out: resume. */
  release() {
    const t = current;
    if (!t || !t.held) return;
    t.held = false;
    arm(t);
  },
};
