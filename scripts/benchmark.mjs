#!/usr/bin/env node

import { execFileSync, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { performance } from "node:perf_hooks";
import { existsSync, mkdirSync, rmSync } from "node:fs";
import { writeFile } from "node:fs/promises";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const repo = resolve(scriptDir, "..");
const outputDir = resolve(repo, "target", "benchmarks");
const defaultWorkloads = [
  "examples/benchmark_numeric.ostrin",
  "examples/arrays.ostrin",
  "examples/quantity_arrays.ostrin",
  "examples/numeric_methods.ostrin",
];

function option(name, fallback) {
  const index = process.argv.indexOf(name);
  return index === -1 ? fallback : Number(process.argv[index + 1]);
}

const iterations = option("--iterations", 7);
const warmups = option("--warmups", 1);
const workloadIndex = process.argv.indexOf("--workloads");
const workloadPaths = workloadIndex === -1
  ? defaultWorkloads
  : process.argv[workloadIndex + 1].split(",").map((value) => value.trim()).filter(Boolean);
if (!Number.isInteger(iterations) || iterations < 1 || !Number.isInteger(warmups) || warmups < 0) {
  throw new Error("--iterations must be >= 1 and --warmups must be >= 0");
}
if (workloadPaths.length === 0) throw new Error("--workloads must contain at least one source path");

const compiler = process.env.OSTRIN_COMPILER || join(repo, "compiler", "target", "release", process.platform === "win32" ? "ostrinc.exe" : "ostrinc");
if (!existsSync(compiler)) {
  throw new Error(`compiler not found at ${compiler}; run cargo build --release --manifest-path compiler/Cargo.toml first`);
}

function run(command, args, label) {
  const started = performance.now();
  const result = spawnSync(command, args, { cwd: repo, encoding: "utf8", timeout: 120000 });
  const elapsedMs = performance.now() - started;
  if (result.error) throw new Error(`${label}: ${result.error.message}`);
  if (result.status !== 0) {
    throw new Error(`${label} failed (${result.status}):\n${result.stdout}\n${result.stderr}`);
  }
  return { elapsedMs, stdout: result.stdout, stderr: result.stderr };
}

mkdirSync(outputDir, { recursive: true });
function median(values) {
  const sorted = [...values].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 0 ? (sorted[middle - 1] + sorted[middle]) / 2 : sorted[middle];
}

function outputHash(output) {
  return createHash("sha256").update(output).digest("hex");
}

function normalizeOutput(output) {
  return output.replaceAll("\r\n", "\n").trim();
}

function executableName(source) {
  const stem = source.replace(/[^a-zA-Z0-9]+/g, "-").replace(/^-|-$/g, "").toLowerCase();
  return join(outputDir, `${stem}${process.platform === "win32" ? ".exe" : ""}`);
}

function benchmarkWorkload(sourcePath, index) {
  const source = resolve(repo, sourcePath);
  if (!existsSync(source)) throw new Error(`workload not found: ${sourcePath}`);
  const executable = executableName(sourcePath);
  rmSync(executable, { force: true });
  const compile = run(compiler, ["--compile", "--out", executable, source], `native compilation ${sourcePath}`);
  if (!existsSync(executable)) throw new Error(`native compilation did not create ${executable}`);

  const expected = normalizeOutput(run(compiler, ["--run", source], `interpreter warmup ${sourcePath}`).stdout);
  const nativeWarmup = normalizeOutput(run(executable, [], `native warmup ${sourcePath}`).stdout);
  if (expected !== nativeWarmup) {
    throw new Error(`interpreter/native output mismatch for ${sourcePath}:\ninterpreter=${expected}\nnative=${nativeWarmup}`);
  }

  for (let warmup = 0; warmup < warmups; warmup += 1) {
    run(compiler, ["--run", source], `interpreter warmup ${sourcePath} ${warmup + 1}`);
    run(executable, [], `native warmup ${sourcePath} ${warmup + 1}`);
  }

  const interpreter = [];
  const native = [];
  for (let iteration = 0; iteration < iterations; iteration += 1) {
    const interpreted = run(compiler, ["--run", source], `interpreter iteration ${sourcePath} ${iteration + 1}`);
    const compiled = run(executable, [], `native iteration ${sourcePath} ${iteration + 1}`);
    if (normalizeOutput(interpreted.stdout) !== expected || normalizeOutput(compiled.stdout) !== expected) {
      throw new Error(`output changed during iteration ${iteration + 1} for ${sourcePath}`);
    }
    interpreter.push(interpreted.elapsedMs);
    native.push(compiled.elapsedMs);
  }

  const interpreterMedianMs = median(interpreter);
  const nativeMedianMs = median(native);
  return {
    index,
    workload: sourcePath.replaceAll("\\", "/"),
    outputBytes: Buffer.byteLength(expected),
    outputSha256: outputHash(expected),
    nativeCompileMs: Number(compile.elapsedMs.toFixed(3)),
    interpreterMs: interpreter.map((value) => Number(value.toFixed(3))),
    nativeMs: native.map((value) => Number(value.toFixed(3))),
    interpreterMedianMs: Number(interpreterMedianMs.toFixed(3)),
    nativeMedianMs: Number(nativeMedianMs.toFixed(3)),
    interpreterToNativeMedianRatio: Number((interpreterMedianMs / nativeMedianMs).toFixed(3)),
  };
}

const commit = (() => {
  try {
    return execFileSync("git", ["rev-parse", "HEAD"], { cwd: repo, encoding: "utf8" }).trim();
  } catch {
    return process.env.GITHUB_SHA || null;
  }
})();
const report = {
  schema: 2,
  workloads: workloadPaths.map((sourcePath, index) => benchmarkWorkload(sourcePath, index)),
  commit,
  compiler: relative(repo, compiler).replaceAll("\\", "/"),
  compilerVersion: run(compiler, ["--version"], "compiler version").stdout.trim(),
  node: process.version,
  platform: process.platform,
  arch: process.arch,
  iterations,
  warmups,
};

const reportPath = join(outputDir, "benchmark.json");
await writeFile(reportPath, `${JSON.stringify(report, null, 2)}\n`);
console.log(JSON.stringify(report, null, 2));
