// Linear-algebra comparison: Ostrin native, Ostrin interpreter, CPython (pure and NumPy), Julia (pure and library).
//
// Usage: node benchmarks/v2/linalg/compare-linalg.mjs [--ostrinc <path>] [--sizes 100,200] [--reps 5] [--out dir]
//
// Kernels: matmul (dense product, checksum), lu_solve (LU with partial pivoting, sum of solution), norms (1-norm, infinity
// norm, Frobenius norm). solve_checked exists only in Ostrin (std.linalg), so it is measured for Ostrin only.
//
// Methods (each one is written into the JSON and the Markdown report):
//  - Ostrin native: compiled once per kernel with `ostrinc --compile` (the compile time is recorded and never falls inside a
//    timed run). EXTERNAL SLOPE: each sample is (T(K=KBIG) - T(K=1)) / (KBIG - 1) for one process pair, measured with
//    process.hrtime, so it excludes the constant process start-up, which is estimated as T(1) - slope.
//  - Ostrin interpreter: the same external slope, with KBIG_INTERP.
//  - CPython and Julia: INTERNAL. One process repeats the kernel INTERNAL_K times and times each repetition with its own clock
//    (time.perf_counter / time_ns). Repetition 1 of each process is the COLD figure (it includes first-call costs such as the
//    NumPy/BLAS first call and Julia's JIT compilation); repetitions 2..K are the WARM figure. Inputs are generated inside the
//    process before the first timed repetition, so input generation is never timed.
//  - Order: in every repetition the implementations run in a rotated order (repetition r starts at implementation r mod m),
//    so no implementation always runs first or last.
//  - Verification: every RESULT line is checked against benchmarks/v2/linalg/reference.mjs (exact integers for matmul and the
//    1- and infinity norms, a justified tolerance for the solve and the Frobenius norm). A sample whose output fails the check,
//    or whose process exits non-zero, marks its cell INCOMPLETE and no figure is reported for it.
//  - Load: the average processor load (Win32_Processor.LoadPercentage, an instantaneous value) is recorded before and after
//    every cell. The machine was not idle; the load is recorded as a condition of the measurement, not controlled.
//  - Threads: OMP_NUM_THREADS, OPENBLAS_NUM_THREADS and MKL_NUM_THREADS are 1 in every child; Julia calls BLAS.set_num_threads(1).
//
// What is NOT measured: residual computation as a separate kernel (it is inside solve_checked, Ostrin only); linear and
// quadratic fits (not implemented in every language, so not compared here). The LU factorisation is measured inside lu_solve,
// not separately.

import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync, existsSync } from "node:fs";
import { arch, cpus, platform, release, tmpdir, totalmem } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { checkRun } from "./reference.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..", "..", "..");
const args = process.argv.slice(2);
const option = (name, fallback) => {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : fallback;
};

const SIZES = option("--sizes", "100,200").split(",").map(Number);
const REPS = Number(option("--reps", "5"));
const INTERP_REPS = Number(option("--interp-reps", "3"));
const KBIG_NATIVE = 40;
const KBIG_INTERP = 3;
const INTERNAL_K = 5;
const COMPILE_SAMPLES = 3;
const TIMEOUT = 3_600_000;
const ostrinc = option("--ostrinc", join(root, "compiler", "target", "release", platform() === "win32" ? "ostrinc.exe" : "ostrinc"));
const outDir = option("--out", join(here, "results"));
const env = { ...process.env, OMP_NUM_THREADS: "1", OPENBLAS_NUM_THREADS: "1", MKL_NUM_THREADS: "1" };
const pyScript = join(here, "python", "linalg_kernels.py");
const jlScript = join(here, "julia", "linalg_kernels.jl");
const ostrinFile = (kernel) => join(here, "ostrin", `${kernel}.ostrin`);
const exeDir = mkdtempSync(join(tmpdir(), "linalg-bin-")); // compiled executables stay out of the repository

// ---------------------------------------------------------------- helpers

