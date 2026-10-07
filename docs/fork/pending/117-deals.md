# Deals expansion and identity polish

## Implemented

- DealBadge keeps a 28px fixed slot (root can set --deal-logo-size for compact rows). Logos are contained with 3px padding on a white canvas; loading/missing/broken sources use warm initials in the same geometry. The accessible company name and tooltip remain. Optional onclick opens company context.
- DealTool has a visible Add to Deals label, warm context styling, an explicit scope selector, live cached conversation count and conversation exclusion/restoration. Existing personal-provider, own-domain and domain-boundary rules remain. Counts respect first-rule precedence and manual exclusions. This conversation only uses an explicit local override and does not broaden any domain rule.
- Local fork_deal_details JSON stores explicit thread assignment/exclusion, company notes and pinned attachment IDs. This uses the existing settings table, no migration. Parsing errors preserve stored content and fail visibly. Notes are bounded to 20,000 characters; membership metadata has explicit size limits.
- CompanyOverview has private local notes, a pinned current document and conversation/people/files/meeting sections. Files reuse AttachmentChips for preview, save-all and original-message links. Notes have explicit Save and Cancel. Unsaved drafts survive switching/closing company overview within the current app session.
- Pinned files are validated against that company's live cached threads. A document from another company, Trash/Spam-only metadata, inline content, unknown IDs and oversized notes are rejected. Notes are never included in outbound mail.
- Manual company membership participates in the Deals cache key, list/count/suggestion/context. Only live threads outside Trash/Spam qualify. Local settings remain the sole authority; no Rebound or AI writes.

## Integration

Root mounts a clickable DealBadge in the message row identity slot. dealsStore.openCompany(name) opens the selected company in Deals with overview expanded. dealsStore.requestAdd(threadId) lets the row context menu select a conversation then request its existing Add to Deals form.

Register fork::deals::{fork_deals_catalog,fork_deals_preview_scope,fork_deals_apply_scope,fork_deals_exclude,fork_deals_save_context}. Commands are exported by the parent deals module (including Tauri generated macros). Payload shapes are in src/fork/deals/api.ts. Merge 117-deals-strings.json. Demo route helpers with the same function names in camelCase are in demo/mock/fork-deals.ts; scoped apply returns updated canonical text.

Source ownership: src/fork/deals/{DealBadge,DealTool,DealsToolbar,CompanyOverview}.svelte, api.ts, store.svelte.ts; src-tauri/src/fork/deals.rs and deals_details.rs; demo/mock/fork-deals.ts. No shared tokens, title/header, migration, version or installer changes by this builder.

## Verification

- cargo test --lib fork::deals: 11 passed, including two new behavior tests for matching scopes/manual override/exclude/restore and notes/pinned document ownership/malformed JSON retention.
- npm run check: 0 errors, 1 existing ComposeForm smell reactivity warning.
- UI sources skill, colour/composition guidance and current STATUS/DECISIONS read. The established warm account swatch precedent and root's dedicated deal tokens informed a restrained context treatment. Existing components and shared attachment preview are reused; no package added.
- node demo/v117-deals-check.mjs: passed in Base dark and light. Checks loaded and failed logos at 28px, scoped count/apply, Escape preserving the reading selection, saved notes and pinned document after reopening, exclusion and restoration. Company pinned-file and Original email links open exact source message 1011 instead of newer reply 1019. Screenshots: docs/fork/shots/1.1.7/company-base-dark.png and company-base-light.png. Both renders inspected.
- All three semantic mutations were killed by behavioral assertions: excluded-map removal, pinned-file ownership guard, assignment-map override. Source restored byte-for-byte, then all 11 Deals tests passed again. Details and restored SHA256: 117-deals-mutation.md. Root owns the formal prove-test run after commit and installation.

## Limits

Company context and preview counts use cached mail/calendar data. Manual conversation-only companies use known external correspondents for people/meeting matches. Pinned metadata is cleared from the visible panel if its source falls out of the local cache. Exclusion overrides one cached thread ID, not future independent conversations with the same subject. Closing the app without saving notes discards that session's note drafts; saved notes persist. Domain scopes append to existing company rules rather than replacing them.
