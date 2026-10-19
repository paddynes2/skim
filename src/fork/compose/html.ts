// Fork (6.3): the two directions between the rich editor and `drafts.body_text`,
// plus the paste sanitiser Squire is given (it has no DOMPurify on this page).
//
// The editor's block model is Squire's default: one `<div>` per line, a blank
// line is `<div><br></div>`. `textToHtml` and `htmlToText` are inverses over
// that model, so the plain body the AI co-author streams (one line per line)
// and what the editor shows never drift by a line, and `splitTail` keeps
// finding the signature and the quote where it always did.

const BLOCKS = new Set(["DIV", "P", "H1", "H2", "H3", "H4", "H5", "H6", "PRE"]);
/** Blocks with a margin: a blank line above and below in the plain text. The
 *  editor's own lines are `<div>`s, so this only shapes HTML written elsewhere
 *  (an agent's `<p>` paragraphs) and never the `textToHtml` round trip. */
const PARAGRAPHS = new Set(["P", "H1", "H2", "H3", "H4", "H5", "H6"]);

export function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

/** Plain text (as the AI writes it, or as a plain draft holds it) to the
 *  editor's HTML: one `<div>` per line, `<br>` for a blank one. */
export function textToHtml(text: string): string {
  const lines = text.replace(/\r\n?/g, "\n").split("\n");
  if (lines.length === 1 && lines[0] === "") return "<div><br></div>";
  return lines.map((l) => (l === "" ? "<div><br></div>" : `<div>${escapeHtml(l)}</div>`)).join("");
}

interface Ctx {
  lines: string[];
  /** The line being built; `null` when no line is open. */
  cur: string | null;
  /** A list item's marker ("- ", "2. "), taken by the first line that starts
   *  inside the item, so `li > div` stays on the marker's line. */
  mark: string | null;
  /** A paragraph (`<p>`, a heading) began or ended: the next line wants a
   *  blank line above it, as the paragraph's margin shows one. */
  gap: boolean;
  /** Index of the blank line `gap` put down, until the next line closes. */
  gapAt: number;
}

/** Open a line if none is, with a waiting list marker in front. */
function start(ctx: Ctx, prefix: string) {
  if (ctx.cur !== null) return;
  const last = ctx.lines[ctx.lines.length - 1];
  if (ctx.gap && last !== undefined && last.trim() !== "") {
    ctx.lines.push("");
    ctx.gapAt = ctx.lines.length - 1;
  }
  ctx.gap = false;
  ctx.cur = ctx.mark ?? prefix;
  ctx.mark = null;
}

/** Close the open line, if any. A block's start and end both do this, so an
 *  empty block (`<div><br></div>`) is one blank line and the sibling after it
 *  starts the next (D73: the sibling used to take the blank line's place). */
function end(ctx: Ctx) {
  if (ctx.cur === null) return;
  // A spacer paragraph (`<p>&nbsp;</p>`) is the blank line the paragraph
  // break already put down, not a second one.
  const spacer = ctx.cur.trim() === "" && ctx.gapAt >= 0 && ctx.gapAt === ctx.lines.length - 1;
  if (!spacer) ctx.lines.push(ctx.cur);
  ctx.gapAt = -1;
  ctx.cur = null;
}

/** A list marker no line took (an empty item) is a line of its own. */
function dropMark(ctx: Ctx) {
  if (ctx.mark === null) return;
  ctx.lines.push(ctx.mark);
  ctx.mark = null;
}

function write(ctx: Ctx, prefix: string, text: string) {
  if (text === "") return;
  start(ctx, prefix);
  ctx.cur += text;
}

function walk(node: Node, ctx: Ctx, prefix: string, list: { ordered: boolean; n: number } | null) {
  const children = Array.from(node.childNodes);
  for (const child of children) {
    if (child.nodeType === Node.TEXT_NODE) {
      const raw = child.textContent ?? "";
      // Markup indentation between blocks is not a line; a block holding
      // only spaces (`<div> </div>`, a line of spaces) is.
      if (ctx.cur === null && children.length > 1 && /^[ \t\r\n\f]*$/.test(raw)) continue;
      write(ctx, prefix, raw.replace(/ /g, " ").replace(/\r?\n/g, " "));
      continue;
    }
    if (child.nodeType !== Node.ELEMENT_NODE) continue;
    const el = child as Element;
    const tag = el.tagName;
    if (tag === "BR") {
      // Ends the line; on an empty line it is the blank line itself.
      start(ctx, prefix);
      end(ctx);
      continue;
    }
    if (BLOCKS.has(tag)) {
      const paragraph = PARAGRAPHS.has(tag);
      end(ctx);
      if (paragraph) ctx.gap = true;
      walk(el, ctx, prefix, null);
      end(ctx);
      if (paragraph) ctx.gap = true;
      continue;
    }
    if (tag === "UL" || tag === "OL") {
      end(ctx);
      walk(el, ctx, prefix, { ordered: tag === "OL", n: 1 });
      end(ctx);
      continue;
    }
    if (tag === "LI") {
      end(ctx);
      dropMark(ctx);
      const mark = list?.ordered ? `${list.n++}. ` : "- ";
      ctx.mark = prefix + mark;
      walk(el, ctx, prefix + " ".repeat(mark.length), null);
      end(ctx);
      dropMark(ctx);
      continue;
    }
    if (tag === "BLOCKQUOTE") {
      end(ctx);
      walk(el, ctx, prefix + "> ", null);
      end(ctx);
      continue;
    }
    if (tag === "A") {
      const href = el.getAttribute("href") ?? "";
      const text = el.textContent ?? "";
      write(ctx, prefix, text);
      if (href && href !== text && !href.startsWith("mailto:" + text)) write(ctx, prefix, ` (${href})`);
      continue;
    }
    walk(el, ctx, prefix, list);
  }
}

