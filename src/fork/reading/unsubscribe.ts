/** Find a sender-provided web link in the original, sanitized message body. */
export function unsubscribeLink(html: string): string | null {
  const template = document.createElement("template");
  template.innerHTML = html;
  const doc = template.content;
  // A forwarded or quoted list's footer belongs to a different sender.
  doc.querySelectorAll("blockquote, .skim-quote, .gmail_quote, #divRplyFwdMsg, #appendonsend").forEach((el) => {
    if (el.id) while (el.nextSibling) el.nextSibling.remove();
    el.remove();
  });
  const direct: string[] = [], preferences: string[] = [];
  for (const anchor of doc.querySelectorAll("a[href]")) {
    const href = anchor.getAttribute("href")?.trim() ?? "";
    let url: URL;
    try { url = new URL(href); } catch { continue; }
    if (!["https:", "http:"].includes(url.protocol) || url.username || url.password) continue;
    const label = (anchor.textContent ?? "").replace(/\s+/g, " ").trim();
    const parent = anchor.parentElement;
    const context = parent?.querySelectorAll("a").length === 1 && (parent.textContent?.length ?? 0) <= 300 ? parent.textContent ?? "" : "";
    if (/\bunsubscribe\b|\bopt[ -]?out\b|\bremove me\b/i.test(label + " " + context) || /(?:^|[\/_?&=.-])(?:unsubscribe|optout)(?:[\/_?&=.-]|$)/i.test(url.pathname + url.search)) {
      direct.push(href);
    } else if (/\b(?:manage|email|subscription|update) preferences\b|\bmanage subscriptions\b/i.test(label)) {
      preferences.push(href);
    }
  }
  return direct[0] ?? preferences[0] ?? null;
}
