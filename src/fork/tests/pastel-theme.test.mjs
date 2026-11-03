import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { heatCategory } from "../heat/colour.ts";
import { parseTokens } from "../../../scripts/fork/contrast.mjs";

test("pastel rows: deal, then needs you, then a person, else automated", () => {
  assert.equal(heatCategory(0.2, true), "deal");
  assert.equal(heatCategory(1, false), "deal");
  assert.equal(heatCategory(0.72, false, true), "hot");
  assert.equal(heatCategory(0.5, false, true), "hot");
  assert.equal(heatCategory(0.45, false, true), "people");
  assert.equal(heatCategory(undefined, false, true), "people");
  assert.equal(heatCategory(0.72, false, false), "auto", "a scored login notice stays plain");
  assert.equal(heatCategory(0, false), "auto");
  assert.equal(heatCategory(undefined, false), "auto");
});

test("pastel-light defines every token base-light defines", () => {
  const t = parseTokens();
  const missing = Object.keys(t["base-light"]).filter((k) => !(k in t["pastel-light"]));
  assert.deepEqual(missing, []);
});

test("every pastel rule is scoped to pastel-light", () => {
  const css = readFileSync(new URL("../theme/pastel.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  for (const rule of css.split("}").map((r) => r.trim()).filter(Boolean)) {
    // Split on top-level commas only: `:is(.a, .b)` is one selector.
    const selectors = [];
    let depth = 0, cur = "";
    for (const ch of rule.split("{")[0]) {
      if (ch === "(") depth++;
      if (ch === ")") depth--;
      if (ch === "," && depth === 0) { selectors.push(cur.trim()); cur = ""; } else cur += ch;
    }
    selectors.push(cur.trim());
    for (const s of selectors) assert.ok(s.startsWith(":root[data-theme=\"pastel-light\"]"), s);
  }
});
