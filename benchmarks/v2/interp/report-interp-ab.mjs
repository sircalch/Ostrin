// Markdown report for a run-interp-ab.mjs JSON file. Every number is read from the JSON; nothing is recomputed from other sources.
//
// Usage: node benchmarks/v2/interp/report-interp-ab.mjs <results/interp-ab-....json> [--out report.md]

import { readFileSync, writeFileSync } from "node:fs";
import { basename } from "node:path";

const [input, ...rest] = process.argv.slice(2);
if (!input) throw new Error("usage: report-interp-ab.mjs <json> [--out file.md]");
const outIndex = rest.indexOf("--out");
const out = outIndex >= 0 ? rest[outIndex + 1] : null;
const data = JSON.parse(readFileSync(input, "utf8"));
const labels = data.builds.map((b) => b.label);
const base = labels[0];
const f = (x, d = 2) => (x === undefined || !Number.isFinite(x) ? "n/a" : x.toFixed(d));

// A comparison with the baseline build is conclusive when the paired slope ranges do not overlap.
function verdict(entry, reference) {
  if (!entry.correct.ok || !reference.correct.ok) return "not verified";
  if (entry.slopeMax < reference.slopeMin) return "faster (conclusive)";
  if (entry.slopeMin > reference.slopeMax) return "slower (conclusive)";
  return "same within noise";
}

const lines = [];
lines.push("# Interpreter A/B: Benchmark V2 extension");
lines.push("");
lines.push(`Source: \`${basename(input)}\`, started ${data.started}, finished ${data.finished}.`);
lines.push("");
lines.push("## Method");
lines.push("");
lines.push(`- External slope: each workload runs with K = 1 and K = ${data.method.kBig}, paired per repetition (${data.method.reps} repetitions).`);
lines.push(`- Order of the builds is rotated at every repetition. Each build's RESULT line is checked before its times are used.`);
lines.push(`- "faster/slower (conclusive)": the paired-slope ranges do not overlap. "same within noise": they overlap.`);
lines.push(`- The baseline is the first build (${base}). Times are wall-clock per run of the workload, including the interpreter's start-up cost, which the slope removes.`);
lines.push("");
lines.push("## Builds");
lines.push("");
lines.push("| Label | Commit | Version | SHA-256 (exe) |");
lines.push("|---|---|---|---|");
for (const b of data.builds) lines.push(`| ${b.label} | \`${b.commit}\` | ${b.version} | \`${b.sha256.slice(0, 16)}…\` |`);
lines.push("");
lines.push("## System");
lines.push("");
const s = data.system.before;
lines.push(`- ${s.platform} ${s.release} ${s.arch}, ${s.cpu}, ${s.logicalCpus} logical CPUs, ${s.memoryGiB} GiB; node ${s.node}.`);
lines.push(`- Thread environment: ${JSON.stringify(s.threadEnvironment)} (the interpreter is single-threaded).`);
lines.push(`- Processor utilisation before the run: ${s.cpuUtilisationPercent}%; after the run: ${data.system.after.cpuUtilisationPercent}%.`);
lines.push(`- Processes with the largest cumulative CPU time (seconds since start, not current load): ${Array.isArray(s.cumulativeCpuProcesses) ? s.cumulativeCpuProcesses.map((p) => `${p.Name} (${p.CPU_s} s)`).join(", ") : "unavailable"}.`);
lines.push("");
lines.push("## Results per workload (median of paired slopes, ms per run)");
lines.push("");
const header = ["Workload", "Group", ...labels.map((l) => `${l} ms/run`), ...labels.slice(1).map((l) => `${l} vs ${base}`)];
lines.push(`| ${header.join(" | ")} |`);
lines.push(`|${header.map(() => "---").join("|")}|`);
for (const [name, p] of Object.entries(data.programs)) {
  const ref = p.builds[base];
  const cells = labels.map((l) => {
    const e = p.builds[l];
    return e.correct.ok ? `${f(e.slopeMedian)} ± ${f(e.slopeIQR)}` : `NOT CORRECT: ${e.correct.message}`;
  });
  const comparisons = labels.slice(1).map((l) => {
    const e = p.builds[l];
    const ratio = e.correct.ok && ref.correct.ok ? `${f(e.slopeMedian / ref.slopeMedian, 3)}× — ${verdict(e, ref)}` : "not verified";
    return ratio;
  });
  lines.push(`| ${name} | ${p.group} | ${cells.join(" | ")} | ${comparisons.join(" | ")} |`);
}
lines.push("");
lines.push("Ranges (min–max of the paired slopes) and start-up estimates:");
lines.push("");
lines.push("| Workload | Build | min–max ms/run | start-up estimate ms | CV of K=1 time (%) | Verified |");
lines.push("|---|---|---|---|---|---|");
for (const [name, p] of Object.entries(data.programs)) {
  for (const l of labels) {
    const e = p.builds[l];
    lines.push(`| ${name} | ${l} | ${f(e.slopeMin)}–${f(e.slopeMax)} | ${f(e.startupEstimate, 1)} | ${f(e.cvPercentT1, 1)} | ${e.correct.ok ? "yes" : "no: " + e.correct.message} |`);
  }
}
lines.push("");
const text = lines.join("\n") + "\n";
if (out) writeFileSync(out, text, "utf8");
else process.stdout.write(text);
