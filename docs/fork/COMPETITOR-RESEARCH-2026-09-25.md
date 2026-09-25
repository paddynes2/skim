# Skim: competitor research and product direction

Research date: 25 September 2026. Prepared for Patrick's personal Skim fork.

## Recommendation

Make Skim exceptional at **finishing important correspondence**. Open a conversation and find the relevant evidence, a useful reply, and a precise account of what remains unresolved. Start a short session and leave with a few real outcomes completed.

The strongest three investments are prepared replies with evidence, specific follow-up reminders, and a bounded Finish session. Reliable, observable receipt of mail is the prerequisite. Saved views and snooze are useful supporting features, but another broad AI chat panel would add little differentiation.

**Patrick's direct feedback during this research:** the existing “On me” and “Waiting” screens are useless. This overrides the initial assumption that those screens were useful foundations. Retire them as broad primary destinations in the proposed design; do not spend the next cycle polishing their labels or adding more inferred threads. Validate explicit per-conversation requests first. Only introduce an aggregate view if repeated use demonstrates a need. No screens were removed during this research task.

This is a product proposal, not a claim that these features have been implemented. The separate inbox-latency change is tracked in STATUS.md.

## Method and limits

Reviewed official product documentation across twelve current competitors and a retired adjacent product, then compared their mechanisms with Skim's source. Vendor descriptions establish advertised behavior, not measured speed, accuracy, adoption or ROI. No paid accounts or trials were created. BrowserOS was unavailable locally, so this is documentation and source research, not a hands-on usability benchmark. Availability and packaging can change after this date.

The recommendations target Patrick's existing Windows client and OS, not a hypothetical mass-market email startup. Architecture recommendations are inferences. Effort ranges are rough engineering estimates, contingent on data ownership, fixtures and UX scope.

## What Skim already has

The baseline is considerably stronger than a basic IMAP client: local SQLite/FTS search, multiple accounts, IDLE, offline operations, keyboard navigation, rich replies, undo and scheduled sending; own-voice AI drafting, summaries, cited mailbox search, attachment context and model choice; calendar/availability, meeting preparation, a CRM drawer, waiting/on-me views and a local MCP surface.

Source anchors, relative to the repository:

| Existing capability | Inspected source | Actual gap |
|---|---|---|
| AI recap and sent-mail style context | `src-tauri/src/commands/ai.rs:1008`, `:1181` | Preparing the right response before the user asks; tracking its freshness |
| Cited mailbox assistant | `src-tauri/src/ai/agent.rs:113`, `:371` | Evidence attached to individual outgoing factual claims, not just an answer's citations |
| Waiting/on-me classification | `src-tauri/src/fork/court.rs:133` | Last-message direction is not a ledger of multiple outstanding deliverables |
| Meeting preparation | `src-tauri/src/fork/prep.rs:1` | Reusing approved meeting outcomes and obligations in the reply flow |
| Read-only CRM lookup/cache | `src-tauri/src/fork/crm.rs:1` | Just-in-time context with clear source and age; live login is a separate operational prerequisite |
| Feature exclusions | `docs/fork/DECISIONS.md:21` | D12 deferred splits, snooze, screener, bundles and templates because they were unrequested; reconsider selectively |

These are source observations, not proof that every integration is connected or exercised live. Do not rebuild capabilities already present under new names.

## What the best competitors teach us

