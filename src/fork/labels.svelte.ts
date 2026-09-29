// Fork (v1.1.1): hide the Labels section, or single labels, in the sidebar.
// `fork_labels_collapsed` folds the whole section (default: folded, Patrick
// does not use labels); `fork_labels_hidden` is a JSON list of folder paths
// hidden one by one. Hiding a label hides the labels nested under it.
import { api } from "../lib/api";

const state = $state({
  collapsed: true,
  hidden: [] as string[],
  /** Hidden labels shown dimmed so one can be brought back. Not persisted. */
  revealHidden: false,
  loaded: false,
});

let loading: Promise<void> | null = null;

function persist(key: string, value: string) {
  void api.setSetting(key, value).catch(() => {});
}

export const labelPrefs = {
  hydrate(s: Record<string, string>) {
    if (s.fork_labels_collapsed !== undefined) state.collapsed = s.fork_labels_collapsed !== "off";
    if (s.fork_labels_hidden !== undefined) {
      try {
        const v = JSON.parse(s.fork_labels_hidden);
        if (Array.isArray(v)) state.hidden = v.filter((x) => typeof x === "string");
      } catch {
        // A damaged value hides nothing.
      }
    }
    state.loaded = true;
  },
  load(): Promise<void> {
    if (state.loaded) return Promise.resolve();
    loading ??= api
      .getSettings()
      .then((s) => this.hydrate(s))
      .catch(() => {
        state.loaded = true;
      });
    return loading;
  },
  get collapsed() {
    return state.collapsed;
  },
  toggleCollapsed() {
    state.collapsed = !state.collapsed;
    persist("fork_labels_collapsed", state.collapsed ? "on" : "off");
  },
  get revealHidden() {
    return state.revealHidden;
  },
  toggleReveal() {
    state.revealHidden = !state.revealHidden;
  },
  get hiddenCount() {
    return state.hidden.length;
  },
  /** True when this path, or a label it is nested under, is hidden. */
  isHidden(path: string): boolean {
    const p = path.toLowerCase();
    return state.hidden.some((h) => {
      const x = h.toLowerCase();
      return p === x || p.startsWith(x + "/");
    });
  },
  /** True only for a label hidden by its own entry (it can be un-hidden here). */
  isHiddenItself(path: string): boolean {
    return state.hidden.some((h) => h.toLowerCase() === path.toLowerCase());
  },
  hide(path: string) {
    if (this.isHiddenItself(path)) return;
    state.hidden = [...state.hidden, path];
    persist("fork_labels_hidden", JSON.stringify(state.hidden));
  },
  unhide(path: string) {
    state.hidden = state.hidden.filter((h) => h.toLowerCase() !== path.toLowerCase());
    if (state.hidden.length === 0) state.revealHidden = false;
    persist("fork_labels_hidden", JSON.stringify(state.hidden));
  },
};
