// Fork (11): fixtures for the meeting-prep commands in the demo harness. The
// main session wires them into `tauri-core.ts`:
//   case "fork_prep":          return ok(forkPrep(args.eventId));
//   case "fork_prep_upcoming": return ok(forkPrepUpcoming());
//   and in handleAi (fork_prep_brief added to AI_COMMANDS):
//   case "fork_prep_brief":    runPrepBrief(channel, requestId); return;
//
// Two events: 901 has two external guests (Anna, in Rebound with an open
// deal; Marcus, not in Rebound); 902 has one guest with no CRM card at all.
// The screenshot harness picks which one the reminder fires for through
// localStorage `skimdemo.prep_upcoming` = "901" | "902" (absent = none).
import * as db from "./data";

const NOW = Math.floor(Date.now() / 1000);

function thread(id: number) {
  const t = db.INBOX_THREADS.find((r) => r.id === id);
  if (!t) throw new Error(`fork-prep: no inbox thread ${id}`);
  return { ...t, accountId: "acc-1", folderId: 1, hitMessageId: id * 10 + 1 };
}

const ANNA_CRM = {
  person: {
    id: "p-anna",
    firstName: "Anna",
    lastName: "Weber",
    fullName: "Anna Weber",
    primaryEmail: "anna.weber@northwind.example",
    city: "Berlin",
    country: "DE",
    tags: ["champion"],
    lastActivityAt: new Date((NOW - 2 * 3600) * 1000).toISOString(),
  },
  company: {
    id: "c-northwind",
    name: "Northwind",
    domain: "northwind.example",
    industry: "Logistics software",
    employeeCountRange: "51-200",
  },
  deals: [
    { id: "d-1", name: "Northwind pilot", value: 24000, status: "open", stageId: "s-3", stageName: "Proposal" },
    { id: "d-0", name: "Discovery workshop", value: 3500, status: "won", stageId: "s-9", stageName: "Won" },
  ],
  activities: [
    {
      id: "a-1",
      activityType: "email_received",
      occurredAt: new Date((NOW - 2 * 3600) * 1000).toISOString(),
      payload: { subject: "Q3 launch — final checklist & open questions" },
    },
    {
      id: "a-2",
      activityType: "meeting_completed",
      occurredAt: new Date((NOW - 6 * 86400) * 1000).toISOString(),
      payload: { subject: "Pilot scoping call" },
    },
  ],
  reminders: null,
};

const EVENTS: Record<number, any> = {
  901: {
    eventId: 901,
    summary: "Q3 launch sync",
    startTs: NOW + 8 * 60,
    endTs: NOW + 38 * 60,
    guests: [
      {
        email: "anna.weber@northwind.example",
        name: "Anna Weber",
        threads: [thread(101), thread(105)],
        crm: ANNA_CRM,
      },
      {
        email: "marcus@acme-partners.example",
        name: "Marcus Lee",
        threads: [thread(102)],
        crm: null,
      },
    ],
  },
  902: {
    eventId: 902,
    summary: "Intro call",
    startTs: NOW + 9 * 60,
    endTs: NOW + 39 * 60,
    guests: [
      {
        email: "priya@brightwave.io",
        name: "Priya Nair",
        threads: [thread(103), thread(107)],
        crm: null,
      },
    ],
  },
};

export function forkPrep(eventId: number) {
  const e = EVENTS[eventId];
  if (!e) throw { code: "prep", message: "event not found" };
  return e;
}

/** The reminder fires for the event named in `skimdemo.prep_upcoming`. */
export function forkPrepUpcoming() {
  let id = 0;
  try {
    id = Number((globalThis as any).localStorage?.getItem("skimdemo.prep_upcoming"));
  } catch {}
  const e = EVENTS[id];
  if (!e) return [];
  return [
    {
      id: e.eventId,
      summary: e.summary,
      startTs: e.startTs,
      guests: e.guests.map((g: any) => ({ email: g.email, name: g.name })),
    },
  ];
}

export const PREP_BRIEF = `**Anna Weber** (Northwind, champion): the Q3 launch is Thursday and three items still need an owner; the *Northwind pilot* deal sits at Proposal ($24k). She is waiting on the landing-page copy from you.

**Marcus Lee** (Acme Partners, not in Rebound): contract redline v3 is ready, section 4.2 is the only open point; two comments are waiting for your reply.

Talking points: confirm the three launch owners, close 4.2 on the call, and ask Anna whether the pilot start moves with the launch.`;

/** Streams the fixture brief over the Channel with the same event shapes the
 *  Rust command sends: `reasoning` once, `delta` per token, then `done`. */
export function runPrepBrief(
  channel: { onmessage: (msg: any) => void },
  requestId: string,
  isCancelled: (id: string) => boolean = () => false,
  typingMs = 24,
  thinkMs = 420,
): void {
  const parts = PREP_BRIEF.match(/\S+\s*/g) ?? [PREP_BRIEF];
  let i = 0;
  const tick = () => {
    if (isCancelled(requestId)) return;
    if (i >= parts.length) {
      channel.onmessage({ type: "done", citations: [] });
      return;
    }
    channel.onmessage({ type: "delta", text: parts[i++] });
    setTimeout(tick, typingMs);
  };
  setTimeout(() => {
    if (isCancelled(requestId)) return;
    channel.onmessage({ type: "reasoning" });
    setTimeout(tick, 200);
  }, thinkMs);
}
