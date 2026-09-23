// Fork (Phase 10): fixtures for `fork_court_list` / `fork_court_counts` in the
// demo harness. The main session wires them into `tauri-core.ts`:
//   case "fork_court_list":   return ok(forkCourtList(args.courtState, args.offset ?? 0));
//   case "fork_court_counts": return ok(forkCourtCounts());
//   case "fork_court_recompute": return ok(undefined);
// Rows are `CourtRow`s: a ThreadRow plus `since` (unix) and `reason`. Oldest
// `since` first, the order the real command returns.
import * as db from "./data";

const NOW = Math.floor(Date.now() / 1000);
const H = 3600;
const D = 86400;

type Row = (typeof db.INBOX_THREADS)[number] & { accountId: string; since: number; reason: string | null };

function row(
  id: number,
  fromName: string,
  fromAddr: string,
  subject: string,
  snippet: string,
  ago: number,
  reason: string | null,
  extra: Partial<Row> = {},
): Row {
  const date = NOW - ago;
  return {
    id,
    accountId: "acc-1",
    fromName,
    fromAddr,
    subject,
    snippet,
    date,
    isRead: true,
    isStarred: false,
    hasAttachments: false,
    messageCount: 2,
    since: date,
    reason,
    ...extra,
  };
}

/** Six threads waiting on him: ages across the colour bands (none / amber /
 *  red), a draft he started, two AI reasons, three with none. */
export const ON_ME: Row[] = [
  row(902, "Lena Kovač", "lena@kovac-legal.example", "Re: NDA for the Halden pilot", "Attached the redline. Can you confirm the term in 3.1 by Friday?", 9 * D, "asks for the deck", { messageCount: 4, hasAttachments: true }),
  row(103, "Priya Nair", "priya@brightwave.io", "Q4 roadmap review", "Can you confirm the two slots before I send the invite round?", 6 * D + 3 * H, "needs a yes or no", { isStarred: true }),
  row(904, "Tomas Rivera", "tomas@rivera-build.example", "Site visit dates", "Either of the two Tuesdays works for us, your call.", 4 * D, null),
  row(102, "Marcus Lee", "marcus@acme-partners.example", "Contract redline — v3 ready for your review", "Legal signed off on everything except section 4.2. I left two comments where…", 3 * D + 2 * H, "draft started", { messageCount: 3, hasAttachments: true }),
  row(101, "Anna Weber", "anna.weber@northwind.example", "Q3 launch — final checklist & open questions", "Pulling the last threads together before Thursday. Three things still need an owner…", 1 * D + 5 * H, null, { isRead: false, messageCount: 5, hasAttachments: true }),
  row(105, "Jordan Fisher", "jordan@fisher.example", "Coffee next week?", "Would be good to catch up. Tue or Wed?", 3 * H, null, { messageCount: 1 }),
];

/** Four threads where he spoke last: the row's sender is still the other
 *  party (the list shows who he is waiting on). */
export const WAITING: Row[] = [
  row(910, "Sofia Ramos", "sofia@ramos-design.example", "Logo revisions round 2", "Sent the two options on Monday, let me know which direction.", 12 * D, null, { messageCount: 6 }),
  row(911, "City Utilities", "billing@cityutilities.example", "Account 4471 — meter reading dispute", "Asked for the corrected statement.", 7 * D + 4 * H, null),
  row(912, "Alex Chen", "alex.chen@northwind.example", "Intro: Alex <> Anna", "Happy to make the intro, over to you both.", 2 * D, null, { messageCount: 3 }),
  row(913, "Marcus Lee", "marcus@acme-partners.example", "Invoice #2291", "Sent the invoice, payment terms 30 days.", 6 * H, null, { messageCount: 1, hasAttachments: true }),
];

export function forkCourtList(state: string, offset: number): Row[] {
  if (offset > 0) return [];
  return state === "waiting" ? WAITING : ON_ME;
}

export function forkCourtCounts(): { onMe: number; waiting: number } {
  return { onMe: ON_ME.length, waiting: WAITING.length };
}
