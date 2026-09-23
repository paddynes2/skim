// node --test src/fork/tests  (Node 22 strips the TypeScript types)
// `htmlToText` needs a DOM and is exercised in the app; `textToHtml` is pure.
import test from "node:test";
import assert from "node:assert/strict";
import { escapeHtml, textToHtml } from "../compose/html.ts";

test("one div per line, a br for a blank line, escaped", () => {
  assert.equal(textToHtml(""), "<div><br></div>");
  assert.equal(textToHtml("Hi Bob"), "<div>Hi Bob</div>");
  assert.equal(textToHtml("Hi\n\nBye"), "<div>Hi</div><div><br></div><div>Bye</div>");
  assert.equal(textToHtml("a <b> & \"c\"\r\nd"), "<div>a &lt;b&gt; &amp; &quot;c&quot;</div><div>d</div>");
  assert.equal(escapeHtml("<"), "&lt;");
});
