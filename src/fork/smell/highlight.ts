// Fork (6.5.3): underlines in the editor, and the mount API the composer uses.
//
//   const smell = attachSmell(editorEl, { scan, onOpen, onReplace? });
//
// Rich text (a contenteditable host, Squire's): the CSS Custom Highlight API
// (`CSS.highlights`, `Highlight`, `Range`) paints `::highlight(smell-hard)` red
// wavy and `::highlight(smell-warn)` amber dotted without touching the DOM the
// editor owns. Plain text (a <textarea>): a mirror <div> behind the textarea
// carries the same text with <mark>s, scrolled in step.
//
// Offsets are into the text `getText()` returns: for a textarea its value up
// to the signature / quote (`ownWords`); for a contenteditable the text of its
// nodes with one "\n" per block break (`textOf`), which is also how a span is
// turned back into a DOM Range. Re-scan is debounced 300ms after input; a click
// (or Alt+Enter with the caret) on an underline calls `onOpen(span, rect)`.
import { ownWords, type ScanResult, type Severity, type Span } from "./scan";

export interface AttachOptions {
  /** The scanner (rules already compiled), so this layer knows nothing of them. */
  scan: (text: string) => ScanResult;
  /** The user's own words. Default: `ownWords(textarea.value)` / `textOf(host)`. */
  getText?: () => string;
  /** Open the popover for a span; `rect` is where it sits on screen. */
  onOpen: (span: Span, rect: DOMRect) => void;
  /** Every scan's result (the health bar and the send check read it). */
  onResult?: (result: ScanResult) => void;
  /** Apply a replacement as one undoable edit. Default: select + `insertText`
   *  (undoable in both a textarea and a contenteditable). Squire callers pass
   *  their own so the editor's undo stack sees it. */
  onReplace?: (start: number, end: number, text: string) => void;
  debounceMs?: number;
}

export interface SmellMount {
  /** Scan now and repaint. */
  rescan(): ScanResult;
  /** Scan after the debounce (what the input listener calls). */
  schedule(): void;
  result(): ScanResult;
  spanAt(offset: number): Span | null;
  /** Put the caret at the span and scroll it into view. */
  jumpTo(span: Span): void;
  /** Replace the span's text (or any range) as one undoable edit, then rescan. */
  replace(start: number, end: number, text: string): void;
  /** The sentence around a span with one sentence of context either side. */
  context(span: Span): { before: string; sentence: string; after: string; start: number; end: number };
  /** Stop painting; remove listeners and the mirror. */
  destroy(): void;
}

const STYLE_ID = "fork-smell-css";
const HL = { hard: "smell-hard", warn: "smell-warn", info: "smell-info" } as const;

/** The global stylesheet: `::highlight()` cannot be Svelte-scoped, and the
 *  mirror's <mark>s share it. Amber is the one colour the theme has no token
 *  for; `--smell-warn` is defined here per theme (pending: move to tokens.css). */
function ensureStyle(): void {
  if (typeof document === "undefined" || document.getElementById(STYLE_ID)) return;
  const st = document.createElement("style");
  st.id = STYLE_ID;
  st.textContent = `
:root { --smell-warn: #b7791f; }
:root[data-theme="cold-dark"], :root[data-theme="warm-dark"] { --smell-warn: #e0b45a; }
::highlight(smell-hard) { text-decoration: underline wavy var(--danger); text-decoration-skip-ink: none; }
::highlight(smell-warn) { text-decoration: underline dotted var(--smell-warn); text-decoration-thickness: 2px; text-underline-offset: 2px; }
::highlight(smell-info) { text-decoration: underline dotted var(--text-faint); }
.smell-mirror { position: absolute; inset: 0; overflow: hidden; pointer-events: none; color: transparent; white-space: pre-wrap; word-wrap: break-word; z-index: 0; }
.smell-mirror mark { background: none; color: transparent; }
.smell-mirror mark.smell-hard { text-decoration: underline wavy var(--danger); text-decoration-skip-ink: none; }
.smell-mirror mark.smell-warn { text-decoration: underline dotted var(--smell-warn); text-decoration-thickness: 2px; text-underline-offset: 2px; }
.smell-mirror mark.smell-info { text-decoration: underline dotted var(--text-faint); }
.smell-host { position: relative; }
.smell-host > textarea { position: relative; z-index: 1; background: transparent; }
`;
  document.head.appendChild(st);
}

