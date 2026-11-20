// Interpreter micro-benchmarks: per-iteration cost of one operation, by repeated external runs.
//
// Usage: node benchmarks/micro/run-micro.mjs [repetitions]
// Each program runs once as warm-up, then `repetitions` times. The cost per iteration is
// (median total − median baseline) / iterations, where the baseline is an empty program run the same way. The
// baseline is the process start and the front end; the figure is an estimate and its uncertainty is shown from the
// spread of both measurements. Results are printed; the caller records them.
import { spawnSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { writeFileSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..", "..");
const compiler = join(root, "compiler", "target", "release", "ostrinc.exe");
const reps = Number(process.argv[2] ?? 5);

const programs = [
  { name: "empty_loop", file: "empty_loop.ostrin", iterations: 10_000_000 },
  { name: "int_add", file: "int_add.ostrin", iterations: 10_000_000 },
  { name: "float_mul_add", file: "float_mul_add.ostrin", iterations: 10_000_000 },
  { name: "array_read", file: "array_read.ostrin", iterations: 10_000_000 },
  { name: "function_call", file: "function_call.ostrin", iterations: 10_000_000 },
  { name: "nested_block", file: "nested_block.ostrin", iterations: 10_000_000 },
  { name: "while_loop", file: "while_loop.ostrin", iterations: 10_000_000 },
  { name: "quantity_add", file: "quantity_add.ostrin", iterations: 1_000_000 },
];

const work = mkdtempSync(join(tmpdir(), "micro-"));
const empty = join(work, "empty.ostrin");
writeFileSync(empty, 'fn main() -> Void {\n    print("RESULT empty 0")\n}\n');

function once(file) {
  const started = process.hrtime.bigint();
  const r = spawnSync(compiler, ["--run", file], { encoding: "utf8", timeout: 900_000, maxBuffer: 64 * 1024 * 1024 });
  const ms = Number(process.hrtime.bigint() - started) / 1e6;
  return { ms, status: r.status, stdout: r.stdout ?? "", stderr: r.stderr ?? "" };
}

function series(file) {
  once(file); // warm-up
  const samples = [];
  let last = null;
  for (let i = 0; i < reps; i += 1) {
    last = once(file);
    if (last.status !== 0) throw new Error(`${file} failed: ${last.stderr || last.stdout}`);
    samples.push(last.ms);
  }
  return { samples, stdout: last.stdout };
}

function median(xs) {
  const s = [...xs].sort((a, b) => a - b);
  const m = Math.floor(s.length / 2);
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
}

const base = series(empty);
const baseMedian = median(base.samples);
const baseSpread = Math.max(...base.samples) - Math.min(...base.samples);
const rows = [];
for (const p of programs) {
  const file = join(root, "benchmarks", "micro", p.file);
  const s = series(file);
  const m = median(s.samples);
  const spread = Math.max(...s.samples) - Math.min(...s.samples);
  const perIter = ((m - baseMedian) * 1e6) / p.iterations; // ns per iteration
  const uncertainty = ((spread + baseSpread) * 1e6) / p.iterations;
  rows.push({ name: p.name, iterations: p.iterations, totalMs: m, baselineMs: baseMedian, nsPerIteration: perIter, uncertaintyNs: uncertainty, result: s.stdout.trim() });
  console.log(`${p.name.padEnd(14)} total ${m.toFixed(1)} ms  baseline ${baseMedian.toFixed(1)} ms  ${perIter.toFixed(1)} ns/iteration ± ${uncertainty.toFixed(1)}  ${s.stdout.trim()}`);
}
rmSync(work, { recursive: true, force: true });
const outFile = join(here, "results-" + new Date().toISOString().replace(/[:.]/g, "-") + ".json");
writeFileSync(outFile, JSON.stringify({ repetitions: reps, baselineSamplesMs: base.samples, rows }, null, 2));
console.log("wrote " + outFile);