const median = (xs) => {
  const s = [...xs].sort((a, b) => a - b);
  const m = Math.floor(s.length / 2);
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
};
const quartile = (xs, q) => {
  const s = [...xs].sort((a, b) => a - b);
  const pos = (s.length - 1) * q;
  const lo = Math.floor(pos);
  const hi = Math.ceil(pos);
  return s[lo] + (s[hi] - s[lo]) * (pos - lo);
};
const summary = (xs) => ({ n: xs.length, median: median(xs), iqr: quartile(xs, 0.75) - quartile(xs, 0.25), min: Math.min(...xs), max: Math.max(...xs) });
const sha256 = (path) => createHash("sha256").update(readFileSync(path)).digest("hex");
const rotate = (xs, k) => xs.map((_, i) => xs[(i + k) % xs.length]);

function timed(command, argv) {
  const started = process.hrtime.bigint();
  const r = spawnSync(command, argv, { encoding: "utf8", env, timeout: TIMEOUT, maxBuffer: 256 * 1024 * 1024 });
  return { ms: Number(process.hrtime.bigint() - started) / 1e6, status: r.status, signal: r.signal, stdout: r.stdout ?? "", stderr: r.stderr ?? "" };
}

function windowsOnly(script, fallback) {
  if (platform() !== "win32") return fallback;
  const r = spawnSync("powershell", ["-NoProfile", "-Command", script], { encoding: "utf8", timeout: 60_000 });
  return (r.stdout || "").trim() || fallback;
}

const processorLoad = () => {
  const value = Number(windowsOnly("(Get-CimInstance Win32_Processor | Measure-Object -Property LoadPercentage -Average).Average", ""));
  return Number.isFinite(value) ? value : "unavailable";
};

function firstLine(command, argv) {
  const r = spawnSync(command, argv, { encoding: "utf8", env, timeout: 120_000 });
  return (r.stdout || r.stderr || "").trim().split(/\r?\n/)[0] || "unavailable";
}

function versions() {
  const numpy = spawnSync("python", ["-c", "import numpy,sys;print(numpy.__version__)"], { encoding: "utf8", env }).stdout.trim() || "unavailable";
  const blas = spawnSync("julia", ["-e", "using LinearAlgebra; print(BLAS.get_config())"], { encoding: "utf8", env }).stdout.trim() || "unavailable";
  return {
    ostrinc: firstLine(ostrinc, ["--version"]),
    python: firstLine("python", ["--version"]),
    numpy: numpy,
    julia: firstLine("julia", ["--version"]),
    juliaBlas: blas,
    node: process.version,
    ostrincSha256: existsSync(ostrinc) ? sha256(ostrinc) : "unavailable",
  };
}

function hardware() {
  const info = { os: `${platform()} ${release()} ${arch()}`, cpuModel: cpus()[0]?.model ?? "unknown", logicalCores: cpus().length, memoryGiB: Math.round((totalmem() / 2 ** 30) * 10) / 10 };
  if (platform() === "win32") {
    const [name, cores] = windowsOnly("$p = Get-CimInstance Win32_Processor | Select-Object -First 1; \"$($p.Name)|$($p.NumberOfCores)\"", "").split("|");
    if (name) info.cpuModel = name.trim();
    if (cores) info.physicalCores = Number(cores);
  }
  return info;
}

// ---------------------------------------------------------------- implementations

