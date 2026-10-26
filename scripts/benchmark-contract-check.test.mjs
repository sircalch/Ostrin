import test from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { repositoryRoot, validateBenchmarkReport } from "./benchmark-contract-check.mjs";
import { normalizedTextSha256 } from "./benchmark-content-hash.mjs";

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
    medianMs: sorted.length % 2 === 0
      ? (sorted[sorted.length / 2 - 1] + sorted[sorted.length / 2]) / 2
      : sorted[Math.floor(sorted.length / 2)],
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
    compilerSha256: "d".repeat(64),
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
      contentHashNormalization: "UTF-8 line endings normalized to LF",
    },
    iterations: 3,
    warmups: 1,
    compileIterations: 3,
    workloads: [{
      index: 0,
      workload: relativeWorkload,
      sourceBytes: source.byteLength,
      sourceSha256: digest(source),
      sourceContentSha256: normalizedTextSha256(source),
      executableSha256: "e".repeat(64),
      runtimeInputs: [],
      outputBytes: 1,
      outputSha256: "b".repeat(64),
      nativeCompileMs: 4,
      nativeCompileMsSamples: [3, 4, 5],
      nativeCompileStatistics: stats([3, 4, 5]),
      interpreterMs,
      nativeMs,
      statistics: { interpreter: stats(interpreterMs), native: stats(nativeMs) },
      interpreterMedianMs: 2,
      nativeMedianMs: 0.75,
      interpreterToNativeMedianRatio: 2.6666666666666665,
    }],
  };
}

test("schema 3 report records source, host, compiler, content identity and timing provenance", () => {
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

test("the ratio contract accounts for rounding amplified by a small native median", () => {
  const report = fixture();
  const workload = report.workloads[0];
  workload.interpreterMs = [114.696, 114.697, 114.698];
  workload.nativeMs = [2.115, 2.116, 2.117];
  workload.statistics = { interpreter: stats(workload.interpreterMs), native: stats(workload.nativeMs) };
  workload.interpreterMedianMs = 114.697;
  workload.nativeMedianMs = 2.116;
  workload.interpreterToNativeMedianRatio = 54.217;

  assert.deepEqual(validateBenchmarkReport(report), []);
  workload.interpreterToNativeMedianRatio = 100;
  assert.ok(validateBenchmarkReport(report).some((error) => error.includes("ratio is not linked to medians")));
});

test("file-backed workloads bind staged runtime inputs to checked-in bytes", () => {
  const report = fixture();
  const relativeInput = "experiments/pendulum-calibration.inputs.json";
  const input = readFileSync(path.join(repositoryRoot, relativeInput));
  report.workloads[0].runtimeInputs = [{
    path: relativeInput,
    stagedAs: "data.json",
    sizeBytes: input.byteLength,
    sha256: digest(input),
    stagedSha256: digest(input),
    contentSha256: normalizedTextSha256(input),
    stagedContentSha256: normalizedTextSha256(input),
  }];
  assert.deepEqual(validateBenchmarkReport(report), []);

  report.workloads[0].runtimeInputs[0].stagedAs = "../outside.json";
  report.workloads[0].runtimeInputs[0].stagedSha256 = "c".repeat(64);
  report.workloads[0].runtimeInputs[0].stagedContentSha256 = "c".repeat(64);
  const errors = validateBenchmarkReport(report);
  assert.ok(errors.some((error) => error.includes("stagedAs must stay within")));
  assert.ok(errors.some((error) => error.includes("staged input hash differs")));
  assert.ok(errors.some((error) => error.includes("staged content hash differs")));

  report.workloads[0].runtimeInputs[0].path = "";
  assert.doesNotThrow(() => validateBenchmarkReport(report));
  assert.ok(validateBenchmarkReport(report).some((error) => error.includes("source path escapes repository")));
});

test("source and file-backed input identities stay stable when a checkout uses CRLF", () => {
  const report = fixture();
  const source = readFileSync(path.join(repositoryRoot, report.workloads[0].workload));
  const lfSource = Buffer.from(source.toString("utf8").replace(/\r\n?/g, "\n"), "utf8");
  const crlfSource = Buffer.from(lfSource.toString("utf8").replace(/\n/g, "\r\n"), "utf8");
  const relativeInput = "experiments/pendulum-calibration.inputs.json";
  const input = readFileSync(path.join(repositoryRoot, relativeInput));
  const lfInput = Buffer.from(input.toString("utf8").replace(/\r\n?/g, "\n"), "utf8");
  const crlfInput = Buffer.from(lfInput.toString("utf8").replace(/\n/g, "\r\n"), "utf8");
  report.workloads[0].sourceBytes = crlfSource.byteLength;
  report.workloads[0].sourceSha256 = digest(crlfSource);
  report.workloads[0].sourceContentSha256 = normalizedTextSha256(crlfSource);
  report.workloads[0].runtimeInputs = [{
    path: relativeInput,
    stagedAs: "data.json",
    sizeBytes: crlfInput.byteLength,
    sha256: digest(crlfInput),
    stagedSha256: digest(crlfInput),
    contentSha256: normalizedTextSha256(crlfInput),
    stagedContentSha256: normalizedTextSha256(crlfInput),
  }];

  assert.notEqual(digest(crlfSource), digest(lfSource));
  assert.equal(report.workloads[0].sourceContentSha256, normalizedTextSha256(lfSource));
  assert.notEqual(digest(crlfInput), digest(lfInput));
  assert.equal(report.workloads[0].runtimeInputs[0].contentSha256, normalizedTextSha256(lfInput));
  const root = mkdtempSync(path.join(tmpdir(), "ostrin-benchmark-content-"));
  try {
    const sourcePath = path.join(root, report.workloads[0].workload);
    const inputPath = path.join(root, relativeInput);
    mkdirSync(path.dirname(sourcePath), { recursive: true });
    mkdirSync(path.dirname(inputPath), { recursive: true });
    writeFileSync(sourcePath, crlfSource);
    writeFileSync(inputPath, crlfInput);
    assert.deepEqual(validateBenchmarkReport(report, root), []);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("compile timing samples and their median must match the declared compile count", () => {
  const report = fixture();
  report.workloads[0].nativeCompileMs = 100;
  const errors = validateBenchmarkReport(report);
  assert.ok(errors.some((error) => error.includes("native compile median is not linked")));
  report.workloads[0].nativeCompileMsSamples.pop();
  assert.ok(validateBenchmarkReport(report).some((error) => error.includes("nativeCompileMsSamples count")));
});
