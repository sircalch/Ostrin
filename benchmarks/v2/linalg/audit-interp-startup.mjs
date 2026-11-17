// Audit of the external-slope start-up estimate for the Ostrin interpreter (lu_solve, n given on the command line).
//
// The earlier comparison (results/linalg-2026-11-16T19-20-15-977Z.json) estimated start-up as T(K=1) - slope, and for
// lu_solve at n = 200 that estimate was negative. This audit measures the model directly instead of inferring it:
//   - K = 0, 1, 2, 3 are each run in a separate process, in an order that is RANDOMISED within every repetition
//     (seeded, so the order can be reproduced), so that no level is always first or always last;
//   - every process is timed with process.hrtime; the output of every run is checked against the reference;
//   - K = 0 runs only the set-up of the program (parse, check, matrices), so its time is the directly measured fixed cost;
//   - a least-squares line T(K) = a + b K is fitted to all runs; the intercept a is the start-up estimate. Its standard
//     error is computed from the residuals, and a 95 % interval uses the t distribution with the residual degrees of
//     freedom. The direct K = 0 measurement is compared with the intercept.
// Classification (written to the JSON, and decided by the rule below, not by eye):
//   VERIFIED     the intercept is consistent with the direct K = 0 time (their intervals overlap) and the slope has a
//                relative standard error below 10 %;
//   INCONCLUSIVE otherwise, with the reason stated (the start-up interval is wider than the direct fixed cost, or the
//                slope is too noisy).
//
// Usage: node benchmarks/v2/linalg/audit-interp-startup.mjs [--n 200] [--reps 12] [--seed 1] [--ostrinc <path>]

import { spawnSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { checkRun } from "./reference.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..", "..", "..");
const args = process.argv.slice(2);
const option = (name, fallback) => (args.includes(name) ? args[args.indexOf(name) + 1] : fallback);
const N = Number(option("--n", "200"));
const REPS = Number(option("--reps", "12"));
let seed = Number(option("--seed", "1"));
const ostrinc = option("--ostrinc", join(root, "compiler", "target", "release", process.platform === "win32" ? "ostrinc.exe" : "ostrinc"));
const program = join(here, "ostrin", "lu_solve.ostrin");
const LEVELS = [0, 1, 2, 3];
const env = { ...process.env, OMP_NUM_THREADS: "1", OPENBLAS_NUM_THREADS: "1" };

// Small seeded generator (mulberry32) so the order of the runs can be reproduced exactly.
function random() {
  seed |= 0;
  seed = (seed + 0x6d2b79f5) | 0;
  let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
  t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
}
function shuffle(xs) {
  const a = [...xs];
  for (let i = a.length - 1; i > 0; i -= 1) {
    const j = Math.floor(random() * (i + 1));
    [a[i], a[j]] = [a[j], a[i]];
  }
  return a;
}

function timed(k) {
  const started = process.hrtime.bigint();
  const r = spawnSync(ostrinc, ["--run", program, "--", String(N), String(k)], { encoding: "utf8", env, timeout: 3_600_000, maxBuffer: 64 * 1024 * 1024, windowsHide: true });
  const ms = Number(process.hrtime.bigint() - started) / 1e6;
  return { k, ms, status: r.status, stdout: r.stdout ?? "", stderr: r.stderr ?? "" };
}

// Least squares T = a + b K over all runs. Returns the coefficients, their standard errors and the residual sd.
function fit(points) {
  const n = points.length;
  const mx = points.reduce((s, p) => s + p.k, 0) / n;
  const my = points.reduce((s, p) => s + p.ms, 0) / n;
  let sxx = 0;
  let sxy = 0;
  for (const p of points) {
    sxx += (p.k - mx) ** 2;
    sxy += (p.k - mx) * (p.ms - my);
  }
  const b = sxy / sxx;
  const a = my - b * mx;
  const rss = points.reduce((s, p) => s + (p.ms - (a + b * p.k)) ** 2, 0);
  const dof = n - 2;
  const sigma = Math.sqrt(rss / dof);
  const seB = sigma / Math.sqrt(sxx);
  const seA = sigma * Math.sqrt(1 / n + (mx * mx) / sxx);
  return { a, b, seA, seB, sigma, dof };
}

// Two-sided 97.5 % quantile of Student's t, for the degrees of freedom used here (exact table for 1 to 30, else 1.96 + 2.4/dof).
const T975 = [0, 12.706, 4.303, 3.182, 2.776, 2.571, 2.447, 2.365, 2.306, 2.262, 2.228, 2.201, 2.179, 2.160, 2.145, 2.131, 2.120, 2.110, 2.101, 2.093, 2.086, 2.080, 2.074, 2.069, 2.064, 2.060, 2.056, 2.052, 2.048, 2.045, 2.042];
const tq = (dof) => (dof <= 30 ? T975[dof] : 1.96 + 2.4 / dof);

const runs = [];
const order = [];
for (let rep = 0; rep < REPS; rep += 1) {
  const sequence = shuffle(LEVELS);
  order.push(sequence);
  for (const k of sequence) {
    const r = timed(k);
    // K = 0 performs no repetition, so its printed value is the empty sum: it is verified by exit status and the presence of
    // the RESULT line only. Every K >= 1 run is verified against the reference.
    let check;
    if (r.status !== 0) check = { ok: false, message: `exit ${r.status}: ${r.stderr.slice(0, 200)}` };
    else if (k === 0) check = /^RESULT lu_solve /m.test(r.stdout) ? { ok: true } : { ok: false, message: "no RESULT line for K = 0" };
    else check = checkRun("lu_solve", N, r.stdout);
    runs.push({ rep, k, ms: r.ms, verified: check.ok, message: check.ok ? null : check.message });
  }
}

