// Reproducible benchmark driver: Ostrin (native and interpreter), Python and Julia on the same five kernels.
//
// Usage: node benchmarks/run.mjs            (BENCH_REPS sets the repetitions, default 7; BENCH_INTERP_REPS, default 3)
//
// What is measured, and how:
//   - correctness first: every kernel's printed result is compared with a reference computed here, in JavaScript and
//     from analytic values, never with the other languages. A failing result is reported and its timings are not used.
//   - compile: the wall time of `ostrinc --compile` (Ostrin native only; Python and Julia have no compile step here).
//   - init: the median wall time of an empty program: the process start of each runtime. For Julia this includes
//     starting the runtime; for Python, the interpreter start.
//   - total: the median wall time of one kernel run, including the process start.
//   - run (estimate): total minus init, both medians. Wall-clock timing from outside the process cannot separate
//     the kernel from the process start more finely, and Ostrin has no clock inside a program, so the estimate is
//     reported with the spread of both measurements.
//   - dispersion: minimum, maximum and median absolute deviation (MAD) over the repetitions.
// Hardware and software versions are recorded with each report. Nothing is extrapolated: a number is either measured
// by this script or it is not reported.

import { spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, existsSync, rmSync } from "node:fs";
import { tmpdir, cpus, totalmem, release, arch } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");
const compiler = join(root, "compiler", "target", "release", process.platform === "win32" ? "ostrinc.exe" : "ostrinc");
const REPS = Number(process.env.BENCH_REPS ?? 7);
const INTERP_REPS = Number(process.env.BENCH_INTERP_REPS ?? 3);
const TIMEOUT_MS = Number(process.env.BENCH_TIMEOUT_MS ?? 900000);

const KERNELS = ["simpson", "bisect", "matmul", "regression", "horner"];

// Independent references. Analytic where a closed form exists; otherwise computed here with JavaScript numbers.
function reference(kernel) {
  switch (kernel) {
    case "simpson":
      return { values: [2.0], tolerance: 1e-12 };
    case "bisect": {
      // The root of x^3 - 2x - 5 by Newton iteration, to full double precision.
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
      // All entries are integers far below 2^53, so the products and sums are exact here.
      return { values: [trace, total], tolerance: 0 };
    }
    case "regression":
      // The data are exactly y = 1 + 2x - 3x^2, so the fitted coefficients are 1, 2 and -3.
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
      throw new Error(`no reference for ${kernel}`);
  }
}

function parseResult(stdout, kernel) {
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
  const values = parseResult(stdout, kernel);
  if (values === null) return { ok: false, message: "no RESULT line" };
  const expected = reference(kernel);
  // The matmul line has a "trace" and a "total" label; parseResult keeps the two numbers in order.
  if (values.length !== expected.values.length) return { ok: false, message: `expected ${expected.values.length} values, got ${values.length}`, values };
  for (let i = 0; i < values.length; i += 1) {
    const error = Math.abs(values[i] - expected.values[i]);
    const scale = Math.max(1, Math.abs(expected.values[i]));
    if (!(error <= expected.tolerance * scale)) {
      return { ok: false, message: `value ${i}: ${values[i]} differs from ${expected.values[i]} by ${error}`, values };
    }
  }
  return { ok: true, values };
}

function timed(command, args, options = {}) {
  const started = process.hrtime.bigint();
  const result = spawnSync(command, args, { encoding: "utf8", timeout: TIMEOUT_MS, maxBuffer: 64 * 1024 * 1024, ...options });
  const elapsed = Number(process.hrtime.bigint() - started) / 1e6;
  return { elapsed, status: result.status, stdout: result.stdout ?? "", stderr: result.stderr ?? "", signal: result.signal };
}

function stats(samples) {
  const sorted = [...samples].sort((a, b) => a - b);
  const median = (xs) => {
    const s = [...xs].sort((a, b) => a - b);
    const mid = Math.floor(s.length / 2);
    return s.length % 2 === 1 ? s[mid] : (s[mid - 1] + s[mid]) / 2;
  };
  const m = median(sorted);
  const mad = median(sorted.map((x) => Math.abs(x - m)));
  return { n: sorted.length, median: m, min: sorted[0], max: sorted[sorted.length - 1], mad };
}

function versions() {
  const line = (command, args) => {
    const r = spawnSync(command, args, { encoding: "utf8" });
    return (r.stdout || r.stderr || "").trim().split("\n")[0] || "unavailable";
  };
  return {
    node: process.version,
    ostrinc: line(compiler, ["--version"]),
    python: line("python", ["--version"]),
    julia: line("julia", ["--version"]),
  };
}