// Each implementation: how to run it for (kernel, n, k), and how it is measured.
const IMPLEMENTATIONS = [
  { id: "ostrin-native", label: "Ostrin native", kind: "external", kernels: ["matmul", "lu_solve", "norms", "solve_checked"], kBig: KBIG_NATIVE, reps: REPS },
  { id: "ostrin-interp", label: "Ostrin interpreter", kind: "external", kernels: ["matmul", "lu_solve", "norms", "solve_checked"], kBig: KBIG_INTERP, reps: INTERP_REPS },
  { id: "python-pure", label: "CPython 3.12 pure", kind: "internal", kernels: ["matmul", "lu_solve", "norms"], command: "python", argv: (k, n, kk) => [pyScript, k, "pure", String(n), String(kk)] },
  { id: "python-numpy", label: "CPython 3.12 + NumPy", kind: "internal", kernels: ["matmul", "lu_solve", "norms"], command: "python", argv: (k, n, kk) => [pyScript, k, "numpy", String(n), String(kk)] },
  { id: "julia-pure", label: "Julia 1.13 pure", kind: "internal", kernels: ["matmul", "lu_solve", "norms"], command: "julia", argv: (k, n, kk) => [jlScript, k, "pure", String(n), String(kk)] },
  { id: "julia-lib", label: "Julia 1.13 + OpenBLAS/LAPACK", kind: "internal", kernels: ["matmul", "lu_solve", "norms"], command: "julia", argv: (k, n, kk) => [jlScript, k, "lib", String(n), String(kk)] },
];

// Parses the REP_MS line of an internal run.
function repMs(stdout) {
  const line = stdout.split(/\r?\n/).find((text) => text.startsWith("REP_MS "));
  if (line === undefined) return null;
  const values = line.slice(7).trim().split(/\s+/).map(Number);
  return values.every(Number.isFinite) && values.length > 0 ? values : null;
}

// One external sample: K = 1 and K = kBig in separate processes; the slope is the time per repetition.
function externalSample(run, kernel, n, kBig) {
  const a = run(n, 1);
  const b = run(n, kBig);
  for (const r of [a, b]) {
    if (r.status !== 0) return { error: `exit ${r.status} ${r.signal ?? ""}: ${r.stderr.slice(0, 200)}` };
    const check = checkRun(kernel, n, r.stdout);
    if (!check.ok) return { error: check.message };
  }
  // A slope that is not positive means the start-up noise is larger than the kernel time: the sample is not a measurement.
  const slope = (b.ms - a.ms) / (kBig - 1);
  if (!(slope > 0)) return { error: `non-positive slope ${slope.toFixed(3)} ms: start-up noise dominates at this size` };
  return { slope, t1: a.ms, tk: b.ms };
}

// ---------------------------------------------------------------- measurement

const results = [];
const hw = hardware();
const ver = versions();
const commit = firstLine("git", ["-C", root, "rev-parse", "--short", "HEAD"]);

// Ostrin native binaries: compiled once per kernel, outside every timed run.
mkdirSync(exeDir, { recursive: true });
const exes = {};
const compileMs = {};
for (const kernel of ["matmul", "lu_solve", "norms", "solve_checked"]) {
  const exe = join(exeDir, `${kernel}${platform() === "win32" ? ".exe" : ""}`);
  const times = [];
  let failure = null;
  for (let i = 0; i < COMPILE_SAMPLES; i += 1) {
    const c = timed(ostrinc, ["--compile", "--out", exe, ostrinFile(kernel)]);
    if (c.status !== 0) { failure = c.stderr.slice(0, 300); break; }
    times.push(c.ms);
  }
  exes[kernel] = failure ? null : exe;
  compileMs[kernel] = failure ? { failure } : summary(times);
}

