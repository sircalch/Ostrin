#!/usr/bin/env node
// Memory and time cost of the scientific abstractions on the same 50,000-iteration accumulation
// loop: plain Float, Quantity<Temperature> interval, TemperaturePoint shift and Measurement with
// a shared source. Uses the opt-in `OSTRIN_MEM_DETAIL=1` report of native executables
// (requested bytes and peak resident set) next to the managed allocation counters.
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseAllocationCounters } from "./benchmark-allocation-profile.mjs";

export const detailScope = "requested bytes through Ostrin managed allocation entry points and OS peak resident set at exit; excludes libc-internal and stack memory";

export function parseMemoryDetail(stderr) {
  const lines = stderr.replaceAll("\r\n", "\n").split("\n").filter((line) => line.startsWith("ostrin memory-detail:"));
  if (lines.length !== 1) throw new Error("expected exactly one native memory-detail report");
  const match = lines[0].match(/^ostrin memory-detail: total_bytes=(0|[1-9]\d*) peak_rss_kb=(0|[1-9]\d*)$/);
  if (!match) throw new Error("malformed native memory-detail report");
  const [totalBytes, peakRssKb] = match.slice(1).map(Number);
  if (!Number.isSafeInteger(totalBytes) || !Number.isSafeInteger(peakRssKb)) throw new Error("memory-detail counter out of range");
  return { totalBytes, peakRssKb, raw: lines[0] };
}

export const workloads = [
  { id: "float", source: "benchmarks/temperature-float.ostrin", role: "control: scalar Float" },
  { id: "quantity-interval", source: "benchmarks/temperature-interval.ostrin", role: "Quantity<Temperature> interval" },
  { id: "temperature-point", source: "benchmarks/temperature-point.ostrin", role: "TemperaturePoint shifted by an interval" },
  { id: "measurement-shared-source", source: "benchmarks/measurement-source-identity.ostrin", role: "Measurement with one shared source" },
];

const median = (values) => {
  const sorted = [...values].sort((a, b) => a - b);
  const mid = sorted.length >> 1;
  return sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
};

function main() {
  const repo = resolve(dirname(fileURLToPath(import.meta.url)), "..");
  const compiler = process.env.OSTRINC
    || join(repo, "compiler", "target", "release", process.platform === "win32" ? "ostrinc.exe" : "ostrinc");
  const runs = Number(process.env.OSTRIN_OVERHEAD_RUNS ?? 9);
  const dir = mkdtempSync(join(tmpdir(), "ostrin-overhead-"));
  const results = [];
  try {
    for (const workload of workloads) {
      const exe = join(dir, `${workload.id}${process.platform === "win32" ? ".exe" : ""}`);
      const build = spawnSync(compiler, ["--compile", "--leak-check", "--out", exe, join(repo, workload.source)], { cwd: repo, encoding: "utf8" });
      if (build.status !== 0) throw new Error(`compile failed for ${workload.id}: ${build.stderr}`);
      const wallMs = [];
      let detail = null;
      let counters = null;
      let stdout = null;
      for (let i = 0; i < runs; i += 1) {
        const started = process.hrtime.bigint();
        const run = spawnSync(exe, [], { encoding: "utf8", env: { ...process.env, OSTRIN_MEM_DETAIL: "1" } });
        wallMs.push(Number(process.hrtime.bigint() - started) / 1e6);
        if (run.status !== 0) throw new Error(`run failed for ${workload.id}`);
        const nextDetail = parseMemoryDetail(run.stderr);
        const nextCounters = parseAllocationCounters(run.stderr);
        if (detail && detail.totalBytes !== nextDetail.totalBytes) throw new Error(`${workload.id}: total_bytes is not deterministic`);
        detail = nextDetail;
        counters = nextCounters;
        stdout = run.stdout.replaceAll("\r\n", "\n");
      }
      results.push({
        id: workload.id,
        role: workload.role,
        output: stdout.trim(),
        totalBytes: detail.totalBytes,
        peakRssKb: detail.peakRssKb,
        peakAllocations: counters.peak,
        totalAllocations: counters.total,
        wallMsMedian: Number(median(wallMs).toFixed(2)),
        wallMsMin: Number(Math.min(...wallMs).toFixed(2)),
        wallMsMax: Number(Math.max(...wallMs).toFixed(2)),
      });
    }
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
  const report = {
    schema: "ostrin.abstraction-overhead/v1",
    scope: detailScope,
    host: { platform: process.platform, arch: process.arch },
    runsPerWorkload: runs,
    note: "Whole-process wall time includes process launch; compare memory columns first. Results are machine-specific.",
    workloads: results,
  };
  const text = `${JSON.stringify(report, null, 2)}\n`;
  if (process.argv.includes("--write")) {
    const out = process.argv[process.argv.indexOf("--write") + 1];
    writeFileSync(out, text);
  } else {
    process.stdout.write(text);
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
