# Phase 7 UI (7.5 calendar screen, 7.6 Meet button + composer seam, 7.7 Settings → Calendar) + Phase 8 `/slots` popover — pending merge

Built by the Phase 7/8 UI agent on top of `docs/fork/pending/7.md` (the Rust
half and `src/fork/calendar/{api,types}.ts`). Everything lives under
`src/fork/calendar/`, `src/fork/slots/` and `demo/mock/fork-calendar.ts`;
three upstream files carry a seam each (TOUCHLIST below). Nothing in this
phase sends, creates or changes a real calendar event: the demo mock is the
only backend the UI talked to during the build.

## mod.rs

Nothing (no Rust in this half).

## generate_handler

Nothing new. The sixteen commands in `pending/7.md` are what the UI calls.

## commands.rs

Nothing.

## settings ALLOWED

All six were pre-declared in `fork/settings.rs` (per `pending/7.md`). The UI
reads them through `get_settings` and writes them through `set_setting` from
`src/fork/calendar/settings.svelte.ts` (`calPrefs`):

    fork_cal_default_len   // 15 | 30 | 45 | 60 (minutes); quick-create + `n` length; Settings chips
    fork_cal_work_start    // "HH:MM"; Settings time input
    fork_cal_work_end      // "HH:MM"; Settings time input
    fork_cal_work_days     // "1,2,3,4,5" Mon=1 … Sun=7; Settings day chips
    fork_cal_second_tz     // IANA zone; Settings; default recipient zone of /slots
    fork_booking_link      // free text; Settings; appended by /slots

`calPrefs` hydrates itself lazily (`calPrefs.load()`, once) rather than
through `prefs.hydrate`, so this phase did not edit `stores/prefs.svelte.ts`
(two other phases were editing it). Optional later fold: call
`calPrefs.hydrate(settings)` next to `prefs.hydrate(settings)` in App.svelte's
boot block and drop the lazy load.

## en.json

Existing keys reused: `fork.nav.calendar`, `fork.nav.meet_now`, `fork.prep.prep`.
New keys (the `fork.cal.err.*` strings are the ones `pending/7.md` proposed):

