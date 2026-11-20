// Interpreter A/B benchmark for Benchmark V2: several Ostrin builds (pinned commits), the same workloads, verified outputs.
//
// Usage: node benchmarks/v2/interp/run-interp-ab.mjs --binary <label>=<path>@<commit> [--binary ...]
//                 [--reps 7] [--kbig 5] [--only name1,name2] [--out <dir>]
//
// Method (the external-slope method of Benchmark V2, applied to several builds of the same interpreter):
//  - Each workload takes K (its repetition count) as `-- K`. Every build runs K = 1 and K = KBIG alternately, REPS times.
//  - The order of the builds is rotated at every repetition, so no build always runs first or last.
//  - For each repetition r the paired slope is s_r = (T_KBIG(r) - T_1(r)) / (KBIG - 1): process start and the front end
//    cancel. The reported slope is the median of s_r; the spread is the interquartile range and the min..max range.
//  - The start-up estimate is median(T_1) - slope. It is an estimate, not a measurement.
//  - Two builds are called different only when the ranges of their paired slopes do not overlap.
//  - Every run's RESULT line is checked against the reference of programs.mjs before its time is used.
// The interpreter has no clock of its own, so only external times are used here; no in-program time is reported.

import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { cpus, totalmem, release, arch, platform } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { PROGRAMS, expectedText } from "./programs.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const args = process.argv.slice(2);
const flags = (name) => args.flatMap((a, i) => (a === name ? [args[i + 1]] : []));
const one = (name, fallback) => (flags(name).length ? flags(name)[0] : fallback);
const REPS = Number(one("--reps", 7));
const KBIG = Number(one("--kbig", 5));
const only = one("--only", null);
const outDir = one("--out", join(here, "..", "results"));
const binaries = flags("--binary").map((spec) => {
  const m = spec.match(/^([^=]+)=(.+)@([0-9a-f]{7,40})$/);
  if (!m) throw new Error(`bad --binary ${spec}: expected label=path@commit`);
  return { label: m[1], path: m[2], commit: m[3] };
});
if (binaries.length < 1) throw new Error("at least one --binary is required");
const env = { ...process.env, OMP_NUM_THREADS: "1", OPENBLAS_NUM_THREADS: "1" };
const programFile = (name) => join(here, "programs", `${name}.ostrin`);

function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

function run(binary, name, k) {
  const started = process.hrtime.bigint();
  const r = spawnSync(binary.path, ["--run", programFile(name), "--", String(k)], {
    encoding: "utf8",
    env,
    timeout: 3_600_000,
    maxBuffer: 64 * 1024 * 1024,
  });
  const ms = Number(process.hrtime.bigint() - started) / 1e6;
  return { ms, status: r.status, stdout: r.stdout ?? "", stderr: r.stderr ?? "" };
}

function resultValue(stdout, name) {
  const line = stdout.split("\n").find((text) => text.startsWith(`RESULT ${name} `));
  return line === undefined ? null : line.slice(`RESULT ${name} `.length).trim();
}

function correct(program, k, stdout) {
  const value = resultValue(stdout, program.name);
  if (value === null) return { ok: false, message: "no RESULT line" };
  const expected = expectedText(program, k);
  if (expected.kind === "int") {
    return value === expected.value ? { ok: true } : { ok: false, message: `got ${value}, expected ${expected.value}` };
  }
  const got = Number(value);
  const want = Number(expected.value);
  const tolerance = 1e-9 * Math.max(1, Math.abs(want));
  return Number.isFinite(got) && Math.abs(got - want) <= tolerance
    ? { ok: true }
    : { ok: false, message: `got ${value}, expected ${expected.value}` };
}

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

function systemInfo() {
  const info = {
    platform: platform(),
    release: release(),
    arch: arch(),
    cpu: cpus()[0]?.model ?? "unknown",
    logicalCpus: cpus().length,
    memoryGiB: Math.round((totalmem() / 2 ** 30) * 10) / 10,
    node: process.version,
    threadEnvironment: { OMP_NUM_THREADS: "1", OPENBLAS_NUM_THREADS: "1" },
  };
  // Current machine load: total processor utilisation sampled over two seconds (Windows performance counter).
  const load = spawnSync(
    "powershell",
    ["-NoProfile", "-Command", "(Get-Counter '\Processor(_Total)\% Processor Time' -SampleInterval 1 -MaxSamples 2).CounterSamples | Select-Object -Last 1 | ForEach-Object { [math]::Round($_.CookedValue, 1) }"],
    { encoding: "utf8", timeout: 60_000 },
  );
  info.cpuUtilisationPercent = Number((load.stdout || "").trim()) || "unavailable";
  // Processes with the largest cumulative CPU time (seconds since each process started; not a current load).
  const ps = spawnSync(
    "powershell",
    ["-NoProfile", "-Command", "Get-Process | Sort-Object CPU -Descending | Select-Object -First 6 Name,@{n='CPU_s';e={[math]::Round($_.CPU,1)}} | ConvertTo-Json -Compress"],
    { encoding: "utf8", timeout: 60_000 },
  );
  try {
    info.cumulativeCpuProcesses = JSON.parse(ps.stdout || "[]");
  } catch {
    info.cumulativeCpuProcesses = "unavailable";
  }
  return info;
}