/** The editor's HTML back to plain text, line for line (see the header). */
export function htmlToText(html: string): string {
  const doc = new DOMParser().parseFromString(html, "text/html");
  const ctx: Ctx = { lines: [], cur: null, mark: null, gap: false, gapAt: -1 };
  walk(doc.body, ctx, "", null);
  end(ctx);
  dropMark(ctx);
  return ctx.lines.map((l) => l.replace(/[ \t]+$/, "")).join("\n");
}

/** Where the quoted original starts in a plain body: the blank line above an
 *  "On … wrote:" line. A paragraph of the user's own that starts with "On"
 *  is not one, since that line must end in "wrote:". -1 when there is none. */
export function quoteAt(text: string): number {
  const m = /\n\nOn [^\n]* wrote:(?:\n|$)/.exec(text);
  return m ? m.index : -1;
}

/** Text with its blank lines removed: what `htmlToText` returned before D73,
 *  so editor HTML stored by an older build still matches its body. */
export function withoutBlankLines(text: string): string {
  return text
    .split("\n")
    .filter((l) => l.trim() !== "")
    .join("\n");
}

// ---- A draft written elsewhere (D73) --------------------------------------

/** Where a Gmail, Outlook, Thunderbird or Skim body stops being the writer's
 *  own words. */
const TAIL =
  ".gmail_quote, .gmail_signature, [data-smartmail=gmail_signature], blockquote[type=cite], " +
  ".skim-sig, #appendonsend, [id^=divRplyFwdMsg], .moz-cite-prefix, .moz-signature";
/** What the plain text cannot carry: the reason to open from the HTML at all. */
const FORMATTING = "a[href], b, strong, i, em, u, s, strike, ul, ol, blockquote, code, pre, h1, h2, h3, h4, h5, h6";
/** What the editor would show wrongly: a table's cells run together, and
 *  hidden text (a preheader) turns visible once styles are dropped. */
const REFUSE = "table, [hidden], [style*='display:none' i], [style*='display: none' i]";

/** The URL notes the two renderings of a link add: `htmlToText`'s
 *  "text (https://…)", Gmail's "text <https://…>" and "a@b <a@b>". */
const URL_NOTES = /\s*(?:\((?:https?:\/\/|mailto:)[^)\s]*\)|<(?:https?:\/\/|mailto:)[^>\s]*>|<[^>\s@]+@[^>\s]+>)/gi;

/** The letters and digits of a text, in order, without URL notes. */
function wordsKey(text: string): string {
  return text.replace(URL_NOTES, "").replace(/[^\p{L}\p{N}]+/gu, "");
}

/** Whether two renderings of a body say the same words, ignoring layout,
 *  punctuation, list marks and where each puts a link's URL. */
export function sameWords(a: string, b: string): boolean {
  return wordsKey(a) === wordsKey(b);
}

/** Remove `node` and everything after it in document order, up to `root`. */
function cutFrom(node: Node, root: Node) {
  let n: Node | null = node;
  let first = true;
  while (n && n !== root) {
    while (n.nextSibling) n.parentNode?.removeChild(n.nextSibling);
    const parent: Node | null = n.parentNode;
    if (first) parent?.removeChild(n);
    first = false;
    n = parent;
  }
}

function isBlank(node: Node): boolean {
  if (node.nodeName === "BR") return true;
  if (node.nodeType !== Node.TEXT_NODE && node.nodeType !== Node.ELEMENT_NODE) return true;
  return !(node.textContent ?? "").replace(/ /g, " ").trim();
}

