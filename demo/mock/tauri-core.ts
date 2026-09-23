// Mock of `@tauri-apps/api/core` for the product demo.
//
// The whole app talks to its Rust backend through `invoke()` and streams AI
// output over a `Channel`. Alias this module in place of the real one and the
// UI runs unchanged in a plain browser, served entirely fake data.

import * as db from "./data";
import { forkRenderedBody } from "./fork-bodies";
import { forkSearchThreads } from "./fork-search";
import { forkCrmLookup, forkCrmStatus } from "./fork-crm";
import { forkPrep, forkPrepUpcoming, runPrepBrief } from "./fork-prep";
import { forkSmellRewrite, SMELL_FIXTURE_DRAFT } from "./fork-smell";
import { forkComposeInvoke } from "./fork-compose";
import { forkCalendarInvoke } from "./fork-calendar";
import { forkCourtCounts, forkCourtList } from "./fork-court";

// The app checks `"__TAURI_INTERNALS__" in window` to decide whether to boot
// (vs. show onboarding). Presence is enough — our aliased invoke does the work.
(globalThis as any).window && ((window as any).__TAURI_INTERNALS__ ||= { demo: true });

// Tunables the recorder can override via localStorage before the app boots.
function num(key: string, fallback: number): number {
  const v = Number((globalThis as any).localStorage?.getItem(key));
  return Number.isFinite(v) && v > 0 ? v : fallback;
}
const TYPING_MS = () => num("skimdemo.typingMs", 24); // per token
const THINK_MS = () => num("skimdemo.thinkMs", 420); // pause before first token
const STEP_MS = () => num("skimdemo.stepMs", 650); // tool-step dwell
// Second mailbox + unified view on demand — off by default so the classic
// single-account shots stay reproducible.
const MULTI = () => {
  try {
    return (globalThis as any).localStorage?.getItem("skimdemo.multiaccount") === "on";
  } catch {
    return false;
  }
};

// ---- AI streaming --------------------------------------------------------
export class Channel<T = unknown> {
  onmessage: (msg: T) => void = () => {};
}

const cancelled = new Set<string>();

function tokens(text: string): string[] {
  // Keep whitespace attached so the streamed text reflows naturally.
  return text.match(/\S+\s*/g) ?? [text];
}

function streamText(
  channel: Channel<any>,
  text: string,
  requestId: string,
  citations: any[] = [],
  startDelay = THINK_MS(),
): void {
  const parts = tokens(text);
  let i = 0;
  const tick = () => {
    if (cancelled.has(requestId)) return;
    if (i >= parts.length) {
      channel.onmessage({ type: "done", citations });
      return;
    }
    channel.onmessage({ type: "delta", text: parts[i++] });
    setTimeout(tick, TYPING_MS());
  };
  setTimeout(tick, startDelay);
}

// The recap panel reads the unread mail before it writes a word, and renders
// that wait as "Reading your unread mail… 1/3". So the fixture has to emit
// `progress` first — text alone would skip the scan and land the digest with an
// empty "0 unread digested" eyebrow and no marked-as-read line.
function runRecap(channel: Channel<any>, requestId: string): void {
  const { text, citations } = db.AI_RECAP;
  const total = citations.length;
  let delay = 0;
  for (let i = 1; i <= total; i++) {
    setTimeout(() => {
      if (cancelled.has(requestId)) return;
      channel.onmessage({ type: "progress", current: i, total });
    }, delay);
    delay += STEP_MS();
  }
  streamText(channel, text, requestId, citations, delay + 200);
}

function runChat(channel: Channel<any>, requestId: string, turns: any[] | undefined): void {
  const { steps, answer, citations } = db.chatTurn(turns);
  let delay = 300;
  steps.forEach((s, idx) => {
    const id = `step-${idx}`;
    setTimeout(() => {
      if (cancelled.has(requestId)) return;
      channel.onmessage({ type: "toolCall", id, kind: s.kind, arg: s.arg });
    }, delay);
    delay += STEP_MS();
    setTimeout(() => {
      if (cancelled.has(requestId)) return;
      channel.onmessage({ type: "toolDone", id, count: s.count });
    }, delay);
    delay += 120;
  });
  streamText(channel, answer, requestId, citations, delay + 200);
}

const AI_COMMANDS = new Set([
  "ai_ask",
  "ai_compose",
  "ai_chat",
  "ai_recap",
  "ai_analyze_style",
  "fork_prep_brief",
]);

