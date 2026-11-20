// Markdown report for a compare-scientific.mjs JSON file. Every number is read from the JSON; nothing is recomputed from other
// sources except the ratios and the overlap test, which are computed from the same stored values.
//
// Usage: node benchmarks/v2/report-scientific.mjs <results/compare-scientific-....json> [--out report.md]

import { readFileSync, writeFileSync } from "node:fs";
import { basename } from "node:path";

const [input, ...rest] = process.argv.slice(2);
if (!input) throw new Error("usage: report-scientific.mjs <json> [--out file.md]");
const outIndex = rest.indexOf("--out");
const out = outIndex >= 0 ? rest[outIndex + 1] : null;
const data = JSON.parse(readFileSync(input, "utf8"));
const f = (x, d = 2) => (x === undefined || x === null || !Number.isFinite(x) ? "n/a" : x.toFixed(d));
const lines = [];
const push = (s = "") => lines.push(s);

const baseline = data.builds[0].label;
const interpCell = (kernel, label) => data.cells.find((c) => c.kernel === kernel && c.implementation === "Ostrin interpreter" && c.label === label);
const cellsOf = (kernel) => data.cells.filter((c) => c.kernel === kernel);

// Two external-slope ranges differ only when they do not overlap.
function interpVerdict(entry, reference) {
  if (entry.incomplete || reference.incomplete) return "incomplete";
  if (entry.rangeMs[1] < reference.rangeMs[0]) return "faster (conclusive)";
  if (entry.rangeMs[0] > reference.rangeMs[1]) return "slower (conclusive)";
  return "same within noise";
}

push("# Scientific comparison: Ostrin interpreter builds, Ostrin native, CPython and Julia");
push("");
push(`Source: \`${basename(input)}\`. Started ${data.started}, finished ${data.finished}.`);
push("");
push("## Builds and versions");
push("");
push("| Label | Commit | SHA-256 (exe) | Version |");
push("|---|---|---|---|");
for (const b of data.builds) push(`| ${b.label} | \`${b.commit}\` | \`${b.sha256.slice(0, 16)}…\` | ${b.version} |`);
push(`| HEAD native | \`${data.native.commit}\` | \`${data.native.sha256.slice(0, 16)}…\` | ${data.native.version} |`);
push("");
push(`Python: ${data.versions.python}. Julia: ${data.versions.julia}. Node: ${data.versions.node}.`);
push("");
push("## System and conditions");
push("");
const h = data.system.hardware;
push(`- ${h.os}; ${h.cpuModel}; ${h.physicalCores ?? "?"} physical / ${h.logicalCores} logical processors; ${h.memoryGiB} GiB.`);
push(`- Thread environment: ${JSON.stringify(data.system.threads)}.`);
push(`- Processor load (instantaneous, Win32_Processor): start ${f(data.load.start.processorPercent, 1)} %, end ${f(data.load.end.processorPercent, 1)} %.`);
push(`- Processes with the largest cumulative CPU time at the start (seconds since each process started, not a current load): ${Array.isArray(data.load.start.topProcesses) ? data.load.start.topProcesses.map((p) => `${p.name} (${p.cumulativeCpuSeconds} s)`).join(", ") : "unavailable"}.`);
push("- The machine was not otherwise idle. Load samples for each cell are in the JSON (`cell.load`).");
push("");
push("## Methods");
push("");
push(`- Ostrin interpreter builds: external slope with K = 1 and K = ${data.kBig.interpreter}, alternating builds, rotated order, ${data.repetitions.interp} repetitions. Two builds differ only when their paired-slope ranges do not overlap (“conclusive”).`);
push(`- Ostrin native (HEAD): compiled ${3} times outside the timed runs (compile time reported separately); external slope with K = 1 and K = ${data.kBig.native}, ${data.repetitions.native} repetitions. The start-up estimate \`median T(1) − slope\` is an estimate.`);
push(`- CPython and Julia: internal timing. Each process repeats the kernel ${data.kBig.internalPerProcess} times; cold = first repetition of each process, warm = the other repetitions. Julia's cold repetition includes first-call compilation. The wall time of a K = 1 process is reported too; it is not subtracted.`);
push("- Every RESULT line was checked against an independent reference before its time was used. A cell that fails is marked INCOMPLETE and has no figure.");
push("- Internal figures (CPython, Julia) and external figures (Ostrin) measure different things and are not directly interchangeable; the report compares each group only with itself, except for the explicit ratios.");
push("");

