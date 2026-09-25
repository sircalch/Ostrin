#!/usr/bin/env node

import { execFileSync, spawnSync } from "node:child_process";
import { performance } from "node:perf_hooks";
import { existsSync, mkdirSync, rmSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const repo = resolve(scriptDir, "..");
const source = resolve(repo, "examples", "benchmark_numeric.ostrin");
const outputDir = resolve(repo, "target", "benchmarks");
const executable = join(outputDir, process.platform === "win32" ? "benchmark_numeric.exe" : "benchmark_numeric");

function option(name, fallback) {
  const index = process.argv.indexOf(name);
  return index === -1 ? fallback : Number(process.argv[index + 1]);
}

const iterations = option("--iterations", 7);
const warmups = option("--warmups", 1);
if (!Number.isInteger(iterations) || iterations < 1 || !Number.isInteger(warmups) || warmups < 0) {
  throw new Error("--iterations must be >= 1 and --warmups must be >= 0");
}

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
rmSync(executable, { force: true });
const compile = run(compiler, ["--compile", "--out", executable, source], "native compilation");
if (!existsSync(executable)) throw new Error(`native compilation did not create ${executable}`);

const expected = run(compiler, ["--run", source], "interpreter warmup").stdout.trim();
const nativeWarmup = run(executable, [], "native warmup").stdout.trim();
if (expected !== nativeWarmup) {
  throw new Error(`interpreter/native output mismatch:\ninterpreter=${expected}\nnative=${nativeWarmup}`);
}

for (let index = 0; index < warmups; index += 1) {
  run(compiler, ["--run", source], `interpreter warmup ${index + 1}`);
  run(executable, [], `native warmup ${index + 1}`);
}

const interpreter = [];
const native = [];
for (let index = 0; index < iterations; index += 1) {
  const interpreted = run(compiler, ["--run", source], `interpreter iteration ${index + 1}`);
  const compiled = run(executable, [], `native iteration ${index + 1}`);
  if (interpreted.stdout.trim() !== expected || compiled.stdout.trim() !== expected) {
    throw new Error(`output changed during iteration ${index + 1}`);
  }
  interpreter.push(interpreted.elapsedMs);
  native.push(compiled.elapsedMs);
}

function median(values) {
  const sorted = [...values].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 0 ? (sorted[middle - 1] + sorted[middle]) / 2 : sorted[middle];
}

const commit = (() => {
  try {
    return execFileSync("git", ["rev-parse", "HEAD"], { cwd: repo, encoding: "utf8" }).trim();
  } catch {
    return process.env.GITHUB_SHA || null;
  }
})();
const report = {
  schema: 1,
  workload: relative(repo, source).replaceAll("\\", "/"),
  commit,
  compiler: relative(repo, compiler).replaceAll("\\", "/"),
  compilerVersion: run(compiler, ["--version"], "compiler version").stdout.trim(),
  node: process.version,
  platform: process.platform,
  arch: process.arch,
  iterations,
  warmups,
  expectedOutput: expected,
  nativeCompileMs: Number(compile.elapsedMs.toFixed(3)),
  interpreterMs: interpreter.map((value) => Number(value.toFixed(3))),
  nativeMs: native.map((value) => Number(value.toFixed(3))),
  interpreterMedianMs: Number(median(interpreter).toFixed(3)),
  nativeMedianMs: Number(median(native).toFixed(3)),
  interpreterToNativeMedianRatio: Number((median(interpreter) / median(native)).toFixed(3)),
};

const reportPath = join(outputDir, "benchmark.json");
await import("node:fs/promises").then(({ writeFile }) => writeFile(reportPath, `${JSON.stringify(report, null, 2)}\n`));
console.log(JSON.stringify(report, null, 2));
