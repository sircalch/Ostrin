// Scientific comparison for Benchmark V2: pinned Ostrin interpreter builds, the Ostrin native build, CPython and Julia, on the
// same kernels and with the same reference checks as run-v2.mjs. It extends that benchmark; it does not replace it.
//
// Usage: node benchmarks/v2/compare-scientific.mjs --native <ostrinc.exe> --native-commit <sha>
//          [--interp label=<ostrinc.exe>@<sha> ...] [--kernels a,b,...] [--reps n] [--out dir]
//
// Methods (each one is labelled in the report; none of them is an exact measure of the kernel's pure time):
//  - Ostrin interpreter builds: EXTERNAL SLOPE, alternating. In every repetition each build runs K = 1 and K = KBIG, and the
//    order of the builds is rotated. The per-run time is the median of the paired slopes (T(KBIG) - T(1)) / (KBIG - 1). Two
//    builds differ only when their paired-slope ranges do not overlap. Each build is warmed up once before the timed runs.
//  - Ostrin native: compiled once per kernel (the compile time is recorded separately and never falls inside a timed run),
//    then EXTERNAL SLOPE with K = 1 and K = KBIG_NATIVE, as in run-v2.mjs.
//  - CPython and Julia: INTERNAL. Each process repeats the kernel INTERNAL_K times and times every repetition with its own
//    clock. The first repetition of each process is the cold figure (for Julia it includes first-call compilation); the
//    remaining repetitions are the warm figure. The wall time of a K = 1 process is also recorded, so the process start
//    cost is visible; it is not subtracted, because the linear model does not hold for these runtimes (see the report).
//  - Every RESULT line is checked against kernel-reference.mjs before its time is used. A run that fails the check, or exits
//    with an error, makes its cell INCOMPLETE and no figure is reported for it.
//  - Load: the average processor load (Win32_Processor.LoadPercentage) and the processes with the largest cumulative CPU time
//    are recorded before and after every cell. They are conditions of the measurement, not controlled quantities.
// Threads: OMP_NUM_THREADS and OPENBLAS_NUM_THREADS are 1 for every child process; Julia runs with --threads=1; CPython is
// single-threaded for these kernels and the Ostrin native build starts no threads.

import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync, mkdtempSync, rmSync } from "node:fs";
import { arch, cpus, platform, release, tmpdir, totalmem } from "node:os";
import { basename, dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { reference } from "./kernel-reference.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..", "..");
const args = process.argv.slice(2);
const flags = (name) => args.flatMap((a, i) => (a === name ? [args[i + 1]] : []));
const one = (name, fallback) => (flags(name).length ? flags(name)[0] : fallback);

const KERNELS = one("--kernels", "simpson,bisect,matmul,regression,horner,sieve,collatz").split(",");
const nativeBinary = one("--native", join(root, "compiler", "target", "release", platform() === "win32" ? "ostrinc.exe" : "ostrinc"));
const nativeCommit = one("--native-commit", "unknown");
const interps = flags("--interp").map((spec) => {
  const m = spec.match(/^([^=]+)=(.+)@([0-9a-f]{7,40})$/);
  if (!m) throw new Error(`bad --interp ${spec}: expected label=path@commit`);
  return { label: m[1], path: m[2], commit: m[3] };
});
if (interps.length < 1) throw new Error("at least one --interp build is required");
const repsOverride = one("--reps", null);
const REPS = { native: repsOverride ? Number(repsOverride) : 7, interp: repsOverride ? Number(repsOverride) : 3, internal: repsOverride ? Number(repsOverride) : 5 };
const KBIG_NATIVE = 10;
const KBIG_INTERP = 2;
const INTERNAL_K = 5;
const COMPILE_SAMPLES = 3;
const outDir = one("--out", join(here, "results"));
const env = { ...process.env, OMP_NUM_THREADS: "1", OPENBLAS_NUM_THREADS: "1" };
const programFile = (kernel) => join(root, "benchmarks", "v2", "ostrin", `${kernel}.ostrin`);
const pythonScript = join(root, "benchmarks", "v2", "python", "run_v2.py");
const juliaScript = join(root, "benchmarks", "v2", "julia", "run_v2.jl");
const TIMEOUT = 3_600_000;

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
const stats = (xs) => ({ n: xs.length, median: median(xs), iqr: quartile(xs, 0.75) - quartile(xs, 0.25), min: Math.min(...xs), max: Math.max(...xs) });
const sha256 = (path) => createHash("sha256").update(readFileSync(path)).digest("hex");

