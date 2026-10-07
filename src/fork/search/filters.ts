import { chipsOf, tokenize } from "./query";
export interface SearchFilters { sender: string; company: string; after: string; before: string; attachments: boolean; filename: string }
const managed = new Set(["from", "company", "after", "before", "has", "filename"]);
export function readFilters(query: string): SearchFilters {
  const chips = chipsOf(query).filter(c => !c.negated);
  const value = (key: string) => chips.filter(c => c.key === key).at(-1)?.value ?? "";
  return {sender:value("from"),company:value("company"),after:value("after"),before:value("before"),attachments:chips.some(c=>c.key==="has"),filename:value("filename")};
}
export function applyFilters(query: string, filters: SearchFilters): string {
  const before=readFilters(query);
  const changed=new Set<string>();
  for(const [key,field] of [["from","sender"],["company","company"],["after","after"],["before","before"],["has","attachments"],["filename","filename"]] as const) {
    if(filters[field]!==before[field]) changed.add(key);
  }
  const removed = new Set(chipsOf(query).filter(c => !c.negated && managed.has(c.key) && changed.has(c.key)).map(c=>c.token));
  const terms=tokenize(query).filter(token=>!removed.has(token));
  for(const [key,value] of [["from",filters.sender],["company",filters.company],["after",filters.after],["before",filters.before],["filename",filters.filename]]) {
    if(!changed.has(key)) continue;
    const clean=value.trim().replace(/["\r\n]/g," ").trim();
    if(clean) terms.push(`${key}:"${clean}"`);
  }
  if(changed.has("has") && filters.attachments) terms.push("has:attachment");
  return terms.join(" ");
}
export interface SavedSearch { name: string; query: string }
export function parseSavedSearches(raw: string | undefined): SavedSearch[] {
  if(!raw) return [];
  const value:unknown=JSON.parse(raw);
  if(!Array.isArray(value) || value.length>30 || value.some(v=>typeof v!=="object" || v===null || typeof v.name!=="string" || !v.name.trim() || v.name.length>80 || typeof v.query!=="string" || !v.query.trim() || v.query.length>4096)) throw new Error("Saved searches have an invalid format. The stored value was not changed.");
  return value.map(v=>({name:v.name,query:v.query}));
}
export function saveSearch(items: SavedSearch[], name: string, query: string): SavedSearch[] {
  name=name.trim();query=query.trim();
  if(!name || name.length>80 || !query || query.length>4096) throw new Error("Enter a search name of 1 to 80 characters and a query of 1 to 4096 characters.");
  const match=items.findIndex(s=>s.name.toLowerCase()===name.toLowerCase());
  if(match>=0) throw new Error("A saved search has this name. Use a different name or remove the existing search.");
  if(items.length>=30) throw new Error("You can save up to 30 searches.");
  return [...items,{name,query}];
}
