// node --test src/fork/tests/smell.test.mjs  (Node 22 strips the TypeScript types)
//
// The scanner, the fact guard and the sentence context are pure; the parity
// run against OS's slop_scrub.py is included when the OS checkout is present
// and skipped (visibly) when it is not.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync, existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { register } from "node:module";
register("./ts-resolve.mjs", import.meta.url);
const { compile, scan, ownWords, maskNoise, mustFix, DASH_RULE } = await import("../smell/scan.ts");
const { facts, guard, missing } = await import("../smell/factguard.ts");
const { sentenceContext } = await import("../smell/highlight.ts");
const { healthParts, sendPrompt } = await import("../smell/health.ts");
const { parseSmellSettings } = await import("../smell/settings.ts");
const { osRoot, runParity } = await import("../../../scripts/fork/smell-parity.mjs");

const HERE = dirname(fileURLToPath(import.meta.url));
const RULES = JSON.parse(readFileSync(resolve(HERE, "..", "smell", "rules.json"), "utf8"));
const CORPUS = JSON.parse(readFileSync(resolve(HERE, "smell-corpus.json"), "utf8"));
const t = (key, vars) => (vars && "n" in vars ? `${key}:${vars.n}` : key);

const rules = compile(RULES);
const cats = (r) => r.spans.map((s) => s.category);

test("every vendored rule compiles as a JS RegExp, and the file names its OS commit", () => {
  assert.equal(rules.rules.length, RULES.rules.length);
  assert.ok(rules.rules.length >= 120, `expected the full rule set, got ${rules.rules.length}`);
  assert.match(RULES.os_commit, /^[0-9a-f]{40}$/);
  assert.ok(RULES.rules.some((r) => r.category === "stock_opener"), "the 6.5.1 rules are in the export");
});

