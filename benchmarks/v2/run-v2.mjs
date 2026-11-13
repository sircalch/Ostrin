// Scientific Benchmark V2: five kernels, equal problem sizes, verified results, and two independent estimates of the
// time per kernel run.
//
// Usage: node benchmarks/v2/run-v2.mjs [--interp <ostrinc binary>] [--label <name>] [--only <impl>]
//   impl is one of: native, interp, python, julia (default: all)
//
// Methods (none of them is an exact measure of the kernel's pure time):
//  1. EXTERNAL SLOPE. Every kernel program takes K, the number of repetitions of its kernel in one process. Each
//     implementation is run with K = 1 and with K = K_big, alternately, REPS times each. The time per kernel run is
//     (median T(K_big) − median T(1)) / (K_big − 1). Process start and the front end are the same in both runs, so
//     they cancel; the uncertainty is the sum of the spreads of both sets divided by (K_big − 1). The intercept
//     median T(1) − slope is an estimate of the start-up cost.
//  2. INTERNAL (Python and Julia only). The program times each repetition with its own clock, so the times exclude
//     process start. Julia's first repetition includes first-call compilation; the warm figure is the median of the
//     remaining repetitions over all processes. Ostrin has no clock inside a program, so it has no internal figure.
//  Compile time (Ostrin native) is measured separately, with `ostrinc --compile`.
//
// Verification: every run's printed result is compared with a reference computed here, before any timing is used.
// Threads: Julia runs with --threads=1; the environment variables OMP_NUM_THREADS and OPENBLAS_NUM_THREADS are set to
// 1 for every child process. CPython is single-threaded for these kernels, and the Ostrin native build does not start
// threads unless --native-threads is given, which is not used here.

import { spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, readFileSync } from "node:fs";
import { tmpdir, cpus, totalmem, release, arch } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..", "..");
const args = process.argv.slice(2);
const flag = (name, fallback) => {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : fallback;
};
const compiler = flag("--interp", join(root, "compiler", "target", "release", process.platform === "win32" ? "ostrinc.exe" : "ostrinc"));
const label = flag("--label", "current");
const only = flag("--only", "all");
const KERNELS = ["simpson", "bisect", "matmul", "regression", "horner"];
const CONFIG = {
  native: { kBig: 10, reps: 7 },
  interp: { kBig: 2, reps: 3 },
  python: { kBig: 5, reps: 5 },
  julia: { kBig: 5, reps: 5 },
};
const TIMEOUT = 3_600_000;
const env = { ...process.env, OMP_NUM_THREADS: "1", OPENBLAS_NUM_THREADS: "1" };

// Independent references (as in benchmarks/run.mjs; the matrix size is 300).
function reference(kernel) {
  switch (kernel) {
    case "simpson":
      return { values: [2.0], tolerance: 1e-12 };
    case "bisect": {
      let x = 2.0;
      for (let i = 0; i < 60; i += 1) x -= (x * x * x - 2 * x - 5) / (3 * x * x - 2);
      return { values: [x], tolerance: 1e-12 };
    }
    case "matmul": {
      const n = 300;
      const a = new Array(n * n);
      const b = new Array(n * n);
      for (let i = 0; i < n; i += 1) {
        for (let j = 0; j < n; j += 1) {
          a[i * n + j] = (i * j + i + 2 * j) % 7;
          b[i * n + j] = (3 * i + j * j) % 5;
        }
      }
      const c = new Array(n * n).fill(0);
      for (let i = 0; i < n; i += 1) {
        for (let j = 0; j < n; j += 1) {
          let acc = 0;
          for (let k = 0; k < n; k += 1) acc += a[i * n + k] * b[k * n + j];
          c[i * n + j] = acc;
        }
      }
      let trace = 0;
      let total = 0;
      for (let i = 0; i < n; i += 1) {
        trace += c[i * n + i];
        for (let j = 0; j < n; j += 1) total += c[i * n + j];
      }
      return { values: [trace, total], tolerance: 0 };
    }
    case "regression":
      return { values: [1, 2, -3], tolerance: 1e-9 };
    case "horner": {
      const degree = 50;
      const coefficients = [];
      for (let k = 0; k <= degree; k += 1) coefficients.push((k % 5) - 2);
      let total = 0;
      for (let i = 0; i < 2000000; i += 1) {
        const x = (i % 1000) / 1000;
        let value = 0;
        for (let k = 0; k <= degree; k += 1) value = value * x + coefficients[degree - k];
        total += value;
      }
      return { values: [total], tolerance: 1e-12 };
    }
    default:
      throw new Error(kernel);
  }
}

function parseNumbers(stdout, kernel) {
  const line = stdout.split("\n").find((text) => text.startsWith(`RESULT ${kernel} `));
  if (line === undefined) return null;
  const numbers = line
    .slice(`RESULT ${kernel} `.length)
    .trim()
    .split(/\s+/)
    .filter((token) => /^[-+]?(\d|\.\d)/.test(token))
    .map(Number);
  return numbers.length > 0 && numbers.every(Number.isFinite) ? numbers : null;
}