for (const size of SIZES) {
  for (const kernel of ["matmul", "lu_solve", "norms", "solve_checked"]) {
    const impls = IMPLEMENTATIONS.filter((impl) => impl.kernels.includes(kernel));
    const cells = Object.fromEntries(impls.map((impl) => [impl.id, { label: impl.label, samples: [], warm: [], cold: [], verified: 0, failure: null, load: {} }]));

    // Warm-up: one untimed run per external implementation (process start, JIT, library initialisation).
    for (const impl of impls.filter((i) => i.kind === "external")) {
      if (!exes[kernel] && impl.id === "ostrin-native") continue;
      const run = impl.id === "ostrin-native" ? (nn, kk) => timed(exes[kernel], [String(nn), String(kk)]) : (nn, kk) => timed(ostrinc, ["--run", ostrinFile(kernel), "--", String(nn), String(kk)]);
      externalSample(run, kernel, size, impl.kBig);
    }

    for (let rep = 0; rep < REPS; rep += 1) {
      const order = rotate(impls, rep % impls.length);
      for (const impl of order) {
        const cell = cells[impl.id];
        if (impl.id === "ostrin-native" && !exes[kernel]) { cell.failure = `compile failed: ${compileMs[kernel].failure}`; continue; }
        if (cell.failure) continue;
        if (impl.id === "ostrin-interp" && rep >= impl.reps) continue;
        const loadBefore = processorLoad();
        if (impl.kind === "external") {
          const run = impl.id === "ostrin-native" ? (nn, kk) => timed(exes[kernel], [String(nn), String(kk)]) : (nn, kk) => timed(ostrinc, ["--run", ostrinFile(kernel), "--", String(nn), String(kk)]);
          const s = externalSample(run, kernel, size, impl.kBig);
          if (s.error) { cell.failure = s.error; continue; }
          cell.samples.push(s.slope);
          cell.t1 = [...(cell.t1 ?? []), s.t1];
          cell.verified += 2;
        } else {
          const r = timed(impl.command, impl.argv(kernel, size, INTERNAL_K));
          if (r.status !== 0) { cell.failure = `exit ${r.status}: ${r.stderr.slice(0, 200)}`; continue; }
          const check = checkRun(kernel, size, r.stdout);
          if (!check.ok) { cell.failure = check.message; continue; }
          const times = repMs(r.stdout);
          if (times === null) { cell.failure = "no REP_MS line"; continue; }
          cell.cold.push(times[0]);
          cell.warm.push(...times.slice(1));
          cell.samples.push(...times.slice(1));
          cell.verified += 1;
        }
        cell.load[`rep${rep}`] = { before: loadBefore, after: processorLoad() };
      }
    }

    for (const impl of impls) {
      const cell = cells[impl.id];
      const row = { size, kernel, implementation: impl.id, label: impl.label, kind: impl.kind };
      if (cell.failure) Object.assign(row, { incomplete: true, message: cell.failure });
      else if (impl.kind === "external") {
        const s = summary(cell.samples);
        Object.assign(row, { incomplete: false, perRunMs: s.median, iqr: s.iqr, min: s.min, max: s.max, samples: cell.samples, startupEstimateMs: median(cell.t1) - s.median, method: `external slope, K=1 and K=${impl.kBig}, ${impl.reps} repetitions` });
      } else {
        const warm = summary(cell.warm);
        Object.assign(row, { incomplete: false, coldMs: summary(cell.cold), warmMs: warm, perRunMs: warm.median, iqr: warm.iqr, min: warm.min, max: warm.max, samples: cell.warm, method: `internal: ${REPS} processes of ${INTERNAL_K} repetitions; cold = repetition 1, warm = repetitions 2..${INTERNAL_K}` });
      }
      row.load = cell.load;
      results.push(row);
      const tag = row.incomplete ? `INCOMPLETE (${row.message.slice(0, 120)})` : `${row.perRunMs.toFixed(3)} ms (IQR ${row.iqr.toFixed(3)})`;
      console.log(`n=${size} ${kernel} ${impl.label}: ${tag}`);
    }
  }
}

// ---------------------------------------------------------------- output

const stamp = new Date().toISOString().replace(/[:.]/g, "-");
const report = { generated: new Date().toISOString(), commit, hardware: hw, versions: ver, sizes: SIZES, reps: REPS, interpReps: INTERP_REPS, kBigNative: KBIG_NATIVE, kBigInterp: KBIG_INTERP, internalK: INTERNAL_K, compileMs, results };
mkdirSync(outDir, { recursive: true });
const jsonPath = join(outDir, `linalg-${stamp}.json`);
writeFileSync(jsonPath, JSON.stringify(report, null, 2) + "\n");
console.log(`wrote ${jsonPath}`);
