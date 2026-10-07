# Deals and search, 1.1.5

Design intent: reuse Base controls, typography and the company logo. Keep the list as the main task. The optional company overview and search form use progressive disclosure.

Source lookup: ui-sources and docs/UI-LANE read. Corpus search "search filter forms progressive disclosure", Svelte stack: 0 matches across 55 entries and 59 harvested files. Adapted the existing Deals parser, Base form styling and SearchChips query semantics. No package or third-party component added.

## Integration

- Mount DealsToolbar below the list header while mail.dealsView. Props: onselect(name), onthread(threadId), onevent(eventId), onsearch(query). It sets dealsStore.selectedCompany before onselect; refresh the list there. Pass dealsStore.selectedCompany || null as the third dealsApi.list argument. Filtering occurs before paging.
- Mount SearchTools below the list header. Props: query (current query or empty), onsearch(query). Existing SearchChips continues to render current operators.
- Register fork::deals::fork_deals_context. Add fork_saved_searches to the settings allowlist. It stores bounded JSON in the existing settings table, no migration.
- Merge 115-deals-search-strings.json into en.json.
- Demo tauri-core: import forkDealsContext, dispatch fork_deals_context with args.company, pass args.company to forkDealsList.
- SettingsDeals is replaced in place; its existing mount stays.

## Behaviour

Company rows preserve the original bytes of untouched lines, including malformed entries and comments. Edits are a local draft until Save companies. Cancel reconstructs the stored text. Parser warnings expose ignored entries without deleting them. List membership remains controlled by the same fork_deals text and backend parser.

The selector filters every paged company conversation. The optional overview shows the latest 12 conversations, up to 30 company people, the latest 30 attachment entries, and next 20 meetings. People match the selected company's domain or exact configured personal email, not every external participant in a thread. Attachments open their source conversation. Meetings are local cached events matched by organizer or attendee; cancelled and ended events are excluded. All-day dates display in UTC to match stored date-only values. The UI states the cache/limits. No remote service or mail body fetch is involved.

Search controls edit positive sender/company/date/attachment filters while retaining other operators and negatives. Company values are a comma-separated OR list of exact email addresses or domains. Domains match the domain itself and subdomains, across from/to/cc. User text is a bound SQLite parameter. Company names in the select resolve to their configured domains/addresses. Saved searches require a distinct name, max 80 characters, nonempty query max 4096 characters, at most 30. Invalid persisted JSON disables writes and reports the failure, preserving the original setting.

## Focused verification

- node --test src/fork/tests/deals115.test.mjs src/fork/tests/search115.test.mjs: 3 pass. Covers malformed line preservation/cancel, negative and unrelated query preservation, saved-search round trip and rejected malformed input/duplicate/limit.
- npm run check: zero errors, existing ComposeForm smell warning.
- Rust cargo test --lib company_: 2 passed (company filtering before paging, context participant/meeting boundaries, domain/address search boundaries and recipient matching).
- Root owns final browser checks, aggregate gates, proof and installed readback. New test files need prove-test treatment with the combined implementation.

No source-control commit, migration, live email write, invitation, credential access or new dependency was made by this builder.
