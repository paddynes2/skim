<script lang="ts">
  // The compose/edit surface, decoupled from any window. Rendered both inside
  // its own native window (Composer.svelte, `chrome` on) and inline in the
  // reading pane for editing a draft from the Drafts folder (`chrome` off).
  import { onDestroy } from "svelte";
  import { aiStream, api, errorMessage } from "../lib/api";
  import { t } from "../lib/i18n/index.svelte";
  import { createSlowStart } from "../lib/slow-start.svelte";
  import { ai } from "../lib/stores/ai.svelte";
  import type { Account, Draft, DraftAttachment } from "../lib/types";
  import AddressInput from "./AddressInput.svelte";
  import WindowControls from "./WindowControls.svelte";
  // Fork (6.1-6.4): rich text, inline reply, send safety.
  import { untrack } from "svelte";
  import RichEditor from "../fork/RichEditor.svelte";
  import SendLater from "../fork/compose/SendLater.svelte";
  import { forkComposeApi } from "../fork/compose/api";
  import { htmlToText, textToHtml } from "../fork/compose/html";
  import type { WhenPick } from "../fork/compose/when";
  import { prefs } from "../fork/stores/prefs.svelte";
  // Fork (6.5): AI-tell underlines, health line, send check.
  import { attachSmell, type SmellMount } from "../fork/smell/highlight";
  import { rules as smellRules } from "../fork/smell/rules";
  import { scan, ownWords, mustFix, type ScanResult, type Span } from "../fork/smell/scan";
  import { loadSmellSettings, type SmellSettings } from "../fork/smell/api";
  import SmellPopover from "../fork/smell/SmellPopover.svelte";
  import SmellHealth from "../fork/smell/SmellHealth.svelte";
  import SmellSendPrompt from "../fork/smell/SmellSendPrompt.svelte";
  // Fork (7.6 / 8): Meet link and availability, inserted at the caret.
  import { openSlotsPopover, isSlotsTrigger } from "../fork/slots/slots";
  import { addMeetLink } from "../fork/calendar/meet";

  let {
    draftId,
    chrome = false,
    variant = "default",
    onSent,
    onDiscarded,
    onClose,
    onLocalSave,
    onPopOut,
  }: {
    draftId: number;
    /** Render the native-window titlebar (min/max/close). Off when inline. */
    chrome?: boolean;
    /** Fork (6.2): `reply` = inline under a message in the reading pane (no
     *  From picker, a pop-out button, Esc closes). */
    variant?: "default" | "reply";
    onSent?: () => void;
    onDiscarded?: () => void;
    onClose?: () => void;
    /** Fired after a debounced local autosave so a host can patch its list. */
    onLocalSave?: (draft: Draft) => void;
    /** Fork (6.2): the inline reply moved to its own window; unmount this copy. */
    onPopOut?: () => void;
  } = $props();

  let draft = $state<Draft | null>(null);
  let showCc = $state(false);
  let sending = $state(false);
  let error = $state("");
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  // Once the draft has been sent or discarded, teardown must not resurrect it
  // with a write-back.
  let settled = false;
  // Whether the user actually changed anything. Guards the server write-back so
  // merely opening a draft and leaving never rewrites it (which would, for an
  // HTML draft, flatten it to plain text on the server).
  let dirty = false;
  // Explicit-save state for the standalone compose window (`chrome`): 'clean'
  // shows nothing, 'dirty' shows the Save button, 'saved' shows "Saved". Editing
  // after a save flips it back to 'dirty'. Unused in the inline editor.
  let saveState = $state<"clean" | "dirty" | "saved">("clean");
  // Set once the draft has been committed to the Drafts folder, so closing the
  // window keeps it instead of dropping the (now real) draft.
  let committed = $state(false);
  // Fork (6.2): what the draft said when it was loaded. `dirty` alone is not
  // proof of an edit: the rich editor reports its first render as input, so
  // an untouched inline reply looked edited and Esc saved it to Drafts.
  let loadedShape = "";
  function shapeOf(d: Draft | null): string {
    return d ? JSON.stringify([d.to, d.cc, d.bcc, d.subject, d.body]) : "";
  }
  function edited(): boolean {
    return dirty && shapeOf(draft) !== loadedShape;
  }

  // For the From picker (several mailboxes, fresh compose only). Replies keep
  // the account of the message they answer; once a draft is committed to a
  // server Drafts folder, moving it would orphan that copy — picker hides.
  let accounts = $state<Account[]>([]);
  const canPickFrom = $derived(
    accounts.length > 1 &&
      draft?.mode === "new" &&
      draft?.originMessageId === null &&
      !committed &&
      variant !== "reply",
  );

  // ---- Fork (6.3): rich text ----
  // Decided once per draft when it loads (a mid-edit toggle would have to
  // convert the text under the cursor). Off: the plain textarea, as upstream.
  let richMode = $state(false);
  // The user's own words as HTML (what fork_draft_html holds) and the plain
  // text the editor last produced for them; `tail` is everything below the
  // words in `draft.body` (signature block, quoted original), which the
  // editor never touches. body_text = wordsText + tail, always.
  let wordsHtml = $state("");
  let wordsText = "";
  let tail = $state("");
  let tailEditing = $state(false);
  let quoteOpen = $state(false);
  let editorRef = $state<RichEditor | null>(null);
  let bodyEl = $state<HTMLTextAreaElement | null>(null);
  let rootEl = $state<HTMLDivElement | null>(null);

  /** The signature block and the quote, split for the read-only rendering. */
  const tailParts = $derived.by(() => {
    const q = tail.indexOf("\n\nOn ");
    const quoteAt = q >= 0 && tail.slice(q).includes(" wrote:\n") ? q : -1;
    const sigText = quoteAt >= 0 ? tail.slice(0, quoteAt) : tail;
    const quote = quoteAt >= 0 ? tail.slice(quoteAt).replace(/^\n+/, "") : "";
    const sig = sigText.startsWith(SIG_MARK) ? sigText.slice(SIG_MARK.length) : sigText.trim();
    const nl = quote.indexOf("\n");
    return {
      sig,
      attribution: nl >= 0 ? quote.slice(0, nl) : quote,
      quoted: nl >= 0 ? quote.slice(nl + 1) : "",
    };
  });

  /** The editor changed the words: keep body_text in step. */
  function onWordsChange(html: string, text: string) {
    wordsHtml = html;
    wordsText = text;
    if (draft) draft.body = text + tail;
    scheduleSave();
  }

  /** The user edits the signature / quote as plain text. */
  function onTailInput() {
    if (draft) draft.body = wordsText + tail;
    scheduleSave();
  }

  // `draft.body` changed under the editor (AI draft, revert, From switch):
  // re-split and show the new words. An edit from the editor itself already
  // matches `wordsText`, so it never round-trips through setHTML.
  $effect(() => {
    const body = draft?.body;
    if (!richMode || body == null) return;
    untrack(() => {
      const [w, tl] = splitTail(body);
      if (w !== wordsText) {
        wordsText = w;
        wordsHtml = textToHtml(w);
        editorRef?.setHTML(wordsHtml);
      }
      if (tl !== tail) tail = tl;
    });
  });

  /** Persist the words as HTML (or drop the row in plain mode). */
  async function flushHtml() {
    if (!draft) return;
    await forkComposeApi.draftHtmlSet(draft.id, richMode ? wordsHtml : null).catch(() => {});
  }

  // ---- Fork (6.1): discard with a second click ----
  let discardArmed = $state(false);
  let discardTimer: ReturnType<typeof setTimeout> | null = null;
  function discardClick() {
    if (discardArmed) {
      discardArmed = false;
      if (discardTimer) clearTimeout(discardTimer);
      void discard();
      return;
    }
    discardArmed = true;
    if (discardTimer) clearTimeout(discardTimer);
    discardTimer = setTimeout(() => (discardArmed = false), 3000);
  }

  async function changeFrom(accountId: string) {
    if (!draft || accountId === draft.accountId) return;
    try {
      // The signature belongs to the mailbox, so the backend swaps it and hands
      // back the rewritten body — an untouched block only, never a hand edit.
      // Send what the editor holds, not what the database last saw: the save is
      // debounced, so a mailbox switched mid-sentence would otherwise come back
      // with the unsaved words rewritten away.
      const moved = await api.setDraftAccount(draft.id, accountId, draft.body);
      draft.accountId = accountId;
      draft.body = moved.body;
    } catch (e) {
      error = errorMessage(e);
    }
  }

  /** Mark an edit: guards the inline write-back and drives the window Save state. */
  function markDirty() {
    dirty = true;
    saveState = "dirty";
  }

  // ---- Attachments ----
  const MAX_ATTACHMENT_BYTES = 25 * 1024 * 1024;
  let attachments = $state<DraftAttachment[]>([]);
  let dragActive = $state(false);
  let fileInput = $state<HTMLInputElement | null>(null);

  function formatSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
    return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  }

  /** Read each file's bytes and stage it on the draft. Shared by the paperclip
   *  button and drag & drop. */
  async function attachFiles(files: FileList | File[] | null) {
    if (!draft || !files) return;
    for (const file of Array.from(files)) {
      if (file.size > MAX_ATTACHMENT_BYTES) {
        error = t("compose.attach_too_large");
        continue;
      }
      try {
        const bytes = Array.from(new Uint8Array(await file.arrayBuffer()));
        const meta = await api.addDraftAttachment(
          draft.id,
          file.name,
          file.type || "application/octet-stream",
          bytes,
        );
        attachments.push(meta);
        markDirty();
      } catch (e) {
        error = errorMessage(e);
      }
    }
  }

  async function pickFiles(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    await attachFiles(input.files);
    input.value = ""; // allow re-picking the same file
  }

  async function removeAttachment(id: number) {
    try {
      await api.removeDraftAttachment(id);
      attachments = attachments.filter((a) => a.id !== id);
      markDirty();
    } catch (e) {
      error = errorMessage(e);
    }
  }

  function onDrop(e: DragEvent) {
    e.preventDefault();
    dragActive = false;
    void attachFiles(e.dataTransfer?.files ?? null);
  }

  /** Pasted screenshots and copied images arrive as unnamed blobs — give them a
   *  sensible filename so the attachment chip and the sent MIME part aren't blank. */
  let pasteSeq = 0;
  function named(file: File): File {
    if (file.name) return file;
    const ext = (file.type.split("/")[1] || "bin").split("+")[0];
    return new File([file], `pasted-${++pasteSeq}.${ext}`, { type: file.type });
  }

  /** Ctrl/Cmd+V inside the compose surface: if the clipboard carries files or an
   *  image (e.g. a screenshot), attach them; otherwise let the paste fall
   *  through so text lands in the field as usual. */
  function onPaste(e: ClipboardEvent) {
    const data = e.clipboardData;
    if (!data) return;
    // Prefer explicit files; fall back to image/file items (screenshots have no
    // entry in `.files` on some platforms, only in `.items`).
    let files = Array.from(data.files);
    if (files.length === 0) {
      files = Array.from(data.items)
        .filter((it) => it.kind === "file")
        .map((it) => it.getAsFile())
        .filter((f): f is File => f !== null);
    }
    if (files.length === 0) return; // plain text — leave the paste alone
    e.preventDefault();
    void attachFiles(files.map(named));
  }

  // ---- AI drafting ----
  let aiAvailable = $state(false);

  // ---- Fork (6.5): AI-tell layer ----
  let smell: SmellMount | null = null;
  let smellSettings = $state<SmellSettings | null>(null);
  let smellResult = $state<ScanResult | null>(null);
  let smellPop = $state<{ span: Span; rect: DOMRect } | null>(null);
  const smellIgnoredOnce = new Set<string>();
  let smellSendPrompt = $state(false);
  let smellPendingWhen: WhenPick | null = null;

  // ---- Fork (7.6 / 8): insert at the caret in either editor ----
  function insertAtCaret(text: string) {
    if (!draft) return;
    if (richMode && editorRef) {
      editorRef.insertText(text);
      return;
    }
    const el = bodyEl;
    const at = el ? el.selectionStart : draft.body.length;
    const end = el ? el.selectionEnd : at;
    draft.body = draft.body.slice(0, at) + text + draft.body.slice(end);
    scheduleSave();
    requestAnimationFrame(() => {
      el?.focus();
      el?.setSelectionRange(at + text.length, at + text.length);
    });
  }
  function shareAvailability() {
    openSlotsPopover((text) => insertAtCaret(text));
  }
  /** `/slots` typed at a line start in the plain editor opens the popover. */
  function onBodyInputSlots() {
    const el = bodyEl;
    if (!el || !draft) return;
    const before = draft.body.slice(0, el.selectionStart);
    if (!isSlotsTrigger(before)) return;
    const at = el.selectionStart - 6;
    draft.body = draft.body.slice(0, at) + draft.body.slice(el.selectionStart);
    requestAnimationFrame(() => el.setSelectionRange(at, at));
    shareAvailability();
  }
  $effect(() => {
    void loadSmellSettings().then((s) => (smellSettings = s)).catch(() => {});
  });
  $effect(() => {
    const el: HTMLElement | null = richMode ? (editorRef?.hostEl() ?? null) : bodyEl;
    const settings = smellSettings;
    if (!el || !settings?.enabled) return;
    const mount = attachSmell(el, {
      scan: (text) => {
        const r = scan(text, smellRules(), { ignoredRules: settings.ignored });
        r.spans = r.spans.filter((s) => !smellIgnoredOnce.has(`${s.start}:${s.rule}`));
        return r;
      },
      getText: richMode ? undefined : () => ownWords(untrack(() => draft?.body) ?? "").text,
      onOpen: (span, rect) => (smellPop = { span, rect }),
      onResult: (r) => (smellResult = r),
    });
    smell = mount;
    mount.rescan();
    return () => {
      mount.destroy();
      if (smell === mount) smell = null;
    };
  });
  let instruction = $state("");
  let instrEl = $state<HTMLTextAreaElement | null>(null);

  // Keep the AI instruction box fitted to its content on every value change —
  // typing, the send-time clear, an error restore. An $effect (not a `use:`
  // action) so it runs *after* bind:value has written the new text into the
  // DOM; measuring scrollHeight before that write leaves the box stuck tall.
  $effect(() => {
    const el = instrEl;
    void instruction; // track typed + programmatic changes
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${el.scrollHeight}px`;
  });
  let aiBusy = $state(false);
  let cancelAi: (() => void) | null = null;
  const slowStart = createSlowStart();
  /** The RFC 3676 sign-off delimiter Rust puts above a signature — the marker
   *  that tells the co-author where the user's own words stop. */
  const SIG_MARK = "\n\n-- \n";
  /** Signature and quoted original, preserved below AI-generated text. */
  let quotedTail = "";

  // The AI drafting session: one shared conversation the user refines turn by
  // turn. User turns are instructions; assistant turns are the drafts the AI
  // produced. The newest assistant turn mirrors the editable body.
  type ChatTurn = { role: "user" | "assistant"; content: string };
  let convo = $state<ChatTurn[]>([]);
  const userTurns = $derived(convo.filter((tn) => tn.role === "user"));
  const hasDraft = $derived(convo.some((tn) => tn.role === "assistant"));

  // Replies keep the client's automatic "Re:" subject — the AI drafts a subject
  // only for new mail. `subjectAuto` stays true while the subject is AI-owned;
  // a manual edit flips it off so the co-author stops overwriting it.
  const isReply = $derived(draft?.replyToMessageId != null);
  let subjectAuto = $state(true);

  /** Split raw model output into { subject, body }. New-email drafts lead with
   *  a `Subject:` header; if it's absent (or this is a reply) the whole text is
   *  the body and the subject is left untouched. */
  function parseDraft(raw: string): { subject: string | null; body: string } {
    const nl = raw.indexOf("\n");
    const firstLine = nl >= 0 ? raw.slice(0, nl) : raw;
    const m = firstLine.match(/^\s*subject\s*:(.*)$/i);
    if (!m) return { subject: null, body: raw };
    const subject = m[1].trim();
    const body = nl >= 0 ? raw.slice(nl + 1).replace(/^\n+/, "") : "";
    return { subject, body };
  }

  // The composer runs in its own window with its own copy of the `ai` store;
  // refresh it here so this webview sees the active provider and key too.
  $effect(() => {
    void ai.refresh().then(() => (aiAvailable = ai.keyPresent));
  });

  /** Split the body into the part being written and the tail that must survive
   *  a rewrite: the signature block, the quoted original, or both. The AI
   *  co-author replaces everything above the split and nothing below it. */
  function splitTail(body: string): [string, string] {
    const quote = body.indexOf("\n\nOn ");
    const marks = [
      body.indexOf(SIG_MARK),
      quote >= 0 && body.slice(quote).includes(" wrote:\n") ? quote : -1,
    ].filter((i) => i >= 0);
    if (marks.length === 0) return [body, ""];
    const idx = Math.min(...marks);
    return [body.slice(0, idx), body.slice(idx)];
  }

  /** Push an instruction into the session and stream the revised draft.
   *  `fromInput` marks instructions typed into the box (as opposed to the tone
   *  chips) so we can clear that box — but only once a draft actually lands. */
  function sendInstruction(text: string, fromInput = false) {
    if (!draft || aiBusy) return;
    const instr = text.trim();
    if (!instr) return;

    // Respect manual edits: sync what the user currently sees back into the last
    // assistant turn, so the AI revises exactly that. For new mail the turn
    // carries the subject header too, so a hand-edited subject is fed back.
    const [current, tail] = splitTail(draft.body);
    quotedTail = tail;
    if (current.trim()) {
      const synced = isReply
        ? current.trim()
        : `Subject: ${draft.subject}\n\n${current.trim()}`;
      for (let i = convo.length - 1; i >= 0; i--) {
        if (convo[i].role === "assistant") {
          convo[i] = { role: "assistant", content: synced };
          break;
        }
      }
    }

    runCompose(instr, fromInput);
  }

  function runCompose(instr: string, fromInput: boolean) {
    if (!draft) return;
    cancelAi?.();
    aiBusy = true;
    error = "";
    draft.body = quotedTail;
    slowStart.arm();
    let streamed = "";
    // The pending instruction rides along with the settled history for the
    // request, but it isn't committed to `convo` until it succeeds — so a
    // failed or empty round leaves the user's text untouched in the (disabled)
    // input, ready to retry or edit.
    const turns = [
      ...convo.map((tn) => ({ role: tn.role, content: tn.content })),
      { role: "user" as const, content: instr },
    ];
    cancelAi = aiStream(
      "ai_compose",
      {
        turns,
        replyToMessageId: draft.replyToMessageId,
      },
      {
        // The model started reasoning: it is working, not still loading.
        reasoning: () => slowStart.clear(),
        delta: (text) => {
          slowStart.clear();
          streamed += text;
          if (!draft) return;
          // Replies stream straight into the body; new mail has its subject
          // parsed off the first line so the field fills before the body does.
          let body = streamed;
          if (!isReply) {
            const parsed = parseDraft(streamed);
            body = parsed.body;
            if (subjectAuto && parsed.subject !== null) draft.subject = parsed.subject;
          }
          draft.body = quotedTail ? `${body}\n${quotedTail}` : body;
        },
        done: () => {
          slowStart.clear();
          const text = streamed.trim();
          if (text) {
            // Success — commit the exchange to the session and (only now) clear
            // the box if this instruction came from it.
            convo.push({ role: "user", content: instr });
            convo.push({ role: "assistant", content: text });
            if (fromInput) instruction = "";
          } else {
            // Nothing usable came back — leave the instruction sitting in the
            // input (never committed, never cleared) and just say so.
            error = t("ai.empty_response");
            if (draft && quotedTail && !draft.body) draft.body = quotedTail;
          }
          aiBusy = false;
          scheduleSave();
        },
        error: (_code, message) => {
          slowStart.clear();
          aiBusy = false;
          error = message;
          // The instruction was never committed or cleared, so it's still in
          // the input for the user to retry or fix — nothing to roll back.
          if (draft && quotedTail && !draft.body) draft.body = quotedTail;
        },
      },
    );
  }

  // The tone chips are just quick instructions fed into the same session; the
  // localized label doubles as the instruction and shows in the transcript.
  function adjust(kind: "shorter" | "warmer" | "formal") {
    sendInstruction(t(`ai.${kind}`));
  }

  /** Restore an earlier variant and drop every iteration after it (linear history —
   *  no branching). `i` indexes userTurns; its variant is the matching assistant turn. */
  function revertTo(i: number) {
    if (!draft || aiBusy) return;
    // userTurns[i] pairs with the i-th assistant turn. convo is strictly
    // alternating (turns are pushed as user+assistant pairs in `done`), so the
    // assistant turn sits at 2*i+1. Guard defensively rather than trust the index.
    const ai = 2 * i + 1;
    const turn = convo[ai];
    if (!turn || turn.role !== "assistant") return;

    // Preserve whatever quoted original is currently below the editable text.
    const [, tail] = splitTail(draft.body);
    quotedTail = tail;

    let body = turn.content;
    if (!isReply) {
      const parsed = parseDraft(turn.content);
      body = parsed.body;
      if (subjectAuto && parsed.subject !== null) draft.subject = parsed.subject;
    }
    draft.body = tail ? `${body}\n${tail}` : body;

    // Truncate: keep this variant as the new head, discard all following iterations.
    convo = convo.slice(0, ai + 1);
    scheduleSave();
  }

  function stopAi() {
    cancelAi?.();
    aiBusy = false;
    slowStart.clear();
    // The pending instruction was never committed to the session, so it stays
    // put in the input — nothing to roll back.
  }

  $effect(() => {
    void (async () => {
      try {
        const d = await api.getDraft(draftId);
        // Fork (6.3): the words as HTML, when the rich editor wrote them and
        // they still say what body_text says (a plain-mode edit elsewhere
        // wins; the row is rebuilt from the text).
        richMode = prefs.richText;
        if (richMode) {
          const [w, tl] = splitTail(d.body);
          wordsText = w;
          tail = tl;
          const stored = await forkComposeApi.draftHtmlGet(draftId).catch(() => null);
          wordsHtml = typeof stored === "string" && htmlToText(stored) === w ? stored : textToHtml(w);
        }
        draft = d;
        loadedShape = shapeOf(d);
        showCc = d.cc.length > 0 || d.bcc.length > 0;
        attachments = await api.listDraftAttachments(draftId);
        accounts = await api.listAccounts();
        // Fork (6.2): an inline reply comes into view and takes the cursor.
        if (variant === "reply") {
          requestAnimationFrame(() => {
            rootEl?.scrollIntoView({ block: "nearest", behavior: "smooth" });
            if (editorRef) editorRef.focus();
            else bodyEl?.focus();
          });
        }
      } catch (e) {
        error = errorMessage(e);
      }
    })();
  });

  function scheduleSave() {
    markDirty();
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(async () => {
      if (!draft) return;
      const snapshot = $state.snapshot(draft) as Draft;
      await api.updateDraft(snapshot);
      await flushHtml();
      // Let an inline host reflect the edit in its list row (subject/preview).
      onLocalSave?.(snapshot);
    }, 800);
  }

  /** Persist to the server: for a Drafts-folder draft this queues the write-back
   *  to the IMAP Drafts folder; for a local-only draft it just saves locally. */
  async function flushServer() {
    // Nothing the user changed → don't rewrite the server copy.
    if (!draft || !edited()) return;
    if (saveTimer) {
      clearTimeout(saveTimer);
      saveTimer = null;
    }
    try {
      await api.saveServerDraft($state.snapshot(draft) as Draft);
    } catch {
      // Best effort — the local autosave already captured the text.
    }
  }

  /** Send now (held for the undo window, 6.4) or at `when` (send later). */
  async function send(when: WhenPick | null = null, smellOverride = false) {
    if (!draft || sending || !draft.to.trim()) return;
    // Fork (6.5): must-fix items (dashes, placeholders, invisible characters)
    // stop the send once; "Send anyway" passes the override.
    if (!smellOverride && smellSettings?.blockHard && smell && mustFix(smell.rescan()).length) {
      smellPendingWhen = when;
      smellSendPrompt = true;
      return;
    }
    smellSendPrompt = false;
    sending = true;
    error = "";
    try {
      if (saveTimer) clearTimeout(saveTimer);
      await api.updateDraft($state.snapshot(draft) as Draft);
      await flushHtml();
      // Fork (6.4): the hold. A scheduled send names its moment; a plain send
      // is held for the undo window (0 = straight out, as upstream).
      const undoSecs = prefs.undoSendSecs;
      const notBefore = when ? when.at : undoSecs > 0 ? Math.floor(Date.now() / 1000) + undoSecs : null;
      await api.sendDraft(draft.id, notBefore, when?.label ?? null);
      settled = true;
      // Remember the mailbox for the next fresh compose in the unified view.
      void api.setSetting("last_from_account", draft.accountId).catch(() => {});
      onSent?.();
    } catch (e) {
      error = errorMessage(e);
      sending = false;
    }
  }

  async function discard() {
    settled = true;
    if (draft) await api.deleteDraft(draft.id).catch(() => {});
    onDiscarded?.();
  }

  /** Explicit save from the compose window: commit the draft to the Drafts
   *  folder so it becomes a real, reopenable message. Optimistic — if offline
   *  the queued op drains later. */
  async function save() {
    if (!draft || saveState !== "dirty") return;
    if (saveTimer) {
      clearTimeout(saveTimer);
      saveTimer = null;
    }
    const snapshot = $state.snapshot(draft) as Draft;
    try {
      await api.updateDraft(snapshot);
      await api.saveServerDraft(snapshot);
      dirty = false;
      committed = true;
      saveState = "saved";
    } catch (e) {
      error = errorMessage(e);
    }
  }

  function onWindowKeydown(e: KeyboardEvent) {
    // Ctrl/Cmd+S saves the compose window's draft. `e.code` so it works on any
    // keyboard layout. Only the standalone window handles this (the inline
    // editor lives under App.svelte's global shortcuts).
    if (chrome && (e.ctrlKey || e.metaKey) && e.code === "KeyS") {
      e.preventDefault();
      void save();
    }
  }

  /** Fork (6.1): Ctrl/Cmd+Enter in the subject or the body sends. */
  function sendKey(e: KeyboardEvent): boolean {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      e.stopPropagation();
      void send();
      return true;
    }
    return false;
  }

  /** Fork (6.2): Esc on the inline reply. Untouched, the empty draft goes;
   *  edited, it stays as a draft. Stops here so the pane's own Esc (deselect)
   *  does not fire too. */
  function onFormKeydown(e: KeyboardEvent) {
    if (variant !== "reply" || e.key !== "Escape") return;
    e.preventDefault();
    e.stopPropagation();
    if (edited()) void close();
    else void discard();
  }

  /** Fork (6.2): continue in a window; this inline copy unmounts. */
  async function popOut() {
    if (!draft) return;
    if (saveTimer) clearTimeout(saveTimer);
    await api.updateDraft($state.snapshot(draft) as Draft);
    await flushHtml();
    settled = true;
    await api.openComposeWindow(draft.id);
    onPopOut?.();
  }

  async function close() {
    settled = true;
    if (chrome) {
      // Fork (6.1): the window's ✕ keeps what was typed. Edited: commit to
      // Drafts (a failed save keeps the row so nothing is lost). Untouched:
      // drop the never-saved local draft so it doesn't linger.
      if (saveState === "dirty") {
        await save();
      } else if (!committed && draft) {
        await api.deleteDraft(draft.id).catch(() => {});
      }
    } else {
      // Inline: closing keeps the draft — flush edits back to the server.
      await flushServer();
    }
    onClose?.();
  }

  // Teardown. Inline (switching draft rows / leaving the folder) still writes
  // edits back to the Drafts folder; the window instead cleans up an unsaved
  // local draft.
  onDestroy(() => {
    if (settled) return;
    settled = true;
    if (chrome) {
      if (!committed && draft) void api.deleteDraft(draft.id).catch(() => {});
    } else if (variant === "reply") {
      // Fork (6.2): the thread changed under the inline reply. Untouched, the
      // empty draft goes; edited, it is kept like a closed inline editor.
      if (edited()) void flushServer();
      else if (draft) void api.deleteDraft(draft.id).catch(() => {});
      onClose?.();
    } else {
      void flushServer();
    }
  });

  const title = $derived(draft?.subject || t("compose.new"));
</script>

<svelte:window onkeydown={onWindowKeydown} />

<!-- Fork (6.2): the keydown only relays Esc from the fields inside the region. -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="compose-form"
  class:inline={!chrome}
  class:reply={variant === "reply"}
  role="region"
  bind:this={rootEl}
  onkeydown={onFormKeydown}
  ondragover={(e) => {
    e.preventDefault();
    dragActive = true;
  }}
  ondragleave={(e) => {
    // Only clear when the pointer actually leaves the surface, not on child
    // boundaries (dragleave fires when moving between child elements).
    if (e.currentTarget === e.target) dragActive = false;
  }}
  ondrop={onDrop}
  onpaste={onPaste}
>
  {#if chrome}
    <WindowControls {title} onclose={close} />
  {/if}

  {#if draft}
    <div class="fields">
      {#if canPickFrom}
        <label class="field">
          <span class="microlabel">{t("compose.from")}</span>
          <select
            class="from-select"
            value={draft.accountId}
            onchange={(e) => void changeFrom(e.currentTarget.value)}
          >
            {#each accounts as a (a.id)}
              <option value={a.id}>
                {a.displayName ? `${a.displayName} <${a.email}>` : a.email}
              </option>
            {/each}
          </select>
        </label>
      {/if}
      <label class="field">
        <span class="microlabel">{t("compose.to")}</span>
        <AddressInput bind:value={draft.to} onchange={scheduleSave} />
        {#if !showCc}
          <button class="cc-toggle microlabel" onclick={() => (showCc = true)}>
            {t("compose.cc")}
          </button>
        {/if}
      </label>
      {#if showCc}
        <label class="field">
          <span class="microlabel">{t("compose.cc")}</span>
          <AddressInput bind:value={draft.cc} onchange={scheduleSave} />
        </label>
        <label class="field">
          <span class="microlabel">{t("compose.bcc")}</span>
          <AddressInput bind:value={draft.bcc} onchange={scheduleSave} />
        </label>
      {/if}
    </div>

    {#if aiAvailable}
      <div class="ai-bar">
        {#if userTurns.length > 0}
          <!-- The running session: instructions given so far, so it's clear
               this is one ongoing conversation about the draft. -->
          <div class="ai-thread">
            {#each userTurns as turn, i (i)}
              <button
                type="button"
                class="ai-turn"
                class:current={i === userTurns.length - 1}
                disabled={aiBusy}
                title={t("ai.revert")}
                aria-label={t("ai.revert")}
                onclick={() => revertTo(i)}
              >
                <span class="label">{turn.content}</span>
                <span class="revert" aria-hidden="true">↩</span>
              </button>
            {/each}
          </div>
        {/if}
        {#if aiBusy}
          <!-- Progress indicator: kept OUTSIDE the scrollable thread so a tall
               instruction can't push it below the fold — it must stay visible
               the whole time the draft is streaming. -->
          <div class="ai-thinking">✦ {slowStart.slow ? t("ai.loading_model") : t("ai.thinking")}</div>
        {/if}
        <form
          class="ai-input"
          onsubmit={(e) => {
            e.preventDefault();
            sendInstruction(instruction, true);
          }}
        >
          <span class="spark">✦</span>
          <textarea
            class="instruction"
            bind:this={instrEl}
            bind:value={instruction}
            placeholder={hasDraft ? t("ai.refine_placeholder") : t("ai.instruction_placeholder")}
            rows="1"
            spellcheck="false"
            disabled={aiBusy}
            onkeydown={(e) => {
              if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
                e.preventDefault();
                sendInstruction(instruction, true);
              }
            }}
          ></textarea>
          {#if aiBusy}
            <button type="button" class="ai-chip" onclick={stopAi}>{t("ai.stop")}</button>
          {:else}
            <button type="submit" class="ai-chip solid" disabled={!instruction.trim()}>
              <span class="label">✦ {hasDraft ? t("ai.refine") : t("ai.generate")}</span>
              <kbd>Ctrl ↵</kbd>
            </button>
          {/if}
        </form>
        {#if hasDraft && !aiBusy}
          <div class="tone-chips">
            <button class="ai-chip" onclick={() => adjust("shorter")}>{t("ai.shorter")}</button>
            <button class="ai-chip" onclick={() => adjust("warmer")}>{t("ai.warmer")}</button>
            <button class="ai-chip" onclick={() => adjust("formal")}>{t("ai.formal")}</button>
          </div>
        {/if}
      </div>
    {/if}

    <div class="fields subject-row">
      <label class="field">
        <span class="microlabel">{t("compose.subject")}</span>
        <input
          bind:value={draft.subject}
          oninput={() => {
            subjectAuto = false;
            scheduleSave();
          }}
          onkeydown={sendKey}
          class="subject"
        />
      </label>
    </div>

    {#if richMode}
      <!-- Fork (6.3): the words in the rich editor; the signature and the
           quoted original below it, read-only until "Edit quoted text". -->
      <RichEditor
        bind:this={editorRef}
        html={wordsHtml}
        onchange={onWordsChange}
        onkeydown={sendKey}
        placeholder={t("compose.body_placeholder")}
      />
      {#if tail}
        <div class="tail">
          {#if tailEditing}
            <textarea class="tail-edit" bind:value={tail} oninput={onTailInput} spellcheck="true" onkeydown={sendKey}></textarea>
          {:else}
            {#if tailParts.sig}
              <pre class="sig">-- {"\n"}{tailParts.sig}</pre>
            {/if}
            {#if tailParts.attribution}
              <div class="quote-head">
                <button
                  type="button"
                  class="quote-pill"
                  onclick={() => (quoteOpen = !quoteOpen)}
                  aria-expanded={quoteOpen}
                  title={t(quoteOpen ? "fork.reading.hide_quoted" : "fork.reading.show_quoted")}
                  aria-label={t(quoteOpen ? "fork.reading.hide_quoted" : "fork.reading.show_quoted")}
                >•••</button>
                {#if quoteOpen}
                  <button type="button" class="quote-edit" onclick={() => (tailEditing = true)}>{t("fork.compose.edit_quoted")}</button>
                {/if}
              </div>
              {#if quoteOpen}
                <pre class="quote">{tailParts.attribution}{"\n"}{tailParts.quoted}</pre>
              {/if}
            {:else if tailParts.sig}
              <button type="button" class="quote-edit" onclick={() => (tailEditing = true)}>{t("fork.compose.edit_signature")}</button>
            {/if}
          {/if}
        </div>
      {/if}
    {:else}
      <textarea
        class="body"
        bind:this={bodyEl}
        bind:value={draft.body}
        oninput={() => {
          onBodyInputSlots();
          scheduleSave();
        }}
        onkeydown={sendKey}
        placeholder={t("compose.body_placeholder")}
        spellcheck="true"
      ></textarea>
    {/if}

    {#if smellResult}<SmellHealth result={smellResult} onjump={(s) => smell?.jumpTo(s)} />{/if}
    {#if smellSendPrompt && smellResult}
      <SmellSendPrompt
        result={smellResult}
        onfix={() => {
          smellSendPrompt = false;
          const first = smellResult ? mustFix(smellResult)[0] : undefined;
          if (first) smell?.jumpTo(first);
        }}
        onsend={() => send(smellPendingWhen, true)}
      />
    {/if}
    {#if smellPop && smell && smellSettings}
      <SmellPopover
        span={smellPop.span}
        rect={smellPop.rect}
        mount={smell}
        settings={smellSettings}
        {aiAvailable}
        onclose={() => (smellPop = null)}
        onignore={(s) => {
          smellIgnoredOnce.add(`${s.start}:${s.rule}`);
          smell?.rescan();
        }}
      />
    {/if}

    {#if attachments.length > 0}
      <div class="attach-row">
        {#each attachments as a (a.id)}
          <div class="chip">
            <svg class="clip" width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2"><path d="M12.5 7.5l-5 5a3 3 0 0 1-4.243-4.243l5.657-5.657a2 2 0 0 1 2.829 2.829l-5.657 5.657a1 1 0 0 1-1.415-1.415l4.95-4.95" /></svg>
            <span class="name">{a.filename}</span>
            <span class="size">{formatSize(a.size)}</span>
            <button class="remove" onclick={() => removeAttachment(a.id)} title={t("compose.attach_remove")} aria-label={t("compose.attach_remove")}>
              <svg width="11" height="11" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.2"><path d="M1 1l8 8M9 1L1 9" /></svg>
            </button>
          </div>
        {/each}
      </div>
    {/if}

    {#if error}
      <div class="error">{error}</div>
    {/if}

    <footer class="bar">
      <!-- Fork (6.4): a split button — Send, and a caret for later. -->
      <div class="send-split">
        <button class="send" onclick={() => send()} disabled={sending || !draft.to.trim()} title="Ctrl ↵">
          {sending ? t("compose.sending") : t("compose.send")}
        </button>
        <SendLater disabled={sending || !draft.to.trim()} onpick={(pick) => send(pick)} />
      </div>
      <button class="attach" onclick={() => fileInput?.click()} title={t("compose.attach")} aria-label={t("compose.attach")}>
        <svg width="17" height="17" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2"><path d="M12.5 7.5l-5 5a3 3 0 0 1-4.243-4.243l5.657-5.657a2 2 0 0 1 2.829 2.829l-5.657 5.657a1 1 0 0 1-1.415-1.415l4.95-4.95" /></svg>
      </button>
      <input bind:this={fileInput} type="file" multiple class="file-input" onchange={pickFiles} />
      <!-- Fork (7.6 / 8) -->
      <button type="button" class="attach fork-tool" onclick={shareAvailability} title={t("fork.compose.share_availability")} aria-label={t("fork.compose.share_availability")}>
        <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round"><rect x="2.5" y="3.5" width="11" height="10" rx="1.5" /><path d="M2.5 6.5h11M5.5 2v3M10.5 2v3M6 9.5l1.5 1.5 3-3" /></svg>
      </button>
      <button type="button" class="attach fork-tool" onclick={() => addMeetLink((link) => insertAtCaret(link))} title={t("fork.compose.add_meet")} aria-label={t("fork.compose.add_meet")}>
        <svg width="17" height="17" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round"><rect x="1.5" y="4.5" width="9" height="7" rx="1.5" /><path d="M10.5 7l4-2.5v7l-4-2.5" /></svg>
      </button>
      <div class="grow"></div>
      {#if variant === "reply"}
        <!-- Fork (6.2): continue in a window. -->
        <button class="popout" onclick={popOut} title={t("fork.compose.pop_out")} aria-label={t("fork.compose.pop_out")}>
          <svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2"><path d="M9 2.5h4.5V7M13.5 2.5L8 8" /><path d="M11.5 9.5v3a1 1 0 0 1-1 1h-7a1 1 0 0 1-1-1v-7a1 1 0 0 1 1-1h3" /></svg>
        </button>
      {/if}
      {#if chrome}
        <!-- Fork (6.1): closing the window keeps an edited draft, so editing
             reveals an explicit Save; a saved draft shows a confirmation until
             the next edit. -->
        {#if saveState === "dirty"}
          <button class="save" onclick={save} title={t("compose.save")}>
            <svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2"><path d="M3 8.5l3.5 3.5L13 4.5" /></svg>
            <kbd>Ctrl S</kbd>
          </button>
        {:else if saveState === "saved"}
          <span class="saved-label">{t("compose.saved")}</span>
        {/if}
      {/if}
      <!-- Fork (6.1): Discard everywhere, on a second click. -->
      <button
        class="discard"
        class:armed={discardArmed}
        onclick={discardClick}
        title={discardArmed ? t("fork.compose.discard_confirm") : t("compose.discard")}
        aria-label={discardArmed ? t("fork.compose.discard_confirm") : t("compose.discard")}
      >
        <svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2"><path d="M3 4h10M6.5 4V2.5h3V4M4.5 4l.5 9.5h6l.5-9.5M6.7 6.5v5M9.3 6.5v5" /></svg>
        {#if discardArmed}<span class="confirm">{t("fork.compose.discard_confirm")}</span>{/if}
      </button>
    </footer>
  {/if}

  {#if dragActive}
    <div class="drop-overlay">
      <div class="drop-hint">{t("compose.drop_hint")}</div>
    </div>
  {/if}
</div>

<style>
  .compose-form {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    position: relative;
  }
  /* Inline in the reading pane: grow to fill the third pane. */
  .compose-form.inline {
    flex: 1;
    min-width: 0;
  }

  .fields {
    padding: 6px 20px 0;
  }
  /* Subject sits just under the AI bar; give it a little breathing room. */
  .subject-row {
    padding-top: 10px;
  }
  .field {
    display: flex;
    align-items: center;
    gap: 12px;
    border-bottom: 1px solid var(--hairline);
    padding: 9px 0;
  }
  .field .microlabel {
    width: 52px;
    flex-shrink: 0;
  }
  .field input {
    flex: 1;
    font-size: 13.5px;
    user-select: text;
  }
  .subject {
    font-weight: 600;
  }
  .from-select {
    flex: 1;
    font-size: 13.5px;
    border: 0;
    background: transparent;
    color: var(--text);
    cursor: pointer;
  }
  .cc-toggle {
    color: var(--text-faint);
  }
  .cc-toggle:hover {
    color: var(--text);
  }

  .body {
    flex: 1;
    resize: none;
    padding: 16px 20px;
    font-size: 14px;
    line-height: 1.6;
    user-select: text;
    cursor: text;
  }

  /* AI drafting bar — violet accent reserved for AI */
  .ai-bar {
    padding: 10px 20px 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .ai-thread {
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: 108px;
    overflow-y: auto;
  }
  .ai-turn {
    align-self: flex-start;
    display: flex;
    align-items: baseline;
    gap: 6px;
    max-width: 100%;
    padding: 4px 10px;
    border: 0;
    border-radius: var(--radius-s);
    background: var(--accent-soft);
    color: var(--text);
    font-size: 12.5px;
    line-height: 1.4;
    text-align: left;
    cursor: pointer;
    transition: background 0.12s ease;
  }
  .ai-turn .label {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .ai-turn:hover,
  .ai-turn:focus-visible {
    background: var(--accent-dim);
  }
  .ai-turn.current {
    opacity: 0.7;
  }
  .ai-turn .revert {
    flex: none;
    color: var(--accent);
    opacity: 0;
    transition: opacity 0.12s ease;
  }
  .ai-turn:hover .revert,
  .ai-turn:focus-visible .revert {
    opacity: 0.7;
  }
  .ai-turn:disabled {
    cursor: default;
    opacity: 0.55;
  }
  .ai-turn:disabled:hover {
    background: var(--accent-soft);
  }
  .ai-turn:disabled:hover .revert {
    opacity: 0;
  }
  .ai-thinking {
    align-self: flex-start;
    color: var(--accent);
    font-size: 12.5px;
    line-height: 1.4;
    animation: pulse 1.2s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.45;
    }
  }
  .ai-input {
    display: flex;
    align-items: center;
    gap: 10px;
    border: 1px solid var(--accent-dim);
    border-radius: var(--radius-m);
    padding: 8px 12px;
  }
  .spark {
    color: var(--accent);
  }
  .ai-input .instruction {
    flex: 1;
    font-size: 13px;
    line-height: 1.5;
    user-select: text;
    resize: none;
    min-height: 20px;
    max-height: 180px;
    overflow-y: auto;
    padding: 0;
    font-family: inherit;
  }
  .ai-input .instruction:disabled {
    opacity: 0.55;
    cursor: default;
  }
  .ai-chip {
    padding: 5px 12px;
    border-radius: 999px;
    border: 1px solid var(--accent-dim);
    color: var(--accent);
    font-size: 12px;
    font-weight: 600;
    flex-shrink: 0;
  }
  .ai-chip:hover:not(:disabled) {
    background: var(--accent-soft);
  }
  .ai-chip.solid {
    background: var(--accent);
    color: var(--on-accent);
    border-color: var(--accent);
    /* Stack the label over the Ctrl+Enter shortcut hint, centered. */
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 3px;
    line-height: 1.15;
  }
  /* Quiet shortcut caption under the label. Deliberately NOT the mono keycap
     treatment here: the UI font keeps the "Ctrl ↵" gap tight and optically
     centered, and small + faded so it hints without pulling focus from the
     label. (A bare <kbd> otherwise defaults to the UA monospace font.) */
  .ai-chip.solid kbd {
    font-family: inherit;
    font-size: 9px;
    font-weight: 500;
    letter-spacing: 0.01em;
    color: var(--on-accent);
    opacity: 0.45;
  }
  .ai-chip.solid:hover:not(:disabled) {
    background: var(--accent);
    opacity: 0.9;
  }
  .ai-chip:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .tone-chips {
    display: flex;
    gap: 6px;
  }

  .error {
    padding: 8px 20px;
    color: var(--danger);
    font-size: 12.5px;
  }

  .bar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 20px;
    border-top: 1px solid var(--hairline);
  }
  .send {
    padding: 8px 22px;
    border-radius: var(--radius-m);
    background: var(--text);
    color: var(--bg);
    font-weight: 700;
    font-size: 13.5px;
  }
  .send:hover:not(:disabled) {
    opacity: 0.88;
  }
  .send:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .grow {
    flex: 1;
  }
  .attach {
    width: 34px;
    height: 34px;
    display: grid;
    place-items: center;
    border-radius: var(--radius-s);
    color: var(--text-dim);
  }
  .attach:hover {
    background: var(--hover);
    color: var(--text);
  }
  .file-input {
    display: none;
  }
  .discard {
    width: 34px;
    height: 34px;
    display: grid;
    place-items: center;
    border-radius: var(--radius-s);
    color: var(--text-dim);
  }
  .discard:hover {
    background: var(--hover);
    color: var(--danger);
  }
  /* Fork (6.1): the second-click state names what the click will do. */
  .discard.armed {
    width: auto;
    padding: 0 10px;
    gap: 6px;
    display: flex;
    align-items: center;
    color: var(--danger);
    background: var(--hover);
    font-size: 12.5px;
    font-weight: 600;
  }
  /* Fork (6.2): the inline reply is a card inside the reading pane, not a
     full-height pane; the pop-out sits with the footer icons. */
  .compose-form.reply {
    height: auto;
    flex: 1 1 auto;
    width: 100%;
    min-height: 320px;
  }
  .compose-form.reply .fields,
  .compose-form.reply .bar,
  .compose-form.reply .ai-bar,
  .compose-form.reply .attach-row {
    padding-left: 16px;
    padding-right: 16px;
  }
  .compose-form.reply .body {
    min-height: 140px;
  }
  .popout {
    width: 34px;
    height: 34px;
    display: grid;
    place-items: center;
    border-radius: var(--radius-s);
    color: var(--text-dim);
  }
  .popout:hover {
    background: var(--hover);
    color: var(--text);
  }
  /* Fork (6.4): the split Send button. */
  .send-split {
    display: flex;
  }
  .send-split .send {
    border-radius: var(--radius-m) 0 0 var(--radius-m);
  }
  /* Fork (6.3): signature and quote under the editor, read-only. */
  .tail {
    padding: 4px 20px 10px;
    font-size: 13px;
    line-height: 1.5;
  }
  .tail pre {
    margin: 0;
    font-family: inherit;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    user-select: text;
  }
  .tail .sig {
    color: var(--text-dim);
    padding: 6px 0;
  }
  .quote-head {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 4px 0;
  }
  .quote-pill {
    padding: 1px 9px;
    border-radius: 999px;
    border: 1px solid var(--hairline-strong);
    color: var(--text-dim);
    font-size: 11px;
    letter-spacing: 0.1em;
    line-height: 1.4;
  }
  .quote-pill:hover,
  .quote-pill[aria-expanded="true"] {
    background: var(--hover);
    color: var(--text);
  }
  .quote-edit {
    font-size: 12px;
    color: var(--text-faint);
  }
  .quote-edit:hover {
    color: var(--text);
    text-decoration: underline;
  }
  .tail .quote {
    color: var(--text-dim);
    border-left: 2px solid var(--hairline-strong);
    padding-left: 10px;
    margin-top: 4px;
  }
  .tail-edit {
    width: 100%;
    min-height: 140px;
    resize: vertical;
    font-size: 13px;
    line-height: 1.5;
    color: var(--text-dim);
    user-select: text;
  }
  /* Explicit Save in the compose window: icon + shortcut hint, muted until hover. */
  .save {
    height: 34px;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 12px;
    border-radius: var(--radius-s);
    color: var(--text-dim);
  }
  .save:hover {
    background: var(--hover);
    color: var(--text);
  }
  .save kbd {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-faint);
  }
  .saved-label {
    display: flex;
    align-items: center;
    height: 34px;
    padding: 0 12px;
    font-size: 12.5px;
    color: var(--text-faint);
  }

  /* Staged-attachment chips, above the footer. */
  .attach-row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    padding: 8px 20px 0;
  }
  .chip {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 5px 6px 5px 10px;
    border: 1px solid var(--hairline-strong);
    border-radius: var(--radius-s);
    font-size: 12.5px;
    max-width: 100%;
  }
  .chip .clip {
    color: var(--text-dim);
    flex-shrink: 0;
  }
  .chip .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chip .size {
    color: var(--text-faint);
    font-family: var(--font-mono);
    font-size: 10.5px;
    flex-shrink: 0;
  }
  .chip .remove {
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    border-radius: var(--radius-s);
    color: var(--text-faint);
    flex-shrink: 0;
  }
  .chip .remove:hover {
    background: var(--hover);
    color: var(--danger);
  }

  /* Drop target feedback while dragging files over the surface. */
  .drop-overlay {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    background: color-mix(in srgb, var(--surface) 78%, transparent);
    pointer-events: none;
    z-index: 10;
  }
  .drop-hint {
    padding: 14px 26px;
    border: 1.5px dashed var(--text-faint);
    border-radius: var(--radius-m);
    color: var(--text);
    font-size: 13.5px;
    font-weight: 600;
    background: var(--surface);
  }
</style>
