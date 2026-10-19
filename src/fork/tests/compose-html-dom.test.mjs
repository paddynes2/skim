// node --test src/fork/tests/compose-html-dom.test.mjs
// The DOM half of compose/html.ts (D73), in headless Chromium with the real
// Squire build: htmlToText, the server-copy words of a draft written elsewhere,
// and the editor round trip that keeps opening a draft from being an edit.
import test, { after, before } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import ts from "typescript";

const here = (p) => fileURLToPath(new URL(p, import.meta.url));
let browser;
let page;

before(async () => {
  const source = readFileSync(here("../compose/html.ts"), "utf8");
  const js = ts.transpileModule(source, {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext },
  }).outputText;
  browser = await chromium.launch();
  page = await browser.newPage();
  await page.setContent("<div id=host></div>");
  await page.addScriptTag({ path: here("../../../node_modules/squire-rte/dist/squire-raw.js") });
  await page.addScriptTag({
    type: "module",
    content: `${js}\nwindow.H = { htmlToText, textToHtml, sanitizeToFragment, serverWordsHtml, openingHtml, sameWords, quoteAt };`,
  });
  await page.waitForFunction(() => window.H !== undefined);
});

after(async () => {
  await browser?.close();
});

/** Run `fn(H, ...args)` in the page. */
const inPage = (fn, ...args) => page.evaluate(([src, a]) => new Function(`return (${src})`)()(window.H, ...a), [fn.toString(), args]);

/** Open `html` in Squire as the composer does and return what it reports. */
const throughSquire = (html) =>
  inPage((H, h) => {
    const host = document.getElementById("host");
    host.innerHTML = "";
    const el = document.createElement("div");
    host.appendChild(el);
    const sq = new window.Squire(el, {
      sanitizeToDOMFragment: (x, ed) => H.sanitizeToFragment(x, ed.getRoot().ownerDocument),
      toPlainText: H.htmlToText,
    });
    let inputs = 0;
    sq.addEventListener("input", () => inputs++);
    sq.setHTML(h);
    // The composer focuses an inline reply after load; a click moves the caret.
    sq.focus();
    sq.moveCursorToEnd();
    const got = sq.getHTML();
    sq.destroy();
    return { got, inputs };
  }, html);

const AGENT_HTML =
  "<p>Hi Sam,</p>\n<p> </p>\n<p>The first paragraph.</p>\n<ul><li>one</li><li>two</li></ul>\n" +
  '<p>You can <a href="https://example.com/chat">pick any time that suits you here</a>.</p>\n<p><br></p>';
const AGENT_TEXT = "Hi Sam,\n \nThe first paragraph.\n\none\ntwo\n\nYou can pick any time that suits you here.\n";

test("htmlToText and textToHtml are inverses, blank lines included", async () => {
  const texts = ["", "Hi Bob", "Hi\n\nBye", "a\n\n\nb\nc", "x\n", "\n\nlead", "a <b> & c\n\n- d"];
  const back = await inPage((H, ts) => ts.map((t) => H.htmlToText(H.textToHtml(t))), texts);
  assert.deepEqual(back, texts);
  // A line of spaces stays a line (trailing spaces are trimmed, as always).
  assert.equal(await inPage((H) => H.htmlToText(H.textToHtml("a\n \nb"))), "a\n\nb");
});

test("htmlToText reads Gmail and agent markup line for line", async () => {
  const got = await inPage(
    (H, cases) => cases.map((h) => H.htmlToText(h)),
    [
      '<div dir="ltr">Hi<div><br></div><div>See <a href="https://x.com">this</a></div></div>',
      "<html><body>Hello<br/><br/>Line two<br/>Thanks<br/></body></html>",
      "<p>A</p>\n<p> </p>\n<p>B</p>",
      "<ul><li>one</li><li><div>two</div></li></ul><ol><li>x</li><li>y</li></ol>",
      "a<br><div>b</div>",
      "<p>A</p><p>B</p><ul><li>c</li></ul><p>D</p>",
      "<div>a</div><ul><li>b</li></ul><div>c</div>",
      // Gmail puts a bare <br> between its words and the quote: a blank line.
      "<div>a</div>b<div>c</div><br><div>d</div>",
      "<ul><li><ul><li>x</li></ul></li></ul>",
    ],
  );
  assert.deepEqual(got, [
    "Hi\n\nSee this (https://x.com)",
    "Hello\n\nLine two\nThanks",
    "A\n\nB",
    "- one\n- two\n1. x\n2. y",
    "a\nb",
    // Paragraphs keep the blank line their margin shows; the editor's divs do not get one.
    "A\n\nB\n\n- c\n\nD",
    "a\n- b\nc",
    "a\nb\nc\n\nd",
    "-\n  - x",
  ]);
});

test("a paragraph that starts with On is not the quote", async () => {
  const got = await inPage(
    (H, bodies) => bodies.map((b) => H.quoteAt(b)),
    ["Fine.\n\nOn Tuesday works\n\nOn Mon, Bob wrote:\n> q", "Fine.\n\nOn Tuesday works", "Hi\n\nOn Mon, Bob wrote:"],
  );
  assert.deepEqual(got, [23, -1, 2]);
});

