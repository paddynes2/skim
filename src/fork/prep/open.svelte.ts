// Which event's prep panel is open, if any (PLAN.md Phase 11). The reminder
// toast's "Prep" action and the calendar event panel's "Prep" button both set
// it; the shell mounts PrepPanel while it is non-null.

let eventId = $state<number | null>(null);

export const prepOpen = {
  get eventId() {
    return eventId;
  },
  open(id: number) {
    eventId = id;
  },
  close() {
    eventId = null;
  },
};