function handleAi(cmd: string, args: any): void {
  const channel: Channel<any> | undefined = args?.channel;
  const requestId: string = args?.requestId ?? "";
  cancelled.delete(requestId);
  if (!channel) return;

  switch (cmd) {
    // The email chat and the palette chat are both continuable: the UI sends the
    // whole history, and the fixtures answer the question that was actually
    // asked (quick prompt, opener, or follow-up).
    case "ai_ask":
      streamText(channel, db.askAnswer(args?.turns), requestId);
      return;
    case "fork_prep_brief":
      runPrepBrief(channel, requestId, (id) => cancelled.has(id), TYPING_MS(), THINK_MS());
      return;
    case "ai_recap":
      runRecap(channel, requestId);
      return;
    case "ai_chat":
      runChat(channel, requestId, args?.turns);
      return;
    case "ai_compose": {
      const isReply = args?.replyToMessageId != null;
      streamText(channel, isReply ? db.AI_REPLY : db.AI_COMPOSE_NEW, requestId);
      return;
    }
    default:
      streamText(channel, db.AI_SUMMARY, requestId);
  }
}

// Fork (3.1): the list filter / order the real queries apply, over fixtures.
function listOpts<T extends { isRead: boolean; isStarred: boolean; date: number }>(
  rows: T[],
  args: { filter?: string; order?: string },
): T[] {
  let out = rows;
  if (args.filter === "unread") out = out.filter((r) => !r.isRead);
  else if (args.filter === "starred") out = out.filter((r) => r.isStarred);
  if (args.order === "unread_first")
    out = [...out].sort((a, b) => Number(!b.isRead) - Number(!a.isRead) || b.date - a.date);
  return out;
}

