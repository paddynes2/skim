// Sender avatars (PLAN.md 2.6): initials on a disc whose colour is a stable
// hash of the address over a six-colour palette. No violet: that is AI's.

/** Two letters for a sender: first letters of the first two words of the
 *  display name, else the first two characters of the address's local part. */
export function initials(name: string, addr: string): string {
  const words = name
    .replace(/[<>"'()]/g, " ")
    .split(/\s+/)
    .filter((w) => w && /[\p{L}\p{N}]/u.test(w));
  if (words.length >= 2) return (first(words[0]) + first(words[1])).toUpperCase();
  if (words.length === 1 && words[0].length >= 2) return words[0].slice(0, 2).toUpperCase();
  const local = addr.split("@")[0] ?? "";
  return (local.slice(0, 2) || "?").toUpperCase();
}

function first(word: string): string {
  return Array.from(word)[0] ?? "";
}

/** 1..6, stable for an address (FNV-1a over the lowercased address). */
export function avatarColor(addr: string): number {
  let h = 0x811c9dc5;
  for (const ch of addr.trim().toLowerCase()) {
    h ^= ch.codePointAt(0) ?? 0;
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return (h % 6) + 1;
}
