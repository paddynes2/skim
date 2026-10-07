// Fork (v1.1.3): the Deals view. Registers the nav hook (so `g e`, the palette
// and the sidebar open it) and keeps the list text the Settings section and
// "Add to Deals" edit. The view itself is the mail store's virtual folder -924.
import { api } from "../../lib/api";
import { mail } from "../../lib/stores/mail.svelte";
import { ui } from "../../lib/stores/ui.svelte";
import { navHooks } from "../nav";
import { DEALS_SETTING, dealsApi } from "./api";
import { dealsCount } from "./count.svelte";

const state = $state({ text: "", loaded: false });
let started = false;

/** The list changed: the open Deals view and the badge follow at once. */
function changed() {
  void dealsCount.refresh();
  if (mail.dealsView) void mail.refreshThreads();
}

export const dealsStore = {
  get text() {
    return state.text;
  },
  get loaded() {
    return state.loaded;
  },
  open() {
    ui.showMail();
    void mail.selectDeals();
  },
  async load() {
    const s = await api.getSettings().catch(() => null);
    state.text = s?.[DEALS_SETTING] ?? "";
    state.loaded = true;
  },
  async save(text: string) {
    state.text = text;
    await api.setSetting(DEALS_SETTING, text);
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
