// Interleaved A/B of two interpreter binaries on the Benchmark V2 kernels (one repetition per run, K = 1).
//
// Usage: node benchmarks/v2/ab-interp.mjs <before-ostrinc> <after-ostrinc> [repetitions]
// The two binaries alternate, so both see the same machine conditions. For each kernel the table gives the median and
// the min–max range of each binary and the ratio of the medians. Ranges that do not overlap are the evidence that a
// difference is not noise; overlapping ranges are not.
import { spawnSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const [before, after, repsArg] = process.argv.slice(2);
if (!before || !after) {
  console.error("usage: node benchmarks/v2/ab-interp.mjs <before-ostrinc> <after-ostrinc> [repetitions]");
  process.exit(1);
}
const reps = Number(repsArg ?? 5);
const here = dirname(fileURLToPath(import.meta.url));
const kernels = ["simpson", "regression", "bisect", "matmul"];

function once(bin, file) {
  const started = process.hrtime.bigint();
  const r = spawnSync(bin, ["--run", file, "--", "1"], { encoding: "utf8", timeout: 900_000, maxBuffer: 64 * 1024 * 1024 });
  if (r.status !== 0) throw new Error(`${bin} failed on ${file}: ${r.stderr}`);
  return Number(process.hrtime.bigint() - started) / 1e6;
}
const median = (xs) => {
  const s = [...xs].sort((a, b) => a - b);
  const m = s.length >> 1;
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
};

console.log("kernel      before median [min–max] ms    after median [min–max] ms     after/before");
for (const k of kernels) {
  const file = join(here, "ostrin", `${k}.ostrin`);
  const t = { before: [], after: [] };
  once(before, file);
  once(after, file);
  for (let i = 0; i < reps; i += 1) {
    t.before.push(once(before, file));
    t.after.push(once(after, file));
  }
  const range = (xs) => `[${Math.min(...xs).toFixed(0)}–${Math.max(...xs).toFixed(0)}]`;
  console.log(
    `${k.padEnd(11)} ${median(t.before).toFixed(0).padStart(8)} ${range(t.before).padEnd(14)} ${median(t.after).toFixed(0).padStart(8)} ${range(t.after).padEnd(14)} ${(median(t.after) / median(t.before)).toFixed(3)}`,
  );
}