| Product | Distinctive mechanism and evidence | Adaptation for Skim |
|---|---|---|
| **Superhuman** | Replies and follow-ups prepared ahead of time; missing-information placeholders; refreshed candidates stop auto-updating after human edits. [Auto Drafts](https://help.superhuman.com/hc/en-us/articles/46005658551053-Auto-Reminders-Auto-Drafts) | Move from a generate button to a ready-to-review candidate. Preserve edited drafts, expose missing facts and refresh separately. |
| **Shortwave** | Search-backed splits, bundles, delivery schedules and grouped todos organize attention. Its assistant adds contextual search and writing. [Settings](https://www.shortwave.com/docs/guides/customize-your-shortwave-settings/), [assistant](https://www.shortwave.com/docs/guides/ai-assistant/) | Offer user-chosen saved searches only when useful; avoid more inferred responsibility screens. Keep receipt immediate even when presentation is batched. |
| **Spark** | Its assistant describes a local private index and sending selected relevant results to the model; focus, set-aside and collaborative tools support processing. [AI](https://sparkmailapp.com/features/ai-assistant), [features](https://sparkmailapp.com/features) | Retrieve narrowly and show context coverage. Local search plus AI is established competition, not a sufficient differentiator alone. |
| **HEY** | Sender screening and separate Imbox, Feed and Paper Trail; Reply Later leads to Focus & Reply. [How it works](https://www.hey.com/how-it-works/) | A finite reply session is more compelling than an endless inbox. Offer noise separation with an obvious full-inbox escape. |
| **Fyxer** | Email categorization and prepared replies inside Gmail/Outlook; meeting notes feed follow-up and preparation. [Email](https://www.fyxer.com/ai-email-assistant), [meetings](https://www.fyxer.com/ai-meeting-notes) | Connect existing OS meeting outputs to correspondence. Avoid building another recorder or copying its marketing productivity numbers. |
| **Missive** | Shared conversations, assignments, internal discussion, collaborative drafts and contextual tasks. [Features](https://missiveapp.com/features) | Borrow clear ownership and context-preserving handoffs. A full shared-inbox product is unnecessary for this personal fork. |
| **Front** | Routing, owners, reply-time goals and response/resolution analytics. [Workflows](https://front.com/product/workflows) | Track the person and expected outcome behind a follow-up. Measure resolution, not how many emails were archived. |
| **Gmail/Gemini** | AI Inbox assembles suggested todos and topics. Its documented beta coverage excludes attachments and several mailbox states, and is limited by geography/language/plan. [Coverage and eligibility](https://support.google.com/mail/answer/16845247?hl=en) | Every AI overview needs a coverage statement. Patrick's South African context must not be assumed eligible for the US-English beta. |
| **Outlook/Copilot** | Assigns priorities with explanations; classification runs alongside delivery so it does not delay incoming mail. [Prioritize](https://support.microsoft.com/en-au/outlook/copilot-outlook/prioritize-my-inbox) | Make this a strict Skim architecture rule: store and display mail first; enrich asynchronously. |
| **Canary** | Markets local indexing/triage alongside cloud-assisted drafting, summaries and answers. [AI](https://canarymail.io/features/ai) | Explain data boundaries per operation. Do not claim that competitors universally train on private email, or that local indexing makes cloud inference local. |
| **Mimestream** | Native Gmail client; Private Push uses opaque change identifiers through a relay, with mailbox sync performed by the app. Local diagnostic logs support investigation. [Security architecture](https://mimestream.com/trust/security-and-privacy) | Invest in provider-aware reliability and content-free diagnostics. This does not authorize a new direct Google integration or replacing IMAP now. |
| **SaneBox** | Separates lower-priority mail, offers reminders/snooze, and describes fail-open behavior: mail still arrives if sorting fails. [FAQ](https://www.sanebox.com/faq), [features](https://www.sanebox.com/help/138-what-is-a-feature) | Classification failure must leave received mail available. Distinguish deferred attention from delayed delivery. |

Notion Mail is a historical reference rather than an active recommendation: Notion's official indexed retirement notice gave 22 September 2026 as its shutdown date; the former product/help endpoints now redirect. This research does not establish why it was retired. [Official help category](https://www.notion.com/en-gb/help/category/notion-mail)

Two additional lessons: Superhuman's [split inbox](https://help.superhuman.com/hc/en-us/articles/46005619081101-Default-Split-Inbox) makes keyboard switching between a few work queues concrete; Spark now documents a [desktop CLI](https://sparkmailapp.com/help/spark-cli), so agent access alone is also becoming ordinary. Skim's advantage should be the quality and continuity of the work these tools enable.

## Three significant improvements

### 1. Prepared replies with evidence

**Experience:** A hypothetical client asks for pricing, availability and an attachment. Skim presents a candidate response with the known facts filled in, a clear placeholder for an unconfirmed price, current availability from the permitted source, and an explicit missing-attachment task. Selecting a price or date opens the exact supporting passage and its source date.

This extends existing drafting and cited retrieval. It does not equate a plausible citation with truth. Evidence can be conflicting, old, incomplete or irrelevant. A claim can be supported by a source and still need human judgment.

The first version should operate only on user-selected threads or a small explicitly enabled queue. Store a candidate separately from the user's draft. Associate each factual claim with a draft revision, source account/identifier/version, exact passage, observation time and support status. Verify the actual outgoing attachment rather than merely quoting an email that mentions it. New source material marks affected claims stale. Human edits invalidate affected associations; generation never silently overwrites those edits.

Before broad precomputation, establish model budgets, cancellation and visible coverage. Do not process the entire mailbox to fill it with mediocre drafts. When context is missing, prepare the precise question that would unblock the reply.

**First implementation step:** a sanitized fixture and data contract for revision-bound evidence, including unsupported facts, contradictory prices, revised dates, missing files and account separation. Then an on-demand prototype using the existing assistant retrieval layer.

**Evaluate:** human-reviewed support precision, missed contradictions, stale-evidence detection, editing time, candidate rejection and zero overwritten human drafts. Compare with today's generate-on-demand flow on the same tasks.

### 2. Specific reminders for what is still owed

**Experience:** Patrick requests both a signed agreement and final pricing. A reply supplies the agreement but says pricing will follow. The conversation should still show “Waiting for final pricing”; a generic “Thanks” must not clear it. A suggested follow-up mentions only the remaining item.

Current ball-in-court classification is too coarse for Patrick's workflow: the identity of the last sender cannot determine whether every request was fulfilled, and he finds both resulting screens useless. Replace the interaction with an optional, explicit request attached to the conversation: deliverable, owner, evidence and optional date. Surface it when actionable, rather than populating another permanent queue. Distinguish a requested date from a date the other party agreed to. Never invent a deadline from urgency alone.

At drafting time suggest the expected return: who, what and when. Let the user confirm it. Incoming messages propose partial completion, clarification or cancellation; uncertain matches remain open. Multiple requests and revised scope retain history rather than merging into a vague single task.

Skim should display and update the accepted OS/Rebound commitment record through an agreed adapter, rather than create a competing task system. A small local suggestion cache can remain provisional until accepted. Integration ownership must be settled before a production schema change.

**First implementation step:** a fixture set covering partial replies, acknowledgments without delivery, reassignment, changed scope, canceled requests and rescheduled dates. Define the expected outstanding obligations after each message.

**Evaluate:** false closures first, then missed completions, corrections per suggestion, duplicate commitments and time spent reconstructing outstanding work. A false closure can cost more than an extra reminder.

### 3. Finish a few things in five minutes

**Experience:** “Finish three replies” opens one conversation at a time with its relevant history, attachment and saved draft. Each has a concrete finish condition. The user can complete it, mark a dependency, skip or stop. Ending the session restores the original inbox position and preserves work.

HEY demonstrates the value of collecting replies into a focused session. Skim can combine that with its keyboard workflow and preparation features. The first version should accept manually selected threads; AI selection is a later experiment. Limit the queue to three, and make five minutes an invitation rather than a countdown pressure mechanism.

A saved draft is progress, not a completed correspondence outcome. A blocked item must leave with a named dependency rather than disappear behind a “done” animation. Sending remains a deliberate user action in the client; background agents remain draft-only.

**First implementation step:** a session envelope holding account/thread references, draft references, original selection/scroll state and completion state. Verify interruption and restoration before adding ranking.

**Evaluate:** accepted queue suggestions, actual completions, unexpectedly blocked items, voluntary stops, restoration accuracy and time to resume. Avoid streaks, guilt and archive-count gamification.

## Reliability comes first

The original complaint—late mail—undermines every smart feature. Measure the stages separately: provider notification, sync start, header commit, UI update, body availability and AI completion. Server Date headers alone do not measure client delivery latency reliably.

The current latency patch bounds silent IDLE/SELECT/DONE failures, shortens reconnection recovery, wakes the worker before DONE and emits updates as soon as new headers are committed. It is not proof of instant end-to-end receipt: folder scans and queued operations still share the worker.

Next investigate priority handling for inbox sync, reconnect after sleep/network transitions, and whether historical backfill can block receipt. Keep diagnostics bounded and content-free. Show account freshness and a recoverable error state without making users understand IMAP. Distinguish “offline”, “last checked”, “server accepted outgoing mail” and “delivered”; they are different facts.

Proposed acceptance targets, **not current measurements**: under a healthy connection, p95 notification-to-visible-header below two seconds; UI reacts within 100 ms of receiving its update event; a dead connection surfaces and recovers within documented timeout bounds. Validate with scripted servers plus network-loss and sleep/wake scenarios. Do not promise literal instant delivery across providers.

## Sequence and scope

| Order | Deliverable | Rough effort | Gate before expanding |
|---|---|---|---|
| 0 | Install tested sync recovery; establish stage timings and reproducible failure cases | 2–5 engineering days for follow-on observability/queue work | Demonstrated recovery, no new header duplication, no enrichment blocking receipt |
| 1 | Manual Finish session and a few saved search views | 3–5 days | Correct restoration and useful completion in repeated personal use |
| 2 | On-demand reply candidate with revision-bound evidence and missing facts | 1–2 weeks | Human-reviewed evidence quality and draft preservation |
| 3 | Explicit per-thread requests and useful reminders with accepted OS ownership | 2–3 weeks | Demonstrated usefulness to Patrick; partial replies do not falsely close obligations; no duplicate authority |
| 4 | Limited precomputation and event-triggered context preparation | 1–2 weeks after prior gates | Better time-to-correct-reply at a bounded cost |

Estimates exclude major connector, permission or schema changes and are not delivery promises. Existing features may shorten implementation; correctness work may lengthen it. Stage 2 and 3 are the larger product investments even though the simpler session prototype comes first.

Snooze and saved views are sensible supporting additions under a narrow reopening of D12. Begin with explicit queries and user choices, not an opaque importance algorithm. Meeting context should reuse the existing OS ingestion pipeline; CRM should remain the authoritative business source where applicable. Do not add another meeting bot, CRM, cross-platform rewrite or team-support suite in this cycle.

Measure **time to correctly finish important correspondence**, missed/overdue obligations and recovery from interruptions. Establish a baseline before claiming improvement. Compare the same representative, sanitized tasks, and require zero human-draft overwrite or cross-account evidence leakage in verification. Privacy claims should explain actual data paths, model choices and retention instead of relying on the word “local”.

## Divergent exploration and pruning

Five independent frames produced thirty possibilities, followed by focused exploration of the top three. Concurrency limits required the five exploratory branches to run in groups. Scores are subjective prioritization aids, not research measurements: novelty N, user value V and feasibility F each range 1–10; weighted score = 0.35N + 0.40V + 0.25F. Feasibility scores describe a bounded prototype, not a production guarantee.

| ID / frame | Candidate | N | V | F | Score |
|---|---|---:|---:|---:|---:|
| L1 logistics | Batch related replies into dispatch windows | 6 | 7 | 9 | 7.15 |
| L2 logistics | Missing-parts queue for blocked drafts | 8 | 8 | 8 | 8.00 |
| L3 logistics | Expected return: owner, deliverable and date | 9 | 9 | 10 | 9.25 |
| L4 logistics | Group work by shared decision or document | 8 | 8 | 6 | 7.50 |
| L5 logistics | Prepare context before a meeting/deadline | 7 | 9 | 7 | 7.80 |
| L6 logistics | Check draft against actual reply requirements | 7 | 9 | 8 | 8.05 |
| G1 game | Resume exact draft, selection and intention | 7 | 8 | 9 | 7.90 |
| G2 game | Bounded five-minute Finish session | 8 | 9 | 10 | 8.90 |
| G3 game | Personal keyboard triage combinations | 6 | 6 | 8 | 6.50 |
| G4 game | Rehearse batch actions before applying | 7 | 7 | 8 | 7.25 |
| G5 game | Suggest one next move for a difficult thread | 7 | 8 | 8 | 7.65 |
| G6 game | Deliberate exit with a saved next action | 7 | 8 | 9 | 7.90 |
| A1 remove assumptions | Decisions as an alternative home view | 9 | 8 | 5 | 7.55 |
| A2 remove assumptions | Gather scattered threads by outcome | 8 | 9 | 5 | 7.65 |
| A3 remove assumptions | Build decisions and evidence before prose | 8 | 9 | 7 | 8.15 |
| A4 remove assumptions | Resurface when a condition changes | 8 | 9 | 6 | 7.90 |
| A5 remove assumptions | Cited changes across thread revisions | 8 | 8 | 6 | 7.50 |
| A6 remove assumptions | Preview obligations remaining after triage | 8 | 8 | 6 | 7.50 |
| O1 3am operations | Clear outgoing queue/acceptance states | 5 | 10 | 9 | 8.00 |
| O2 3am operations | Account freshness and paused-sync state | 4 | 10 | 10 | 7.90 |
| O3 3am operations | Recovery tray for interrupted operations | 7 | 9 | 7 | 7.80 |
| O4 3am operations | Per-message action history and recovery | 7 | 8 | 7 | 7.40 |
| O5 3am operations | Reconcile uncertain sends before retry | 6 | 10 | 6 | 7.60 |
| O6 3am operations | Offline continuity with visible pending work | 4 | 9 | 8 | 7.00 |
| R1 regulator | Evidence receipt for each factual draft claim | 8 | 9 | 10 | 8.90 |
| R2 regulator | Show obligations introduced by a draft | 8 | 9 | 7 | 8.15 |
| R3 regulator | Mark sources disputed or superseded | 8 | 8 | 6 | 7.50 |
| R4 regulator | Preview recipient exposure to internal context | 7 | 9 | 6 | 7.55 |
| R5 regulator | Show AI coverage and source freshness | 6 | 9 | 9 | 7.95 |
| R6 regulator | Turn missing support into a precise question | 7 | 8 | 9 | 7.90 |

Clusters: prepared work (L2/L5/L6/A3/R1/R2/R3/R6); obligation continuity (L3/A2/A4/A5/A6); focused execution (L1/L4/G1–G6/A1); reliability and trust (O1–O6/R4/R5). L3, R1 and G2 became the focused recommendations above. Lower-novelty reliability work remains a prerequisite despite the novelty-weighted ranking.

Focus findings sharpened the shortlist: partial fulfillment is the hard case for Waiting; draft/source revision identity is the hard case for evidence; interruption and truthful completion are the hard cases for Finish. Useful subsequent variants are residual-only follow-ups, changed-context review, attachment receipts, explicit blocked exits and one-thread sessions.

Reject these traps: autonomous sending; preparing every possible reply; hiding invoices behind uncertain priority labels; a second commitment database; treating citations as guarantees; treating an email open as proof of understanding; a visual redesign before reliable receipt; copying a competitor's entire feature list. Keep the ordinary inbox available throughout.

The hypothesis worth testing is concrete: **can Patrick open Skim, find three important conversations already prepared, and finish them correctly with much less reconstruction?**