const BLOCKS = new Set(["P", "DIV", "LI", "BLOCKQUOTE", "H1", "H2", "H3", "H4", "PRE", "TR", "UL", "OL"]);

interface Segment {
  node: Text;
  start: number;
  end: number;
}

/** Walk a contenteditable: its plain text and the map from text offsets back
 *  to text nodes. A <br> and the end of a block element contribute one "\n"
 *  that belongs to no node. */
export function textOf(host: Node): { text: string; segments: Segment[] } {
  let text = "";
  const segments: Segment[] = [];
  const walk = (n: Node) => {
    if (n.nodeType === Node.TEXT_NODE) {
      const t = (n as Text).data;
      if (t.length) {
        segments.push({ node: n as Text, start: text.length, end: text.length + t.length });
        text += t;
      }
      return;
    }
    if (n.nodeType !== Node.ELEMENT_NODE) return;
    const el = n as Element;
    if (el.tagName === "BR") {
      text += "\n";
      return;
    }
    for (const c of Array.from(el.childNodes)) walk(c);
    if (BLOCKS.has(el.tagName) && !text.endsWith("\n") && el !== host) text += "\n";
  };
  walk(host);
  return { text, segments };
}

function pointFor(segments: Segment[], offset: number, end: boolean): { node: Text; offset: number } | null {
  for (const s of segments) {
    if (offset >= s.start && (offset < s.end || (end && offset === s.end))) return { node: s.node, offset: offset - s.start };
  }
  // an offset that falls on a virtual "\n": snap to the nearest node edge
  let best: Segment | null = null;
  for (const s of segments) if (s.end <= offset && (!best || s.end > best.end)) best = s;
  if (best) return { node: best.node, offset: best.end - best.start };
  return segments.length ? { node: segments[0].node, offset: 0 } : null;
}

function offsetOf(segments: Segment[], node: Node, offset: number): number | null {
  if (node.nodeType === Node.TEXT_NODE) {
    const s = segments.find((x) => x.node === node);
    return s ? s.start + Math.min(offset, s.end - s.start) : null;
  }
  // an element position: the first text node at or after the child index
  const kids = Array.from(node.childNodes);
  for (let i = offset; i < kids.length; i++) {
    const inner = textOf(kids[i]).segments[0]?.node ?? null;
    const s = inner && segments.find((x) => x.node === inner);
    if (s) return s.start;
  }
  const last = segments[segments.length - 1];
  return last ? last.end : 0;
}

