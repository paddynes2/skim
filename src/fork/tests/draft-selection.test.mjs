import test from "node:test";
import assert from "node:assert/strict";
import { draftMessageId, draftMessageIds } from "../compose/draft-selection.ts";

const thread = [{ id: 10, folderId: 1 }, { id: 11, folderId: 2 }, { id: 12, folderId: 3 }];
test("a grouped Drafts row opens its draft even when Sent is newer", () => {
  assert.equal(draftMessageId(thread, [2], null), 11);
});
test("explicit selection cannot reopen a sent or vanished message", () => {
  assert.equal(draftMessageId(thread, [2], 11), 11);
  assert.equal(draftMessageId(thread, [2], 12), null);
  assert.equal(draftMessageId(thread, [2], 99), null);
  assert.equal(draftMessageId(thread, [4], null), null);
});
test("unified Drafts accepts real draft folders across accounts", () => {
  assert.equal(draftMessageId([...thread, { id: 13, folderId: 4 }, { id: 14, folderId: 3 }], [2, 4], null), 13);
});
test("deleting grouped drafts excludes sent and inbox copies", () => {
  assert.deepEqual(draftMessageIds([...thread, { id: 13, folderId: 2 }], [2]), [11, 13]);
  assert.deepEqual(draftMessageIds(thread, [2], 12), []);
});
