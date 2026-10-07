# Reading pane 1.1.5 handoff

- Latest message opens by default. Older messages have individual sender, date and preview rows. Expand all loads missing messages with three workers. Collapse all keeps an active inline draft and its original message visible. A draft remains associated with its original message when another message is selected. The DOM contains one composer.
- Search conversation matches all headers and previews, plus text extracted with DOMParser from loaded RenderedBody HTML. Coverage is explicit. Load remaining messages is user-triggered and retries failed bodies. Files aggregates non-inline attachments from loaded bodies, grouped by source message, through the existing AttachmentChips.
- Same-thread metadata refresh retains body and view caches. Thread requests have sequence guards. Body refresh keeps the current body until a replacement succeeds, exposes refresh errors with Retry, and ignores stale results. First fetch has a visible text skeleton.
- Reply all is primary with multiple other participants. Otherwise Reply is primary. Forward and single-person Reply are secondary. AI actions are grouped at the right. Reply selection is explicit per expanded message.
- Strings in 115-reading-strings.json are merged into en.json.

Verification: npm run check passes except the existing ComposeForm warning. demo/v114-check.mjs retains its nav, logo and editor assertions and adapts history/preview/scope selectors. demo/v115-check.mjs passes in both Base themes. It checks in-thread search, two attachments after full loading, expansion, collapse, single composer, original draft target, list keyboard resize, Deals overview, saved search and settings persistence after reload, calendar visibility/workweek/month navigation, duplicate draft and unknown guest availability, following scope and normal-state absence of discard controls. It makes no live email or calendar writes.

Capture with SKIM_CAPTURE=1; default test execution does not rewrite screenshots. Final fictional captures live in docs/fork/shots/1.1.5. Inspected Inbox at 1200 and 900px, calendar preview at 1600px, and calendar editor at 900px. No clipped editor controls or horizontal pane overflow. Default in-thread search is collapsed and secondary controls remain quiet.

Demo mock set_setting persists fork keys to localStorage, matching get_settings. Static server 1421 is running as session57382 from demo/dist-demo in this worktree.

Limits: the fixture event bus does not emit live sync or mail refresh events. Error/retry and refresh race guards are implemented but not exercised by the demo. Full search/file coverage requires loading all message bodies; the interface states that boundary. No human visual acceptance is recorded. No commit was made by this agent.