/** The words of a draft's server copy (Gmail web, an agent's API draft) as
 *  editor HTML: cut at its quote or signature, cleaned with the paste
 *  allowlist, blank lines at the end dropped. `null` unless it carries
 *  formatting the plain text cannot and says the same words as `words` (the
 *  plain words). With no plain words (an HTML-only draft) any words do. */
export function serverWordsHtml(html: string, words: string): string | null {
  const doc = new DOMParser().parseFromString(html, "text/html");
  const own = doc.querySelector(".skim-words");
  if (!own) {
    const tail = doc.body.querySelector(TAIL);
    if (tail) cutFrom(tail, doc.body);
  }
  const root = own ?? doc.body;
  if (root.querySelector(REFUSE)) return null;
  const holder = doc.createElement("div");
  holder.appendChild(sanitizeToFragment(root.innerHTML, doc));
  while (holder.lastChild && isBlank(holder.lastChild)) holder.removeChild(holder.lastChild);
  const out = holder.innerHTML;
  const text = htmlToText(out);
  if (!wordsKey(text)) return null;
  if (!wordsKey(words)) return out;
  if (!holder.querySelector(FORMATTING)) return null;
  return sameWords(text, words) ? out : null;
}

/** The server copy of a draft, as `fork_draft_source` returns it. */
export interface DraftSource {
  html: string | null;
  text: string | null;
}

function hasFormatting(html: string): boolean {
  return new DOMParser().parseFromString(html, "text/html").body.querySelector(FORMATTING) !== null;
}

/** The HTML the editor opens a draft's words with (D73). `words` is the plain
 *  body above its signature and quote. `source` is null once this build has
 *  stored the editor's words, so the user's own edits always win.
 *
 *  1. The server copy's words, when they say what `words` says and no stored
 *     editor HTML (an older build's) has formatting of its own: a draft from
 *     Gmail or an agent keeps its links, lists and bold.
 *  2. The stored editor HTML, when it says what the words say.
 *  3. The words as plain lines. */
export function openingHtml(words: string, stored: string | null, source: DraftSource | null): string {
  if (source?.html && (stored === null || !hasFormatting(stored))) {
    const server = serverWordsHtml(source.html, words);
    if (server !== null) return server;
  }
  if (stored !== null && withoutBlankLines(htmlToText(stored)) === withoutBlankLines(words)) return stored;
  return textToHtml(words);
}

// ---- Paste sanitiser ------------------------------------------------------

/** What pasted HTML may keep: structure and inline formatting only. Images
 *  never land in the editor (a pasted image becomes an attachment via the
 *  composer's own paste handler); the Rust side sanitises again before send. */
const KEEP = new Set([
  "DIV", "P", "BR", "B", "STRONG", "I", "EM", "U", "S", "STRIKE", "A", "UL", "OL", "LI",
  "BLOCKQUOTE", "SPAN", "CODE", "PRE", "H1", "H2", "H3", "H4", "H5", "H6", "HR",
]);
const DROP = new Set([
  "SCRIPT", "STYLE", "HEAD", "TITLE", "META", "LINK", "IFRAME", "OBJECT", "EMBED", "SVG",
  "MATH", "IMG", "INPUT", "BUTTON", "FORM", "TEXTAREA", "SELECT", "VIDEO", "AUDIO", "TEMPLATE",
]);

function safeHref(href: string | null): string | null {
  if (!href) return null;
  const v = href.trim();
  return /^(https?:\/\/|mailto:)/i.test(v) ? v : null;
}

function copyClean(from: Node, into: Node, doc: Document) {
  for (const child of Array.from(from.childNodes)) {
    if (child.nodeType === Node.TEXT_NODE) {
      into.appendChild(doc.createTextNode(child.textContent ?? ""));
      continue;
    }
    if (child.nodeType !== Node.ELEMENT_NODE) continue;
    const el = child as Element;
    const tag = el.tagName;
    if (DROP.has(tag)) continue;
    if (!KEEP.has(tag)) {
      // Unknown wrapper (table, font, …): keep its words, drop the element.
      copyClean(el, into, doc);
      continue;
    }
    const out = doc.createElement(tag.toLowerCase());
    if (tag === "A") {
      const href = safeHref(el.getAttribute("href"));
      if (href) {
        out.setAttribute("href", href);
        out.setAttribute("rel", "noopener noreferrer");
      }
    }
    copyClean(el, out, doc);
    into.appendChild(out);
  }
}

/** Squire's `sanitizeToDOMFragment`: a fragment of `doc` holding only the
 *  allowlisted elements, no attributes but a safe `href`. */
export function sanitizeToFragment(html: string, doc: Document): DocumentFragment {
  const parsed = new DOMParser().parseFromString(html, "text/html");
  const frag = doc.createDocumentFragment();
  copyClean(parsed.body, frag, doc);
  return frag;
}
