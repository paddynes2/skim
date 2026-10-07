/** Keep each original line until the user edits or removes that row. */
export interface DealEditorRow { id: number; name: string; entries: string; original: string; edited: boolean; note: boolean }
export function dealRows(text: string): DealEditorRow[] {
  return text.split("\n").map((original, id) => {
    const line = original.trim();
    const colon = line.indexOf(":");
    const named = colon > 0 && !/^(mailto|https?):/i.test(line);
    return { id, name: named ? line.slice(0, colon).trim() : "", entries: named ? line.slice(colon + 1).trim() : line, original, edited: false, note: !line || line.startsWith("#") };
  });
}
export function serializeDealRows(rows: DealEditorRow[]): string {
  return rows.map((r) => r.edited ? (r.name.trim() ? `${r.name.trim()}: ${r.entries.trim()}` : r.entries.trim()) : r.original).join("\n");
}
export function invalidDealName(name: string): boolean { return /[:\r\n]/.test(name) || name.trim().startsWith("#"); }
