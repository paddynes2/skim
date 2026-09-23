// Fork preferences (the `fork_*` settings keys, see src-tauri/src/fork/settings.rs).
// Hydrated from `get_settings` at boot, written back through `set_setting`.
import { api } from "../../lib/api";

export type Density = "comfortable" | "compact";
export type AfterArchive = "next" | "previous" | "list";
export type ListOrder = "date" | "unread_first";

const state = $state({
  density: "comfortable" as Density,
  avatars: false,
  zoom: 1,
  afterArchive: "next" as AfterArchive,
  listOrder: "date" as ListOrder,
  replyInline: true,
  richText: true,
  undoSendSecs: 10,
  smell: true,
  smellBlockHard: true,
  smellIgnored: [] as string[],
  crmDrawer: false,
  mcp: true,
});

export const ZOOM_MIN = 0.8;
export const ZOOM_MAX = 1.5;
export const ZOOM_STEP = 0.1;

function clampZoom(v: number): number {
  if (!Number.isFinite(v)) return 1;
  return Math.round(Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, v)) * 100) / 100;
}

function persist(key: string, value: string) {
  void api.setSetting(key, value).catch(() => {});
}

export const prefs = {
  /** Apply a settings map (boot path). Unknown or absent keys keep defaults. */
  hydrate(s: Record<string, string>) {
    if (s.fork_density === "compact" || s.fork_density === "comfortable") state.density = s.fork_density;
    if (s.fork_avatars) state.avatars = s.fork_avatars === "on";
    if (s.fork_zoom) state.zoom = clampZoom(Number(s.fork_zoom));
    if (s.fork_after_archive === "next" || s.fork_after_archive === "previous" || s.fork_after_archive === "list")
      state.afterArchive = s.fork_after_archive;
    if (s.fork_list_order === "date" || s.fork_list_order === "unread_first") state.listOrder = s.fork_list_order;
    if (s.fork_reply_inline) state.replyInline = s.fork_reply_inline !== "off";
    if (s.fork_rich_text) state.richText = s.fork_rich_text !== "off";
    if (s.fork_undo_send_secs) {
      const n = Number(s.fork_undo_send_secs);
      if ([0, 5, 10, 20, 30].includes(n)) state.undoSendSecs = n;
    }
    if (s.fork_smell) state.smell = s.fork_smell !== "off";
    if (s.fork_smell_block_hard) state.smellBlockHard = s.fork_smell_block_hard !== "off";
    if (s.fork_smell_ignored) {
      try {
        const list = JSON.parse(s.fork_smell_ignored);
        if (Array.isArray(list)) state.smellIgnored = list.filter((x) => typeof x === "string");
      } catch {
        // a broken value is an empty list
      }
    }
    if (s.fork_crm_drawer) state.crmDrawer = s.fork_crm_drawer === "open";
    if (s.fork_mcp) state.mcp = s.fork_mcp !== "off";
  },

  get density() {
    return state.density;
  },
  setDensity(d: Density) {
    state.density = d;
    persist("fork_density", d);
  },
  get avatars() {
    return state.avatars;
  },
  setAvatars(on: boolean) {
    state.avatars = on;
    persist("fork_avatars", on ? "on" : "off");
  },
  get zoom() {
    return state.zoom;
  },
  /** Store a zoom level (the caller applies it to the webview, see fork/zoom.ts). */
  setZoom(v: number) {
    state.zoom = clampZoom(v);
    persist("fork_zoom", String(state.zoom));
    return state.zoom;
  },
  get afterArchive() {
    return state.afterArchive;
  },
  setAfterArchive(v: AfterArchive) {
    state.afterArchive = v;
    persist("fork_after_archive", v);
  },
  get listOrder() {
    return state.listOrder;
  },
  setListOrder(v: ListOrder) {
    state.listOrder = v;
    persist("fork_list_order", v);
  },
  get replyInline() {
    return state.replyInline;
  },
  setReplyInline(on: boolean) {
    state.replyInline = on;
    persist("fork_reply_inline", on ? "on" : "off");
  },
  get richText() {
    return state.richText;
  },
  setRichText(on: boolean) {
    state.richText = on;
    persist("fork_rich_text", on ? "on" : "off");
  },
  get undoSendSecs() {
    return state.undoSendSecs;
  },
  setUndoSendSecs(n: number) {
    state.undoSendSecs = n;
    persist("fork_undo_send_secs", String(n));
  },
  get smell() {
    return state.smell;
  },
  setSmell(on: boolean) {
    state.smell = on;
    persist("fork_smell", on ? "on" : "off");
  },
  get smellBlockHard() {
    return state.smellBlockHard;
  },
  setSmellBlockHard(on: boolean) {
    state.smellBlockHard = on;
    persist("fork_smell_block_hard", on ? "on" : "off");
  },
  get smellIgnored() {
    return state.smellIgnored;
  },
  setSmellIgnored(list: string[]) {
    state.smellIgnored = [...new Set(list)];
    persist("fork_smell_ignored", JSON.stringify(state.smellIgnored));
  },
  get crmDrawer() {
    return state.crmDrawer;
  },
  setCrmDrawer(open: boolean) {
    state.crmDrawer = open;
    persist("fork_crm_drawer", open ? "open" : "closed");
  },
  get mcp() {
    return state.mcp;
  },
  setMcp(on: boolean) {
    state.mcp = on;
    persist("fork_mcp", on ? "on" : "off");
  },
};
