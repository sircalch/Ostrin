import test from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import path from "node:path";
import { repositoryRoot, validateBenchmarkReport } from "./benchmark-contract-check.mjs";

function digest(value) {
  return createHash("sha256").update(value).digest("hex");
}

function stats(samples) {
  const sorted = [...samples].sort((a, b) => a - b);
  const mean = samples.reduce((sum, value) => sum + value, 0) / samples.length;
  const variance = samples.reduce((sum, value) => sum + ((value - mean) ** 2), 0) / samples.length;
  const rank = (sorted.length - 1) * 0.95;
  const lower = Math.floor(rank);
  const upper = Math.ceil(rank);
  return {
    samples: samples.length,
    minMs: sorted[0],
    maxMs: sorted.at(-1),
    meanMs: mean,
    medianMs: sorted[1],
    p95Ms: lower === upper ? sorted[lower] : sorted[lower] + ((sorted[upper] - sorted[lower]) * (rank - lower)),
    stdevMs: Math.sqrt(variance),
  };
}

function fixture() {
  const relativeWorkload = "examples/benchmark_numeric.ostrin";
  const source = readFileSync(path.join(repositoryRoot, relativeWorkload));
  const interpreterMs = [1, 2, 3];
  const nativeMs = [0.5, 0.75, 1];
  return {
    schema: 3,
    commit: "a".repeat(40),
    git: { commit: "a".repeat(40), clean: true },
    compiler: "compiler/target/release/ostrinc",
    compilerVersion: "ostrinc 0.1.0",
    node: "v24.0.0",
    host: {
      platform: "linux",
      arch: "x64",
      os: { type: "Linux", release: "6.0", version: "Linux" },
      cpu: { model: "test CPU", logicalCores: 4 },
      memoryBytes: 1024,
    },
    nativeCompile: {
      target: "native",
      cCompiler: { command: "cc", version: "cc test" },
      compilerManagedDefaults: ["-O2", "-ffp-contract=off"],
      argvCapture: "not intercepted; see compiler/src/main.rs",
      environment: { OSTRIN_CC: null, OSTRIN_CFLAGS: null },
    },
    methodology: {
      clock: "performance.now",
      outputCheck: "exact",
      memory: "not captured",
    },
    iterations: 3,
    warmups: 1,
    workloads: [{
      index: 0,
      workload: relativeWorkload,
      sourceBytes: source.byteLength,
      sourceSha256: digest(source),
      outputBytes: 1,
      outputSha256: "b".repeat(64),
      nativeCompileMs: 4,
      interpreterMs,
      nativeMs,
      statistics: { interpreter: stats(interpreterMs), native: stats(nativeMs) },
      interpreterMedianMs: 2,
      nativeMedianMs: 0.75,
      interpreterToNativeMedianRatio: 2.6666666666666665,
    }],
  };
}

test("schema 3 report records source, host, compiler and timing provenance", () => {
  assert.deepEqual(validateBenchmarkReport(fixture()), []);
});

test("the contract rejects a changed workload even when output metadata looks valid", () => {
  const report = fixture();
  report.workloads[0].sourceSha256 = "c".repeat(64);
  const errors = validateBenchmarkReport(report);
  assert.ok(errors.some((error) => error.includes("sourceSha256 does not match")));
});

test("the contract rejects reports without dispersion statistics", () => {
  const report = fixture();
  delete report.workloads[0].statistics.native;
  const errors = validateBenchmarkReport(report);
  assert.ok(errors.some((error) => error.includes("native: statistics are missing")));
});
