#!/usr/bin/env node
// Bounded Linux/GCC phase profiling; this does not add a clock API to Ostrin.
import { spawn, spawnSync, execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { performance } from "node:perf_hooks";
import os from "node:os";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { instrumentationPlan, pendulumPhases, profilerSource, parsePhaseReport } from "./benchmark-native-phases.mjs";
import { parseAllocationCounters } from "./benchmark-allocation-profile.mjs";
import { assertBenchmarkWorkloadSucceeded } from "./benchmark-runtime-inputs.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const output = path.join(root, "target", "native-phase-profile");
const sourcePath = "examples/viz_pendulum_calibration.ostrin";
const inputPath = "experiments/pendulum-calibration.inputs.json";
const compiler = path.resolve(root, process.env.OSTRIN_COMPILER || "compiler/target/release/ostrinc");
const cc = process.env.OSTRIN_CC || "gcc";
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");
const normalize = (text) => text.replaceAll("\r\n", "\n").trim();
const iterationsIndex = process.argv.indexOf("--iterations");
const iterations = iterationsIndex < 0 ? 7 : Number(process.argv[iterationsIndex + 1]);
const median = (values) => { const xs = [...values].sort((a, b) => a - b); return (xs[Math.floor((xs.length - 1) / 2)] + xs[Math.floor(xs.length / 2)]) / 2; };

function run(command, args, cwd = root) {
  const result = spawnSync(command, args, { cwd, encoding: "utf8", timeout: 180000, maxBuffer: 16 * 1024 * 1024 });
  if (result.error || result.status !== 0) throw new Error(`${command} failed: ${result.error?.message || result.stderr}`);
  return result;
}

function runSample(executable, cwd, expectEntry) {
  return new Promise((resolve, reject) => {
    const start = performance.now();
    const child = spawn(executable, [], { cwd, stdio: ["ignore", "pipe", "pipe"], timeout: 120000 });
    let stdout = "", stderr = "", entryLatencyMs = null, overflow = false;
    child.stdout.setEncoding("utf8"); child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => { stdout += chunk; if (stdout.length > 16 * 1024 * 1024) { overflow = true; child.kill(); } });
    child.stderr.on("data", (chunk) => {
      stderr += chunk;
      if (entryLatencyMs === null && stderr.includes("ostrin phase entry\n")) entryLatencyMs = performance.now() - start;
      if (stderr.length > 1024 * 1024) { overflow = true; child.kill(); }
    });
    child.once("error", reject);
    child.once("close", (code, signal) => {
      if (code !== 0 || overflow || (expectEntry && entryLatencyMs === null)) {
        reject(new Error(`profile process failed (${code}/${signal}): ${stderr}`)); return;
      }
      resolve({ elapsedMs: performance.now() - start, entryLatencyMs, stdout, stderr });
    });
  });
}

