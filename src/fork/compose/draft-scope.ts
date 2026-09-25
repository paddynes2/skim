import { api } from "../../lib/api";
import { draftMessageIds } from "./draft-selection";

export async function draftFolderIds(folderId: number, accountIds: string[]): Promise<number[]> {
  if (folderId >= 0) return [folderId];
  return (await Promise.all(accountIds.map((id) => api.listFolders(id))))
    .flat().filter((f) => f.role === "drafts").map((f) => f.id);
}

/** A Drafts action must never include the conversation's sent copies. */
export async function scopedDraftIds(
  threadId: number, selectedId: number | null, folderIds: number[],
): Promise<number[]> {
  return draftMessageIds((await api.getThread(threadId)).messages, folderIds, selectedId);
}
