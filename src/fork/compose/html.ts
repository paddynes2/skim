// Fork (6.3): the two directions between the rich editor and `drafts.body_text`,
// plus the paste sanitiser Squire is given (it has no DOMPurify on this page).
//
// The editor's block model is Squire's default: one `<div>` per line, a blank
// line is `<div><br></div>`. `textToHtml` and `htmlToText` are inverses over
// that model, so the plain body the AI co-author streams (one line per line)
// and what the editor shows never drift by a line, and `splitTail` keeps
// finding the signature and the quote where it always did.

const BLOCKS = new Set(["DIV", "P", "H1", "H2", "H3", "H4", "H5", "H6", "PRE"]);

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
  /** The line being built, `null` before the first block. */
  cur: string | null;
  /** True right after a block opened a line and nothing was written to it yet. */
  fresh: boolean;
}

function open(ctx: Ctx, prefix: string) {
  if (ctx.fresh && ctx.cur !== null) {
    // A block directly inside a block (li > div): same line, wider prefix.
    ctx.cur = prefix;
    return;
  }
  if (ctx.cur !== null) ctx.lines.push(ctx.cur);
  ctx.cur = prefix;
  ctx.fresh = true;
}

function write(ctx: Ctx, prefix: string, text: string) {
  if (text === "") return;
  if (ctx.cur === null) ctx.cur = prefix;
  ctx.cur += text;
  ctx.fresh = false;
}

function walk(node: Node, ctx: Ctx, prefix: string, list: { ordered: boolean; n: number } | null) {
  const children = Array.from(node.childNodes);
  children.forEach((child, i) => {
    if (child.nodeType === Node.TEXT_NODE) {
      write(ctx, prefix, (child.textContent ?? "").replace(/ /g, " ").replace(/\n/g, ""));
      return;
    }
    if (child.nodeType !== Node.ELEMENT_NODE) return;
    const el = child as Element;
    const tag = el.tagName;
    if (tag === "BR") {
      // A trailing BR only marks an empty block (already an open, empty line).
      if (i === children.length - 1) return;
      open(ctx, prefix);
      return;
    }
    if (BLOCKS.has(tag)) {
      open(ctx, prefix);
      walk(el, ctx, prefix, null);
      return;
    }
    if (tag === "UL" || tag === "OL") {
      walk(el, ctx, prefix, { ordered: tag === "OL", n: 1 });
      return;
    }
    if (tag === "LI") {
      const mark = list?.ordered ? `${list.n++}. ` : "- ";
      open(ctx, prefix + mark);
      walk(el, ctx, prefix + " ".repeat(mark.length), null);
      return;
    }
    if (tag === "BLOCKQUOTE") {
      walk(el, ctx, prefix + "> ", null);
      return;
    }
    if (tag === "A") {
      const href = el.getAttribute("href") ?? "";
      const text = el.textContent ?? "";
      write(ctx, prefix, text);
      if (href && href !== text && !href.startsWith("mailto:" + text)) write(ctx, prefix, ` (${href})`);
      return;
    }
    walk(el, ctx, prefix, list);
  });
}

/** The editor's HTML back to plain text, line for line (see the header). */
export function htmlToText(html: string): string {
  const doc = new DOMParser().parseFromString(html, "text/html");
  const ctx: Ctx = { lines: [], cur: null, fresh: false };
  walk(doc.body, ctx, "", null);
  if (ctx.cur !== null) ctx.lines.push(ctx.cur);
  return ctx.lines.map((l) => l.replace(/[ \t]+$/, "")).join("\n");
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
