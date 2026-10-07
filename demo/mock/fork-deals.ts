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

const manual:Record<number,string>={};
const excluded=new Set<number>();
const notes:Record<string,string>={};
const pins:Record<string,number>={};
let currentText=DEMO_DEALS_TEXT;
const nameOf=(id:number)=>excluded.has(id)?null:(manual[id]??DEALS[id]??null);

export function forkDealsList(offset: number, company: string | null = null) {
  if (offset > 0) return [];
  return db.INBOX_THREADS.filter((t) => nameOf(t.id) && (!company || nameOf(t.id) === company)).map((t) => ({
    ...t,
    accountId: "acc-1",
    messageId: null,
    deal: nameOf(t.id)!,
    dealDomain: forkDealsCatalog().deals.find(d=>d.name===nameOf(t.id))?.domains[0] ?? null,
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
    deal: nameOf(threadId),
    personEntry: t.fromAddr, companyEntry: domain === "gmail.com" ? null : domain, excluded: excluded.has(threadId),
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

export function forkDealsContext(company: string) {
  const conversations=forkDealsList(0,company);
  const attachments=conversations.flatMap(t=>db.renderedBody(t.id*10+1).attachments.map(a=>({...a,threadId:t.id,date:t.date})));
  return {conversationCount:conversations.length,conversations,people:conversations.map(t=>({name:t.fromName,addr:t.fromAddr})),attachments,meetings:[],notes:notes[company]??"",pinned:attachments.find(a=>a.id===pins[company])??null};
}
export function forkDealsCatalog(){const p=forkDealsPreview(currentText);for(const name of Object.values(manual))if(!p.deals.some(d=>d.name===name))p.deals.push({name,domains:[],addresses:[]});return p;}
type ScopeInput={threadId:number;name:string;scope:string;entry:string};
function scopeMatches(input:ScopeInput){if(input.scope==='conversation')return db.INBOX_THREADS.filter(t=>t.id===input.threadId);return db.INBOX_THREADS.filter(t=>input.scope==='person'?t.fromAddr===input.entry:t.fromAddr.endsWith(`@${input.entry}`)||t.fromAddr.endsWith(`.${input.entry}`));}
export function forkDealsPreviewScope(input:ScopeInput){if(!input.name.trim())throw new Error('Enter a company name.');if(!['conversation','person','company'].includes(input.scope))throw new Error('Choose a matching scope.');if(input.scope==='company'&&input.entry==='gmail.com')throw new Error('Use a person address for a personal mail provider.');if(input.scope==='conversation')return{count:1,entry:''};const potential=scopeMatches(input).filter(t=>!nameOf(t.id)||nameOf(t.id)===input.name||excluded.has(t.id));return{count:potential.length,entry:input.entry};}
export function forkDealsApplyScope(input:ScopeInput){if(!forkDealsPreviewScope(input).count)throw new Error('No matching conversations.');if(input.scope==='conversation')manual[input.threadId]=input.name;else{for(const row of scopeMatches(input)){if(!DEALS[row.id]||DEALS[row.id]===input.name)DEALS[row.id]=input.name;}delete manual[input.threadId];if(!currentText.includes(`${input.name}:`))currentText+=`\n${input.name}: ${input.entry}`;}excluded.delete(input.threadId);return currentText;}
export function forkDealsExclude(threadId:number,value:boolean){if(value)excluded.add(threadId);else excluded.delete(threadId);}
export function forkDealsSaveContext(company:string,value:string,pinnedId:number|null){if(value.length>20000)throw new Error('Notes too long.');if(pinnedId!==null&&!forkDealsContext(company).attachments.some(a=>a.id===pinnedId))throw new Error('Choose a file from this company.');notes[company]=value;if(pinnedId===null)delete pins[company];else pins[company]=pinnedId;}
