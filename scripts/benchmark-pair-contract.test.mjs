import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import {
  compareQuantityFloatOutputs,
  floatArrayWorkload,
  quantityArrayWorkload,
  validateCheckedInQuantityFloatPair,
  validateQuantityFloatPairSources,
} from "./benchmark-pair-contract.mjs";

test("checked-in Float and Quantity array workloads use aligned operations", () => {
  const pair = validateCheckedInQuantityFloatPair();
  assert.equal(pair.floatExpression, "(values * values + values * 1.0) / 3.0");
  assert.equal(pair.quantityExpression, "(values * values + values * (1 m)) / 3.0");
});

test("the pair contract rejects the prior unmatched Float addition", () => {
  const floatSource = readFileSync(path.resolve("examples", "benchmark_float_array.ostrin"), "utf8")
    .replace("values * 1.0", "values");
  const quantitySource = readFileSync(path.resolve("examples", "benchmark_quantity_array.ostrin"), "utf8");
  assert.throws(
    () => validateQuantityFloatPairSources(floatSource, quantitySource),
    /operations are not aligned/,
  );
});

test("cross-workload outputs compare numeric payloads while ignoring unit labels", () => {
  assert.deepEqual(
    compareQuantityFloatOutputs(
      "1128.3333333333333\n1004.1919543150658\n",
      "1128.3333333333333 m^2\n1004.1919543150658 m^2\n",
    ),
    {
      comparedValues: 2,
      maximumAbsoluteDifference: 0,
      absoluteTolerance: 1e-13,
      relativeTolerance: 1e-12,
    },
  );
});

test("cross-workload output comparison rejects mismatched values and shapes", () => {
  assert.throws(
    () => compareQuantityFloatOutputs("1\n2\n", "1 m^2\n2.1 m^2\n"),
    /outputs differ at value 1/,
  );
  assert.throws(
    () => compareQuantityFloatOutputs("1\n2\n", "1 m^2\n"),
    /output lengths differ/,
  );
});