function hardware() {
  const info = { os: `${process.platform} ${release()} ${arch()}`, cpuModel: cpus()[0]?.model ?? "unknown", logicalCores: cpus().length, memoryGiB: Math.round(totalmem() / 2 ** 30) };
  if (process.platform === "win32") {
    const r = spawnSync("powershell", ["-NoProfile", "-Command", "(Get-CimInstance Win32_Processor | Select-Object -First 1).Name + '|' + (Get-CimInstance Win32_Processor | Select-Object -First 1).NumberOfCores + '|' + (Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory"], { encoding: "utf8" });
    const [name, cores, memory] = (r.stdout || "").trim().split("|");
    if (name) info.cpuModel = name;
    if (cores) info.physicalCores = Number(cores);
    if (memory) info.memoryGiB = Math.round(Number(memory) / 2 ** 30);
  }
  return info;
}

const work = mkdtempSync(join(tmpdir(), "ostrin-bench-"));
const report = { generated: new Date().toISOString(), repetitions: REPS, interpreterRepetitions: INTERP_REPS, hardware: hardware(), versions: versions(), cells: [] };

function writeProgram(name, text) {
  const file = join(work, `${name}.ostrin`);
  writeFileSync(file, text);
  return file;
}

// Empty programs: the process start of each runtime.
const emptyOstrin = writeProgram("empty", `fn main() -> Void {\n    print("RESULT empty 0")\n}\n`);
const emptyPython = join(work, "empty.py");
writeFileSync(emptyPython, "pass\n");
const pythonKernels = join(root, "benchmarks", "python", "kernels.py");
const juliaKernels = join(root, "benchmarks", "julia", "kernels.jl");

// One warm-up run, then `reps` timed runs. Every timed run must exit cleanly and return the reference values; a run that
// does not (a timeout, a crash, a wrong value) makes the whole measurement incomplete, and it is recorded as such.
function measure(label, runOnce, reps, verify = null) {
  runOnce();
  const samples = [];
  const outputs = [];
  for (let i = 0; i < reps; i += 1) {
    const last = runOnce();
    if (last.status !== 0) {
      return { label, incomplete: true, completedRuns: samples.length, failed: `run ${i + 1} of ${reps}: exit ${last.status} ${last.signal ?? ""}`.trim(), stderr: last.stderr.slice(0, 400) };
    }
    if (verify !== null) {
      const checked = verify(last.stdout);
      if (!checked.ok) return { label, incomplete: true, completedRuns: samples.length, failed: `run ${i + 1} of ${reps}: ${checked.message}` };
    }
    samples.push(last.elapsed);
    outputs.push(last.stdout);
  }
  return { label, incomplete: false, stats: stats(samples), samples: samples.map((x) => Math.round(x * 10) / 10), outputs };
}

function internalTimings(outputs) {
  // The Python and Julia programs print "TIMING cold_ms <x> warm_ms <y>" when run with --timed.
  const cold = [];
  const warm = [];
  for (const text of outputs) {
    const line = text.split("\n").find((x) => x.startsWith("TIMING "));
    if (line === undefined) return null;
    const parts = line.trim().split(/\s+/);
    cold.push(Number(parts[2]));
    warm.push(Number(parts[4]));
  }
  return { cold: stats(cold), warm: stats(warm) };
}

const inits = {};
inits.ostrinNative = null;
{
  // Ostrin native: the compile step, then the empty program and the kernel programs as native executables.
  const exeDir = mkdtempSync(join(work, "native-"));
  const compileSamples = [];
  const exeEmpty = join(exeDir, "empty.exe");
  for (let i = 0; i < REPS; i += 1) {
    const c = timed(compiler, ["--compile", "--out", exeEmpty, emptyOstrin]);
    if (c.status !== 0) throw new Error(`compile of the empty program failed: ${c.stderr}`);
    compileSamples.push(c.elapsed);
  }
  inits.ostrinNative = measure("ostrin native init", () => timed(exeEmpty, []), REPS);
  inits.ostrinNativeCompile = stats(compileSamples);
  for (const kernel of KERNELS) {
    const file = join(root, "benchmarks", "ostrin", `${kernel}.ostrin`);
    const exe = join(exeDir, `${kernel}.exe`);
    const compile = timed(compiler, ["--compile", "--out", exe, file]);
    const cell = { language: "Ostrin native", kernel, compile: compile.status === 0 ? { median: compile.elapsed } : { failed: compile.stderr.slice(0, 300) } };
    if (compile.status !== 0) {
      report.cells.push(cell);
      continue;
    }
    const first = timed(exe, []);
    const checked = check(kernel, first.stdout);
    cell.correct = checked.ok;
    cell.values = checked.values;
    if (!checked.ok) {
      cell.error = checked.message;
      report.cells.push(cell);
      continue;
    }
    const compileReps = [compile.elapsed];
    for (let i = 1; i < REPS; i += 1) compileReps.push(timed(compiler, ["--compile", "--out", exe, file]).elapsed);
    cell.compile = { ...stats(compileReps) };
    cell.total = measure("total", () => timed(exe, []), REPS, (out) => check(kernel, out));
    cell.init = inits.ostrinNative;
    cell.complete = !cell.total.incomplete && !inits.ostrinNative.incomplete;
    delete cell.total.outputs;
    report.cells.push(cell);
  }
  report.ostrinNativeInit = inits.ostrinNative.stats ?? inits.ostrinNative;
}

