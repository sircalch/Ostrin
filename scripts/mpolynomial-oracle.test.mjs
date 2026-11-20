import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  buildCases,
  buildDivisionCases,
  divisionEdgeAssertions,
  edgeAssertions,
  expectedDivisionOutputs,
  expectedOutputs,
  runDivisionProperties,
  runProperties,
} from "./mpolynomial-oracle.mjs";

const here = dirname(fileURLToPath(import.meta.url));

test("oracle edge assertions hold", () => {
  assert.deepEqual(edgeAssertions(), []);
});

test("oracle property checks hold for fixed seeds", () => {
  for (const seed of [1, 2, 3, 20260731]) {
    const counts = runProperties(seed, 150);
    assert.equal(counts.trials, 150);
    assert.deepEqual(counts.failures, [], `seed ${seed}`);
  }
});

test("interchange cases agree with the rules and have the expected kinds", () => {
  const cases = buildCases();
  const outputs = expectedOutputs(cases);
  assert.equal(outputs.length, cases.length);
  assert.equal(cases.filter((c) => c.kind === "refuse").length, 9);
  assert.ok(outputs.filter((line) => line.endsWith("refused")).length === 9);
});

test("generated oracle files are current", () => {
  execFileSync(process.execPath, [join(here, "mpolynomial-oracle.mjs"), "--check"], { stdio: "pipe" });
});

test("division edge assertions hold", () => {
  assert.deepEqual(divisionEdgeAssertions(), []);
});

test("division property checks hold for fixed seeds", () => {
  for (const seed of [1, 2, 3, 20260731]) {
    const counts = runDivisionProperties(seed, 100);
    assert.equal(counts.trials, 100);
    assert.deepEqual(counts.failures, [], `seed ${seed}`);
  }
});

test("division cases agree with the rules and have the expected kinds", () => {
  const cases = buildDivisionCases();
  const outputs = expectedDivisionOutputs(cases);
  assert.equal(outputs.length, cases.length);
  assert.equal(outputs.filter((line) => line.endsWith("refused")).length, 4);
});