function timed(command, argv) {
  const started = process.hrtime.bigint();
  const r = spawnSync(command, argv, { encoding: "utf8", env, timeout: TIMEOUT, maxBuffer: 256 * 1024 * 1024 });
  return { ms: Number(process.hrtime.bigint() - started) / 1e6, status: r.status, signal: r.signal, stdout: r.stdout ?? "", stderr: r.stderr ?? "" };
}

function resultValues(stdout, kernel) {
  const line = stdout.split(/\r?\n/).find((text) => text.startsWith(`RESULT ${kernel} `));
  if (line === undefined) return null;
  const tokens = line.slice(`RESULT ${kernel} `.length).trim().split(/\s+/).filter((t) => /^[-+]?(\d|\.\d)/.test(t)).map(Number);
  return tokens.length > 0 && tokens.every(Number.isFinite) ? tokens : null;
}

function verify(kernel, stdout) {
  const values = resultValues(stdout, kernel);
  if (values === null) return { ok: false, message: "no RESULT line" };
  const expected = reference(kernel);
  if (values.length !== expected.values.length) return { ok: false, message: `expected ${expected.values.length} value(s), got ${values.length}` };
  for (let i = 0; i < values.length; i += 1) {
    const error = Math.abs(values[i] - expected.values[i]);
    if (!(error <= expected.tolerance * Math.max(1, Math.abs(expected.values[i])))) {
      return { ok: false, message: `value ${i}: ${values[i]} differs from the reference ${expected.values[i]}` };
    }
  }
  return { ok: true };
}

function windowsOnly(script, fallback) {
  if (platform() !== "win32") return fallback;
  const r = spawnSync("powershell", ["-NoProfile", "-Command", script], { encoding: "utf8", timeout: 60_000 });
  return (r.stdout || "").trim() || fallback;
}

// Average processor load over all logical processors, a percentage at this instant (not an average over time).
const processorLoad = () => {
  const text = windowsOnly("(Get-CimInstance Win32_Processor | Measure-Object -Property LoadPercentage -Average).Average", "");
  const value = Number(text);
  return Number.isFinite(value) ? value : "unavailable";
};

function topProcesses() {
  const text = windowsOnly("Get-Process | Sort-Object CPU -Descending | Select-Object -First 6 Name,@{n='CPU_s';e={[math]::Round($_.CPU,1)}} | ConvertTo-Json -Compress", "[]");
  try {
    const rows = JSON.parse(text);
    return (Array.isArray(rows) ? rows : [rows]).map((row) => ({ name: row.Name, cumulativeCpuSeconds: row.CPU_s }));
  } catch {
    return "unavailable";
  }
}

function firstLine(command, argv) {
  const r = spawnSync(command, argv, { encoding: "utf8", timeout: 60_000 });
  return ((r.stdout || r.stderr || "").trim().split(/\r?\n/)[0]) || "unavailable";
}

function hardware() {
  const info = { os: `${platform()} ${release()} ${arch()}`, cpuModel: cpus()[0]?.model ?? "unknown", logicalCores: cpus().length, memoryGiB: Math.round((totalmem() / 2 ** 30) * 10) / 10 };
  if (platform() === "win32") {
    const text = windowsOnly("$p = Get-CimInstance Win32_Processor | Select-Object -First 1; \"$($p.Name)|$($p.NumberOfCores)\"", "");
    const [name, cores] = text.split("|");
    if (name) info.cpuModel = name.trim();
    if (cores) info.physicalCores = Number(cores);
  }
  return info;
}

// Externally measured Ostrin cell (interpreter builds alternate inside measureInterpreters; the native build uses this).
function slopeFor(runFor, kBig, reps, kernel) {
  runFor(1);
  runFor(kBig);
  const t1 = [];
  const tk = [];
  const slopes = [];
  for (let i = 0; i < reps; i += 1) {
    const a = runFor(1);
    const b = runFor(kBig);
    for (const r of [a, b]) {
      if (r.status !== 0) return { incomplete: true, message: `exit ${r.status} ${r.signal ?? ""}: ${r.stderr.slice(0, 200)}` };
    }
    for (const r of [a, b]) {
      const check = verify(kernel, r.stdout);
      if (!check.ok) return { incomplete: true, message: check.message };
    }
    t1.push(a.ms);
    tk.push(b.ms);
    slopes.push((b.ms - a.ms) / (kBig - 1));
  }
  return { incomplete: false, slopeMedian: median(slopes), slopeIqr: quartile(slopes, 0.75) - quartile(slopes, 0.25), slopeMin: Math.min(...slopes), slopeMax: Math.max(...slopes), t1: stats(t1), tk: stats(tk), slopes };
}

