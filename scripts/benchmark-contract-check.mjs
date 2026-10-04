#!/usr/bin/env node

import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
export const reportPath = path.join(repositoryRoot, "target", "benchmarks", "benchmark.json");

function sha256(value) {
  return createHash("sha256").update(value).digest("hex");
}

function median(values) {
  const sorted = [...values].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 0
    ? (sorted[middle - 1] + sorted[middle]) / 2
    : sorted[middle];
}

function expectedStatistics(values) {
  const sorted = [...values].sort((a, b) => a - b);
  const mean = values.reduce((sum, value) => sum + value, 0) / values.length;
  const variance = values.reduce((sum, value) => sum + ((value - mean) ** 2), 0) / values.length;
  const rank = (sorted.length - 1) * 0.95;
  const lower = Math.floor(rank);
  const upper = Math.ceil(rank);
  const p95 = lower === upper
    ? sorted[lower]
    : sorted[lower] + ((sorted[upper] - sorted[lower]) * (rank - lower));
  return {
    samples: values.length,
    minMs: sorted[0],
    maxMs: sorted.at(-1),
    meanMs: mean,
    medianMs: median(values),
    p95Ms: p95,
    stdevMs: Math.sqrt(variance),
  };
}

function finite(value) {
  return typeof value === "number" && Number.isFinite(value);
}

function closeEnough(actual, expected) {
  return finite(actual) && Math.abs(actual - expected) <= 0.01;
}

function check(errors, condition, message) {
  if (!condition) errors.push(message);
}

function validateStatistics(errors, statistics, samples, label) {
  check(errors, statistics && typeof statistics === "object", `${label}: statistics are missing`);
  if (!statistics || typeof statistics !== "object") return;
  const expected = expectedStatistics(samples);
  check(errors, statistics.samples === samples.length, `${label}: statistics.samples does not match timing samples`);
  for (const key of ["minMs", "maxMs", "meanMs", "medianMs", "p95Ms", "stdevMs"]) {
    check(errors, closeEnough(statistics[key], expected[key]), `${label}: ${key} does not match timing samples`);
  }
}

function validateWorkload(errors, workload, index, report, root) {
  const label = `workloads[${index}]`;
  check(errors, workload && typeof workload === "object", `${label}: workload is missing`);
  if (!workload || typeof workload !== "object") return;
  check(errors, typeof workload.workload === "string" && workload.workload.length > 0, `${label}: source path is missing`);
  const source = path.resolve(root, workload.workload || "");
  check(errors, path.relative(root, source) && !path.relative(root, source).startsWith("..") && !path.isAbsolute(path.relative(root, source)), `${label}: source path escapes repository`);
  if (existsSync(source)) {
    const sourceBytes = readFileSync(source);
    check(errors, workload.sourceBytes === sourceBytes.byteLength, `${label}: sourceBytes does not match the checked-in workload`);
    check(errors, workload.sourceSha256 === sha256(sourceBytes), `${label}: sourceSha256 does not match the checked-in workload`);
  } else {
    check(errors, false, `${label}: workload source does not exist (${workload.workload})`);
  }
  check(errors, /^[a-f0-9]{64}$/.test(workload.sourceSha256 || ""), `${label}: sourceSha256 is not a SHA-256 digest`);
  check(errors, /^[a-f0-9]{64}$/.test(workload.outputSha256 || ""), `${label}: outputSha256 is not a SHA-256 digest`);
  check(errors, Number.isInteger(report.iterations) && report.iterations >= 1, "report: iterations must be a positive integer");
  for (const mode of ["interpreter", "native"]) {
    const samples = workload[`${mode}Ms`];
    check(errors, Array.isArray(samples) && samples.length === report.iterations, `${label}: ${mode}Ms sample count does not match iterations`);
    if (!Array.isArray(samples) || samples.length === 0) continue;
    check(errors, samples.every((sample) => finite(sample) && sample >= 0), `${label}: ${mode}Ms contains an invalid duration`);
    validateStatistics(errors, workload.statistics?.[mode], samples, `${label}.${mode}`);
  }
  check(errors, closeEnough(workload.interpreterMedianMs, workload.statistics?.interpreter?.medianMs), `${label}: interpreter median is not linked to statistics`);
  check(errors, closeEnough(workload.nativeMedianMs, workload.statistics?.native?.medianMs), `${label}: native median is not linked to statistics`);
  check(errors, closeEnough(
    workload.interpreterToNativeMedianRatio,
    workload.nativeMedianMs > 0 ? workload.interpreterMedianMs / workload.nativeMedianMs : NaN,
  ), `${label}: interpreter/native ratio is not linked to medians`);
}

