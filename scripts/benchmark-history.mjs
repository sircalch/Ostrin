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

const comparableFields = ["compiler", "node", "platform", "arch", "iterations", "warmups"];
const regressionThresholdPercent = 25;
const transitions = [];
for (let index = 1; index < result.entries.length; index += 1) {
  const previous = result.entries[index - 1];
  const current = result.entries[index];
  const sameEnvironment = comparableFields.every((field) => previous[field] === current[field]);
  const previousByWorkload = new Map(previous.workloads.map((workload) => [workload.workload, workload]));
  const comparableWorkloads = current.workloads.filter((workload) => {
    const prior = previousByWorkload.get(workload.workload);
    return prior && prior.sourceSha256 && workload.sourceSha256 && prior.sourceSha256 === workload.sourceSha256;
  });
  if (!sameEnvironment || comparableWorkloads.length !== current.workloads.length) {
    transitions.push({
      from: previous.sourceCommit,
      to: current.sourceCommit,
      status: "NOT_COMPARABLE",
      reason: !sameEnvironment ? "environment metadata differs" : "source hashes are missing or differ",
    });
    continue;
  }
  const regressions = comparableWorkloads.map((workload) => {
    const prior = previousByWorkload.get(workload.workload);
    return {
      workload: workload.workload,
      nativeMedianDeltaPercent: ((workload.nativeMedianMs / prior.nativeMedianMs) - 1) * 100,
      interpreterMedianDeltaPercent: ((workload.interpreterMedianMs / prior.interpreterMedianMs) - 1) * 100,
      ratioDeltaPercent: ((workload.interpreterToNativeMedianRatio / prior.interpreterToNativeMedianRatio) - 1) * 100,
    };
  });
  const maxNativeRegressionPercent = Math.max(...regressions.map((item) => item.nativeMedianDeltaPercent));
  const maxInterpreterRegressionPercent = Math.max(...regressions.map((item) => item.interpreterMedianDeltaPercent));
  const status = maxNativeRegressionPercent <= regressionThresholdPercent
    && maxInterpreterRegressionPercent <= regressionThresholdPercent ? "PASS" : "REVIEW_REQUIRED";
  transitions.push({
    from: previous.sourceCommit,
    to: current.sourceCommit,
    status,
    thresholdPercent: regressionThresholdPercent,
    maxNativeRegressionPercent: Number(maxNativeRegressionPercent.toFixed(3)),
    maxInterpreterRegressionPercent: Number(maxInterpreterRegressionPercent.toFixed(3)),
    workloads: regressions,
  });
}

result.analysis = {
  schema: "ostrin.benchmark-history-analysis/v1",
  policy: "Only adjacent snapshots with identical environment metadata and identical source hashes are compared; positive median deltas are regressions.",
  regressionThresholdPercent,
  transitions,
  status: transitions.some((transition) => transition.status === "REVIEW_REQUIRED")
    || !transitions.some((transition) => transition.status === "PASS") ? "OPEN" : "READY",
};

writeFileSync("validation/benchmark-history.json", `${JSON.stringify(result, null, 2)}\n`, "utf8");
console.log(`benchmark-history: wrote ${result.entries.length} historical entries`);
