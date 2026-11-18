// Writes a Markdown report from a results file written by benchmarks/run.mjs.
//
// Usage: node benchmarks/report.mjs benchmarks/results/benchmark-<stamp>.json
// The report is written next to the results file with the same stamp (REPORT-<stamp>.md). It contains only numbers
// present in the results file; nothing is recomputed from other sources.

import { readFileSync, writeFileSync } from "node:fs";
import { basename, dirname, join } from "node:path";

const input = process.argv[2];
if (!input) {
  console.error("usage: node benchmarks/report.mjs <results.json>");
  process.exit(1);
}
const data = JSON.parse(readFileSync(input, "utf8"));

const f = (x, digits = 1) => (x === undefined || x === null ? "–" : Number(x).toFixed(digits));
const cellStats = (cell) => cell?.stats ?? cell ?? null;

function runEstimate(cell) {
  const total = cellStats(cell.total);
  const init = cellStats(cell.init);
  if (!total || !init) return null;
  return { value: total.median - init.median, resolution: Math.max(total.mad, init.mad) };
}

const lines = [];
lines.push(`# Benchmark report`);
lines.push("");
lines.push(`Source: \`benchmarks/results/${basename(input)}\`. Run started ${data.generated}. Repetitions: ${data.repetitions} (Ostrin interpreter: ${data.interpreterRepetitions}).`);
lines.push("");
lines.push(`**Run status: ${data.allComplete ? "complete (every cell finished and passed its correctness check)" : "INCOMPLETE: at least one cell did not finish or did not pass; the numbers of such cells are not results"}.**`);
lines.push("");
lines.push("## Environment");
lines.push("");
lines.push(`- Hardware: ${data.hardware.cpuModel}; ${data.hardware.physicalCores ?? "?"} physical cores, ${data.hardware.logicalCores} logical; ${data.hardware.memoryGiB} GiB memory.`);
lines.push(`- Operating system: ${data.hardware.os}.`);
lines.push(`- Versions: ${data.versions.ostrinc}; ${data.versions.python}; ${data.versions.julia}; Node ${data.versions.node}.`);
lines.push("");
lines.push("## Method");
lines.push("");
lines.push("- Correctness is checked before timing, against references computed independently (analytic values or JavaScript).");
lines.push("- `total` is the median wall time of one run, including process start; `init` is the median of an empty program; `run ≈` is `total − init` (estimate).");
lines.push("- Dispersion: minimum, maximum and median absolute deviation (MAD) over the repetitions.");
lines.push("- A `run ≈` value smaller than the larger MAD of the two measurements is below the measurement resolution and is not interpreted.");
lines.push("- Ostrin has no clock inside a program, so the kernel time is always estimated from outside the process.");
lines.push("");
lines.push("## Results");
lines.push("");
lines.push("| Implementation | Kernel | Correct | total (ms, median) | min–max (ms) | MAD (ms) | init (ms) | run ≈ (ms) |");
lines.push("|---|---|---|---|---|---|---|---|");
for (const cell of data.cells) {
  const total = cellStats(cell.total);
  const init = cellStats(cell.init);
  const run = runEstimate(cell);
  const resolution = run && run.value < run.resolution ? " (below resolution)" : "";
  const status = cell.complete === false || (cell.total && cell.total.incomplete) ? "INCOMPLETE" : cell.correct ? "yes" : "NO";
  lines.push(
    `| ${cell.language} | ${cell.kernel} | ${status} | ${total ? f(total.median) : "–"} | ${total ? `${f(total.min)}–${f(total.max)}` : "–"} | ${total ? f(total.mad) : "–"} | ${init ? f(init.median) : "–"} | ${run ? f(run.value) + resolution : "–"} |`,
  );
}
lines.push("");
const internal = data.cells.filter((cell) => cell.internal && cell.internal.cold);
if (internal.length > 0) {
  lines.push("## In-process timings (Python and Julia only)");
  lines.push("");
  lines.push("`cold` is the first call of the kernel inside the process (for Julia it includes compilation of the kernel); `warm` is a second call in the same process. Neither includes process start.");
  lines.push("");
  lines.push("| Implementation | Kernel | cold median (ms) | cold min–max | warm median (ms) | warm min–max | warm MAD (ms) |");
  lines.push("|---|---|---|---|---|---|---|");
  for (const cell of internal) {
    const c = cell.internal.cold;
    const w = cell.internal.warm;
    lines.push(`| ${cell.language} | ${cell.kernel} | ${f(c.median, 2)} | ${f(c.min, 2)}–${f(c.max, 2)} | ${f(w.median, 2)} | ${f(w.min, 2)}–${f(w.max, 2)} | ${f(w.mad, 2)} |`);
  }
  lines.push("");
}
lines.push("");
const compiles = data.cells.filter((cell) => cell.compile && cell.compile.median !== undefined || cell.compile?.stats);
if (compiles.length > 0) {
  lines.push("## Ostrin native compile");
  lines.push("");
  lines.push("| Kernel | compile median (ms) | min–max (ms) | MAD (ms) |");
  lines.push("|---|---|---|---|");
  for (const cell of compiles) {
    const c = cellStats(cell.compile);
    lines.push(`| ${cell.kernel} | ${f(c.median)} | ${f(c.min)}–${f(c.max)} | ${f(c.mad)} |`);
  }
  lines.push("");
}
const failures = data.cells.filter((cell) => !cell.correct);
lines.push("## Correctness");
lines.push("");
if (failures.length === 0) {
  lines.push("Every cell produced the reference values; no timing was discarded.");
} else {
  for (const cell of failures) lines.push(`- ${cell.language}, ${cell.kernel}: ${cell.error ?? "failed"}`);
}
lines.push("");
lines.push("## Limits");
lines.push("");
lines.push("- The kernels are equivalent algorithms written in each language; they are not tuned library calls. NumPy and other compiled libraries are not used.");
lines.push("- The symbolic engine is not benchmarked against other systems: there is no equivalent operation with the same semantics in Python or Julia here.");
lines.push("- Wall-clock timing includes process start and the operating system's scheduling; the results on this machine do not generalise to other machines.");
lines.push("- No Wolfram or MATLAB results are included.");
lines.push("");

const out = join(dirname(input), basename(input).replace(/^benchmark-/, "REPORT-").replace(/\.json$/, ".md"));
writeFileSync(out, lines.join("\n"));
console.log(`wrote ${out}`);
