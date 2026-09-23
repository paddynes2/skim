// Fork (6.3, 6.4) fixtures: the words-as-HTML store behind the rich editor and
// the scheduled sends behind the "Scheduled" folder. Wired from
// `tauri-core.ts` through `forkComposeInvoke` (one case block, see
// docs/fork/pending/6.md).

const NOW = Math.floor(Date.now() / 1000);

/** `fork_draft_html` rows, by draft id. */
const HTML: Record<number, string> = {};

let opSeq = 9000;

export interface MockScheduled {
  opId: number;
  draftId: number;
  accountId: string;
  to: string;
  subject: string;
  snippet: string;
  notBefore: number;
  label: string | null;
}

/** Three sends on a hold, soonest first, as the Scheduled list shows them. */
export const FORK_SCHEDULED: MockScheduled[] = [
  {
    opId: 9001,
    draftId: 7001,
    accountId: "acc-1",
    to: "Anna Weber <anna.weber@northwind.example>",
    subject: "Re: Q3 launch",
    snippet: "Thanks Anna. Thursday works, I have moved the review to 14:00 so we can go through the numbers first.",
    notBefore: NOW + 2 * 3600,
    label: null,
  },
  {
    opId: 9002,
    draftId: 7002,
    accountId: "acc-1",
    to: "Marcus Lee <marcus.lee@contoso.example>",
    subject: "Board pack: two corrections",
    snippet: "Marcus, two figures in the pack need a second look before it goes out.",
    notBefore: NOW + 22 * 3600,
    label: "Tomorrow 08:00",
  },
  {
    opId: 9003,
    draftId: 7003,
    accountId: "acc-1",
    to: "Priya Nair <priya.nair@fabrikam.example>",
    subject: "Intro: Priya x Daniel",
    snippet: "Priya, meet Daniel. He runs the finance function at Fabrikam and has the same reporting problem you described.",
    notBefore: NOW + 5 * 86400,
    label: "Mon 08:00",
  },
];

let scheduled = [...FORK_SCHEDULED];

/** Handle a Phase 6 command. Returns `undefined` when `cmd` is not ours. */
export function forkComposeInvoke(cmd: string, args: any): { value: unknown } | undefined {
  switch (cmd) {
    case "fork_draft_html_get":
      return { value: HTML[args.draftId] ?? null };
    case "fork_draft_html_set":
      if (args.wordsHtml == null) delete HTML[args.draftId];
      else HTML[args.draftId] = args.wordsHtml;
      return { value: undefined };
    case "send_draft": {
      const opId = ++opSeq;
      if (args.notBefore != null) {
        scheduled = [
          ...scheduled,
          {
            opId,
            draftId: args.draftId,
            accountId: "acc-1",
            to: "",
            subject: "",
            snippet: "",
            notBefore: args.notBefore,
            label: args.label ?? null,
          },
        ].sort((a, b) => a.notBefore - b.notBefore);
      }
      return { value: opId };
    }
    case "fork_scheduled_list":
      return { value: scheduled };
    case "fork_cancel_scheduled": {
      const row = scheduled.find((s) => s.opId === args.opId);
      if (!row) return { value: Promise.reject(new Error("not scheduled")) };
      scheduled = scheduled.filter((s) => s.opId !== args.opId);
      return { value: row.draftId };
    }
    case "fork_send_now":
      scheduled = scheduled.map((s) => (s.opId === args.opId ? { ...s, notBefore: NOW, label: null } : s));
      return { value: undefined };
    default:
      return undefined;
  }
}
