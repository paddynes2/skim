// Fork (v1.1.3): fixtures for the Deals commands in the demo harness. Fictional
// companies only; the real list lives in the user's local settings.
import * as db from "./data";

const DEALS: Record<number, string> = {
  101: "Northwind",
  102: "Acme Partners",
  103: "Brightwave",
  105: "Northwind",
  107: "Brightwave",
};

export const DEMO_DEALS_TEXT = "Northwind: northwind.example\nAcme Partners: acme-partners.example\nBrightwave: brightwave.io";

export function forkDealsList(offset: number) {
  if (offset > 0) return [];
  return db.INBOX_THREADS.filter((t) => t.id in DEALS).map((t) => ({
    ...t,
    accountId: "acc-1",
    messageId: null,
    deal: DEALS[t.id],
  }));
}

export function forkDealsCount() {
  const rows = forkDealsList(0);
  return { unread: rows.filter((r) => !r.isRead).length, total: rows.length };
}

export function forkDealsSuggest(threadId: number) {
  const t = db.INBOX_THREADS.find((x) => x.id === threadId);
  if (!t) return null;
  const domain = t.fromAddr.split("@")[1] ?? "";
  const label = domain.split(".")[0] ?? "";
  return {
    deal: DEALS[threadId] ?? null,
    name: label.charAt(0).toUpperCase() + label.slice(1),
    entry: domain,
  };
}

/** A rough stand-in for the Rust parser, enough for the Settings preview. */
export function forkDealsPreview(text: string) {
  const deals: { name: string; domains: string[]; addresses: string[] }[] = [];
  const ignored: { entry: string; why: string }[] = [];
  for (const raw of String(text).split("\n")) {
    const line = raw.trim();
    if (!line || line.startsWith("#")) continue;
    const [name, rest = ""] = line.includes(":") ? line.split(/:(.*)/) : ["", line];
    const d = { name: name.trim(), domains: [] as string[], addresses: [] as string[] };
    for (const e of rest.split(/[,; \t]+/).map((x) => x.trim().toLowerCase()).filter(Boolean)) {
      if (e.includes("@")) d.addresses.push(e);
      else if (e === "gmail.com") ignored.push({ entry: e, why: "personal" });
      else d.domains.push(e);
    }
    if (d.domains.length || d.addresses.length) deals.push(d);
  }
  return { deals, ignored };
}