// Internal figures for CPython and Julia: REPS processes with INTERNAL_K repetitions, and REPS processes with one repetition.
function internalFor(command, argsFor, kernel) {
  const cold = [];
  const warm = [];
  const wallOne = [];
  for (let i = 0; i < REPS.internal; i += 1) {
    const r = timed(command, argsFor(kernel, INTERNAL_K));
    if (r.status !== 0) return { incomplete: true, message: `exit ${r.status}: ${r.stderr.slice(0, 200)}` };
    const check = verify(kernel, r.stdout);
    if (!check.ok) return { incomplete: true, message: check.message };
    const line = r.stdout.split(/\r?\n/).find((x) => x.startsWith("REP_MS "));
    if (!line) return { incomplete: true, message: "no REP_MS line" };
    const ts = line.slice("REP_MS ".length).trim().split(/\s+/).map(Number);
    cold.push(ts[0]);
    warm.push(...ts.slice(1));
  }
  for (let i = 0; i < REPS.internal; i += 1) {
    const r = timed(command, argsFor(kernel, 1));
    if (r.status !== 0) return { incomplete: true, message: `K=1 exit ${r.status}: ${r.stderr.slice(0, 200)}` };
    const check = verify(kernel, r.stdout);
    if (!check.ok) return { incomplete: true, message: `K=1: ${check.message}` };
    wallOne.push(r.ms);
  }
  return { incomplete: false, cold: stats(cold), warm: stats(warm), wallOneProcess: stats(wallOne), coldSamples: cold, warmSamples: warm };
}

const pyArgs = (kernel, k) => [pythonScript, kernel, String(k)];
const juliaArgs = (kernel, k) => ["--threads=1", "--startup-file=no", juliaScript, kernel, String(k)];

// ---------------------------------------------------------------- run

const started = new Date();
const work = mkdtempSync(join(tmpdir(), "compare-sci-"));
const builds = interps.map((b) => ({ ...b, sha256: sha256(b.path), version: firstLine(b.path, ["--version"]) }));
const nativeInfo = { path: nativeBinary, commit: nativeCommit, sha256: sha256(nativeBinary), version: firstLine(nativeBinary, ["--version"]) };
const versions = { node: process.version, python: firstLine("python", ["--version"]), julia: firstLine("julia", ["--version"]) };
const system = { hardware: hardware(), threads: { OMP_NUM_THREADS: 1, OPENBLAS_NUM_THREADS: 1, julia: 1, python: 1, ostrinNative: "single-threaded" } };
const startLoad = { processorPercent: processorLoad(), topProcesses: topProcesses() };
const cells = [];

