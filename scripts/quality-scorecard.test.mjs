import test from "node:test";
import assert from "node:assert/strict";
import { buildScorecard, hasDirtySourceChanges } from "./quality-scorecard.mjs";

test("effect coverage in the scorecard reflects every registered R3 bundle", async () => {
  const scorecard = await buildScorecard();
  assert.equal(scorecard.effectCoverage.knownCalls, scorecard.r3.knownEffectCallCount);
  assert.equal(scorecard.effectCoverage.symbolicCalls, scorecard.r3.symbolicEffectCallCount);
  assert.ok(scorecard.effectCoverage.symbolicCalls > 0);
  assert.equal(scorecard.effectCoverage.unresolvedCalls, scorecard.r3.unresolvedEffectCallCount);
  assert.equal(scorecard.effectCoverage.complete, true);
  assert.equal(scorecard.effectCoverage.scope, "29 registered R3 experiment bundles");
});

test("generated website evidence does not mark a clean source snapshot dirty", () => {
  assert.equal(hasDirtySourceChanges([
    "validation/r3-readiness.json",
    "validation/quality-scorecard.json",
    "website/status-data.js",
    "website/assets/experiments/lines.ostrin-experiment.json",
  ]), false);
  assert.equal(hasDirtySourceChanges(["website\\assets\\experiments\\lines.ostrin-experiment.json"]), false);
});

test("source edits and unrelated generated files remain visible as dirty", () => {
  assert.equal(hasDirtySourceChanges(["compiler/src/main.rs"]), true);
  assert.equal(hasDirtySourceChanges(["website/assets/viz/lines.svg"]), true);
  assert.equal(hasDirtySourceChanges(["list_phi_probe.ostrin"]), true);
});
