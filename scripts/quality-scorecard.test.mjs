import test from "node:test";
import assert from "node:assert/strict";
import { hasDirtySourceChanges } from "./quality-scorecard.mjs";

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
