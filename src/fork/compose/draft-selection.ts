/** Threads span folders. Never turn their latest sent message into a draft. */
export function draftMessageIds(
  messages: readonly { id: number; folderId: number }[],
  draftFolderIds: readonly number[],
  selectedId: number | null = null,
): number[] {
  const candidates = messages.filter((m) => draftFolderIds.includes(m.folderId));
  return candidates.filter((m) => selectedId === null || m.id === selectedId).map((m) => m.id);
}

export function draftMessageId(
  messages: readonly { id: number; folderId: number }[],
  draftFolderIds: readonly number[],
  selectedId: number | null,
): number | null {
  return draftMessageIds(messages, draftFolderIds, selectedId).at(-1) ?? null;
}
