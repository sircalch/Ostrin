import test from "node:test";
import assert from "node:assert/strict";
import { validateBenchmarkMatrix } from "./benchmark-matrix-check.mjs";

const platforms = ["linux", "darwin", "win32"];

function entry(platform, { sourceContentSha256 = "a".repeat(64), outputSha256 = "b".repeat(64) } = {}) {
  const rawSuffix = platform === "win32" ? "c" : "d";
  return {
    path: `${platform}/benchmark.json`,
    report: {
      platform,
      commit: "1".repeat(40),
      methodology: { contentHashNormalization: "CRLF and CR normalized to LF" },
      workloads: [{
        workload: "examples/viz_pendulum_calibration.ostrin",
        sourceSha256: rawSuffix.repeat(64),
        sourceContentSha256,
        outputSha256,
        runtimeInputs: [{
          path: "experiments/pendulum-calibration.inputs.json",
          stagedAs: "data.json",
          sha256: rawSuffix.repeat(64),
          contentSha256: "e".repeat(64),
        }],
      }],
    },
  };
}

function matrix(overrides = {}) {
  return platforms.map((platform) => entry(platform, overrides[platform]));
}

test("the matrix accepts identical content with host-specific raw hashes", () => {
  assert.deepEqual(validateBenchmarkMatrix(matrix()), []);
});

test("the matrix rejects a platform with different source or output content", () => {
  const reports = matrix({
    "win32": { sourceContentSha256: "f".repeat(64) },
  });
  assert.ok(validateBenchmarkMatrix(reports).some((error) => error.includes("normalized source identity differs")));

  const changedOutput = matrix({
    "darwin": { outputSha256: "0".repeat(64) },
  });
  assert.ok(validateBenchmarkMatrix(changedOutput).some((error) => error.includes("normalized output differs")));
});

test("the matrix rejects different runtime input content", () => {
  const reports = matrix();
  reports[2].report.workloads[0].runtimeInputs[0].contentSha256 = "f".repeat(64);
  assert.ok(validateBenchmarkMatrix(reports).some((error) => error.includes("runtime input identity differs")));
});

test("the matrix requires exactly one report for each platform", () => {
  assert.ok(validateBenchmarkMatrix(matrix().slice(0, 2)).some((error) => error.includes("missing the win32 report")));
  assert.ok(validateBenchmarkMatrix([...matrix(), entry("linux")]).some((error) => error.includes("duplicate linux report")));
});