test("an agent's draft keeps its link, list and paragraphs", async () => {
  const words = await inPage((H, h, t) => H.serverWordsHtml(h, t), AGENT_HTML, AGENT_TEXT);
  assert.ok(words, "server words used");
  assert.match(words, /<a href="https:\/\/example\.com\/chat" rel="noopener noreferrer">pick any time that suits you here<\/a>/);
  assert.match(words, /<ul><li>one<\/li><li>two<\/li><\/ul>/);
  assert.ok(!/<p><br><\/p>$/.test(words), "the trailing blank paragraph is dropped");
  const text = await inPage((H, h) => H.htmlToText(h), words);
  assert.equal(
    text,
    "Hi Sam,\n\nThe first paragraph.\n\n- one\n- two\n\nYou can pick any time that suits you here (https://example.com/chat).",
  );
});

test("opening the server words in the editor is not an edit", async () => {
  const words = await inPage((H, h, t) => H.serverWordsHtml(h, t), AGENT_HTML, AGENT_TEXT);
  const { got, inputs } = await throughSquire(words);
  assert.equal(inputs, 0);
  const [before, after] = await inPage((H, a, b) => [H.htmlToText(a), H.htmlToText(b)], words, got);
  assert.equal(after, before);
  for (const t of ["Hi\n\nBye", "a\n\n\nb\nc", "x\n"]) {
    const html = await inPage((H, x) => H.textToHtml(x), t);
    const back = await inPage((H, h) => H.htmlToText(h), (await throughSquire(html)).got);
    assert.equal(back, t);
  }
});

test("a Gmail web reply opens with its words only, cut at the quote", async () => {
  const html =
    '<div dir="ltr">Hi<div><br></div><div>See <a href="https://x.com">this</a> and <b>bold</b></div></div><br>' +
    '<div class="gmail_quote"><div dir="ltr" class="gmail_attr">On Mon, Bob &lt;bob@x.com&gt; wrote:<br></div>' +
    '<blockquote class="gmail_quote">old words</blockquote></div>';
  const words = await inPage((H, h) => H.serverWordsHtml(h, "Hi\n\nSee this <https://x.com> and *bold*"), html);
  assert.ok(words);
  assert.match(words, /<a href="https:\/\/x\.com" rel="noopener noreferrer">this<\/a>/);
  assert.match(words, /<b>bold<\/b>/);
  assert.ok(!words.includes("old words") && !words.includes("wrote"));
});

test("the server copy is refused when it would show the wrong thing", async () => {
  const got = await inPage(
    (H, cases) => cases.map(([h, t]) => H.serverWordsHtml(h, t)),
    [
      ['<p>Hi <a href="https://x.com">there</a></p>', "Hi everyone"],
      ["<p>Hi there</p><div><br></div>", "Hi there"],
      ['<table><tr><td>Fee</td><td>R50k</td></tr></table><p><b>x</b></p>', "Fee R50k\nx"],
      ['<span style="display: none">preheader</span><p><b>Hi</b></p>', "Hi"],
    ],
  );
  assert.deepEqual(got, [null, null, null, null]);
});

test("unsafe markup never reaches the editor", async () => {
  const got = await inPage((H) =>
    H.serverWordsHtml(
      '<p onclick="x()"><b>Hi</b> <a href="javascript:alert(1)">there</a><script>alert(2)</script><img src=x onerror=y></p>',
      "Hi there",
    ),
  );
  assert.equal(got, "<p><b>Hi</b> <a>there</a></p>");
});

test("an HTML-only draft opens from its HTML", async () => {
  const got = await inPage((H) => H.serverWordsHtml("<div>Only <i>HTML</i></div>", ""));
  assert.equal(got, "<div>Only <i>HTML</i></div>");
});

test("which HTML a draft opens with", async () => {
  const src = { html: AGENT_HTML, text: AGENT_TEXT };
  const got = await inPage(
    (H, s, body) => {
      const flat = H.textToHtml(body);
      const own = "<div>Hi <b>Sam</b></div><div><br></div><div>Bye</div>";
      return [
        H.openingHtml(body, null, s).includes("href="),
        // Opened once by 1.1.25, which stored the flattened words.
        H.openingHtml(body, flat, s).includes("href="),
        // Formatting the user added with an older build wins over the server copy.
        H.openingHtml("Hi Sam\n\nBye", own, {
          html: '<p>Hi <a href="https://x.com">Sam</a></p><p>Bye</p>',
          text: "Hi Sam\n\nBye",
        }) === own,
        // Words edited with an older build and not on the server yet: never the server copy.
        H.openingHtml(body + " More.", null, s).includes("href="),
        // Stored HTML written before D73 still matches its blank-line-less body.
        H.openingHtml("Hi Sam\nBye", own, null) === own,
        // Words edited in plain mode elsewhere: the text wins.
        H.openingHtml("Hi Tom\nBye", own, null) === H.textToHtml("Hi Tom\nBye"),
        H.openingHtml("Plain", null, null) === H.textToHtml("Plain"),
      ];
    },
    src,
    AGENT_TEXT,
  );
  assert.deepEqual(got, [true, true, true, false, true, true, true]);
});