async function main() {
  if (process.platform !== "linux") throw new Error("native phase profiling currently requires Linux and GCC");
  if (!Number.isSafeInteger(iterations) || iterations < 1 || iterations > 100) throw new Error("--iterations must be between 1 and 100");
  mkdirSync(output, { recursive: true });
  const reportPath = path.join(output, "report.json"); rmSync(reportPath, { force: true });
  const revision = execFileSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8" }).trim();
  const gitClean = !execFileSync("git", ["status", "--porcelain"], { cwd: root, encoding: "utf8" }).trim();
  const source = path.join(root, sourcePath), input = readFileSync(path.join(root, inputPath));
  const sourceHash = sha256(readFileSync(source)), compilerHash = sha256(readFileSync(compiler));
  writeFileSync(path.join(output, "source.ostrin"), readFileSync(source));
  writeFileSync(path.join(output, "data.json"), input);
  const cPath = path.join(output, "emitted.c"), profiledCPath = path.join(output, "instrumented.c");
  run(compiler, ["--emit-c", "--leak-check", "--out", cPath, source]);
  const emitted = readFileSync(cPath, "utf8");
  const plan = instrumentationPlan(emitted);
  const templatePath = path.join(root, "scripts", "native-phase-profiler.c");
  const template = readFileSync(templatePath, "utf8");
  writeFileSync(profiledCPath, emitted + "\n" + profilerSource(template));
  const control = path.join(output, "control"), profiled = path.join(output, "profiled");
  // Match native source defaults. Environment overrides are rejected here so
  // sanitizer/fast-math flags cannot silently contaminate the timing experiment.
  if (process.env.OSTRIN_CFLAGS?.trim()) throw new Error("run phase profiling without OSTRIN_CFLAGS overrides");
  const flags = ["-O2", "-pthread", "-lm", "-ffp-contract=off"];
  run(cc, ["emitted.c", "-o", "control", ...flags], output);
  run(cc, ["instrumented.c", "-o", "profiled", ...flags, ...plan.flags], output);
  const withInput = async (operation) => {
    const cwd = mkdtempSync(path.join(output, "input-"));
    try {
      writeFileSync(path.join(cwd, "data.json"), input);
      const result = await operation(cwd);
      if (sha256(readFileSync(path.join(cwd, "data.json"))) !== sha256(input)) throw new Error("staged input changed during execution");
      return result;
    }
    finally { rmSync(cwd, { recursive: true, force: true }); }
  };
  const expected = await withInput((cwd) => normalize(run(compiler, ["--run", source], cwd).stdout));
  assertBenchmarkWorkloadSucceeded(expected, "", "phase profile interpreter");
  if (!expected.includes("<svg")) throw new Error("expected the complete scientific report and SVG");
  writeFileSync(path.join(output, "expected.stdout"), expected);
  const samples = [];
  // One untimed warmup for each executable, validated through the same path.
  for (let iteration = -1; iteration < iterations; iteration += 1) {
    const order = iteration % 2 === 0 ? ["control", "profiled"] : ["profiled", "control"];
    const sample = { order };
    for (const kind of order) {
      const result = await withInput((cwd) => runSample(kind === "control" ? control : profiled, cwd, kind === "profiled"));
      assertBenchmarkWorkloadSucceeded(result.stdout, result.stderr, kind);
      if (normalize(result.stdout) !== expected) throw new Error(`${kind} output differs from interpreted report/SVG`);
      const counters = parseAllocationCounters(result.stderr);
      sample[kind] = { elapsedMs: result.elapsedMs, outputSha256: sha256(expected), allocations: counters };
      if (kind === "profiled") {
        const phases = parsePhaseReport(result.stderr);
        sample.profiled.entryObservationMs = result.entryLatencyMs;
        sample.profiled.phaseReport = phases;
        sample.profiled.launchExitTransportResidualMs = result.elapsedMs - phases.phases[0].inclusiveNs / 1e6;
        if (sample.profiled.launchExitTransportResidualMs < 0) throw new Error("phase time exceeds observed process duration");
      }
    }
    if (iteration >= 0) samples.push(sample);
  }
  if (sourceHash !== sha256(readFileSync(source)) || compilerHash !== sha256(readFileSync(compiler))
    || sha256(input) !== sha256(readFileSync(path.join(root, inputPath)))) throw new Error("profile inputs changed during collection");
  const report = {
    schema: "ostrin.native-phase-profile/v1", revision, gitClean, iterations, warmups: 1,
    source: { path: sourcePath, sha256: sourceHash }, input: { path: inputPath, sha256: sha256(input) },
    compilerSha256: compilerHash, compilerVersion: run(compiler, ["--version"]).stdout.trim(),
    cc: { command: cc, version: run(cc, ["--version"]).stdout.trim() },
    host: { platform: process.platform, arch: process.arch, node: process.version, cpu: os.cpus()[0]?.model, osRelease: os.release() },
    emittedCSha256: sha256(emitted), instrumentedCSha256: sha256(readFileSync(profiledCPath)),
    profilerSourceSha256: sha256(template), controlSha256: sha256(readFileSync(control)), profiledSha256: sha256(readFileSync(profiled)),
    flags, instrumentationFlags: plan.flags, selectedPhases: pendulumPhases,
    discoveredCFunctions: plan.definitions,
    runnerScriptSha256: sha256(readFileSync(fileURLToPath(import.meta.url))),
    planScriptSha256: sha256(readFileSync(path.join(root, "scripts", "benchmark-native-phases.mjs"))),
    methodology: {
      clock: "CLOCK_MONOTONIC nanoseconds inside selected emitted C functions; Node performance.now process observations",
      scope: "Single-thread Linux/GCC pendulum workflow; profiling changes optimization/instrumentation overhead. Control uses the same emitted C without instrumentation.",
      entryObservation: "Launch-to-entry-marker delivery upper observation, including loader/scheduling/pipe/event-loop delay; not pure loader time.",
      residual: "Process elapsed minus C-main inclusive time; includes launch, exit, stream transport, receipt and profiler teardown. Not pure startup time.",
      durations: "Inclusive/exclusive phase samples are instrumented observations, never overhead-corrected or claimed as uninstrumented kernel time. Individual medians need not add up; each raw sample partitions its root interval exactly.",
      output: "All control/profiled runs must match normalized interpreted report and SVG; zero live managed allocations required.",
    },
    medians: {
      controlProcessMs: median(samples.map((s) => s.control.elapsedMs)),
      profiledProcessMs: median(samples.map((s) => s.profiled.elapsedMs)),
      entryObservationMs: median(samples.map((s) => s.profiled.entryObservationMs)),
      phases: pendulumPhases.map((phase, index) => ({ name: phase.name,
        inclusiveMs: median(samples.map((s) => s.profiled.phaseReport.phases[index].inclusiveNs / 1e6)),
        exclusiveMs: median(samples.map((s) => s.profiled.phaseReport.phases[index].exclusiveNs / 1e6)),
      })),
    }, samples,
  };
  writeFileSync(reportPath, JSON.stringify(report, null, 2) + "\n");
  console.log(JSON.stringify({ status: "pass", revision, gitClean, medians: report.medians, reportPath }, null, 2));
}
main().catch((error) => { console.error(`benchmark-phases: ${error.message}`); process.exitCode = 1; });
