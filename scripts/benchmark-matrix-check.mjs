#!/usr/bin/env node

import { readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const expectedPlatforms = new Set(["linux", "darwin", "win32"]);

function check(errors, condition, message) {
  if (!condition) errors.push(message);
}

function workloadMap(report, label, errors) {
  const workloads = report?.workloads;
  check(errors, Array.isArray(workloads) && workloads.length > 0, `${label}: report has no workloads`);
  const map = new Map();
  for (const workload of Array.isArray(workloads) ? workloads : []) {
    check(errors, typeof workload?.workload === "string" && workload.workload.length > 0,
      `${label}: workload path is missing`);
    if (typeof workload?.workload === "string") {
      check(errors, !map.has(workload.workload), `${label}: duplicate workload ${workload.workload}`);
      map.set(workload.workload, workload);
    }
  }
  return map;
}

function inputMap(workload, label, errors) {
  const inputs = workload?.runtimeInputs;
  check(errors, Array.isArray(inputs), `${label}: runtimeInputs must be an array`);
  const map = new Map();
  for (const input of Array.isArray(inputs) ? inputs : []) {
    const key = `${input?.path}\0${input?.stagedAs}`;
    check(errors, typeof input?.path === "string" && typeof input?.stagedAs === "string",
      `${label}: runtime input path or staged name is missing`);
    check(errors, !map.has(key), `${label}: duplicate runtime input ${key.replaceAll("\0", " → ")}`);
    map.set(key, input);
  }
  return map;
}

/** Verify platform-independent source/input/output identity while allowing host-specific raw bytes. */
export function validateBenchmarkMatrix(entries) {
  const errors = [];
  check(errors, Array.isArray(entries), "matrix entries must be an array");
  if (!Array.isArray(entries)) return errors;

  const byPlatform = new Map();
  for (const entry of entries) {
    const report = entry?.report;
    const label = entry?.path || report?.platform || "benchmark report";
    check(errors, report && typeof report === "object", `${label}: report is missing`);
    if (!report || typeof report !== "object") continue;
    check(errors, expectedPlatforms.has(report.platform), `${label}: unexpected platform ${report.platform}`);
    check(errors, !byPlatform.has(report.platform), `${label}: duplicate ${report.platform} report`);
    byPlatform.set(report.platform, { report, label });
  }

  for (const platform of expectedPlatforms) {
    check(errors, byPlatform.has(platform), `matrix is missing the ${platform} report`);
  }
  check(errors, byPlatform.size === expectedPlatforms.size, "matrix must contain exactly one report per supported platform");
  if (byPlatform.size !== expectedPlatforms.size) return errors;

  const reference = byPlatform.get("linux").report;
  const referenceLabel = byPlatform.get("linux").label;
  for (const [platform, { report, label }] of byPlatform) {
    check(errors, report.commit === reference.commit, `${label}: tested commit differs from ${referenceLabel}`);
    check(errors, report.methodology?.contentHashNormalization === reference.methodology?.contentHashNormalization,
      `${label}: content hash normalization differs from ${referenceLabel}`);
  }

  const referenceWorkloads = workloadMap(reference, referenceLabel, errors);
  for (const [platform, { report, label }] of byPlatform) {
    const workloads = workloadMap(report, label, errors);
    check(errors, workloads.size === referenceWorkloads.size, `${label}: workload set differs from ${referenceLabel}`);
    for (const [workloadPath, referenceWorkload] of referenceWorkloads) {
      const workload = workloads.get(workloadPath);
      check(errors, Boolean(workload), `${label}: missing workload ${workloadPath}`);
      if (!workload) continue;
      check(errors, typeof workload.sourceContentSha256 === "string"
        && workload.sourceContentSha256 === referenceWorkload.sourceContentSha256,
      `${label}: normalized source identity differs for ${workloadPath}`);
      check(errors, workload.outputSha256 === referenceWorkload.outputSha256,
        `${label}: normalized output differs for ${workloadPath}`);

      const referenceInputs = inputMap(referenceWorkload, `${referenceLabel}:${workloadPath}`, errors);
      const inputs = inputMap(workload, `${label}:${workloadPath}`, errors);
      check(errors, inputs.size === referenceInputs.size, `${label}: runtime input set differs for ${workloadPath}`);
      for (const [key, referenceInput] of referenceInputs) {
        const input = inputs.get(key);
        check(errors, Boolean(input), `${label}: missing runtime input ${key.replaceAll("\0", " → ")}`);
        if (!input) continue;
        const identityField = typeof referenceInput.contentSha256 === "string"
          ? "contentSha256"
          : "sha256";
        check(errors, typeof input[identityField] === "string"
          && input[identityField] === referenceInput[identityField],
        `${label}: runtime input identity differs for ${referenceInput.path}`);
      }
    }
  }
  return errors;
}

function findReports(directory) {
  const reports = [];
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const target = path.join(directory, entry.name);
    if (entry.isDirectory()) reports.push(...findReports(target));
    else if (entry.isFile() && entry.name === "benchmark.json") {
      reports.push({ path: target, report: JSON.parse(readFileSync(target, "utf8")) });
    }
  }
  return reports;
}

function main() {
  const artifactDirectory = process.argv[2];
  if (!artifactDirectory || !statSync(artifactDirectory, { throwIfNoEntry: false })?.isDirectory()) {
    console.error("benchmark-matrix-check: pass the directory containing downloaded platform artifacts");
    process.exitCode = 1;
    return;
  }
  const errors = validateBenchmarkMatrix(findReports(artifactDirectory));
  if (errors.length) {
    for (const error of errors) console.error(`benchmark-matrix-check: ${error}`);
    process.exitCode = 1;
    return;
  }
  console.log("benchmark-matrix-check: platform source, input and output identities agree for Linux, macOS and Windows");
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
