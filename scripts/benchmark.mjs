#!/usr/bin/env node

import { execFileSync, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { performance } from "node:perf_hooks";
import os from "node:os";
import { existsSync, mkdirSync, readFileSync, rmSync } from "node:fs";
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
  "examples/numeric_lu.ostrin",
  "examples/numeric_svd.ostrin",
  "examples/numeric_complex_linear_algebra.ostrin",
  "examples/viz_scatter_fit.ostrin",
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

function statistic(values) {
  const sorted = [...values].sort((a, b) => a - b);
  const mean = values.reduce((sum, value) => sum + value, 0) / values.length;
  const variance = values.reduce((sum, value) => sum + ((value - mean) ** 2), 0) / values.length;
  const percentileRank = (sorted.length - 1) * 0.95;
  const lower = Math.floor(percentileRank);
  const upper = Math.ceil(percentileRank);
  const p95 = lower === upper
    ? sorted[lower]
    : sorted[lower] + ((sorted[upper] - sorted[lower]) * (percentileRank - lower));
  return {
    samples: sorted.length,
    minMs: Number(sorted[0].toFixed(3)),
    maxMs: Number(sorted.at(-1).toFixed(3)),
    meanMs: Number(mean.toFixed(3)),
    medianMs: Number(median(values).toFixed(3)),
    p95Ms: Number(p95.toFixed(3)),
    stdevMs: Number(Math.sqrt(variance).toFixed(3)),
  };
}

function compilerToolchain() {
  const configured = process.env.OSTRIN_CC?.trim();
  const candidates = configured ? [configured] : ["cc", "gcc", "clang"];
  for (const command of candidates) {
    const result = spawnSync(command, ["--version"], { cwd: repo, encoding: "utf8", timeout: 10000 });
    if (!result.error && result.status === 0) {
      const version = (result.stdout || result.stderr || "").trim().split(/\r?\n/, 1)[0] || "unknown";
      return { command, version };
    }
  }
  return { command: candidates[0] || "unknown", version: "unavailable" };
}

function gitSnapshot() {
  let commit = process.env.GITHUB_SHA || "unknown";
  let clean = false;
  try {
    commit = execFileSync("git", ["rev-parse", "HEAD"], { cwd: repo, encoding: "utf8" }).trim() || commit;
    const status = execFileSync("git", ["status", "--porcelain"], { cwd: repo, encoding: "utf8" });
    clean = status.trim() === "";
  } catch {
    // The report still records the fallback commit and explicitly marks the
    // checkout as unverified instead of hiding the missing Git context.
  }
  return { commit, clean };
}

function hostMetadata() {
  const cpu = os.cpus()[0];
  return {
    platform: process.platform,
    arch: process.arch,
    os: {
      type: os.type(),
      release: os.release(),
      version: typeof os.version === "function" ? os.version() : "unknown",
    },
    cpu: {
      model: cpu?.model || "unknown",
      logicalCores: os.cpus().length,
    },
    memoryBytes: os.totalmem(),
  };
}

function nativeCompileMetadata() {
  const flags = ["-O2"];
  if (process.platform !== "win32") flags.push("-pthread", "-lm");
  if (process.env.OSTRIN_CFLAGS?.trim()) flags.push(...process.env.OSTRIN_CFLAGS.trim().split(/\s+/));
  flags.push("-ffp-contract=off");
  return {
    target: "native",
    cCompiler: compilerToolchain(),
    // These defaults are assembled by ostrinc itself in compiler/src/main.rs;
    // the runner records them as source-defined defaults, not as an intercepted
    // subprocess argv. OSTRIN_CFLAGS is recorded separately as an input.
    compilerManagedDefaults: flags,
    argvCapture: "not intercepted; see compiler/src/main.rs",
    environment: {
      OSTRIN_CC: process.env.OSTRIN_CC || null,
      OSTRIN_CFLAGS: process.env.OSTRIN_CFLAGS || null,
    },
  };
}

function executableName(source) {
  const stem = source.replace(/[^a-zA-Z0-9]+/g, "-").replace(/^-|-$/g, "").toLowerCase();
  return join(outputDir, `${stem}${process.platform === "win32" ? ".exe" : ""}`);
}

function benchmarkWorkload(sourcePath, index) {
  const source = resolve(repo, sourcePath);
  if (!existsSync(source)) throw new Error(`workload not found: ${sourcePath}`);
  const sourceBytes = readFileSync(source);
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
    sourceBytes: sourceBytes.byteLength,
    sourceSha256: outputHash(sourceBytes),
    outputBytes: Buffer.byteLength(expected),
    outputSha256: outputHash(expected),
    nativeCompileMs: Number(compile.elapsedMs.toFixed(3)),
    interpreterMs: interpreter.map((value) => Number(value.toFixed(3))),
    nativeMs: native.map((value) => Number(value.toFixed(3))),
    statistics: {
      interpreter: statistic(interpreter),
      native: statistic(native),
    },
    interpreterMedianMs: Number(interpreterMedianMs.toFixed(3)),
    nativeMedianMs: Number(nativeMedianMs.toFixed(3)),
    interpreterToNativeMedianRatio: Number((interpreterMedianMs / nativeMedianMs).toFixed(3)),
  };
}

const git = gitSnapshot();
const report = {
  schema: 3,
  workloads: workloadPaths.map((sourcePath, index) => benchmarkWorkload(sourcePath, index)),
  commit: git.commit,
  git,
  compiler: relative(repo, compiler).replaceAll("\\", "/"),
  compilerVersion: run(compiler, ["--version"], "compiler version").stdout.trim(),
  node: process.version,
  host: hostMetadata(),
  platform: process.platform,
  arch: process.arch,
  nativeCompile: nativeCompileMetadata(),
  methodology: {
    clock: "node:perf_hooks performance.now wall-clock process duration",
    outputCheck: "normalized UTF-8 stdout must match interpreter and native output exactly",
    memory: "not captured; peak child RSS and allocation counters are a planned metric",
  },
  iterations,
  warmups,
};

const reportPath = join(outputDir, "benchmark.json");
await writeFile(reportPath, `${JSON.stringify(report, null, 2)}\n`);
console.log(JSON.stringify(report, null, 2));
