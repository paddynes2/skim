// Phase 9: the CRM drawer's state, the focused address it follows and the
// `i` key registration. The drawer itself (`CrmDrawer.svelte`) renders this.
//
// Who sets the address: the reading pane knows which message is focused; it
// calls `crmFocus.setEmail(focused?.from.addr ?? null)` from an effect (one
// touch, see docs/fork/pending/9.md "## App.svelte"). The drawer looks the
// address up when it is open, and again when the address changes.
import { navHooks } from "../nav";
import { prefs } from "../stores/prefs.svelte";
import { crmApi } from "./api";
import { CRM_SETUP_CODES, type CrmError, type CrmLookup, type CrmStatus } from "./types";

const state = $state({
  email: null as string | null,
  status: null as CrmStatus | null,
  lookup: null as CrmLookup | null,
  /** Which address `lookup` belongs to. */
  lookupFor: null as string | null,
  loading: false,
  /** A failure code from the last call (`crm_http`, ...) or null. */
  error: null as CrmError | null,
});

let seq = 0;

function asError(e: unknown): CrmError {
  if (e && typeof e === "object" && "code" in e && "message" in e) {
    return { code: String((e as CrmError).code), message: String((e as CrmError).message) };
  }
  return { code: "crm_http", message: String(e) };
}

export const crmFocus = {
  get email() {
    return state.email;
  },
  /** The reading pane reports the focused message's sender here. */
  setEmail(addr: string | null) {
    const next = addr?.trim().toLowerCase() || null;
    if (next === state.email) return;
    state.email = next;
    if (prefs.crmDrawer) void crm.refresh();
  },
};

export const crm = {
  get status() {
    return state.status;
  },
  get lookup() {
    return state.lookupFor === state.email ? state.lookup : null;
  },
  get loading() {
    return state.loading;
  },
  get error() {
    return state.error;
  },
  /** True when the drawer should show its "Connect" state instead of a card. */
  get needsSetup() {
    const s = state.status;
    if (s && (!s.configured || !s.connected || !s.workspaceId)) return true;
    return state.error !== null && CRM_SETUP_CODES.has(state.error.code);
  },
  /** Base URL for "Open in Rebound" links. */
  get baseUrl() {
    return state.status?.baseUrl ?? "https://rebound.patricknesbitt.ai";
  },

  async loadStatus(): Promise<void> {
    try {
      state.status = await crmApi.status();
    } catch (e) {
      state.status = null;
      state.error = asError(e);
    }
  },

  /** Look the current address up (served from Rust's cache when fresh). */
  async refresh(): Promise<void> {
    const email = state.email;
    const my = ++seq;
    state.error = null;
    if (!state.status) await this.loadStatus();
    if (my !== seq) return;
    if (!email || this.needsSetup) {
      state.lookup = null;
      state.lookupFor = email;
      return;
    }
    state.loading = true;
    try {
      const found = await crmApi.lookup(email);
      if (my !== seq) return;
      state.lookup = found;
      state.lookupFor = email;
    } catch (e) {
      if (my !== seq) return;
      state.lookup = null;
      state.lookupFor = email;
      state.error = asError(e);
      // A setup error means the status we hold is stale (token revoked, ...).
      if (CRM_SETUP_CODES.has(state.error.code)) void this.loadStatus();
    } finally {
      if (my === seq) state.loading = false;
    }
  },

  /** Settings changed the connection: forget what we showed and re-read. */
  invalidate(status: CrmStatus | null = null): void {
    state.status = status;
    state.lookup = null;
    state.lookupFor = null;
    state.error = null;
    if (prefs.crmDrawer) void this.refresh();
  },

  toggle(): void {
    prefs.setCrmDrawer(!prefs.crmDrawer);
    if (prefs.crmDrawer) void this.refresh();
  },
};

/** Wire the `i` key (src/fork/keys.ts calls `navHooks.crmToggle`). Call once
 *  from App.svelte's script (module level is fine; it only assigns a hook). */
export function registerCrmNav(): void {
  navHooks.crmToggle = () => crm.toggle();
}