```json
{
  "fork.cal.settings_title": "Calendar",
  "fork.cal.client_id": "Client ID",
  "fork.cal.client_secret": "Client secret",
  "fork.cal.client_help": "Paste the Desktop-app OAuth client from Google Cloud (Calendar API + Meet REST API enabled, consent screen Internal).",
  "fork.cal.client_stored": "Using the pasted client.",
  "fork.cal.client_built_in": "Using the built-in client.",
  "fork.cal.save": "Save",
  "fork.cal.saved": "Saved",
  "fork.cal.clear": "Clear",
  "fork.cal.account": "Account",
  "fork.cal.connected": "Connected",
  "fork.cal.not_connected": "Not connected",
  "fork.cal.connect": "Connect",
  "fork.cal.connecting": "Connecting…",
  "fork.cal.disconnect": "Disconnect",
  "fork.cal.connect_needs_client": "Connect needs a client ID first.",
  "fork.cal.pending_ops": "{n} pending",
  "fork.cal.calendars_shown": "Calendars shown",
  "fork.cal.primary": "primary",
  "fork.cal.read_only": "read only",
  "fork.cal.default_len": "Default length",
  "fork.cal.minutes": "{n} min",
  "fork.cal.working_hours": "Working hours",
  "fork.cal.work_end": "End of working hours",
  "fork.cal.working_days": "Working days",
  "fork.cal.day_mon": "Mon",
  "fork.cal.day_tue": "Tue",
  "fork.cal.day_wed": "Wed",
  "fork.cal.day_thu": "Thu",
  "fork.cal.day_fri": "Fri",
  "fork.cal.day_sat": "Sat",
  "fork.cal.day_sun": "Sun",
  "fork.cal.second_tz": "Second time zone",
  "fork.cal.booking_link": "Booking link",
  "fork.cal.today": "Today",
  "fork.cal.prev": "Previous",
  "fork.cal.next": "Next",
  "fork.cal.views": "View",
  "fork.cal.view_day": "Day",
  "fork.cal.view_week": "Week",
  "fork.cal.view_month": "Month",
  "fork.cal.view_agenda": "Agenda",
  "fork.cal.new_event": "New event",
  "fork.cal.event": "Event",
  "fork.cal.untitled": "(No title)",
  "fork.cal.no_events": "Nothing scheduled",
  "fork.cal.open_settings": "Open Settings → Calendar",
  "fork.cal.setup_unconfigured": "Google Calendar is not set up yet. Paste the OAuth client ID and secret in Settings, then connect your account.",
  "fork.cal.setup_disconnected": "Google Calendar is not connected for this account. Connect it in Settings to see and edit your events.",
  "fork.cal.title_placeholder": "Title",
  "fork.cal.all_day": "All day",
  "fork.cal.starts": "Starts",
  "fork.cal.ends": "Ends",
  "fork.cal.ends_before_starts": "The end is before the start.",
  "fork.cal.calendar": "Calendar",
  "fork.cal.guests": "Guests",
  "fork.cal.location": "Where",
  "fork.cal.location_placeholder": "Room, address or link",
  "fork.cal.description_placeholder": "Notes",
  "fork.cal.meet": "Meet",
  "fork.cal.add_meet": "Add Google Meet",
  "fork.cal.join": "Join",
  "fork.cal.going": "Going?",
  "fork.cal.rsvp_btn_accepted": "Yes",
  "fork.cal.rsvp_btn_declined": "No",
  "fork.cal.rsvp_btn_tentative": "Maybe",
  "fork.cal.rsvp_accepted": "accepted",
  "fork.cal.rsvp_declined": "declined",
  "fork.cal.rsvp_tentative": "maybe",
  "fork.cal.rsvp_needsAction": "no answer",
  "fork.cal.create": "Create",
  "fork.cal.close": "Close",
  "fork.cal.cancel": "Cancel",
  "fork.cal.delete": "Delete",
  "fork.cal.delete_confirm": "Delete this event?",
  "fork.cal.keep": "Keep",
  "fork.cal.open_in_google": "Open in Google Calendar",
  "fork.cal.guests_label": "Guests",
  "fork.cal.guests_create": "Send invites to the guests?",
  "fork.cal.guests_update": "Send the update to the guests?",
  "fork.cal.guests_delete": "Tell the guests the event is cancelled?",
  "fork.cal.guests_rsvp": "Notify the organiser of your answer?",
  "fork.cal.guests_n_one": "{n} guest other than you.",
  "fork.cal.guests_n_other": "{n} guests other than you.",
  "fork.cal.guests_send": "Send",
  "fork.cal.guests_no_send": "Don't send",
  "fork.cal.guests_notify": "Notify",
  "fork.cal.guests_no_notify": "Don't notify",
  "fork.cal.chip_open": "Open in the calendar",
  "fork.cal.now": "now",
  "fork.cal.in_min": "in {n}m",
  "fork.cal.in_hour": "in {h}h {m}m",
  "fork.cal.meet_copied": "Meet link copied",
  "fork.cal.meet_opened": "Meet link opened",
  "fork.cal.meet_inserted": "Meet link added",
  "fork.cal.meet_not_connected": "Google is not connected: meet.new opened in the browser instead.",
  "fork.cal.in_your_calendar": "In your calendar",
  "fork.cal.conflicts_with": "Conflicts with",
  "fork.cal.ops_failed": "A calendar change could not be saved to Google.",
  "fork.cal.err.gcal_not_configured": "Google client ID is not set. Paste it under Settings → Calendar.",
  "fork.cal.err.gcal_not_connected": "Google Calendar is not connected for this account.",
  "fork.cal.err.gcal_account_mismatch": "That browser is signed in to a different Google account.",
  "fork.cal.err.gcal_scope": "Google did not grant the calendar permissions. Disconnect and connect again.",
  "fork.cal.err.gcal_api": "Google Calendar answered with an error.",
  "fork.cal.err.gcal_input": "That event is not valid.",
  "fork.cal.err.gcal_op": "The change is waiting on an earlier one that has not reached Google yet.",
  "fork.cal.err.oauth_cancelled": "The Google sign-in was cancelled.",
  "fork.cal.err.network": "No connection to Google right now.",
  "fork.cal.err.no_calendar": "Pick a calendar first.",
  "fork.slots.title": "Share availability",
  "fork.slots.duration": "Length",
  "fork.slots.days": "Days",
  "fork.slots.days_n": "{n} days",
  "fork.slots.recipient_tz": "Their zone",
  "fork.slots.tz_placeholder": "Europe/London",
  "fork.slots.tz_unknown": "Unknown time zone; the list shows your zone only.",
  "fork.slots.preview": "Preview",
  "fork.slots.loading": "Reading your calendar…",
  "fork.slots.none": "No free slots in that window.",
  "fork.slots.or_pick": "Or pick a time:",
  "fork.slots.insert": "Insert"
}
```

