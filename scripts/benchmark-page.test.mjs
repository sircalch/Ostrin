import test from "node:test";
import assert from "node:assert/strict";
import { pageDataFromReport, renderPageData } from "./benchmark-page.mjs";

function report() {
  return {
    schema: 3,
    commit: "a".repeat(40),
    compilerVersion: "ostrinc 0.1.0",
    node: "v24.0.0",
    platform: "linux",
    arch: "x64",
    iterations: 1,
    warmups: 0,
    host: { os: { type: "Linux", release: "6", version: "Linux" } },
    git: { clean: true },
    nativeCompile: { compilerManagedDefaults: ["-O2"] },
    methodology: { memory: "not captured" },
    workloads: [{
      workload: "examples/benchmark_numeric.ostrin",
      sourceBytes: 10,
      sourceSha256: "b".repeat(64),
      outputBytes: 1,
      outputSha256: "c".repeat(64),
      nativeCompileMs: 1,
      statistics: null,
      interpreterMedianMs: 2,
      nativeMedianMs: 1,
      interpreterToNativeMedianRatio: 2,
    }],
  };
}

test("benchmark page accepts schema 3 and preserves provenance fields", () => {
  const data = pageDataFromReport(report());
  assert.equal(data.schema, 1);
  assert.equal(data.host.os.release, "6");
  assert.deepEqual(data.nativeCompile.compilerManagedDefaults, ["-O2"]);
  assert.match(renderPageData(data), /globalThis\.OSTRIN_BENCHMARK/);
});

test("benchmark page rejects the removed schema 2 report", () => {
  assert.throws(() => pageDataFromReport({ ...report(), schema: 2 }), /expected schema 3/);
});
