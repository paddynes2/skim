import test from "node:test";
import assert from "node:assert/strict";
import { eventState, zoneClock } from "../calendar/presentation.ts";

test("calendar state reflects cancellation before RSVP and never calls a private event accepted", () => {
  assert.equal(eventState({status:"cancelled",self_response:"accepted"}),"cancelled");
  assert.equal(eventState({status:"confirmed",self_response:"declined"}),"declined");
  assert.equal(eventState({status:"tentative",self_response:null}),"tentative");
  assert.equal(eventState({status:"confirmed",self_response:"accepted"}),"accepted");
  assert.equal(eventState({status:"confirmed",self_response:null}),"confirmed");
});
test("second-zone clock follows the displayed date across daylight-saving boundaries", () => {
  const before = new Date(2026,2,7,12), after = new Date(2026,2,9,12);
  const expected = (date) => new Date(date.getFullYear(),date.getMonth(),date.getDate(),12,30).toLocaleTimeString("en",{timeZone:"America/New_York",hour:"2-digit",minute:"2-digit",hour12:false});
  assert.equal(zoneClock(before,12,30,"America/New_York"),expected(before));
  assert.equal(zoneClock(after,12,30,"America/New_York"),expected(after));
  assert.equal(zoneClock(before,12,30,"Not/AZone"),"");
});
