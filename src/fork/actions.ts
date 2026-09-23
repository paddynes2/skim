// The fork's one action module (PLAN.md 3.2 grows this). Phase 1.2 seeds it
// with the rule for where Archive is offered.

/** Roles in which "archive" makes no sense: the mail is already filed away
 *  (Sent), or leaving (Trash, Spam). Gmail would create an "Archive" label. */
const NO_ARCHIVE_ROLES = new Set(["sent", "trash", "junk"]);

/** Whether the Archive action is offered for mail in a folder with `role`
 *  (`null` = a user label, `undefined` = unknown/unified: offered). */
export function archiveOffered(role: string | null | undefined): boolean {
  return !(role && NO_ARCHIVE_ROLES.has(role));
}
