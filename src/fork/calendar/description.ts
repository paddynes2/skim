// Event descriptions (D97). Google stores them as a small HTML subset, and
// anyone who invites the user writes one, so a description is untrusted.
// `safeDescription` rebuilds it from an allow-list: text, line breaks,
// paragraphs, bold, italic, underline, lists and links to https or mailto.
// Everything else (images, styles, scripts, iframes, unknown tags,
// attributes) is dropped and only its text kept. Images go entirely, so an
// invite cannot load a tracking pixel. Parsing happens in an inert document
// (DOMParser), which runs no script and fetches nothing.

const KEEP = new Set(["b", "strong", "i", "em", "u", "br", "p", "div", "ul", "ol", "li", "a"]);
const SKIP = new Set(["script", "style", "img", "iframe", "object", "embed", "svg", "math", "video", "audio", "picture", "source", "head", "title", "template", "noscript"]);
const SAFE_HREF = /^(https:\/\/|mailto:)/i;

function escapeText(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}
function escapeAttr(s: string): string {
  return escapeText(s).replace(/"/g, "&quot;");
}

/** True when the description holds markup worth rendering (otherwise show it as plain text). */
export function looksLikeHtml(s: string | null | undefined): boolean {
  return !!s && /<\s*(br|p|div|a|b|strong|i|em|u|ul|ol|li|span)\b[^>]*>/i.test(s);
}

function walk(node: Node, out: string[]): void {
  for (const child of Array.from(node.childNodes)) {
    if (child.nodeType === 3) {
      out.push(escapeText(child.textContent ?? ""));
      continue;
    }
    if (child.nodeType !== 1) continue;
    const el = child as Element;
    const tag = el.tagName.toLowerCase();
    if (SKIP.has(tag)) continue;
    if (!KEEP.has(tag)) {
      walk(el, out);
      continue;
    }
    if (tag === "br") {
      out.push("<br>");
      continue;
    }
    if (tag === "a") {
      const href = (el.getAttribute("href") ?? "").trim();
      if (SAFE_HREF.test(href)) {
        out.push(`<a href="${escapeAttr(href)}">`);
        walk(el, out);
        out.push("</a>");
      } else {
        walk(el, out);
      }
      continue;
    }
    const name = tag === "div" ? "p" : tag;
    out.push(`<${name}>`);
    walk(el, out);
    out.push(`</${name}>`);
  }
}

/** Safe HTML for a description, or a plain-text description with its links and line breaks kept. */
export function safeDescription(raw: string | null | undefined): string {
  if (!raw) return "";
  if (!looksLikeHtml(raw)) {
    return escapeText(raw)
      .replace(/(https:\/\/[^\s<>"')\]]+)/g, (u) => `<a href="${u.replace(/"/g, "&quot;")}">${u}</a>`)
      .replace(/\n/g, "<br>");
  }
  const doc = new DOMParser().parseFromString(`<body>${raw}</body>`, "text/html");
  const out: string[] = [];
  walk(doc.body, out);
  return out.join("");
}