for (const kernel of data.kernels) {
  push(`## ${kernel}`);
  push("");
  push("| Implementation | Build | Per run (ms) | Range (ms) | Uncertainty / IQR (ms) | Other figures | Verdict |");
  push("|---|---|---|---|---|---|---|");
  const cells = cellsOf(kernel);
  const ref = interpCell(kernel, baseline);
  for (const c of cells) {
    if (c.implementation === "Ostrin interpreter") {
      const verdict = c.label === baseline || !ref ? "reference" : interpVerdict(c, ref);
      const ratio = c.label === baseline || c.incomplete || ref.incomplete ? "" : ` ratio ${f(c.perRunMs / ref.perRunMs, 3)} vs ${baseline}`;
      push(`| Ostrin interpreter | ${c.label} (\`${c.commit}\`) | ${c.incomplete ? "INCOMPLETE" : f(c.perRunMs)} | ${c.incomplete ? c.message : `${f(c.rangeMs[0])}–${f(c.rangeMs[1])}`} | ${c.incomplete ? "" : f(c.uncertaintyMs)} | ${ratio.trim() || "—"} | ${verdict}${ratio && !c.incomplete ? "" : ""} |`);
    } else if (c.implementation === "Ostrin native") {
      const compile = c.compileMs ? `compile ${f(c.compileMs.median, 0)} ms (median of ${c.compileMs.n})` : "compile failed";
      const start = c.startupEstimateMs !== undefined ? `start-up estimate ${f(c.startupEstimateMs, 1)} ms` : "";
      push(`| Ostrin native | HEAD (\`${c.commit}\`) | ${c.incomplete ? "INCOMPLETE" : f(c.perRunMs)} | ${c.incomplete ? c.message : `${f(c.rangeMs[0])}–${f(c.rangeMs[1])}`} | ${c.incomplete ? "" : f(c.uncertaintyMs)} | ${[compile, start].filter(Boolean).join("; ")} | observed |`);
    } else {
      const cold = c.cold ? `cold ${f(c.cold.median)} ms (median of ${c.cold.n})` : "";
      const one = c.wallOneProcess ? `K=1 process wall ${f(c.wallOneProcess.median, 0)} ms` : "";
      push(`| ${c.implementation} | internal, warm | ${c.incomplete ? "INCOMPLETE" : f(c.perRunMs)} | ${c.incomplete ? c.message : `${f(c.rangeMs[0])}–${f(c.rangeMs[1])}`} | ${c.incomplete ? "" : f(c.uncertaintyMs)} | ${[cold, one].filter(Boolean).join("; ")} | observed (internal timer) |`);
    }
  }
  push("");
}

push("## Limits");
push("");
push("- Ostrin and CPython/Julia figures come from different measurement methods (external slope versus internal timers). Their ratios are indicative and are labelled as such in the report text.");
push("- Interpreter repetitions are few (K = 1 and K = 2, three repetitions) because the heavy kernels take tens of seconds each; ranges are reported, not confidence intervals.");
push("- Load was not controlled. Non-overlap of ranges is the only criterion used to call a difference conclusive.");
push("- Python and Julia were not run with the interpreter workloads; the sizes are the same kernel sizes in all languages.");
push("");

const text = lines.join("\n") + "\n";
if (out) writeFileSync(out, text, "utf8");
else process.stdout.write(text);
