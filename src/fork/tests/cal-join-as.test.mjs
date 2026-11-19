// D101: Join and "Open in Google Calendar" open as the account the meeting
// belongs to. Twin of src-tauri/src/fork/calendar/join_as.rs.
import test from "node:test";
import assert from "node:assert/strict";
import { joinAs, withAccount } from "../calendar/presentation.ts";

const own = ["me@work.test", "me@home.test", "me@side.test"];

test("the self guest wins when it is an own address", () => {
  const attendees_json = JSON.stringify([{ email: "boss@other.test", organizer: true }, { email: "Me@Home.test", self: true }]);
  assert.equal(joinAs({ attendees_json }, "me@home.test", "me@work.test", own), "me@home.test");
});

test("a shared calendar of an own address opens as that address", () => {
  assert.equal(joinAs({ attendees_json: null }, "me@side.test", "me@work.test", own), "me@side.test");
});

test("an invited own address on someone else's calendar wins", () => {
  const attendees_json = JSON.stringify([{ email: "me@home.test" }, { email: "partner@other.test", self: true }]);
  assert.equal(joinAs({ attendees_json }, "partner@other.test", "me@work.test", own), "me@home.test");
});

test("someone else's calendar falls back to the syncing account", () => {
  const attendees_json = JSON.stringify([{ email: "partner@other.test", self: true }]);
  assert.equal(joinAs({ attendees_json }, "partner@other.test", "Me@Work.test", own), "me@work.test");
  assert.equal(joinAs({ attendees_json: "not json" }, "x", null, own), null);
});

test("Meet and Calendar links carry the account; nothing else does", () => {
  assert.equal(withAccount("https://meet.google.com/abc-defg-hij", "me@home.test"), "https://meet.google.com/abc-defg-hij?authuser=me%40home.test");
  assert.equal(withAccount("https://meet.google.com/abc-defg-hij?authuser=0&hs=1", "me@side.test"), "https://meet.google.com/abc-defg-hij?authuser=me%40side.test&hs=1");
  assert.equal(withAccount("https://www.google.com/calendar/event?eid=xyz", "me@work.test"), "https://www.google.com/calendar/event?eid=xyz&authuser=me%40work.test");
  assert.equal(withAccount("https://us02web.zoom.us/j/1", "me@work.test"), "https://us02web.zoom.us/j/1");
  assert.equal(withAccount("https://www.google.com/search?q=x", "me@work.test"), "https://www.google.com/search?q=x");
  assert.equal(withAccount("https://meet.google.com/abc", null), "https://meet.google.com/abc");
});

// D102: which mailbox an event belongs to, for its colour and tag.
import { eventAccount, isPersonCalendar, calendarShortName } from "../calendar/presentation.ts";

test("an event on another person's calendar with no own guest belongs to no mailbox", () => {
  const attendees_json = JSON.stringify([{ email: "friend@other.test", self: true }]);
  assert.equal(eventAccount({ attendees_json }, "friend@other.test", "me@work.test", own), null);
  assert.equal(joinAs({ attendees_json }, "friend@other.test", "me@work.test", own), "me@work.test");
});

test("a group or holiday calendar belongs to the account that synced it", () => {
  assert.equal(isPersonCalendar("en.sa#holiday@group.v.calendar.google.com"), false);
  assert.equal(isPersonCalendar("c_123@group.calendar.google.com"), false);
  assert.equal(isPersonCalendar("friend@other.test"), true);
  assert.equal(eventAccount({ attendees_json: null }, "en.sa#holiday@group.v.calendar.google.com", "me@side.test", own), "me@side.test");
});

test("another person's calendar is named by the part before the @", () => {
  assert.equal(calendarShortName("friend@other.test", "friend@other.test"), "friend");
  assert.equal(calendarShortName("Team", "c_1@group.calendar.google.com"), "Team");
});
