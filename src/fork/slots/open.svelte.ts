// Which composer asked for /slots, if any (PLAN.md Phase 8). The popover is
// mounted by the shell while `pending` is set; `insert` is the composer's
// cursor writer, handed over by `openSlotsPopover`.

let pending = $state<{ insert: (text: string) => void } | null>(null);

export const slotsOpen = {
  get pending() {
    return pending;
  },
  open(insert: (text: string) => void) {
    pending = { insert };
  },
  close() {
    pending = null;
  },
};
