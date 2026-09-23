// Fork (6.5): fixtures for the AI-smell composer layer in the demo harness.
// The main session wires them into `tauri-core.ts`:
//   case "fork_smell_rewrite": return forkSmellRewrite(args, channel);
// and the composer's demo draft body can be `SMELL_FIXTURE_DRAFT` when the
// `skimdemo.fork_smell_fixture` flag is "on".
import { guard } from "../../src/fork/smell/factguard";

/** A draft carrying several tells: a stock opener (warn), an em dash (hard),
 *  an unfilled merge variable (hard), "rather than" three times (contrast
 *  repeat), a stock closer (warn), plus a signature and a quoted original that
 *  must NOT be scanned (they carry "delve" and a dash of their own). */
export const SMELL_FIXTURE_DRAFT = `Hi Jim,

I wanted to reach out about the handover pack for {{company}}. The fee is R140,000, due Tuesday 14:00 — that covers the full pack. Rather than hire, rather than outsource, rather than wait, the owner steps back and the buyer pays for a business that runs.

Let me know if you have any questions.

Patrick

-- 
Patrick Nesbitt, CA(SA), CFA
AutoSpark — delve into nothing

On Mon, 22 Sep 2026 at 09:14, Jim Carter <jim@acme.com> wrote:
> Thanks Patrick, delve into the numbers and revert.
`;

/** Three rewrites for the fixture's flagged sentences, plus one per sentence
 *  that changes a fact so the guard visibly rejects it (`rejected: 1`). */
const REWRITES: Record<string, string[]> = {
  "I wanted to reach out about the handover pack for {{company}}.": [
    "The handover pack for {{company}} is ready.",
    "Here is where the handover pack for {{company}} stands.",
    "The handover pack for {{company}} needs one decision from you.",
    "The handover pack is ready.",
  ],
  "Let me know if you have any questions.": [
    "Can you confirm Tuesday 14:00?",
    "Does Tuesday 14:00 still work for you?",
    "One question: does Tuesday 14:00 hold?",
    "Does Wednesday 14:00 still work for you?",
  ],
};

const GENERIC = (s: string) => [
  s.replace(/^I wanted to reach out about /i, "About ").replace(/—|–/g, ","),
  s.replace(/—|–/g, ":"),
  s.replace(/rather than /gi, "not "),
  s.replace(/\d/g, "9"),
];

/** Mirrors `fork::smell::fork_smell_rewrite`: streams a little liveness over
 *  the channel, then resolves the guarded options. */
export function forkSmellRewrite(
  args: { sentence: string; before: string; after: string; reason: string },
  channel: { onmessage: (m: unknown) => void },
): Promise<{ options: string[]; rejected: number }> {
  const candidates = REWRITES[args.sentence.trim()] ?? GENERIC(args.sentence.trim());
  const options = candidates.filter((o) => o.trim() !== args.sentence.trim() && guard(args.sentence, o)).slice(0, 3);
  const rejected = candidates.length - options.length;
  return new Promise((resolve) => {
    setTimeout(() => channel.onmessage({ type: "reasoning" }), 80);
    setTimeout(() => channel.onmessage({ type: "delta", text: "[" }), 200);
    setTimeout(() => {
      channel.onmessage({ type: "done", citations: [] });
      resolve({ options, rejected });
    }, 420);
  });
}