for (const kernel of KERNELS) {
  // Ostrin interpreter builds: alternating, rotated, one warm-up per build.
  const interpResults = Object.fromEntries(builds.map((b) => [b.label, { correct: true, slopes: [], t1: [], tk: [], message: null, load: {} }]));
  for (const b of builds) interpResults[b.label].load.before = processorLoad();
  for (const b of builds) {
    const warm = timed(b.path, ["--run", programFile(kernel), "--", "1"]);
    if (warm.status !== 0 || !verify(kernel, warm.stdout).ok) {
      interpResults[b.label].correct = false;
      interpResults[b.label].message = warm.status !== 0 ? `exit ${warm.status}` : verify(kernel, warm.stdout).message;
    }
  }
  for (let rep = 0; rep < REPS.interp; rep += 1) {
    const order = builds.map((_, i) => builds[(i + rep) % builds.length]);
    for (const b of order) {
      const entry = interpResults[b.label];
      if (!entry.correct) continue;
      const a = timed(b.path, ["--run", programFile(kernel), "--", "1"]);
      const k = timed(b.path, ["--run", programFile(kernel), "--", String(KBIG_INTERP)]);
      for (const [r, n] of [[a, 1], [k, KBIG_INTERP]]) {
        if (r.status !== 0) {
          entry.correct = false;
          entry.message = `exit ${r.status} at K=${n}`;
        } else if (!verify(kernel, r.stdout).ok) {
          entry.correct = false;
          entry.message = verify(kernel, r.stdout).message;
        }
      }
      if (entry.correct) {
        entry.t1.push(a.ms);
        entry.tk.push(k.ms);
        entry.slopes.push((k.ms - a.ms) / (KBIG_INTERP - 1));
      }
    }
  }
  for (const b of builds) {
    const e = interpResults[b.label];
    e.load.after = processorLoad();
    const cell = { kernel, implementation: "Ostrin interpreter", label: b.label, commit: b.commit, sha256: b.sha256, method: `external slope, alternating, K=1 and K=${KBIG_INTERP}, ${REPS.interp} repetitions` };
    if (!e.correct || e.slopes.length === 0) {
      Object.assign(cell, { incomplete: true, message: e.message ?? "no repetition completed" });
    } else {
      Object.assign(cell, {
        incomplete: false,
        perRunMs: median(e.slopes),
        rangeMs: [Math.min(...e.slopes), Math.max(...e.slopes)],
        uncertaintyMs: quartile(e.slopes, 0.75) - quartile(e.slopes, 0.25),
        samples: e.slopes,
      });
    }
    cell.load = e.load;
    cells.push(cell);
  }

  // Ostrin native (HEAD build): compile first, outside every timed run.
  {
    const exe = join(work, `${kernel}-native.exe`);
    const compileMs = [];
    let compileFailure = null;
    const loadBefore = processorLoad();
    for (let i = 0; i < COMPILE_SAMPLES; i += 1) {
      const c = timed(nativeBinary, ["--compile", "--out", exe, programFile(kernel)]);
      if (c.status !== 0) {
        compileFailure = c.stderr.slice(0, 300);
        break;
      }
      compileMs.push(c.ms);
    }
    const cell = { kernel, implementation: "Ostrin native", label: "HEAD native", commit: nativeCommit, sha256: nativeInfo.sha256, method: `compile ${COMPILE_SAMPLES}x outside the timed runs; external slope, K=1 and K=${KBIG_NATIVE}, ${REPS.native} repetitions`, compileMs: compileFailure ? null : stats(compileMs) };
    if (compileFailure) {
      Object.assign(cell, { incomplete: true, message: `compile failed: ${compileFailure}` });
    } else {
      const m = slopeFor((k) => timed(exe, [String(k)]), KBIG_NATIVE, REPS.native, kernel);
      if (m.incomplete) Object.assign(cell, m);
      else Object.assign(cell, { incomplete: false, perRunMs: m.slopeMedian, rangeMs: [m.slopeMin, m.slopeMax], uncertaintyMs: m.slopeIqr, samples: m.slopes, startupEstimateMs: m.t1.median - m.slopeMedian });
    }
    cell.load = { before: loadBefore, after: processorLoad() };
    cells.push(cell);
  }

  // CPython and Julia: internal figures.
  for (const [impl, command, argsFor, method] of [
    ["CPython 3.12", "python", pyArgs, "internal: each process repeats the kernel 5 times and times each repetition; cold = first repetition, warm = the others"],
    ["Julia 1.13 (1 thread)", "julia", juliaArgs, "internal: as CPython; cold includes the first-call compilation of the kernel"],
  ]) {
    const loadBefore = processorLoad();
    const m = internalFor(command, argsFor, kernel);
    const cell = { kernel, implementation: impl, label: impl, method, load: { before: loadBefore, after: processorLoad() } };
    if (m.incomplete) Object.assign(cell, m);
    else Object.assign(cell, { incomplete: false, cold: m.cold, warm: m.warm, wallOneProcess: m.wallOneProcess, perRunMs: m.warm.median, rangeMs: [m.warm.min, m.warm.max], uncertaintyMs: m.warm.iqr, samples: m.warmSamples });
    cells.push(cell);
  }
  console.log(`done ${kernel}`);
}

const endLoad = { processorPercent: processorLoad(), topProcesses: topProcesses() };
rmSync(work, { recursive: true, force: true });
const finished = new Date();
const payload = {
  schema: "ostrin-benchmark-v2-scientific-compare/1",
  started: started.toISOString(),
  finished: finished.toISOString(),
  kernels: KERNELS,
  repetitions: REPS,
  kBig: { native: KBIG_NATIVE, interpreter: KBIG_INTERP, internalPerProcess: INTERNAL_K },
  builds,
  native: nativeInfo,
  versions,
  system,
  load: { start: startLoad, end: endLoad },
  cells,
};
mkdirSync(outDir, { recursive: true });
const stamp = started.toISOString().replace(/[:.]/g, "-");
const jsonPath = join(outDir, `compare-scientific-${stamp}.json`);
writeFileSync(jsonPath, JSON.stringify(payload, null, 2) + "\n", "utf8");
console.log(`wrote ${jsonPath}`);
console.log(basename(jsonPath));
