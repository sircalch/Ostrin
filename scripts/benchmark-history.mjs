import { execFileSync } from "node:child_process";
import { writeFileSync } from "node:fs";

const snapshots = ["e7eb17c", "310ed6c", "b3c997e"];

function readSnapshot(ref) {
  const source = execFileSync("git", ["show", `${ref}:website/benchmark-data.js`], { encoding: "utf8" });
  const start = source.indexOf("Object.freeze(") + "Object.freeze(".length;
  const end = source.lastIndexOf(");");
  if (start < "Object.freeze(".length || end <= start) {
    throw new Error(`benchmark-history: invalid snapshot at ${ref}`);
  }
  const data = JSON.parse(source.slice(start, end));
  return {
    sourceCommit: ref,
    benchmarkCommit: data.commit,
    compiler: data.compiler,
    node: data.node,
    platform: data.platform,
    arch: data.arch,
    iterations: data.iterations,
    warmups: data.warmups,
    gitClean: data.git?.clean ?? null,
    workloads: data.workloads.map((workload) => ({
      workload: workload.workload,
      sourceSha256: workload.sourceSha256,
      outputSha256: workload.outputSha256,
      interpreterMedianMs: workload.interpreterMedianMs,
      nativeMedianMs: workload.nativeMedianMs,
      interpreterToNativeMedianRatio: workload.interpreterToNativeMedianRatio,
    })),
  };
}

const result = {
  schema: "ostrin.benchmark-history/v1",
  generatedFrom: "checked-in website/benchmark-data.js snapshots",
  policy: "Historical entries preserve their original commit and environment; they are not cross-machine comparisons.",
  entries: snapshots.map(readSnapshot),
};

writeFileSync("validation/benchmark-history.json", `${JSON.stringify(result, null, 2)}\n`, "utf8");
console.log(`benchmark-history: wrote ${result.entries.length} historical entries`);
