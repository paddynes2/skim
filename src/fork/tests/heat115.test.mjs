import test from "node:test";
import assert from "node:assert/strict";
import { heatBackground, rampColour, rampPosition } from "../heat/colour.ts";

test("scores map onto the ramp and clamp at both ends", () => {
  assert.equal(rampPosition(0), 0);
  assert.equal(rampPosition(0.1), 0);
  assert.equal(rampPosition(0.75), 1);
  assert.equal(rampPosition(1), 1);
  assert.ok(Math.abs(rampPosition(0.425) - 0.5) < 1e-9);
});

test("cold is periwinkle and hot is mint, in both themes", () => {
  const [cr, , cb] = rampColour(0);
  const [hr, hg, hb] = rampColour(1);
  assert.ok(cb > cr, "cold leans blue");
  assert.ok(hg > hr && hg > hb, "hot leans green");
  const [lr, , lb] = rampColour(0, true);
  assert.ok(lb > lr);
  assert.equal(rampColour(1)[3], 0.32);
});

test("no heat means no background, and the tint fades to the right", () => {
  assert.equal(heatBackground(undefined), undefined);
  const bg = heatBackground(0.72);
  assert.match(bg, /^linear-gradient\(90deg, rgba\(/);
  const alphas = [...bg.matchAll(/,(\d\.\d{3})\)/g)].map((m) => Number(m[1]));
  assert.equal(alphas.length, 3);
  assert.ok(alphas[0] > alphas[1] && alphas[1] > alphas[2]);
});
