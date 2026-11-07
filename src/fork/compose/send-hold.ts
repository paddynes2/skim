// Fork (D95): what the compose window shows while a send is held.
//
// A send from the standalone compose window no longer closes the window at
// once. The window stays open through the undo hold, names who the mail goes
// to, counts down, offers Undo, and closes only after the send is confirmed.

import { splitAddresses } from "../calendar/guests";

/** The backend refuses a cancel inside this many seconds of the send time
 *  (scheduler.rs `cancel`), so Undo leaves the screen before then. */
export const CANCEL_MARGIN_SECS = 1;

/** With no `mail:sent` this long after the hold ends (a large attachment
 *  can take a while), the window closes
 *  anyway: the queued op stays, and a failure shows in the main window. */
export const CONFIRM_TIMEOUT_MS = 120_000;

/** How long "Sent" stays on screen before the window closes. */
export const SENT_LINGER_MS = 1_200;

/** Every recipient across To, Cc and Bcc, addresses only, de-duplicated. */
export function recipients(to: string, cc = "", bcc = ""): string[] {
  const out: string[] = [];
  for (const a of [...splitAddresses(to), ...splitAddresses(cc), ...splitAddresses(bcc)]) {
    if (!out.includes(a)) out.push(a);
  }
  return out;
}

/** Whole seconds left until `notBefore` (unix seconds), never below zero. */
export function secondsLeft(notBefore: number, nowMs: number): number {
  return Math.max(0, Math.ceil(notBefore - nowMs / 1000));
}

/** Undo still works: the hold has more than the cancel margin to run. */
export function canUndo(notBefore: number | null, nowMs: number): boolean {
  return notBefore !== null && notBefore - nowMs / 1000 > CANCEL_MARGIN_SECS;
}