## App.svelte

Four snippets.

1. Imports (next to the other `./fork/...` imports):

```ts
  import CalendarView from "./fork/calendar/CalendarView.svelte";
  import TitlebarExtras from "./fork/calendar/TitlebarExtras.svelte";
  import SlotsPopover from "./fork/slots/SlotsPopover.svelte";
  import { calendar } from "./fork/calendar/store.svelte";
```

2. Start the calendar store once the mailbox has booted (inside the boot
   block, right after the `await Promise.race([booting, ...])` line, next to
   `void ai.refresh();`). It registers `navHooks.calendar` (`g c`, palette row)
   and `navHooks.meetNow` (`m`, palette row), subscribes to `calendar:updated`
   (re-reads the window on screen + the titlebar chip) and
   `calendar:ops_failed` (toast with the backend message), reads the
   per-account status, and polls the next-event chip every 30 s. Keep the
   returned stop function for teardown if the shell has one:

```ts
        stopCalendar = calendar.start(); // Phase 7: hooks, calendar:* listeners, next-event poll
```

with `let stopCalendar: (() => void) | null = null;` beside the other shell
state and `stopCalendar?.()` in the teardown.

3. The view switch (App.svelte:473-500 in the plan's numbering). The sidebar
   stays; the calendar replaces MessageList + ReadingPane (and the drafts
   editor / recap branches) while `ui.view === "calendar"`:

```svelte
      <main class="panes">
        <Sidebar />
        {#if ui.view === "calendar"}
          <CalendarView />
        {:else}
          <MessageList />
          {#if mail.selectedFolder?.role === "drafts" && draftEditorId !== null}
            ... (unchanged upstream branches) ...
          {:else}
            <ReadingPane />
          {/if}
        {/if}
      </main>
```

4. Titlebar mounts + the /slots popover (next to `<Titlebar />` and `<Toast />`;
   both are `position: fixed`, so the position in the tree does not matter).
   `TitlebarExtras` draws the next-event chip and the Meet-now button as one
   strip over the titlebar's empty middle (right: 150px, clear of the window
   controls) so `Titlebar.svelte` needs no upstream edit. If the main session
   prefers them inside `Titlebar.svelte`'s `.controls`, mount
   `<NextEventChip />` and `<MeetNow />` there instead and drop the wrapper.

```svelte
  <Titlebar />
  <TitlebarExtras />
  ...
  <SlotsPopover />
```

Keys: none for App.svelte. The calendar view handles `d w m a t j k n Esc`
itself with a capture-phase window listener while `ui.view === "calendar"`
(guards: palette / settings / shortcuts closed, not typing, no modifiers), so
App's mail bindings never see them there and `src/fork/keys.ts` is unchanged.
`g c` and `m` already route through `navHooks` in keys.ts.

## keys

None added. Bindings that exist or are handled locally:

| key | where | what |
|---|---|---|
| `g c` | keys.ts (exists) | `navHooks.calendar` → `ui.showCalendar()` |
| `m` | keys.ts (exists) | `navHooks.meetNow` → `calendar.meetNow()` (outside the calendar view) |
| `d` `w` `m` `a` | CalendarView (capture) | Day / Week / Month / Agenda |
| `t` | CalendarView | today |
| `j` / `k` | CalendarView | next / previous period |
| `n` | CalendarView | new event (next half hour, default length) |
| `Esc` | CalendarView | close the event panel / draft |
| `Ctrl Enter` | EventPanel, SlotsPopover | save / insert |

