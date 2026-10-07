import { chipsOf, textOf } from "./query";
export function searchTerms(query: string): string[] {
  return [...textOf(query).replaceAll('"', "").split(/\s+/), ...chipsOf(query).filter(c => !c.negated && ["filename", "subject", "from", "to"].includes(c.key)).map(c => c.value)].filter(Boolean).slice(0, 30);
}
function folded(text: string) {
  const positions: number[] = []; let value = ""; let offset = 0;
  for (const char of text) { const normalized = char.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase(); for (let i=0;i<normalized.length;i++) positions.push(offset); value += normalized; offset += char.length; }
  positions.push(text.length); return { value, positions };
}
export function highlightParts(text: string, query: string): {text: string; match: boolean}[] {
  const source = folded(text); const ranges: [number,number][] = [];
  for (const term of searchTerms(query)) { const needle = folded(term).value; if (!needle) continue; let at = source.value.indexOf(needle); while (at >= 0) { ranges.push([source.positions[at], source.positions[at + needle.length] ?? text.length]); at = source.value.indexOf(needle, at + needle.length); } }
  ranges.sort((a,b) => a[0] - b[0]); const merged: [number,number][] = [];
  for (const range of ranges) { const last = merged.at(-1); if (last && range[0] <= last[1]) last[1] = Math.max(last[1], range[1]); else merged.push([...range]); }
  const parts: {text:string;match:boolean}[] = []; let start = 0;
  for (const [from,to] of merged) { if (from > start) parts.push({text:text.slice(start,from),match:false}); parts.push({text:text.slice(from,to),match:true}); start=to; }
  if (start < text.length || !parts.length) parts.push({text:text.slice(start),match:false}); return parts;
}
export function matchingPassage(text: string, query: string, width=220): string {
  const clean=text.replace(/\s+/g," ").trim(); const pieces=highlightParts(clean,query); let at=0;
  for(const piece of pieces){if(piece.match)break;at+=piece.text.length;}
  if(at===clean.length)at=0;const from=Math.max(0,at-60);const to=Math.min(clean.length,from+width);
  return `${from ? "…" : ""}${clean.slice(from,to)}${to<clean.length ? "…" : ""}`;
}