function versionOf(binary) {
  const r = spawnSync(binary.path, ["--version"], { encoding: "utf8" });
  return (r.stdout || r.stderr || "").trim();
}

const selected = PROGRAMS.filter((p) => only === null || only.split(",").includes(p.name));
const started = new Date();
const stamp = started.toISOString().replace(/[:.]/g, "-");
const builds = binaries.map((b) => ({ ...b, sha256: sha256(b.path), version: versionOf(b) }));
const before = systemInfo();
const results = {};
for (const program of selected) {
  results[program.name] = { group: program.group, kind: program.kind, builds: {} };
  // verification and warm-up at K = 1, for every build
  for (const b of builds) {
    const first = run(b, program.name, 1);
    const check = first.status === 0 ? correct(program, 1, first.stdout) : { ok: false, message: `exit ${first.status}` };
    results[program.name].builds[b.label] = { correct: check, T1: [], TB: [], slope: [] };
    if (!check.ok) {
      console.log(`${program.name} ${b.label}: NOT CORRECT (${check.message})`);
    }
  }
  // timed repetitions, order rotated per repetition
  for (let rep = 0; rep < REPS; rep += 1) {
    const order = builds.map((_, i) => builds[(i + rep) % builds.length]);
    for (const b of order) {
      const one1 = run(b, program.name, 1);
      const bigRun = run(b, program.name, KBIG);
      const entry = results[program.name].builds[b.label];
      for (const [run_, k] of [[one1, 1], [bigRun, KBIG]]) {
        if (run_.status !== 0) entry.correct = { ok: false, message: `exit ${run_.status} at K=${k}` };
        else {
          const check = correct(program, k, run_.stdout);
          if (!check.ok) entry.correct = check;
        }
      }
      entry.T1.push(one1.ms);
      entry.TB.push(bigRun.ms);
      entry.slope.push((bigRun.ms - one1.ms) / (KBIG - 1));
    }
  }
  for (const b of builds) {
    const e = results[program.name].builds[b.label];
    e.slopeMedian = median(e.slope);
    e.slopeIQR = quartile(e.slope, 0.75) - quartile(e.slope, 0.25);
    e.slopeMin = Math.min(...e.slope);
    e.slopeMax = Math.max(...e.slope);
    e.startupEstimate = median(e.T1) - e.slopeMedian;
    e.cvPercentT1 = (100 * Math.sqrt(e.T1.reduce((s, x) => s + (x - median(e.T1)) ** 2, 0) / e.T1.length)) / median(e.T1);
    console.log(
      `${program.name.padEnd(20)} ${b.label.padEnd(10)} slope ${e.slopeMedian.toFixed(2).padStart(9)} ms/run  IQR ${e.slopeIQR.toFixed(2).padStart(7)}  ok=${e.correct.ok}`,
    );
  }
}
const after = systemInfo();
const finished = new Date();
const payload = {
  schema: "ostrin-benchmark-v2-interp-ab/1",
  started: started.toISOString(),
  finished: finished.toISOString(),
  method: {
    reps: REPS,
    kBig: KBIG,
    order: "rotated per repetition",
    slope: "median over repetitions of (T(KBIG) - T(1)) / (KBIG - 1), paired per repetition",
    distinguishable: "paired-slope ranges do not overlap",
  },
  builds: builds.map(({ label, commit, path, sha256: hash, version }) => ({ label, commit, path, sha256: hash, version })),
  system: { before, after },
  programs: results,
};
mkdirSync(outDir, { recursive: true });
const jsonPath = join(outDir, `interp-ab-${stamp}.json`);
writeFileSync(jsonPath, JSON.stringify(payload, null, 2) + "\n", "utf8");
console.log(`wrote ${jsonPath}`);
