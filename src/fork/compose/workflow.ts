export interface ComposeSnippet { name:string; text:string }
export function parseSnippets(raw:string|undefined):ComposeSnippet[]{
  if(!raw)return[];const value:unknown=JSON.parse(raw);
  if(!Array.isArray(value)||value.length>40||value.some(v=>!v||typeof v.name!=="string"||!v.name.trim()||v.name.length>80||typeof v.text!=="string"||!v.text.trim()||v.text.length>12000))throw new Error("Saved snippets have an invalid format. The stored snippets were not changed.");
  return value.map(v=>({name:v.name,text:v.text}));
}
export function addSnippet(items:ComposeSnippet[],name:string,text:string):ComposeSnippet[]{
  name=name.trim();text=text.trim();if(!name||name.length>80||!text||text.length>12000)throw new Error("Enter a name and snippet text, up to 12,000 characters.");
  if(items.length>=40)throw new Error("You can save up to 40 snippets.");
  if(items.some(s=>s.name.toLowerCase()===name.toLowerCase()))throw new Error("A snippet already has this name.");
  return[...items,{name,text}];
}
export function mentionsMissingAttachment(text:string,count:number):boolean{
  if(count>0)return false;
  const own=text.split(/\n\s*(?:On .+wrote:|>)/)[0].replace(/\n--\s*\n[\s\S]*/,"");
  return own.split(/[.!?\n]/).some(sentence=>!/(?:no|not|without|don't|do not)\s+(?:an?\s+)?attach(?:ment|ed)?/i.test(sentence)&&/\b(?:attached|attachment|enclosed|attaching)\b/i.test(sentence));
}
/** Keep older autosaves ahead of the final save used by a window transition. */
export function serialSaves<T>(persist:(value:T)=>Promise<void>){
  let pending=Promise.resolve();
  return(value:T)=>{const result=pending.catch(()=>{}).then(()=>persist(value));pending=result;return result;};
}
export async function transferDraft(save:()=>Promise<void>,open:()=>Promise<void>,finish:()=>void){await save();await open();finish();}
