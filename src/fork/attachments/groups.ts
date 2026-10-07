import type { AttachmentMeta } from "../../lib/types";
/** Only a content hash can establish identity. Filename and size alone cannot. */
export function groupAttachments<T extends AttachmentMeta>(files:T[],fingerprints:Record<number,string>):T[][]{
  const groups=new Map<string,T[]>();
  for(const file of files){const hash=fingerprints[file.id];const key=hash?`${hash}:${file.size}`:`id:${file.id}`;const group=groups.get(key);if(group){if(!group.some(old=>old.id===file.id))group.push(file);}else groups.set(key,[file]);}
  return [...groups.values()];
}
