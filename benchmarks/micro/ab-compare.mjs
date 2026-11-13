// A/B comparison of two interpreter binaries on the same micro-benchmarks, with runs interleaved so that both binaries
// see the same machine conditions.
//
// Usage: node benchmarks/micro/ab-compare.mjs <binaryA> <binaryB> [repetitions]
// For each program: an empty baseline and the program are run alternately with A and B. The per-iteration cost is
// (median total − median baseline) / iterations, for each binary, with the spread of the measurements. The difference
// is reported as a ratio of the two costs with its uncertainty from both spreads. These are external measurements
// including process start; the baseline is an estimate of that start.
import { spawnSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { writeFileSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";

const [binA, binB, repsArg] = process.argv.slice(2);
if (!binA || !binB) {
  console.error("usage: node benchmarks/micro/ab-compare.mjs <binaryA> <binaryB> [repetitions]");
  process.exit(1);
}
const reps = Number(repsArg ?? 10);
const here = dirname(fileURLToPath(import.meta.url));
const programs = [
  { name: "function_call", file: "function_call.ostrin", iterations: 10_000_000 },
  { name: "int_add", file: "int_add.ostrin", iterations: 10_000_000 },
  { name: "empty_loop", file: "empty_loop.ostrin", iterations: 10_000_000 },
];
const work = mkdtempSync(join(tmpdir(), "ab-"));
const empty = join(work, "empty.ostrin");
writeFileSync(empty, 'fn main() -> Void {\n    print("RESULT empty 0")\n}\n');

function once(bin, file) {
  const started = process.hrtime.bigint();
  const r = spawnSync(bin, ["--run", file], { encoding: "utf8", timeout: 900_000, maxBuffer: 64 * 1024 * 1024 });
  const ms = Number(process.hrtime.bigint() - started) / 1e6;
  if (r.status !== 0) throw new Error(`${bin} failed on ${file}: ${r.stderr || r.stdout}`);
  return { ms, stdout: r.stdout.trim() };
}
const median = (xs) => {
  const s = [...xs].sort((a, b) => a - b);
  const m = Math.floor(s.length / 2);
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
};
const spread = (xs) => Math.max(...xs) - Math.min(...xs);

const results = [];
for (const p of programs) {
  const file = join(here, p.file);
  const samples = { A: { base: [], run: [] }, B: { base: [], run: [] } };
  const outputs = { A: "", B: "" };
  // warm-up for each binary
  for (const bin of [binA, binB]) {
    once(bin, empty);
    once(bin, file);
  }
  for (let i = 0; i < reps; i += 1) {
    for (const [label, bin] of [["A", binA], ["B", binB]]) {
      samples[label].base.push(once(bin, empty).ms);
      const r = once(bin, file);
      samples[label].run.push(r.ms);
      outputs[label] = r.stdout;
    }
  }
  const cost = (label) => {
    const s = samples[label];
    const base = median(s.base);
    const per = ((median(s.run) - base) * 1e6) / p.iterations;
    const unc = ((spread(s.run) + spread(s.base)) * 1e6) / p.iterations;
    return { per, unc, base };
  };
  const a = cost("A");
  const b = cost("B");
  const ratio = b.per / a.per;
  results.push({ program: p.name, iterations: p.iterations, repetitions: reps, A: a, B: b, ratioBoverA: ratio, outputsAgree: outputs.A === outputs.B, output: outputs.A });
  console.log(
    `${p.name.padEnd(13)} A ${a.per.toFixed(1)} ± ${a.unc.toFixed(1)} ns  B ${b.per.toFixed(1)} ± ${b.unc.toFixed(1)} ns  B/A ${ratio.toFixed(3)}  outputs agree: ${outputs.A === outputs.B}`,
  );
}
rmSync(work, { recursive: true, force: true });
const outFile = join(here, "ab-" + new Date().toISOString().replace(/[:.]/g, "-") + ".json");
writeFileSync(outFile, JSON.stringify({ A: binA, B: binB, repetitions: reps, results }, null, 2));
console.log("wrote " + outFile);