Inside the calendar view `m` means Month (the plan's 7.5 key map); Meet now
stays reachable there through the titlebar button and the palette.

## composer seam (Phase 6 agent, `src/fork/compose/`)

Two hooks, both already exported:

```ts
import { openSlotsPopover, isSlotsTrigger } from "../slots/slots";
import { addMeetLink } from "../calendar/meet";
```

- Toolbar button "Share availability" (or "/slots"): `openSlotsPopover((text) => insertAtCursor(text))`.
  The popover is mounted by the shell (App.svelte snippet 4); it resolves
  through the callback with the plain list and closes itself.
- `/slots` trigger: on input, when `isSlotsTrigger(textBeforeCaret)` is true
  (the six characters `/slots` at a line start, caret right after them),
  delete that token and call `openSlotsPopover(insert)` with the same
  callback.
- Toolbar button "Add Meet link": `addMeetLink((link) => insertAtCursor(link))`.
  Connected: a fresh space is inserted and a toast says so. Not connected:
  meet.new opens in the browser and nothing is inserted.

## tauri-core mock

One import + one line before the `switch` in `invoke()`; the fixtures live in
`demo/mock/fork-calendar.ts`:

```ts
import { forkCalendarInvoke } from "./fork-calendar";
// ...inside invoke(), after the AI_COMMANDS branch, before `switch (cmd)`:
  const cal = forkCalendarInvoke(cmd, args);
  if (cal) return "err" in cal ? Promise.reject(cal.err) : ok(cal.ok);
```

Covers `fork_google_client_get/set/clear`, `fork_meet_create`, all eleven
`fork_cal_*` commands and `fork_free_slots`. Setup state via localStorage
`skimdemo.fork_cal` = `"unconfigured" | "disconnected" | "connected"`
(absent = connected). The week fixture is relative to the current week:
timed events, an all-day event, one `needsAction`, one declined, one
`tentative`, two with `hangout_link`, one starting in 25 minutes for the chip;
ids 901/902 line up with `fork-prep.ts`. The harness also reads
`skimdemo.fork_cal_second_tz` / `skimdemo.fork_booking_link` through the
existing `skimdemo.fork_*` → settings passthrough.

## fork-shots

```js
  // Phase 7: the calendar screen.
  "cal-week": { phase: "7", setup: async (page) => { await openCalendar(page); } },
  "cal-day": { phase: "7", setup: async (page) => { await openCalendar(page); await page.keyboard.press("d"); await sleep(300); } },
  "cal-month": { phase: "7", setup: async (page) => { await openCalendar(page); await page.keyboard.press("m"); await sleep(300); } },
  "cal-agenda": { phase: "7", setup: async (page) => { await openCalendar(page); await page.keyboard.press("a"); await sleep(300); } },
  // Phase 7: the event panel on the Tuesday "Q3 launch sync" (guests, Meet, Prep).
  "cal-panel": {
    phase: "7",
    setup: async (page) => {
      await openCalendar(page);
      await page.locator(".ec-event", { hasText: "Q3 launch sync" }).first().click();
      await page.locator(".panel .attendees li").nth(1).waitFor();
      await sleep(250);
    },
  },
  // Phase 7: quick create from `n`.
  "cal-quick": {
    phase: "7",
    setup: async (page) => {
      await openCalendar(page);
      await page.keyboard.press("n");
      await page.locator(".panel input.title").waitFor();
      await sleep(250);
    },
  },
  // Phase 7: the "Send invites to the guests?" prompt (quick create + a guest + Create).
  "cal-guests-prompt": {
    phase: "7",
    setup: async (page) => {
      await openCalendar(page);
      await page.keyboard.press("n");
      await page.locator(".panel input.title").fill("Pilot kickoff");
      await page.locator(".panel .guests input").fill("anna.weber@northwind.example");
      await page.locator(".panel .foot .btn.primary").click();
      await page.locator(".dialog").waitFor();
      await sleep(200);
    },
  },
  // Phase 7: next-event chip ("Call with Anna Weber · in 25m · Join") is in every calendar shot's titlebar;
  // this one shoots the inbox so the chip shows over mail too.
  "cal-chip": { phase: "7", setup: async (page) => { await openInbox(page); await page.locator("[data-testid=next-event]").waitFor(); } },
  // Phase 7: Settings → Calendar in the three setup states.
  "cal-settings-connected": { phase: "7", setup: async (page) => { await openCalSettings(page); } },
  "cal-settings-disconnected": { phase: "7", flags: { "skimdemo.fork_cal": "disconnected" }, setup: async (page) => { await openCalSettings(page); } },
  "cal-settings-unconfigured": { phase: "7", flags: { "skimdemo.fork_cal": "unconfigured" }, setup: async (page) => { await openCalSettings(page); } },
  // Phase 7: the setup text on the calendar screen when nothing is connected.
  "cal-unconfigured": { phase: "7", flags: { "skimdemo.fork_cal": "unconfigured" }, setup: async (page) => { await openCalendar(page); } },
  // Phase 7.6: Meet now → "Meet link copied" toast.
  "cal-meet-now": {
    phase: "7",
    setup: async (page) => {
      await openInbox(page);
      await page.keyboard.press("m");
      await page.locator(".toast", { hasText: "Meet link" }).waitFor();
    },
  },
  // Phase 8: the /slots popover with its preview (needs the composer seam; until then call
  // `openSlotsPopover` from the console: page.evaluate(() => window.__skimSlots?.()) or shoot via the toolbar button).
  "slots-popover": {
    phase: "8",
    setup: async (page) => {
      await openInbox(page);
      await page.keyboard.press("Control+N"); // the composer
      await page.locator(".toolbar button", { hasText: "availability" }).click();
      await page.locator(".pop .preview").waitFor();
      await sleep(200);
    },
  },
```

with two helpers next to `openInbox` / `openHero`:

```js
async function openCalendar(page) {
  await openInbox(page);
  await page.locator(".sidebar .item.calendar").click();
  await page.locator(".fork-cal .ec-event").first().waitFor({ timeout: 20000 });
  await sleep(400);
}
async function openCalSettings(page) {
  await openInbox(page);
  await page.locator(".sidebar .footer .item", { hasText: "Settings" }).click();
  await page.locator(".cal-settings").waitFor();
  await sleep(300);
}
```

The Playwright context needs `permissions: ["clipboard-read", "clipboard-write"]`
for the Meet-now toast to read "copied" (without it the toast says "opened").

Shots taken during the build (scratch harness mounting the same components
against the same mock, 1440×900, all four themes) are in
`docs/fork/shots/7/`: week, day, month, agenda, panel, quick, guests prompt,
settings ×3 states, unconfigured week, meet-now toast, slots popover.

## TOUCHLIST

| file | symbol | change | reason |
|---|---|---|---|
| `src/lib/stores/ui.svelte.ts` | `state.view`, `ui.view`, `ui.showCalendar()`, `ui.showMail()` | new field + 3 members | 7.5: the calendar view switch |
| `src/components/Sidebar.svelte` | folders section | one "Calendar" item (with `G C` caption) above the folders; folder clicks call `ui.showMail()` first; `class:selected` on folders also requires `ui.view === "mail"` | 7.5: Calendar in the sidebar; a folder click leaves the calendar |
| `src/components/InviteCard.svelte` | template, imports | one import line + one line `<InviteCardExtras {invite} />` before the closing `</div>` | 7.5: "In your calendar" + conflicts |
| `package.json`, `package-lock.json` | dependencies | `@event-calendar/core` `5.14.1` (exact) | 7.5 calendar grid (MIT) |

## DECISIONS

- **2026-09-23 D30 (7.5):** the grid uses the package's Svelte 5 `Calendar`
  component with a `$state` options object, not `createCalendar`. The
  `svelte` export condition (what vite-plugin-svelte resolves) points at
  `src/index.svelte.js`, which exports only the component + plugins;
  `createCalendar` exists only in the compiled `dist/` entry. View, date,
  events and `editable` are options the component follows reactively, so no
  instance methods are needed.
- **2026-09-23 D31 (7.5):** the four calendar keys are handled by
  CalendarView itself, capture phase on `window`, only while
  `ui.view === "calendar"`, rather than through `keys.ts`. Inside the
  calendar `m` is Month (plan 7.5), so the global `m` = Meet now applies
  outside it; Meet now stays one click/palette row away inside.
- **2026-09-23 D32 (7.5):** the titlebar chip + Meet button ship as
  `TitlebarExtras.svelte`, a fixed strip over the titlebar's middle mounted
  from App.svelte, so `Titlebar.svelte` carries no upstream touch. Easy to
  move inside the titlebar later (two component mounts).
- **2026-09-23 D33 (7.4/7.5):** the guests prompt returns `"all" | "none" |
  null`; every write path (`calendar.create/patch/remove/rsvp`) awaits it
  before naming `sendUpdates`, and `null` (Cancel / Esc) drops the change
  (drag/resize reverts the grid). No answer is a default; Enter does nothing
  in the dialog. RSVP always asks ("Notify the organiser?") per D27.
- **2026-09-23 D34 (7.5):** InviteCard matching: the invite UID is compared
  with the synced `google_id` as `uid`, `uid === id@google.com` and
  `uid.split("@")[0] === id`; if none match, same summary + same start. Google
  imports keep the sender's UID as iCalUID but mint their own id, so a
  UID-only match would miss every externally sent invite.
- **2026-09-23 D35 (7.7):** calendar prefs live in
  `src/fork/calendar/settings.svelte.ts` (`calPrefs`), lazily hydrated, not in
  `stores/prefs.svelte.ts`, to stay out of a file two other phases edit.
- **2026-09-23 D36 (7.6):** Meet now hands the clipboard a
  `ClipboardItem` whose text is the pending `fork_meet_create` promise, so
  the write happens inside the click gesture; `writeText` after the await is
  the fallback, and the toast says "opened" instead of "copied" if both are
  refused.
- **2026-09-23 D37 (8):** the recipient-zone abbreviation tries `en-GB`,
  `en-ZA`, `en-US`, `en-AU`, `en-IE` and takes the first non-`GMT±` answer
  (`SAST`, `BST`, `CET`), falling back to `GMT+2`. The single-locale answer
  for Johannesburg is `GMT+2` in `en-US`.

## migrations

None.

## Cargo.toml / package.json

`package.json`: `@event-calendar/core` `5.14.1` added with `--save-exact`
(also `package-lock.json`). MIT, Svelte 5, README read from `node_modules`.
The Phase 6 agent's `squire-rte` line was left as it was.

## status

**Done:**
- 7.5 `CalendarView.svelte` (Day/Week/Month/Agenda, toolbar, keys, setup
  card, error line), `ec-theme.css` (EventCalendar variables → Skim tokens,
  all four themes; event fill = calendar colour muted into the surface;
  declined struck + faded, tentative dashed, local-only dotted, selected
  outlined), drag/resize → `patch` with the guests prompt and revert on
  cancel/error, click/drag on empty slot → quick create, `EventPanel.svelte`
  (360px; title, all-day, start/end date+time, calendar picker for new
  events, guests via `AddressInput`, attendee list with answers, location,
  "Add Google Meet" switch, Join, description, RSVP Yes/No/Maybe when
  Patrick is a guest, Prep when there are other guests, Open in Google,
  Delete = two clicks with Keep), `GuestsPrompt.svelte`,
  `NextEventChip.svelte`, `InviteCardExtras.svelte`, `store.svelte.ts`
  (window, statuses, calendars, selection, writes, chip poll, hooks,
  listeners), `guests.ts` (pure helpers), `types.ts` additive extensions.
- 7.6 `MeetNow.svelte` + `calendar.meetNow()` (meet.new when not connected;
  create → clipboard inside the gesture → open → toast), `meet.ts`
  `addMeetLink(insert)` for the composer toolbar.
- 7.7 `SettingsCalendar.svelte` (client id/secret masked + Save/Clear,
  Connect/Disconnect per account with status, calendars checkbox list,
  default length, working hours + days, second zone with datalist, booking
  link) + `settings.svelte.ts`.
- 8 `src/fork/slots/{slots.ts,open.svelte.ts,SlotsPopover.svelte}`:
  duration 30/45/60, days 3/5/10, recipient zone defaulting to
  `fork_cal_second_tz`, preview, Insert. Verified output in the harness:
  `Thu 24 Sep: 10:00, 14:30, 16:00 SAST (09:00, 13:30, 15:00 BST)` … +
  `Or pick a time: <link>`.
- `demo/mock/fork-calendar.ts` with the three setup states.
- Upstream seams: `ui.svelte.ts`, `Sidebar.svelte`, `InviteCard.svelte`.

**Not done / needs the main session:**
- `SettingsFork.svelte`: one line to mount the settings section (owned file):
  `import SettingsCalendar from "./calendar/SettingsCalendar.svelte";` and
  `<SettingsCalendar />` after the existing `<section class="fork-list">`.
- App.svelte snippets above; the tauri-core line; en.json keys; the
  fork-shots entries. Until App.svelte mounts the calendar there is no way
  to reach the view in the app.
- The composer seams (`/slots`, "Share availability", "Add Meet link")
  belong to the Phase 6 agent's toolbar.
- Not exercised against Google: no client on this box (the plan's human step).
  The first real connect + first real create is the test; the mock echoes
  the shapes in `types.ts`.
- `npm run demo:*` / `fork-shots.mjs` scenarios not run (need the owned
  files above). Instead a scratch Vite harness mounted the components with
  the same mock and injected the strings; shots in `docs/fork/shots/7/`.

**Gates (2026-09-23):**
- `npm run check`: 0 errors, 0 warnings in this phase's files. One error in
  `src/fork/court/store.svelte.ts` (Phase 10, mid-build: `mail.selectCourt`
  does not exist yet), not this phase's.
- `npm run build`: green (Vite 7, includes the calendar chunk).
- Harness run (12 scenes × 4 themes + interaction checks): no page errors;
  quick create → Create with a guest → prompt shown → "Don't send" → row
  appears; `m` → Month, `n` → panel; `/slots` Insert → the list above.