{
  // Ostrin interpreter: the same programs run by `ostrinc --run`. There is no compile step; the init is the process
  // start of an empty program run by the interpreter.
  const init = measure("ostrin interpreter init", () => timed(compiler, ["--run", emptyOstrin]), INTERP_REPS);
  for (const kernel of KERNELS) {
    const file = join(root, "benchmarks", "ostrin", `${kernel}.ostrin`);
    const cell = { language: "Ostrin interpreter", kernel };
    const first = timed(compiler, ["--run", file]);
    const checked = check(kernel, first.stdout);
    cell.correct = checked.ok;
    cell.values = checked.values;
    if (!checked.ok) {
      cell.error = checked.message;
      report.cells.push(cell);
      continue;
    }
    cell.total = measure("total", () => timed(compiler, ["--run", file]), INTERP_REPS, (out) => check(kernel, out));
    cell.init = init;
    cell.complete = !cell.total.incomplete && !init.incomplete;
    delete cell.total.outputs;
    report.cells.push(cell);
  }
  report.ostrinInterpreterInit = init.stats ?? init;
}

for (const language of ["Python", "Julia"]) {
  const command = language === "Python" ? "python" : "julia";
  const script = language === "Python" ? pythonKernels : juliaKernels;
  // The wall-clock total runs the kernel once, as a program. --timed (used only for the in-process measurement below)
  // makes the program run the kernel a second time, so it must never be used for the wall-clock total.
  const argsFor = (kernel) => (language === "Python" ? [script, kernel] : ["--startup-file=no", script, kernel]);
  const argsTimed = (kernel) => (language === "Python" ? [script, kernel, "--timed"] : ["--startup-file=no", script, kernel, "--timed"]);
  const initArgs = language === "Python" ? [emptyPython] : ["--startup-file=no", "-e", "nothing"];
  const init = measure(`${language} init`, () => timed(command, initArgs), REPS);
  for (const kernel of KERNELS) {
    const cell = { language, kernel };
    const first = timed(command, argsFor(kernel));
    const checked = check(kernel, first.stdout);
    cell.correct = checked.ok;
    cell.values = checked.values;
    if (!checked.ok) {
      cell.error = checked.message ?? first.stderr.slice(0, 300);
      cell.complete = false;
      report.cells.push(cell);
      continue;
    }
    cell.total = measure("total", () => timed(command, argsFor(kernel)), REPS, (out) => check(kernel, out));
    cell.init = init;
    cell.complete = !cell.total.incomplete && !init.incomplete;
    delete cell.total.outputs;
    // In-process timings come from separate runs with --timed; they are not part of the wall-clock totals above.
    const internalRuns = measure("internal", () => timed(command, argsTimed(kernel)), REPS, (out) => check(kernel, out));
    cell.internal = internalRuns.incomplete ? { incomplete: true, failed: internalRuns.failed } : internalTimings(internalRuns.outputs);
    report.cells.push(cell);
  }
  report[`${language.toLowerCase()}Init`] = init.stats ?? init;
}

rmSync(work, { recursive: true, force: true });

report.allComplete = report.cells.every((cell) => cell.complete === true && cell.correct === true);
const stamp = new Date().toISOString().replace(/[:.]/g, "-");
const outDir = join(root, "benchmarks", "results");
mkdirSync(outDir, { recursive: true });
const jsonPath = join(outDir, `benchmark-${stamp}.json`);
writeFileSync(jsonPath, JSON.stringify(report, null, 2));
console.log(`wrote ${jsonPath}`);
for (const cell of report.cells) {
  const total = cell.total?.stats;
  const init = cell.init?.stats ?? cell.init;
  const run = total && init && init.median !== undefined ? total.median - init.median : null;
  console.log(`${cell.language.padEnd(18)} ${cell.kernel.padEnd(10)} correct=${cell.correct ?? false} ` +
    (total ? `total=${total.median.toFixed(0)} ms (min ${total.min.toFixed(0)}, max ${total.max.toFixed(0)}, MAD ${total.mad.toFixed(0)}) run~${run?.toFixed(0)} ms` : `INCOMPLETE: ${cell.error ?? cell.total?.failed ?? "not measured"}`));
}