function check(kernel, stdout) {
  const values = parseNumbers(stdout, kernel);
  if (values === null) return { ok: false, message: "no RESULT line" };
  const expected = reference(kernel);
  if (values.length !== expected.values.length) return { ok: false, message: `expected ${expected.values.length} values, got ${values.length}` };
  for (let i = 0; i < values.length; i += 1) {
    const error = Math.abs(values[i] - expected.values[i]);
    if (!(error <= expected.tolerance * Math.max(1, Math.abs(expected.values[i])))) {
      return { ok: false, message: `value ${i}: ${values[i]} differs from ${expected.values[i]}` };
    }
  }
  return { ok: true, values };
}

function timed(command, argv, options = {}) {
  const started = process.hrtime.bigint();
  const r = spawnSync(command, argv, { encoding: "utf8", timeout: TIMEOUT, maxBuffer: 64 * 1024 * 1024, env, ...options });
  return { ms: Number(process.hrtime.bigint() - started) / 1e6, status: r.status, signal: r.signal, stdout: r.stdout ?? "", stderr: r.stderr ?? "" };
}

const median = (xs) => {
  const s = [...xs].sort((a, b) => a - b);
  const m = Math.floor(s.length / 2);
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
};
const spread = (xs) => Math.max(...xs) - Math.min(...xs);
const stats = (xs) => ({ n: xs.length, median: median(xs), min: Math.min(...xs), max: Math.max(...xs), mad: median(xs.map((x) => Math.abs(x - median(xs)))) });

function loadSnapshot() {
  if (process.platform !== "win32") return [];
  const r = spawnSync("powershell", ["-NoProfile", "-Command", "Get-Process | Sort-Object CPU -Descending | Select-Object -First 6 Name,CPU | ConvertTo-Json -Compress"], { encoding: "utf8" });
  try {
    const rows = JSON.parse(r.stdout || "[]");
    return (Array.isArray(rows) ? rows : [rows]).map((row) => ({ name: row.Name, cpuSeconds: Math.round(row.CPU) }));
  } catch {
    return [];
  }
}

function versions() {
  const first = (command, argv) => {
    const r = spawnSync(command, argv, { encoding: "utf8" });
    return (r.stdout || r.stderr || "").trim().split("\n")[0] || "unavailable";
  };
  return { node: process.version, ostrinc: first(compiler, ["--version"]), python: first("python", ["--version"]), julia: first("julia", ["--version"]) };
}

function hardware() {
  const info = { os: `${process.platform} ${release()} ${arch()}`, cpuModel: cpus()[0]?.model ?? "unknown", logicalCores: cpus().length, memoryGiB: Math.round(totalmem() / 2 ** 30) };
  if (process.platform === "win32") {
    const r = spawnSync("powershell", ["-NoProfile", "-Command", "(Get-CimInstance Win32_Processor | Select-Object -First 1).Name + '|' + (Get-CimInstance Win32_Processor | Select-Object -First 1).NumberOfCores"], { encoding: "utf8" });
    const [name, cores] = (r.stdout || "").trim().split("|");
    if (name) info.cpuModel = name.trim();
    if (cores) info.physicalCores = Number(cores);
  }
  return info;
}

const work = mkdtempSync(join(tmpdir(), "v2-"));
const report = {
  generated: new Date().toISOString(),
  label,
  compiler,
  config: CONFIG,
  hardware: hardware(),
  versions: versions(),
  threads: { julia: 1, python: 1, ostrinNative: "single-threaded unless --native-threads", envOMP_NUM_THREADS: 1, envOPENBLAS_NUM_THREADS: 1 },
  loadBefore: loadSnapshot(),
  cells: [],
};

function programFile(kernel, implementation) {
  // The Ostrin kernels take K as their first argument; the Python and Julia runners take the name and K.
  if (implementation === "ostrin") return join(root, "benchmarks", "v2", "ostrin", `${kernel}.ostrin`);
  return null;
}

function measureSlope(runFor, kBig, reps, kernel, label) {
  // Warm-up with K = 1 for both settings, then REPS alternating pairs.
  runFor(1);
  runFor(kBig);
  const t1 = [];
  const tk = [];
  let outputs = [];
  for (let i = 0; i < reps; i += 1) {
    const a = runFor(1);
    if (a.status !== 0) return { incomplete: true, message: `K=1 run ${i + 1}: exit ${a.status} ${a.signal ?? ""} ${a.stderr.slice(0, 200)}` };
    t1.push(a.ms);
    const b = runFor(kBig);
    if (b.status !== 0) return { incomplete: true, message: `K=${kBig} run ${i + 1}: exit ${b.status} ${b.signal ?? ""} ${b.stderr.slice(0, 200)}` };
    tk.push(b.ms);
    outputs.push(b.stdout, a.stdout);
  }
  const bad = outputs.map((out) => check(kernel, out)).find((c) => !c.ok);
  if (bad) return { incomplete: true, message: `${label}: ${bad.message}` };
  const slope = (median(tk) - median(t1)) / (kBig - 1);
  const uncertainty = (spread(tk) + spread(t1)) / (kBig - 1);
  return {
    incomplete: false,
    perRunMs: slope,
    uncertaintyMs: uncertainty,
    startupEstimateMs: median(t1) - slope,
    t1: stats(t1),
    tk: stats(tk),
  };
}

