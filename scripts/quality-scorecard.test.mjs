import test from "node:test";
import assert from "node:assert/strict";
import { buildScorecard, hasDirtySourceChanges } from "./quality-scorecard.mjs";

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

test("scorecard separates closed entry rows from module-wide symbolic effect inventory", async () => {
  const scorecard = await buildScorecard();
  assert.equal(scorecard.r3.status, "OPEN");
  assert.equal(scorecard.r3.captureReadyCount + scorecard.r3.captureOpenCount, scorecard.r3.bundleCount);
  assert.equal(scorecard.r3.closedEntryEffectBundleCount, scorecard.r3.bundleCount);
  assert.equal(scorecard.r3.incompleteEffectBundleCount, 0);
  assert.equal(scorecard.r3.unresolvedEffectCallCount, 0);
  assert.ok(scorecard.r3.symbolicEffectCallCount > 0);
  assert.ok(scorecard.effectCoverage.knownCalls > 0);
  assert.ok(scorecard.effectCoverage.symbolicCalls > 0);
  assert.equal(scorecard.effectCoverage.unresolvedCalls, 0);
  assert.equal(scorecard.effectCoverage.countUnit, "distinct_function_operation_edges");
  assert.equal(scorecard.effectCoverage.aggregation, "sum_of_per_bundle_distinct_function_operation_edges");
  assert.equal(scorecard.effectCoverage.bundleCount, scorecard.r3.bundleCount);
  assert.equal(scorecard.effectCoverage.scope, `registered experiment bundles (${scorecard.r3.bundleCount})`);
  assert.equal(scorecard.effectCoverage.complete, true);
});