/**
 * Validate the evidence contract emitted by scripts/benchmark.mjs.
 * This checks provenance and internal consistency; it does not make a
 * performance claim or compare Ostrin with another language.
 */
export function validateBenchmarkReport(report, root = repositoryRoot) {
  const errors = [];
  check(errors, report && typeof report === "object", "report is not an object");
  if (!report || typeof report !== "object") return errors;
  check(errors, report.schema === 3, "report: expected benchmark schema 3");
  check(errors, /^[a-f0-9]{40}$/.test(report.commit || ""), "report: commit must be a full Git revision");
  check(errors, report.git && typeof report.git.clean === "boolean", "report: Git cleanliness is missing");
  check(errors, typeof report.compiler === "string" && report.compiler.length > 0, "report: compiler path is missing");
  check(errors, typeof report.compilerVersion === "string" && report.compilerVersion.length > 0, "report: compiler version is missing");
  check(errors, /^v\d+/.test(report.node || ""), "report: Node.js version is missing");
  check(errors, Number.isInteger(report.iterations) && report.iterations >= 1, "report: iterations must be a positive integer");
  check(errors, Number.isInteger(report.warmups) && report.warmups >= 0, "report: warmups must be a non-negative integer");

  const host = report.host;
  check(errors, host && typeof host === "object", "report: host metadata is missing");
  check(errors, typeof host?.platform === "string" && typeof host?.arch === "string", "report: host platform/architecture is missing");
  check(errors, host?.os && typeof host.os.type === "string" && typeof host.os.release === "string" && typeof host.os.version === "string", "report: OS release metadata is missing");
  check(errors, host?.cpu && typeof host.cpu.model === "string" && Number.isInteger(host.cpu.logicalCores) && host.cpu.logicalCores > 0, "report: CPU metadata is missing");
  check(errors, finite(host?.memoryBytes) && host.memoryBytes > 0, "report: host memory metadata is missing");

  const nativeCompile = report.nativeCompile;
  check(errors, nativeCompile?.target === "native", "report: native compile target is missing");
  check(errors, Array.isArray(nativeCompile?.compilerManagedDefaults) && nativeCompile.compilerManagedDefaults.length > 0, "report: compiler-managed native defaults are missing");
  check(errors, typeof nativeCompile?.argvCapture === "string" && nativeCompile.argvCapture.length > 0, "report: compiler argv capture status is missing");
  check(errors, typeof nativeCompile?.cCompiler?.command === "string" && typeof nativeCompile?.cCompiler?.version === "string", "report: C compiler metadata is missing");
  check(errors, nativeCompile?.environment && Object.prototype.hasOwnProperty.call(nativeCompile.environment, "OSTRIN_CC") && Object.prototype.hasOwnProperty.call(nativeCompile.environment, "OSTRIN_CFLAGS"), "report: compiler environment inputs are missing");
  check(errors, typeof report.methodology?.clock === "string" && typeof report.methodology?.outputCheck === "string" && typeof report.methodology?.memory === "string", "report: benchmark methodology is incomplete");

  check(errors, Array.isArray(report.workloads) && report.workloads.length > 0, "report: no workloads recorded");
  for (const [index, workload] of (report.workloads || []).entries()) validateWorkload(errors, workload, index, report, root);
  return errors;
}

function main() {
  if (!existsSync(reportPath)) {
    console.error(`benchmark-contract-check: missing ${path.relative(repositoryRoot, reportPath)}; run scripts/benchmark.mjs first`);
    process.exitCode = 1;
    return;
  }
  let report;
  try {
    report = JSON.parse(readFileSync(reportPath, "utf8"));
  } catch (error) {
    console.error(`benchmark-contract-check: invalid JSON (${error.message})`);
    process.exitCode = 1;
    return;
  }
  const errors = validateBenchmarkReport(report);
  if (errors.length > 0) {
    for (const error of errors) console.error(`benchmark-contract-check: ${error}`);
    process.exitCode = 1;
  } else {
    console.log(`benchmark-contract-check: ok (schema 3 · ${report.workloads.length} workloads · ${report.iterations} samples · ${report.warmups} warmups)`);
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