const valid = runs.filter((r) => r.verified);
const direct = valid.filter((r) => r.k === 0).map((r) => r.ms);
const points = valid.filter((r) => r.k >= 1).map((r) => ({ k: r.k, ms: r.ms }));
const linear = fit(points.concat(valid.filter((r) => r.k === 0).map((r) => ({ k: 0, ms: r.ms }))));
const dofA = linear.dof;
const t = tq(dofA);
const interval = (value, se) => [value - t * se, value + t * se];
const intercept = interval(linear.a, linear.seA);
const slope = interval(linear.b, linear.seB);
const mean = (xs) => xs.reduce((s, x) => s + x, 0) / xs.length;
const directMean = mean(direct);
const directSd = Math.sqrt(direct.reduce((s, x) => s + (x - directMean) ** 2, 0) / (direct.length - 1));
const directInterval = [directMean - tq(direct.length - 1) * directSd / Math.sqrt(direct.length), directMean + tq(direct.length - 1) * directSd / Math.sqrt(direct.length)];
const overlap = intercept[0] <= directInterval[1] && directInterval[0] <= intercept[1];
const relSeSlope = linear.seB / linear.b;

// Paired increments within each repetition of the design: K1 - K0 is the first repetition of a process; K2 - K1 and K3 - K2
// are steady repetitions (each adds one repetition to the same process). If the first repetition is systematically
// different, the linear model T(K) = a + bK is misspecified and the intercept is not a start-up estimate.
const pair = (hi, lo) => {
  const diffs = [];
  for (const rep of new Set(valid.map((r) => r.rep))) {
    const h = valid.find((r) => r.rep === rep && r.k === hi);
    const l = valid.find((r) => r.rep === rep && r.k === lo);
    if (h && l) diffs.push(h.ms - l.ms);
  }
  const m = mean(diffs);
  const s = Math.sqrt(diffs.reduce((acc, x) => acc + (x - m) ** 2, 0) / (diffs.length - 1));
  return { n: diffs.length, mean: m, sd: s, se: s / Math.sqrt(diffs.length) };
};
const firstRepetition = pair(1, 0);
const steadyA = pair(2, 1);
const steadyB = pair(3, 2);
const firstDiffers = Math.abs(firstRepetition.mean - (steadyA.mean + steadyB.mean) / 2) > 3 * Math.hypot(firstRepetition.se, (steadyA.se + steadyB.se) / 2);

let classification;
let reason;
if (overlap && relSeSlope < 0.1 && !firstDiffers) {
  classification = "VERIFIED";
  reason = "the start-up intercept agrees with the direct K = 0 measurement, the first repetition does not differ from steady repetitions, and the slope has a relative standard error below 10 %";
} else {
  classification = "INCONCLUSIVE";
  const parts = [];
  if (!overlap) parts.push("the start-up intercept and the direct K = 0 measurement have disjoint 95 % intervals");
  if (firstDiffers) parts.push(`the first repetition of a process differs from steady repetitions (${firstRepetition.mean.toFixed(0)} ms against ${((steadyA.mean + steadyB.mean) / 2).toFixed(0)} ms), so the linear start-up model does not hold`);
  if (relSeSlope >= 0.1) parts.push(`the slope has a relative standard error of ${(relSeSlope * 100).toFixed(1)} %`);
  reason = parts.join("; ");
}

const report = {
  schema: "ostrin.interpreter-startup-audit/v0",
  generated: new Date().toISOString(),
  program: "benchmarks/v2/linalg/ostrin/lu_solve.ostrin",
  n: N,
  reps: REPS,
  seed: Number(option("--seed", "1")),
  levels: LEVELS,
  orderPerRepetition: order,
  runs,
  allVerified: runs.every((r) => r.verified),
  directK0: { samples: direct, mean: directMean, sd: directSd, interval95: directInterval },
  fit: { intercept: linear.a, interceptSe: linear.seA, interceptInterval95: intercept, slope: linear.b, slopeSe: linear.seB, slopeInterval95: slope, residualSd: linear.sigma, dof: dofA, t },
  pairedIncrements: { firstRepetition_K1_minus_K0: firstRepetition, steady_K2_minus_K1: steadyA, steady_K3_minus_K2: steadyB, firstDiffersFromSteady: firstDiffers },
  classification,
  reason,
};
const stamp = new Date().toISOString().replace(/[:.]/g, "-");
const outDir = join(here, "results");
mkdirSync(outDir, { recursive: true });
const outPath = join(outDir, `audit-interp-startup-n${N}-${stamp}.json`);
writeFileSync(outPath, JSON.stringify(report, null, 2) + "\n");
console.log(`n=${N} reps=${REPS} runs=${runs.length} verified=${report.allVerified}`);
console.log(`K=0 direct mean ${directMean.toFixed(1)} ms (95% ${directInterval.map((x) => x.toFixed(1)).join(" .. ")})`);
console.log(`fit: intercept ${linear.a.toFixed(1)} ms (95% ${intercept.map((x) => x.toFixed(1)).join(" .. ")}), slope ${linear.b.toFixed(1)} ms per repetition (95% ${slope.map((x) => x.toFixed(1)).join(" .. ")})`);
console.log(`classification: ${classification}: ${reason}`);
console.log(`wrote ${outPath}`);