test("a pattern that does not compile fails compile() by name", () => {
  const bad = structuredClone(RULES);
  bad.rules.push({ category: "broken", severity: "warn", pattern: "(?<=[.!?] ", flags: "im", suggestion: "x", min_hits: 1 });
  assert.throws(() => compile(bad), /smell rule broken#0 does not compile/);
});

test("spans are exact on a fixture: offsets, text, severity, suggestion", () => {
  const text = "Hi Jim,\n\nI wanted to reach out about the pack. Let me know if you have any questions.";
  const r = scan(text, rules);
  const opener = r.spans.find((s) => s.category === "stock_opener");
  const closer = r.spans.find((s) => s.category === "stock_closer");
  assert.ok(opener && closer);
  assert.equal(text.slice(opener.start, opener.end), "I wanted to reach out");
  assert.equal(opener.text, "I wanted to reach out");
  assert.equal(opener.severity, "warn");
  assert.equal(opener.rule, "stock_opener#0");
  assert.match(opener.suggestion, /wind-up/);
  assert.equal(text.slice(closer.start, closer.end), "Let me know if you have any questions");
  assert.deepEqual(r.spans.map((s) => s.start), [...r.spans.map((s) => s.start)].sort((a, b) => a - b), "sorted");
});

test("em dash and en dash are hard, every occurrence, with Patrick's suggestion", () => {
  const r = scan("One — two – three.", rules);
  const dashes = r.spans.filter((s) => s.category === "dash");
  assert.equal(dashes.length, 2);
  assert.ok(dashes.every((s) => s.severity === "hard" && s.rule === DASH_RULE));
  assert.deepEqual(dashes.map((s) => [s.start, s.end]), [[4, 5], [10, 11]]);
  assert.equal(mustFix(r).length, 2);
  assert.equal(sendPrompt(r, t), "fork.smell.send_must_fix:2");
});

test("hard tier: invisible characters, homoglyphs, unfilled placeholders, mixed quotes", () => {
  const r = scan("Hi {{first_name}}, the vаluation​ is “done” and \"ready\".", rules);
  const c = cats(r);
  for (const want of ["placeholder_unfilled", "homoglyph", "invisible_char", "quote_dialect"]) {
    assert.ok(c.includes(want), `missing ${want} in ${c}`);
  }
  assert.ok(r.spans.filter((s) => s.category !== "dash").every((s) => s.severity === "hard"));
  const inv = r.spans.find((s) => s.category === "invisible_char");
  assert.equal(inv.text, "​");
  assert.match(inv.suggestion, /zero-width space/);
});

test("ownWords stops at the signature and the quoted original; neither is scanned", () => {
  const body = "Fine by me.\n\n-- \nPatrick, delve into it\n\nOn Mon, Jim wrote:\n> Let me know if you have any questions.";
  const own = ownWords(body);
  assert.equal(own.text, "Fine by me.");
  assert.equal(own.end, 11);
  assert.deepEqual(scan(own.text, rules).spans, []);
  const quoteFirst = "Fine by me.\n\nOn Mon, Jim wrote:\n> x\n\n-- \nsig";
  assert.equal(ownWords(quoteFirst).text, "Fine by me.");
  assert.equal(ownWords("No tail here").end, 12);
});

test("maskNoise keeps offsets: fences, inline code and > quote lines become spaces", () => {
  const text = "a `deep dive` b\n> deep dive\n```\ndeep dive\n```\nc";
  const masked = maskNoise(text);
  assert.equal(masked.length, text.length);
  assert.equal(masked.split("\n").length, text.split("\n").length);
  assert.ok(!/deep dive/.test(masked));
  assert.deepEqual(scan(text, rules).spans, []);
});

test("density gate: the tier-2 vocabulary needs two hits, one stays clean", () => {
  assert.deepEqual(cats(scan("The pack is robust.", rules)), []);
  const two = scan("The pack is robust and the process will foster trust.", rules);
  assert.equal(two.spans.filter((s) => s.category === "ai_vocab_t2").length, 2);
});

test("a contrast frame repeated past the email threshold underlines every occurrence", () => {
  const text = "Rather than hire, rather than outsource, rather than wait, the owner steps back.";
  const r = scan(text, rules);
  const rep = r.spans.filter((s) => s.category === "contrast_repeat");
  assert.equal(rep.length, 3);
  assert.ok(rep.every((s) => s.severity === "warn" && s.text.toLowerCase() === "rather than"));
  assert.deepEqual(r.health.contrastRepeat, { phrase: "rather than", count: 3 });
  const two = scan("Rather than hire, rather than wait.", rules);
  assert.equal(two.spans.filter((s) => s.category === "contrast_repeat").length, 0, "below threshold: no underline");
  assert.deepEqual(two.health.contrastRepeat, { phrase: "rather than", count: 2 }, "but the health line still says so");
});

test("health: no position only on a long enough draft; parts only for what fired", () => {
  const short = scan("Fine by me.", rules);
  assert.equal(short.health.noPosition, false);
  assert.deepEqual(healthParts(short, t), []);
  const long = scan(Array(60).fill("The pack covers the handover and the pricing.").join(" "), rules);
  assert.ok(long.health.words >= 250);
  assert.equal(long.health.noPosition, true);
  const parts = healthParts(long, t);
  assert.deepEqual(parts.map((p) => p.kind), ["position"]);
  const withPosition = scan(Array(60).fill("I think the pack covers the handover.").join(" "), rules);
  assert.equal(withPosition.health.noPosition, false);
  const mixed = scan("I wanted to reach out — rather than hire, rather than wait, rather than stall.", rules);
  assert.deepEqual(healthParts(mixed, t).map((p) => [p.kind, p.text]), [
    ["hard", "fork.smell.health_hard:1"],
    ["tells", "fork.smell.health_tells:1"],
    ["contrast", "‘rather than’ ×3"],
  ]);
});

test("ignored rules and the template flag drop exactly their spans", () => {
  const text = "I wanted to reach out to {{first_name}}.";
  const all = scan(text, rules);
  assert.deepEqual(cats(all).sort(), ["placeholder_unfilled", "stock_opener"]);
  assert.deepEqual(cats(scan(text, rules, { ignoredRules: new Set(["stock_opener#0"]) })), ["placeholder_unfilled"]);
  assert.deepEqual(cats(scan(text, rules, { template: true })), ["stock_opener"]);
  assert.equal(scan("Honestly, fine.", rules).spans.length, 0, "info tier off by default");
  assert.equal(scan("Honestly, fine.", rules, { includeInfo: true }).spans.length, 1);
});

test("the exemplar drafts are clean at hard + warn", () => {
  for (const s of CORPUS.filter((c) => c.name.endsWith("_clean"))) {
    const r = scan(s.text, rules);
    assert.deepEqual(r.spans.filter((x) => x.severity !== "info"), [], s.name);
  }
});

test("fact guard: numbers, dates, amounts, URLs, emails, names (mirrors smell.rs)", () => {
  const f = facts("Ali sends R140,000 on Tuesday 14:00 to jim@acme.com, see https://acme.com/x.");
  for (const w of ["R140,000", "Tuesday", "14:00", "jim@acme.com", "https://acme.com/x"]) assert.ok(f.has(w), w);
  assert.ok(!f.has("Ali"), "the first word's capital is grammar");
  assert.ok(facts("Ali sends it.", false).has("Ali"));
  assert.ok(!facts("So I said yes.").has("I"));
  const orig = "The fee is R140,000, due Tuesday 14:00.";
  assert.equal(guard(orig, "R140,000 is due Tuesday 14:00."), true);
  assert.equal(guard(orig, "The fee is R14,000, due Tuesday 14:00."), false, "amount changed");
  assert.equal(guard(orig, "The fee is R140,000, due Tuesday 15:00."), false, "time changed");
  assert.equal(guard(orig, "The fee is R140,000, due Wednesday 14:00."), false, "day changed");
  assert.deepEqual(missing(orig, "The fee is due Tuesday 14:00."), ["R140,000"]);
  const names = "Send the pack to Jim Carter at jim@acme.com or via https://acme.com/upload.";
  assert.equal(guard(names, "Jim Carter gets the pack at jim@acme.com or https://acme.com/upload."), true);
  assert.equal(guard(names, "Send the pack to Jim at jim@acme.com or via https://acme.com/upload."), false);
});

test("sentence context: the flagged sentence with one sentence either side", () => {
  const text = "First one. I wanted to reach out about it. Last one here.";
  const at = text.indexOf("I wanted");
  const c = sentenceContext(text, at, at + 21);
  assert.equal(c.sentence, "I wanted to reach out about it.");
  assert.equal(c.before, "First one.");
  assert.equal(c.after, "Last one here.");
  assert.equal(text.slice(c.start, c.end), c.sentence);
  const edge = sentenceContext("Only this.", 0, 4);
  assert.deepEqual([edge.before, edge.sentence, edge.after], ["", "Only this.", ""]);
  const paras = sentenceContext("Para one\n\nPara two here", 10, 14);
  assert.equal(paras.sentence, "Para two here");
  assert.equal(paras.before, "Para one");
});

test("settings parse with defaults on, and a broken ignore list is empty", () => {
  const d = parseSmellSettings({});
  assert.deepEqual([d.enabled, d.blockHard, [...d.ignored]], [true, true, []]);
  const s = parseSmellSettings({ fork_smell: "off", fork_smell_block_hard: "off", fork_smell_ignored: '["a#0", 3]' });
  assert.deepEqual([s.enabled, s.blockHard, [...s.ignored]], [false, false, ["a#0"]]);
  assert.deepEqual([...parseSmellSettings({ fork_smell_ignored: "{" }).ignored], []);
});

const os = osRoot([]);
const haveOs = existsSync(resolve(os, "tools", "outreach", "slop_scrub.py"));
test("parity with OS slop_scrub.py over the 30-sample corpus", { skip: haveOs ? false : `no OS checkout at ${os}` }, () => {
  const { samples, failures } = runParity(os);
  assert.equal(samples, 30);
  assert.deepEqual(failures, []);
});
