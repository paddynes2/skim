// D97 Look A helpers: pastel mapping, Join detection, week numbers,
// unanswered invites, plain-text descriptions and the month grid's week start.
import test from "node:test";
import assert from "node:assert/strict";
import { pastelFor, joinLink, isoWeek, needsReply, initials } from "../calendar/presentation.ts";
import { looksLikeHtml, safeDescription } from "../calendar/description.ts";
import { monthDays } from "../calendar/planning.ts";

test("a calendar colour maps to the nearest pastel; greys and junk get periwinkle", () => {
  assert.equal(pastelFor("#0b8043"), "--p-mint-2"); // Basil, a deep green
  assert.equal(pastelFor("#33b679"), "--p-mint");
  assert.equal(pastelFor("#7986cb"), "--p-peri");
  assert.equal(pastelFor("#e67c73"), "--p-rose");
  assert.equal(pastelFor("#f6bf26"), "--p-amber");
  assert.equal(pastelFor("#039be5"), "--p-sky");
  assert.equal(pastelFor("#616161"), "--p-peri");
  assert.equal(pastelFor("var(--x)"), "--p-peri");
  assert.equal(pastelFor(null), "--p-peri");
});

test("Join prefers Meet from the event, then a Meet, Zoom or Teams link, and nothing else", () => {
  assert.deepEqual(joinLink({ hangout_link: "https://meet.google.com/abc-defg-hij", location: null, description: null }), { url: "https://meet.google.com/abc-defg-hij", kind: "meet" });
  assert.deepEqual(joinLink({ hangout_link: null, location: "https://us06web.zoom.us/j/123?pwd=x", description: null }), { url: "https://us06web.zoom.us/j/123?pwd=x", kind: "zoom" });
  assert.equal(joinLink({ hangout_link: null, location: null, description: "Join: https://teams.microsoft.com/meet/363?p=abc." })?.url, "https://teams.microsoft.com/meet/363?p=abc");
  assert.equal(joinLink({ hangout_link: null, location: "https://evil.example.test/meet.google.com/x", description: null }), null);
  assert.equal(joinLink({ hangout_link: null, location: "http://zoom.us/j/1", description: null }), null);
});

test("ISO week numbers", () => {
  assert.equal(isoWeek(new Date(2026, 10, 9)), 46);
  assert.equal(isoWeek(new Date(2026, 0, 1)), 1);
  assert.equal(isoWeek(new Date(2027, 0, 1)), 53);
});

test("an unanswered invite is pending, not mine to organise, and not cancelled", () => {
  const own = ["me@example.test"];
  assert.equal(needsReply({ self_response: "needsAction", organizer_email: "boss@example.test", status: "confirmed" }, own), true);
  assert.equal(needsReply({ self_response: "needsAction", organizer_email: "me@example.test", status: "confirmed" }, own), false);
  assert.equal(needsReply({ self_response: "needsAction", organizer_email: "boss@example.test", status: "cancelled" }, own), false);
  assert.equal(needsReply({ self_response: "accepted", organizer_email: "boss@example.test", status: "confirmed" }, own), false);
});

test("initials", () => {
  assert.equal(initials("Pat Example"), "PE");
  assert.equal(initials("wes.ley@example.test"), "WL");
  assert.equal(initials("alex@example.test"), "A");
});

test("a plain-text description is escaped, keeps line breaks and links only https", () => {
  assert.equal(looksLikeHtml("a < b"), false);
  assert.equal(looksLikeHtml("Hi<br>there"), true);
  const out = safeDescription("Agenda: 1 < 2 & 3 > 2\nhttps://example.test/a and javascript:alert(1) http://plain.example.test");
  assert.ok(out.includes("1 &lt; 2 &amp; 3 &gt; 2<br>"));
  assert.ok(out.includes('<a href="https://example.test/a">https://example.test/a</a>'));
  assert.ok(!out.includes('href="javascript'));
  assert.ok(!out.includes('href="http://'));
});

test("the month grid starts on the chosen weekday", () => {
  const nov = new Date(2026, 10, 1, 12);
  assert.equal(monthDays(nov)[0].getDay(), 1);
  assert.equal(monthDays(nov, 0)[0].getDay(), 0);
  assert.equal(monthDays(nov, 6)[0].getDay(), 6);
  assert.equal(monthDays(nov).length, 42);
});