const SENT_END = /[.!?]["')\]]*\s/g;

/** Sentence boundaries around [start, end): the sentence the span sits in, with
 *  one sentence either side. Pure, exported for tests. */
export function sentenceContext(text: string, start: number, end: number) {
  const bounds: number[] = [0];
  for (const m of text.matchAll(SENT_END)) bounds.push((m.index ?? 0) + m[0].length);
  for (const m of text.matchAll(/\n\s*\n/g)) bounds.push((m.index ?? 0) + m[0].length);
  bounds.push(text.length);
  const b = [...new Set(bounds)].sort((x, y) => x - y);
  let si = 0;
  while (si + 1 < b.length && b[si + 1] <= start) si++;
  let ei = si;
  while (ei + 1 < b.length && b[ei + 1] < end) ei++;
  const s0 = b[si];
  const s1 = b[Math.min(ei + 1, b.length - 1)];
  const before = si > 0 ? text.slice(b[si - 1], s0) : "";
  const after = ei + 2 < b.length ? text.slice(s1, b[ei + 2]) : "";
  const raw = text.slice(s0, s1);
  const lead = raw.length - raw.trimStart().length;
  const sentence = raw.trim();
  return { before: before.trim(), sentence, after: after.trim(), start: s0 + lead, end: s0 + lead + sentence.length };
}

export function attachSmell(editorEl: HTMLElement, opts: AttachOptions): SmellMount {
  ensureStyle();
  const isTextarea = editorEl instanceof HTMLTextAreaElement;
  const debounce = opts.debounceMs ?? 300;
  let current: ScanResult = { spans: [], health: { words: 0, hedgesPer1k: 0, positionsPer1k: 0, contrastRepeat: null, noPosition: false } };
  let timer: ReturnType<typeof setTimeout> | null = null;
  let segments: Segment[] = [];
  let mirror: HTMLDivElement | null = null;
  let disposed = false;

  const getText = (): string => {
    if (opts.getText) return opts.getText();
    if (isTextarea) return ownWords(editorEl.value).text;
    const walked = textOf(editorEl);
    segments = walked.segments;
    return walked.text;
  };

  // ---- painting ----
  const supportsHighlights = typeof CSS !== "undefined" && "highlights" in CSS && typeof Highlight !== "undefined";

  function paintHighlights(): void {
    if (!supportsHighlights) return;
    if (!opts.getText) segments = textOf(editorEl).segments;
    for (const sev of ["hard", "warn", "info"] as Severity[]) {
      const hl = new Highlight();
      for (const s of current.spans) {
        if (s.severity !== sev) continue;
        const a = pointFor(segments, s.start, false);
        const b = pointFor(segments, s.end, true);
        if (!a || !b) continue;
        const r = new Range();
        try {
          r.setStart(a.node, a.offset);
          r.setEnd(b.node, b.offset);
        } catch {
          continue;
        }
        hl.add(r);
      }
      CSS.highlights.set(HL[sev], hl);
    }
  }

  function ensureMirror(): HTMLDivElement {
    if (mirror) return mirror;
    const ta = editorEl as HTMLTextAreaElement;
    const parent = ta.parentElement ?? document.body;
    parent.classList.add("smell-host");
    mirror = document.createElement("div");
    mirror.className = "smell-mirror";
    mirror.setAttribute("aria-hidden", "true");
    parent.insertBefore(mirror, ta);
    return mirror;
  }

  function syncMirrorBox(): void {
    if (!mirror) return;
    const ta = editorEl as HTMLTextAreaElement;
    const cs = getComputedStyle(ta);
    for (const p of ["font", "fontSize", "fontFamily", "lineHeight", "letterSpacing", "padding", "border", "boxSizing", "textIndent"] as const) {
      mirror.style[p] = cs[p];
    }
    mirror.style.borderColor = "transparent";
    mirror.style.width = `${ta.offsetWidth}px`;
    mirror.style.height = `${ta.offsetHeight}px`;
    mirror.style.top = `${ta.offsetTop}px`;
    mirror.style.left = `${ta.offsetLeft}px`;
    mirror.scrollTop = ta.scrollTop;
  }

  function paintMirror(): void {
    const m = ensureMirror();
    const text = (editorEl as HTMLTextAreaElement).value;
    const frag = document.createDocumentFragment();
    let pos = 0;
    current.spans.forEach((s, i) => {
      if (s.start < pos) return; // overlapping: the earlier span wins
      frag.appendChild(document.createTextNode(text.slice(pos, s.start)));
      const mark = document.createElement("mark");
      mark.className = `smell ${HL[s.severity]}`;
      mark.dataset.i = String(i);
      mark.textContent = text.slice(s.start, s.end);
      frag.appendChild(mark);
      pos = s.end;
    });
    frag.appendChild(document.createTextNode(text.slice(pos) + "\n"));
    m.replaceChildren(frag);
    syncMirrorBox();
  }

  function paint(): void {
    if (disposed) return;
    if (isTextarea) paintMirror();
    else paintHighlights();
  }

  // ---- scanning ----
  function rescan(): ScanResult {
    current = opts.scan(getText());
    paint();
    opts.onResult?.(current);
    return current;
  }
  function schedule(): void {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => {
      timer = null;
      rescan();
    }, debounce);
  }

  // ---- hit testing ----
  function spanAt(offset: number): Span | null {
    return current.spans.find((s) => offset >= s.start && offset <= s.end) ?? null;
  }
  function caretOffset(): number | null {
    if (isTextarea) return (editorEl as HTMLTextAreaElement).selectionStart;
    const sel = document.getSelection();
    if (!sel || !sel.anchorNode || !editorEl.contains(sel.anchorNode)) return null;
    if (!opts.getText) segments = textOf(editorEl).segments;
    return offsetOf(segments, sel.anchorNode, sel.anchorOffset);
  }
  function rectFor(span: Span): DOMRect {
    if (isTextarea) {
      const i = current.spans.indexOf(span);
      const mark = mirror?.querySelector<HTMLElement>(`mark[data-i="${i}"]`);
      return mark?.getBoundingClientRect() ?? editorEl.getBoundingClientRect();
    }
    const a = pointFor(segments, span.start, false);
    const b = pointFor(segments, span.end, true);
    if (a && b) {
      const r = new Range();
      try {
        r.setStart(a.node, a.offset);
        r.setEnd(b.node, b.offset);
        return r.getBoundingClientRect();
      } catch {
        /* fall through */
      }
    }
    return editorEl.getBoundingClientRect();
  }
  function openAt(offset: number | null): boolean {
    if (offset == null) return false;
    const s = spanAt(offset);
    if (!s) return false;
    opts.onOpen(s, rectFor(s));
    return true;
  }

  const onInput = () => schedule();
  const onClick = (e: MouseEvent) => {
    let off: number | null = null;
    if (isTextarea) off = (editorEl as HTMLTextAreaElement).selectionStart;
    else {
      const doc = document as Document & { caretPositionFromPoint?: (x: number, y: number) => { offsetNode: Node; offset: number } | null };
      const pos = doc.caretPositionFromPoint?.(e.clientX, e.clientY);
      if (pos) off = offsetOf(segments, pos.offsetNode, pos.offset);
      else off = caretOffset();
    }
    openAt(off);
  };
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Enter" && e.altKey && !e.ctrlKey && !e.metaKey) {
      if (openAt(caretOffset())) e.preventDefault();
    }
  };
  const onScroll = () => {
    if (mirror) mirror.scrollTop = (editorEl as HTMLTextAreaElement).scrollTop;
  };
  editorEl.addEventListener("input", onInput);
  editorEl.addEventListener("click", onClick);
  editorEl.addEventListener("keydown", onKey);
  if (isTextarea) editorEl.addEventListener("scroll", onScroll);
  const ro = typeof ResizeObserver !== "undefined" ? new ResizeObserver(() => syncMirrorBox()) : null;
  if (isTextarea) ro?.observe(editorEl);

  function select(start: number, end: number): boolean {
    if (isTextarea) {
      const ta = editorEl as HTMLTextAreaElement;
      ta.focus();
      ta.setSelectionRange(start, end);
      return true;
    }
    if (!opts.getText) segments = textOf(editorEl).segments;
    const a = pointFor(segments, start, false);
    const b = pointFor(segments, end, true);
    if (!a || !b) return false;
    const r = new Range();
    try {
      r.setStart(a.node, a.offset);
      r.setEnd(b.node, b.offset);
    } catch {
      return false;
    }
    editorEl.focus();
    const sel = document.getSelection();
    sel?.removeAllRanges();
    sel?.addRange(r);
    return true;
  }

  return {
    rescan,
    schedule,
    result: () => current,
    spanAt,
    jumpTo(span) {
      if (select(span.start, span.start)) {
        if (isTextarea) {
          const ta = editorEl as HTMLTextAreaElement;
          const mark = mirror?.querySelector<HTMLElement>(`mark[data-i="${current.spans.indexOf(span)}"]`);
          if (mark) ta.scrollTop = Math.max(0, mark.offsetTop - ta.clientHeight / 2);
        } else {
          document.getSelection()?.focusNode?.parentElement?.scrollIntoView({ block: "nearest" });
        }
      }
    },
    replace(start, end, text) {
      if (opts.onReplace) opts.onReplace(start, end, text);
      else if (select(start, end)) document.execCommand("insertText", false, text);
      rescan();
    },
    context(span) {
      return sentenceContext(getText(), span.start, span.end);
    },
    destroy() {
      disposed = true;
      if (timer) clearTimeout(timer);
      editorEl.removeEventListener("input", onInput);
      editorEl.removeEventListener("click", onClick);
      editorEl.removeEventListener("keydown", onKey);
      editorEl.removeEventListener("scroll", onScroll);
      ro?.disconnect();
      mirror?.remove();
      mirror = null;
      if (supportsHighlights) for (const k of Object.values(HL)) CSS.highlights.delete(k);
    },
  };
}
