import test from "node:test";
import assert from "node:assert/strict";
import { resolve } from "node:path";
import { repositoryRoot } from "./benchmark-contract-check.mjs";
import {
  assertBenchmarkWorkloadSucceeded,
  repositoryRelativeWorkloadPath,
  runtimeInputFilesFor,
} from "./benchmark-runtime-inputs.mjs";

const pendulumInput = [{
  path: "experiments/pendulum-calibration.inputs.json",
  stagedAs: "data.json",
}];

test("runtime inputs are selected from normalized relative and absolute workload paths", () => {
  const absolutePath = resolve(repositoryRoot, "examples", "viz_pendulum_calibration.ostrin");
  for (const sourcePath of [
    "examples/viz_pendulum_calibration.ostrin",
    "./examples/viz_pendulum_calibration.ostrin",
    "examples\\viz_pendulum_calibration.ostrin",
    absolutePath,
  ]) {
    assert.deepEqual(runtimeInputFilesFor(repositoryRoot, sourcePath), pendulumInput, sourcePath);
    assert.equal(repositoryRelativeWorkloadPath(repositoryRoot, sourcePath), "examples/viz_pendulum_calibration.ostrin");
  }
});

test("unregistered and out-of-repository workloads do not inherit fixture inputs", () => {
  assert.deepEqual(runtimeInputFilesFor(repositoryRoot, "examples/benchmark_numeric.ostrin"), []);
  assert.deepEqual(runtimeInputFilesFor(repositoryRoot, resolve(repositoryRoot, "..", "outside.ostrin")), []);
});

test("benchmark samples reject successful processes that print analysis-error diagnostics", () => {
  assert.doesNotThrow(() => assertBenchmarkWorkloadSucceeded("g = 9.8084 m/s²", "", "native"));
  for (const output of [
    "input error: could not read data.json",
    "fit error: slope must be positive",
    "measurement error: invalid uncertainty",
    "uncertainty error: value is not finite",
  ]) {
    assert.throws(
      () => assertBenchmarkWorkloadSucceeded(output, "", "interpreter"),
      /reported an analysis error instead of a benchmark result/,
    );
  }
  assert.throws(
    () => assertBenchmarkWorkloadSucceeded("", "input error: missing data.json", "native"),
    /reported an analysis error instead of a benchmark result/,
  );
});