for (const kernel of KERNELS) {
  if (only !== "all" && !["native", "interp", "python", "julia"].includes(only) && only !== kernel) continue;

  if (only === "all" || only === "native") {
    const exe = join(work, `${kernel}-native.exe`);
    const compileSamples = [];
    let compileFailure = null;
    for (let i = 0; i < 5; i += 1) {
      const c = timed(compiler, ["--compile", "--out", exe, programFile(kernel, "ostrin")]);
      if (c.status !== 0) {
        compileFailure = c.stderr.slice(0, 300);
        break;
      }
      compileSamples.push(c.ms);
    }
    const cell = { implementation: "Ostrin native", kernel, compileMs: compileFailure ? null : stats(compileSamples), compileFailure };
    if (!compileFailure) {
      const { kBig, reps } = CONFIG.native;
      const m = measureSlope((k) => timed(exe, [String(k)]), kBig, reps, kernel, "native");
      Object.assign(cell, m, { kBig, reps });
    }
    report.cells.push(cell);
  }

  if (only === "all" || only === "interp") {
    const { kBig, reps } = CONFIG.interp;
    const m = measureSlope((k) => timed(compiler, ["--run", programFile(kernel, "ostrin"), "--", String(k)]), kBig, reps, kernel, "interpreter");
    report.cells.push({ implementation: `Ostrin interpreter (${label})`, kernel, ...m, kBig, reps });
  }

  if (only === "all" || only === "python") {
    const { kBig, reps } = CONFIG.python;
    const script = join(root, "benchmarks", "v2", "python", "run_v2.py");
    const m = measureSlope((k) => timed("python", [script, kernel, String(k)]), kBig, reps, kernel, "python");
    const cell = { implementation: "Python (CPython 3.12)", kernel, ...m, kBig, reps };
    if (!m.incomplete) {
      // Internal figures: one process per repetition set, each repetition timed inside the process.
      const cold = [];
      const warm = [];
      for (let i = 0; i < reps; i += 1) {
        const r = timed("python", [script, kernel, String(kBig)]);
        const line = r.stdout.split("\n").find((x) => x.startsWith("REP_MS "));
        if (!line) continue;
        const ts = line.slice("REP_MS ".length).trim().split(/\s+/).map(Number);
        cold.push(ts[0]);
        warm.push(...ts.slice(1));
      }
      cell.internal = { cold: stats(cold), warm: stats(warm) };
    }
    report.cells.push(cell);
  }

  if (only === "all" || only === "julia") {
    const { kBig, reps } = CONFIG.julia;
    const script = join(root, "benchmarks", "v2", "julia", "run_v2.jl");
    const m = measureSlope((k) => timed("julia", ["--threads=1", "--startup-file=no", script, kernel, String(k)]), kBig, reps, kernel, "julia");
    const cell = { implementation: "Julia 1.13 (1 thread)", kernel, ...m, kBig, reps };
    if (!m.incomplete) {
      const cold = [];
      const warm = [];
      for (let i = 0; i < reps; i += 1) {
        const r = timed("julia", ["--threads=1", "--startup-file=no", script, kernel, String(kBig)]);
        const line = r.stdout.split("\n").find((x) => x.startsWith("REP_MS "));
        if (!line) continue;
        const ts = line.slice("REP_MS ".length).trim().split(/\s+/).map(Number);
        cold.push(ts[0]);
        warm.push(...ts.slice(1));
      }
      cell.internal = { cold: stats(cold), warm: stats(warm) };
    }
    report.cells.push(cell);
  }
}

report.loadAfter = loadSnapshot();
rmSync(work, { recursive: true, force: true });
const stamp = new Date().toISOString().replace(/[:.]/g, "-");
const outDir = join(here, "results");
mkdirSync(outDir, { recursive: true });
const jsonPath = join(outDir, `v2-${label}-${stamp}.json`);
writeFileSync(jsonPath, JSON.stringify(report, null, 2));
console.log(`wrote ${jsonPath}`);
for (const c of report.cells) {
  const head = `${c.implementation.padEnd(30)} ${c.kernel.padEnd(10)}`;
  if (c.incomplete) console.log(`${head} INCOMPLETE: ${c.message}`);
  else console.log(`${head} per run ${c.perRunMs.toFixed(1)} ± ${c.uncertaintyMs.toFixed(1)} ms  startup~${c.startupEstimateMs.toFixed(1)} ms${c.internal ? `  internal warm ${c.internal.warm.median.toFixed(2)} ms` : ""}`);
}