// ---- Plain command surface ----------------------------------------------
export function invoke<T = any>(cmd: string, args: any = {}): Promise<T> {
  // Probes can watch which commands the UI issued: `window.__skimCalls = []`.
  (globalThis as any).__skimCalls?.push(cmd);
  if (AI_COMMANDS.has(cmd)) {
    handleAi(cmd, args);
    return Promise.resolve(undefined as T);
  }
  // Fork (6): compose + scheduler commands.
  const fc = forkComposeInvoke(cmd, args);
  if (fc) return Promise.resolve(fc.value as T);
  // Fork (7/8): Google, calendar and free-slot commands.
  const cal = forkCalendarInvoke(cmd, args);
  if (cal) return "err" in cal ? Promise.reject(cal.err) : Promise.resolve(cal.ok as T);

  const ok = <R>(v: R) => Promise.resolve(v as unknown as T);

  switch (cmd) {
    // accounts
    case "list_accounts":
      return ok(MULTI() ? [db.ACCOUNT, db.ACCOUNT2] : [db.ACCOUNT]);
    case "inbox_unread_counts":
      return ok(MULTI() ? { "acc-1": 3, "acc-2": 2 } : { "acc-1": 3 });
    case "google_oauth_available":
      return ok(false);
    case "microsoft_oauth_available":
      return ok(false);
    case "autoconfig_lookup":
      return ok(null);

    // settings — the recorder/screenshotter can force a theme via localStorage.
    // Theme is two-axis ("<cold|warm>-<light|dark>"); warm-light is the app default.
    case "get_settings": {
      let theme = "warm-light";
      try {
        theme = (globalThis as any).localStorage?.getItem("skimdemo.theme") || "warm-light";
      } catch {}
      // Fork keys: any `skimdemo.fork_*` localStorage entry is served as a
      // setting, so the screenshot harness can drive density / avatars / etc.
      const fork: Record<string, string> = {};
      try {
        const ls = (globalThis as any).localStorage;
        for (let i = 0; i < (ls?.length ?? 0); i++) {
          const k = ls.key(i) as string;
          if (k.startsWith("skimdemo.fork_")) fork[k.slice("skimdemo.".length)] = ls.getItem(k);
        }
      } catch {}
      return ok({ locale: "en", theme, images_policy: "ask", group_threads: "on", ...fork });
    }
    case "set_setting":
      return ok(undefined);

    // mail
    case "list_folders":
      return ok(MULTI() && args.accountId === "acc-2" ? db.FOLDERS2 : db.FOLDERS);
    case "list_unified_folders":
      return ok(db.UNIFIED_FOLDERS);
    case "list_unified_threads":
    case "list_unified_messages":
      return ok(args.offset > 0 ? [] : listOpts(db.unifiedList(args.role ?? null), args));
    case "folder_ref":
      return ok(db.folderRef(args.folderId));
    // Threads vs. flat messages: the app picks one based on the group_threads
    // setting. The fixtures serve for both.
    case "list_threads":
    case "list_messages": {
      const list = db.THREADS_BY_FOLDER[args.folderId] ?? [];
      return ok(args.offset > 0 ? [] : listOpts(list, args));
    }
    case "get_thread":
      return ok(db.threadDetail(args.threadId));
    case "get_message_body": {
      // Fork (5.3): `skimdemo.fork_body` swaps in a quoted-reply fixture.
      const kind = (globalThis as any).localStorage?.getItem("skimdemo.fork_body");
      if (kind === "gmail" || kind === "outlook" || kind === "plain" || kind === "forward")
        return ok(forkRenderedBody(args.messageId, kind));
      return ok(db.renderedBody(args.messageId));
    }
    case "thread_message_ids":
      return ok([args.threadId * 10 + 1]);
    case "thread_message_ids_bulk":
      return ok((args.threadIds as number[]).map((id) => id * 10 + 1));
    // Move destinations: the account's folders minus the one the mail is in
    // (Inbox, in every demo scenario) and the roles that aren't destinations.
    case "folder_message_count":
      return ok((db.THREADS_BY_FOLDER[args.folderId] ?? []).length);
    case "move_targets":
      return ok(
        (MULTI() ? db.FOLDERS2 : db.FOLDERS).filter(
          (f) => !["inbox", "starred", "drafts", "all"].includes(f.role ?? ""),
        ),
      );
    // ---- fork commands (src/fork/api.ts) ----
    case "fork_role_total":
      return ok(args.role === "starred" ? (MULTI() ? 3 : 2) : 0);
    case "fork_removal_snapshot":
      return ok([
        {
          accountId: "acc-1",
          folderId: 1,
          folderImapName: "INBOX",
          messageIds: (args.messageIds as number[]).map((id) => `<${id}@demo.example>`),
        },
      ]);
    case "fork_restore":
      return ok(undefined);
    case "fork_sync_folder":
      return ok(undefined);
    case "fork_search_threads":
      return ok(forkSearchThreads(args.query ?? "", args.offset ?? 0));
    // ---- Phase 9 CRM (demo/mock/fork-crm.ts) ----
    case "fork_crm_status":
      return ok(forkCrmStatus());
    case "fork_crm_lookup":
      return forkCrmLookup(String(args.email ?? "")) as Promise<T>;
    case "fork_crm_connect":
    case "fork_crm_pick_workspace":
    case "fork_crm_disconnect":
    case "fork_crm_set_config":
      return ok(forkCrmStatus());
    // ---- Phase 10 court ----
    case "fork_court_list":
      return ok(forkCourtList(args.courtState, args.offset ?? 0));
    case "fork_court_counts":
      return ok(forkCourtCounts());
    case "fork_court_recompute":
      return ok(undefined);
    // ---- Phase 11 prep ----
    case "fork_prep":
      return ok(forkPrep(args.eventId));
    case "fork_prep_upcoming":
      return ok(forkPrepUpcoming());
    // ---- Phase 12 MCP ----
    case "fork_mcp_status": {
      const on = (globalThis as any).localStorage?.getItem("skimdemo.fork_mcp") !== "off";
      return ok({
        enabled: on,
        listening: on,
        port: 8342,
        endpoint: "http://127.0.0.1:8342/mcp",
        token: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        addCommand:
          'claude mcp add --transport http skim http://127.0.0.1:8342/mcp --header "Authorization: Bearer 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" --scope user',
      });
    }
    case "fork_mcp_set_enabled": {
      const on = !!args.on;
      (globalThis as any).localStorage?.setItem("skimdemo.fork_mcp", on ? "on" : "off");
      return ok({
        enabled: on,
        listening: on,
        port: 8342,
        endpoint: "http://127.0.0.1:8342/mcp",
        token: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        addCommand:
          'claude mcp add --transport http skim http://127.0.0.1:8342/mcp --header "Authorization: Bearer 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" --scope user',
      });
    }
    // ---- Phase 6.5 smell ----
    case "fork_smell_rewrite":
      return forkSmellRewrite(args, args.channel) as Promise<T>;
    case "take_pending_open":
      return ok(null);
    case "search_messages":
      return ok(db.searchHits(args.query ?? ""));

    // compose
    case "create_draft":
      return ok(db.createDraft());
    case "update_account_identity":
      return ok(db.updateAccountIdentity(args.accountId, args.displayName, args.signature));
    case "set_draft_account":
      return ok(db.setDraftAccount(args.draftId, args.accountId, args.body));
    case "get_draft":
      return ok(db.getDraft(args.draftId));
    case "get_reply_template": {
      const tpl = db.replyTemplate(args.messageId, args.mode);
      // Fork (6.5): the AI-tell fixture draft for the screenshot harness.
      if ((globalThis as any).localStorage?.getItem("skimdemo.fork_smell_fixture") === "on")
      {
        const withTells = { ...tpl, body: SMELL_FIXTURE_DRAFT };
        db.updateDraft(withTells);
        return ok(withTells);
      }
      return ok(tpl);
    }
    case "update_draft":
      db.updateDraft(args.draft);
      return ok(undefined);
    case "list_draft_attachments":
      return ok([]);
    case "suggest_addresses":
      return ok([]);

    // AI key gate — the demo always has a key so AI actions are visible.
    case "ai_key_status":
      return ok({ provider: "anthropic", anthropic: true, openrouter: false, custom: false });
    case "ai_cancel":
      cancelled.add(args.requestId);
      return ok(undefined);

    // Fire-and-forget mutations: mark read, star, archive, delete, send…
    default:
      return ok(undefined);
  }
}

export const convertFileSrc = (p: string) => p;
export const transformCallback = (cb: (r: any) => void) => cb;
export const isTauri = () => true;
export const PluginListener = class {};
