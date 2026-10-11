import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const history = JSON.parse(readFileSync("validation/benchmark-history.json", "utf8"));

test("benchmark history contains multiple provenance-backed commits", () => {
  assert.equal(history.schema, "ostrin.benchmark-history/v1");
  assert.ok(history.entries.length >= 3);
  assert.equal(new Set(history.entries.map((entry) => entry.benchmarkCommit)).size, history.entries.length);
  for (const entry of history.entries) {
    assert.match(entry.benchmarkCommit, /^[a-f0-9]{40}$/);
    assert.ok(entry.platform && entry.arch);
    assert.equal(entry.iterations, 7);
    assert.equal(entry.warmups, 1);
    assert.ok(entry.workloads.length >= 4);
    for (const workload of entry.workloads) {
      assert.match(workload.outputSha256, /^[a-f0-9]{64}$/);
      assert.ok(Number.isFinite(workload.interpreterMedianMs));
      assert.ok(Number.isFinite(workload.nativeMedianMs));
      assert.ok(Number.isFinite(workload.interpreterToNativeMedianRatio));
    }
  }
});
