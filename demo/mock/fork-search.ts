// Fork (4.3): fixture for `fork_search_threads` in the demo harness. The main
// session wires it into `tauri-core.ts`:
//   case "fork_search_threads": return ok(forkSearchThreads(args.query ?? "", args.offset ?? 0));
import * as db from "./data";

const KEYS = /^-?(from|to|cc|subject|is|has|in|before|after|older_than|newer_than):/i;

/** Every inbox thread whose sender or subject contains the free words of the
 *  query; `from:` narrows by sender, `is:unread` / `is:starred` /
 *  `has:attachment` by flag. Other operators are accepted and ignored, so any
 *  typed query gets a plausible list. */
export function forkSearchThreads(query: string, offset: number): typeof db.INBOX_THREADS {
  if (offset > 0) return [];
  const tokens = query.match(/(?:[^\s"]+|"[^"]*")+/g) ?? [];
  const words: string[] = [];
  let from: string | null = null;
  let unread: boolean | null = null;
  let starred: boolean | null = null;
  let attachment: boolean | null = null;
  for (const raw of tokens) {
    const tok = raw.replace(/"/g, "");
    if (!KEYS.test(tok)) {
      if (!tok.startsWith("-")) words.push(tok.toLowerCase());
      continue;
    }
    const neg = tok.startsWith("-");
    const [key, value] = tok.replace(/^-/, "").split(":", 2);
    const v = (value ?? "").toLowerCase();
    if (key.toLowerCase() === "from") from = v;
    else if (key.toLowerCase() === "is" && v === "unread") unread = !neg;
    else if (key.toLowerCase() === "is" && v === "read") unread = neg;
    else if (key.toLowerCase() === "is" && v === "starred") starred = !neg;
    else if (key.toLowerCase() === "has" && v.startsWith("attachment")) attachment = !neg;
  }
  const rows = db.INBOX_THREADS.filter((t) => {
    const hay = `${t.fromName} ${t.fromAddr} ${t.subject} ${t.snippet}`.toLowerCase();
    if (words.some((w) => !hay.includes(w))) return false;
    if (from !== null && !`${t.fromName} ${t.fromAddr}`.toLowerCase().includes(from)) return false;
    if (unread !== null && t.isRead === unread) return false;
    if (starred !== null && t.isStarred !== starred) return false;
    if (attachment !== null && t.hasAttachments !== attachment) return false;
    return true;
  });
  return rows;
}
