// Fork (v1.1.3): the Deals view. Registers the nav hook (so `g e`, the palette
// and the sidebar open it) and keeps the list text the Settings section and
// "Add to Deals" edit. The view itself is the mail store's virtual folder -924.
import { api } from "../../lib/api";
import { mail } from "../../lib/stores/mail.svelte";
import { ui } from "../../lib/stores/ui.svelte";
import { navHooks } from "../nav";
import { DEALS_SETTING, dealsApi } from "./api";
import { dealsCount } from "./count.svelte";

const state = $state({ text: "", loaded: false, failed: false, selectedCompany: "" });
let started = false;

/** The list changed: the open Deals view and the badge follow at once. */
function changed() {
  void dealsCount.refresh();
  if (mail.dealsView) void mail.refreshThreads();
}

export const dealsStore = {
  get selectedCompany() { return state.selectedCompany; },
  selectCompany(name: string) { state.selectedCompany = name; },
  get text() {
    return state.text;
  },
  get loaded() {
    return state.loaded;
  },
  /** The last load failed: the text shown is not the stored list. */
  get failed() {
    return state.failed;
  },
  open() {
    ui.showMail();
    void mail.selectDeals();
  },
  /** A failed read leaves `loaded` false, so nothing can save an empty box
   *  over the stored list. */
  async load() {
    try {
      const s = await api.getSettings();
      state.text = s?.[DEALS_SETTING] ?? "";
      state.loaded = true;
      state.failed = false;
    } catch {
      state.failed = true;
    }
  },
  /** Throws when the write fails; the stored text is only updated after it lands. */
  async save(text: string) {
    if (!state.loaded) throw new Error("the Deals list was not loaded");
    await api.setSetting(DEALS_SETTING, text);
    state.text = text;
    changed();
  },
  async add(name: string, entry: string) {
    state.text = await dealsApi.add(name, entry);
    changed();
  },
  start(): () => void {
    if (started) return () => {};
    started = true;
    navHooks.deals = () => dealsStore.open();
    void dealsCount.refresh();
    void dealsStore.load();
    return () => {
      delete navHooks.deals;
      started = false;
    };
  },
};
