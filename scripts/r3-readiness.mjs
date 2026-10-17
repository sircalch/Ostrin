#!/usr/bin/env node

import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { validateEffectCapture, validateEntryEffectSignature } from "./experiment-bundle.mjs";

export const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
export const bundlesDirectory = path.join(repositoryRoot, "website/assets/experiments");
export const outputPath = path.join(repositoryRoot, "validation/r3-readiness.json");

const requiredPaths = [
  "provenance.commit",
  "provenance.compiler",
  "provenance.target",
  "provenance.runtime.schema",
  "provenance.runtime.target",
  "provenance.runtime.compiler_lockfile",
  "provenance.inputs",
  "provenance.outputs",
  "provenance.parameters",
  "provenance.execution",
  "provenance.figure_metadata",
];

function getPath(value, dottedPath) {
  return dottedPath.split(".").reduce((current, key) => current == null ? undefined : current[key], value);
}

function present(value) {
  if (value == null) return false;
  if (typeof value === "string") return value.length > 0;
  if (Array.isArray(value)) return value.length > 0;
  return typeof value === "object" ? Object.keys(value).length > 0 : true;
}

function isObject(value) {
  return value != null && typeof value === "object" && !Array.isArray(value);
}

function validFileReference(value) {
  return isObject(value)
    && typeof value.path === "string" && value.path.length > 0
    && typeof value.sha256 === "string" && /^sha256:[a-f0-9]{64}$/.test(value.sha256);
}

function captureExtensionErrors(runtime) {
  const errors = [];
  if (!isObject(runtime)
    || runtime.schema !== "ostrin.runtime/v1"
    || runtime.status !== "partial"
    || runtime.target !== "wasm32-wasip1"
    || runtime.wasi !== "preview1"
    || runtime.runner !== "WASI host") {
    errors.push("provenance.runtime.contract");
  }
  if (!validFileReference(runtime?.compiler_lockfile)) {
    errors.push("provenance.runtime.compiler_lockfile");
  }
  const host = runtime?.host;
  if (!isObject(host)
    || host.schema !== "ostrin.host/v1"
    || host.source !== "github-actions"
    || typeof host.runner_os !== "string" || host.runner_os.length === 0 || host.runner_os === "unknown"
    || typeof host.runner_arch !== "string" || host.runner_arch.length === 0 || host.runner_arch === "unknown"
    || typeof host.runner_environment !== "string" || host.runner_environment.length === 0 || host.runner_environment === "unknown"
    || !isObject(host.image) || typeof host.image.os !== "string" || host.image.os.length === 0 || host.image.os === "unknown"
    || typeof host.image.version !== "string" || host.image.version.length === 0 || host.image.version === "unknown"
    || typeof host.node !== "string" || host.node.length === 0
    || !isObject(host.hardware) || typeof host.hardware.arch !== "string" || host.hardware.arch.length === 0
    || !Number.isInteger(host.hardware.cpu_count) || host.hardware.cpu_count < 1
    || !["LE", "BE"].includes(host.hardware.endianness)
    || host.wasi !== "preview1" || host.captured !== true) {
    errors.push("provenance.runtime.host");
  }

  const dependencies = runtime?.dependencies;
  if (!isObject(dependencies)
    || !validFileReference(dependencies.compiler_lockfile)
    || !validFileReference(dependencies.project_manifest)
    || !validFileReference(dependencies.project_lockfile)) {
    errors.push("provenance.runtime.dependencies");
  }

  const scheduler = runtime?.scheduler;
  if (!isObject(scheduler)
    || typeof scheduler.mode !== "string" || scheduler.mode.length === 0
    || typeof scheduler.concurrency !== "string" || scheduler.concurrency.length === 0
    || scheduler.status !== "declared") {
    errors.push("provenance.runtime.scheduler");
  }
  return errors;
}

export function inspectBundle(bundle, file) {
  const missing = requiredPaths.filter((field) => !present(getPath(bundle, field)));
  const runtime = bundle?.provenance?.runtime;
  const missingR3Extensions = [];
  const incomplete = [];
  missingR3Extensions.push(...captureExtensionErrors(runtime));
  const effects = bundle?.provenance?.effects;
  let entryEffectSignature = null;
  if (!present(effects)) {
    missingR3Extensions.push("provenance.effects");
  } else {
    const effectErrors = validateEffectCapture(effects);
    const symbolicCallCount = effects.effect_coverage?.symbolic_calls;
    const symbolicCalls = effects.symbolic_effect_calls;
    const hasSymbolicCalls = symbolicCalls !== undefined;
    const symbolicCallsMalformed = hasSymbolicCalls && (!isObject(symbolicCalls)
      || Object.values(symbolicCalls).some((calls) => !Array.isArray(calls)
        || !calls.every((call) => typeof call === "string")));
    const symbolicCountMalformed = symbolicCallCount !== undefined
      && (!Number.isInteger(symbolicCallCount) || symbolicCallCount < 0);
    if (effectErrors.length > 0 || symbolicCallsMalformed || symbolicCountMalformed) {
      missingR3Extensions.push("provenance.effects.effect_coverage");
    }
    const signatureErrors = validateEntryEffectSignature(
      effects.entry_effect_signature,
      effects.entry,
      effects,
    );
    if (signatureErrors.length > 0) {
      missingR3Extensions.push("provenance.effects.entry_effect_signature");
    } else {
      entryEffectSignature = { ...effects.entry_effect_signature };
      if (!entryEffectSignature.row_closed) incomplete.push("provenance.effects.entry_effect_signature");
    }
  }
  const effectCoverage = effects?.effect_coverage;
  return {
    file,
    id: bundle?.id ?? null,
    currentLevel: bundle?.provenance?.level ?? bundle?.reproducibility?.level ?? null,
    status: missing.length === 0 && missingR3Extensions.length === 0 && incomplete.length === 0
      ? "READY"
      : "OPEN",
    requiredFields: { missing },
    r3Extensions: { missing: missingR3Extensions, incomplete },
    entryEffectSignature,
    effectCoverage: effectCoverage && typeof effectCoverage === "object"
      ? { ...effectCoverage }
      : null,
    entryEffectClosure: diagnoseEntryEffectClosure(effects),
  };
}

export function diagnoseEntryEffectClosure(effects) {
  const entry = typeof effects?.entry === "string" && effects.entry.length > 0
    ? effects.entry
    : null;
  const functions = effects?.functions;
  const unresolvedMap = effects?.unresolved_calls;
  const symbolicMap = effects?.symbolic_effect_calls;
  if (!entry || !isObject(functions) || !Object.hasOwn(functions, entry)
    || !isObject(unresolvedMap) || !isObject(symbolicMap)) {
    return {
      entry,
      status: "unavailable",
      unresolvedCalls: null,
      symbolicCallbacks: null,
    };
  }

  const unresolvedCalls = unresolvedMap[entry] ?? [];
  const symbolicCallbacks = symbolicMap[entry] ?? [];
  if (!Array.isArray(unresolvedCalls) || !unresolvedCalls.every((call) => typeof call === "string")
    || !Array.isArray(symbolicCallbacks) || !symbolicCallbacks.every((call) => typeof call === "string")) {
    return {
      entry,
      status: "unavailable",
      unresolvedCalls: null,
      symbolicCallbacks: null,
    };
  }

  return {
    entry,
    status: unresolvedCalls.length === 0 && symbolicCallbacks.length === 0 ? "closed" : "open",
    unresolvedCalls,
    symbolicCallbacks,
  };
}

function effectOperationSyntax(operation) {
  if (operation.includes("::")) return "qualifiedPath";
  if (operation.includes(".")) return "memberPath";
  if (/^[\p{Alphabetic}_][\p{Alphabetic}\p{Number}_]*$/u.test(operation)) return "identifier";
  return "other";
}

export function diagnoseEffectCalls(effects) {
  const bySyntax = { identifier: 0, memberPath: 0, qualifiedPath: 0, other: 0 };
  const operationReferences = new Map();
  const symbolicCallbacks = new Map();
  const unresolved = isObject(effects?.unresolved_calls) ? effects.unresolved_calls : {};
  const symbolicAvailable = isObject(effects?.symbolic_effect_calls);
  const symbolic = symbolicAvailable ? effects.symbolic_effect_calls : {};
  let referenceCount = 0;
  for (const calls of Object.values(unresolved)) {
    if (!Array.isArray(calls)) continue;
    for (const operation of calls) {
      if (typeof operation !== "string") continue;
      referenceCount++;
      bySyntax[effectOperationSyntax(operation)]++;
      operationReferences.set(operation, (operationReferences.get(operation) ?? 0) + 1);
    }
  }
  let symbolicCallbackReferenceCount = 0;
  for (const [functionName, callbacks] of Object.entries(symbolic)) {
    if (!Array.isArray(callbacks)) continue;
    for (const callback of callbacks) {
      if (typeof callback !== "string") continue;
      symbolicCallbackReferenceCount++;
      const operation = `${functionName}.${callback}`;
      symbolicCallbacks.set(operation, (symbolicCallbacks.get(operation) ?? 0) + 1);
    }
  }
  const functions = effects?.functions;
  const callsByFunction = effects?.function_calls;
  let directlyReachableSymbolicCallbacks = null;
  let directlyReachableFunctionCount = null;
  if (symbolicAvailable && isObject(functions) && isObject(callsByFunction)
    && typeof effects?.entry === "string" && Object.hasOwn(functions, effects.entry)) {
    const reachable = new Set([effects.entry]);
    const pending = [effects.entry];
    let graphRowsComplete = true;
    while (pending.length > 0) {
      const caller = pending.pop();
      const calls = callsByFunction[caller];
      if (!Array.isArray(calls)) {
        graphRowsComplete = false;
        continue;
      }
      for (const target of calls) {
        if (typeof target === "string" && Object.hasOwn(functions, target) && !reachable.has(target)) {
          reachable.add(target);
          pending.push(target);
        }
      }
    }
    if (graphRowsComplete) {
      directlyReachableFunctionCount = reachable.size;
      directlyReachableSymbolicCallbacks = new Map();
      for (const functionName of reachable) {
        const callbacks = symbolic[functionName] ?? [];
        if (!Array.isArray(callbacks)) continue;
        for (const callback of callbacks) {
          if (typeof callback !== "string") continue;
          const operation = `${functionName}.${callback}`;
          directlyReachableSymbolicCallbacks.set(
            operation,
            (directlyReachableSymbolicCallbacks.get(operation) ?? 0) + 1,
          );
        }
      }
    }
  }
  return {
    referenceCount,
    uniqueOperationNames: operationReferences.size,
    bySyntax,
    operations: operationReferences,
    symbolicCallbackReferenceCount,
    uniqueSymbolicCallbacks: symbolicCallbacks.size,
    symbolicInventoryAvailable: symbolicAvailable,
    symbolicCallbacks,
    directlyReachableFunctionCount,
    directlyReachableSymbolicCallbacks,
  };
}

function buildGapDiagnostics(records) {
  const missingFields = new Map();
  const incompleteFields = new Map();
  const effectOperationReferences = new Map();
  const symbolicCallbackReferences = new Map();
  const directlyReachableSymbolicReferences = new Map();
  const effectSyntaxCounts = { identifier: 0, memberPath: 0, qualifiedPath: 0, other: 0 };
  let effectOperationReferenceCount = 0;
  let symbolicCallbackReferenceCount = 0;
  let symbolicInventoryBundleCount = 0;
  let directlyReachableSymbolicReferenceCount = 0;
  let reachableGraphBundleCount = 0;
  const entrypointBundles = [];
  const reproducibility = {
    embeddedInputSnapshotBundles: 0,
    declaredEmbeddedInputSnapshotCount: 0,
    ciVerifiedExecutionBundles: 0,
    recordedVerifiedReplayBundles: 0,
    numericComparisonNotClaimedBundles: 0,
    numericPolicyToleranceMentionBundles: 0,
  };

  for (const { entry, effectCalls, bundle: sourceBundle } of records) {
    const bundle = entry.id ?? entry.file;
    const provenance = sourceBundle?.provenance ?? {};
    const declaredSnapshots = Array.isArray(provenance.runtime?.input_snapshots)
      ? provenance.runtime.input_snapshots.filter((snapshot) =>
        snapshot?.embedded === true && typeof snapshot.path === "string"
          && /^sha256:[a-f0-9]{64}$/.test(snapshot.sha256 ?? ""))
      : [];
    if (declaredSnapshots.length > 0) reproducibility.embeddedInputSnapshotBundles++;
    reproducibility.declaredEmbeddedInputSnapshotCount += declaredSnapshots.length;
    if (provenance.execution?.status === "verified-in-ci") reproducibility.ciVerifiedExecutionBundles++;
    if (provenance.replay?.status === "verified") reproducibility.recordedVerifiedReplayBundles++;
    const numericPolicy = provenance.runtime?.platform_policy?.numeric;
    if (typeof numericPolicy === "string" && /not performed or claimed/i.test(numericPolicy)) {
      reproducibility.numericComparisonNotClaimedBundles++;
    }
    if (typeof numericPolicy === "string" && /tolerances?/i.test(numericPolicy)) {
      reproducibility.numericPolicyToleranceMentionBundles++;
    }
    for (const field of entry.r3Extensions.missing) {
      if (!missingFields.has(field)) missingFields.set(field, new Set());
      missingFields.get(field).add(bundle);
    }
    for (const field of entry.r3Extensions.incomplete) {
      if (!incompleteFields.has(field)) incompleteFields.set(field, new Set());
      incompleteFields.get(field).add(bundle);
    }
    effectOperationReferenceCount += effectCalls.referenceCount;
    symbolicCallbackReferenceCount += effectCalls.symbolicCallbackReferenceCount;
    if (effectCalls.symbolicInventoryAvailable) symbolicInventoryBundleCount++;
    if (effectCalls.directlyReachableSymbolicCallbacks !== null) {
      reachableGraphBundleCount++;
      for (const [operation, count] of effectCalls.directlyReachableSymbolicCallbacks) {
        directlyReachableSymbolicReferenceCount += count;
        if (!directlyReachableSymbolicReferences.has(operation)) {
          directlyReachableSymbolicReferences.set(operation, { references: 0, bundles: new Set() });
        }
        const current = directlyReachableSymbolicReferences.get(operation);
        current.references += count;
        current.bundles.add(bundle);
      }
    }
    entrypointBundles.push({
      bundle,
      ...entry.entryEffectClosure,
    });
    for (const [syntax, count] of Object.entries(effectCalls.bySyntax)) {
      effectSyntaxCounts[syntax] += count;
    }
    for (const [operation, count] of effectCalls.operations) {
      if (!effectOperationReferences.has(operation)) {
        effectOperationReferences.set(operation, { references: 0, bundles: new Set() });
      }
      const current = effectOperationReferences.get(operation);
      current.references += count;
      current.bundles.add(bundle);
    }
    for (const [operation, count] of effectCalls.symbolicCallbacks) {
      if (!symbolicCallbackReferences.has(operation)) {
        symbolicCallbackReferences.set(operation, { references: 0, bundles: new Set() });
      }
      const current = symbolicCallbackReferences.get(operation);
      current.references += count;
      current.bundles.add(bundle);
    }
  }

  const summarize = (fields) => [...fields]
    .map(([field, bundles]) => ({ field, bundleCount: bundles.size, bundles: [...bundles].sort() }))
    .sort((left, right) => left.field.localeCompare(right.field));
  const topOperations = [...effectOperationReferences]
    .map(([operation, counts]) => ({
      operation,
      syntax: effectOperationSyntax(operation),
      references: counts.references,
      bundleCount: counts.bundles.size,
    }))
    .sort((left, right) => right.references - left.references || left.operation.localeCompare(right.operation))
    .slice(0, 12);
  const entrypointEffects = {
    countUnit: "one recorded entry function per experiment bundle",
    availableBundleCount: entrypointBundles.filter((item) => item.status !== "unavailable").length,
    closedBundleCount: entrypointBundles.filter((item) => item.status === "closed").length,
    openBundleCount: entrypointBundles.filter((item) => item.status === "open").length,
    unavailableBundleCount: entrypointBundles.filter((item) => item.status === "unavailable").length,
    unresolvedCallReferences: entrypointBundles.reduce((total, item) =>
      total + (item.unresolvedCalls?.length ?? 0), 0),
    symbolicCallbackReferences: entrypointBundles.reduce((total, item) =>
      total + (item.symbolicCallbacks?.length ?? 0), 0),
    bundles: entrypointBundles.sort((left, right) => left.bundle.localeCompare(right.bundle)),
    interpretation: "This reports the recorded entry function row separately from module-wide inventory. It is diagnostic only and does not prove complete runtime reachability, host capture or R3 readiness.",
  };
  const topSymbolicCallbacks = [...symbolicCallbackReferences]
    .map(([operation, counts]) => ({
      operation,
      references: counts.references,
      bundleCount: counts.bundles.size,
    }))
    .sort((left, right) => right.references - left.references || left.operation.localeCompare(right.operation))
    .slice(0, 12);
  const topDirectlyReachableSymbolicCallbacks = [...directlyReachableSymbolicReferences]
    .map(([operation, counts]) => ({
      operation,
      references: counts.references,
      bundleCount: counts.bundles.size,
      bundles: [...counts.bundles].sort(),
    }))
    .sort((left, right) => right.references - left.references || left.operation.localeCompare(right.operation))
    .slice(0, 12);

  return {
    captureGaps: {
      missingExtensionFields: summarize(missingFields),
      incompleteExtensionFields: summarize(incompleteFields),
    },
    effectInventory: {
      countUnit: "recorded function-operation references in unresolved_calls; not source invocation count",
      operationReferences: effectOperationReferenceCount,
      uniqueOperationNames: effectOperationReferences.size,
      bySyntax: effectSyntaxCounts,
      topOperations,
      symbolicInventoryStatus: symbolicInventoryBundleCount === records.length
        ? "available"
        : symbolicInventoryBundleCount === 0 ? "unavailable" : "partial",
      symbolicInventoryBundleCount,
      symbolicInventoryUnavailableBundleCount: records.length - symbolicInventoryBundleCount,
      symbolicCallbackReferences: symbolicInventoryBundleCount === records.length
        ? symbolicCallbackReferenceCount : null,
      uniqueSymbolicCallbacks: symbolicInventoryBundleCount === records.length
        ? symbolicCallbackReferences.size : null,
      observedSymbolicCallbackReferences: symbolicCallbackReferenceCount,
      topSymbolicCallbacks,
      directReachability: {
        method: "follow function_calls edges only when the operation exactly matches a function key",
        status: reachableGraphBundleCount === records.length
          ? "available"
          : reachableGraphBundleCount === 0 ? "unavailable" : "partial",
        availableBundleCount: reachableGraphBundleCount,
        unavailableBundleCount: records.length - reachableGraphBundleCount,
        symbolicCallbackReferences: reachableGraphBundleCount === records.length
          ? directlyReachableSymbolicReferenceCount
          : null,
        uniqueSymbolicCallbacks: reachableGraphBundleCount === records.length
          ? directlyReachableSymbolicReferences.size
          : null,
        topSymbolicCallbacks: topDirectlyReachableSymbolicCallbacks,
        interpretation: "A missing or dynamic call edge can make this direct graph incomplete. Top callbacks summarize available graphs only; numeric totals are null unless every bundle has graph evidence. This is diagnostic, never a substitute for the conservative entry effect row or R3 gate.",
      },
      interpretation: "Syntax groups are descriptive only. An identifier may be a builtin, local function or callback; a member path may be a method, field or host API. This report does not determine type resolution or prove purity.",
    },
    entrypointEffects,
    reproducibilityEvidence: {
      ...reproducibility,
      interpretation: "Counts reflect declarations in the local bundle files. Embedded snapshot declarations do not establish completeness of external inputs; verified-in-ci labels and recorded replay statuses are not re-executed by this diagnostic; mentioning numeric tolerances does not mean they were assessed or scientifically justified.",
    },
    notAssessed: [
      "Whether an operation resolves to a typed function or intrinsic",
      "Whether external inputs are fully captured and privacy-safe",
      "Whether numerical tolerances are scientifically justified",
    ],
  };
}

export function buildReadiness(root = repositoryRoot) {
  const directory = path.join(root, "website/assets/experiments");
  const files = readdirSync(directory).filter((file) => file.endsWith(".ostrin-experiment.json")).sort();
  const records = files.map((file) => {
    const absolute = path.join(directory, file);
    const bundle = JSON.parse(readFileSync(absolute, "utf8"));
    const entry = inspectBundle(bundle, path.relative(root, absolute).replaceAll("\\", "/"));
    return { entry, effectCalls: diagnoseEffectCalls(bundle?.provenance?.effects), bundle };
  });
  const entries = records.map(({ entry }) => entry);
  return {
    schema: "ostrin.r3-readiness/v1",
    status: entries.every((entry) => entry.status === "READY") ? "READY" : "OPEN",
    requiredFields: requiredPaths,
    bundleCount: entries.length,
    baseReadyCount: entries.filter((entry) => entry.requiredFields.missing.length === 0).length,
    extensionReadyCount: entries.filter((entry) =>
      entry.r3Extensions.missing.length === 0 && entry.r3Extensions.incomplete.length === 0).length,
    readyCount: entries.filter((entry) => entry.status === "READY").length,
    openCount: entries.filter((entry) => entry.status !== "READY").length,
    captureReadyCount: entries.filter((entry) =>
      entry.requiredFields.missing.length === 0 && entry.r3Extensions.missing.length === 0).length,
    captureOpenCount: entries.filter((entry) =>
      entry.requiredFields.missing.length > 0 || entry.r3Extensions.missing.length > 0).length,
    incompleteEffectBundleCount: entries.filter((entry) => entry.r3Extensions.incomplete.includes("provenance.effects.entry_effect_signature")).length,
    closedEntryEffectBundleCount: entries.filter((entry) => entry.entryEffectSignature?.row_closed === true).length,
    knownEffectCallCount: entries.reduce((total, entry) => total + (entry.effectCoverage?.known_calls ?? 0), 0),
    symbolicEffectCallCount: entries.reduce((total, entry) => total + (entry.effectCoverage?.symbolic_calls ?? 0), 0),
    unresolvedEffectCallCount: entries.reduce((total, entry) => total + (entry.effectCoverage?.unresolved_calls ?? 0), 0),
    gapDiagnostics: buildGapDiagnostics(records),
    effectCoverageCountUnit: "distinct_function_operation_edges",
    entries,
  };
}

export function requiredCaptureErrors(report) {
  return report.entries.flatMap((entry) => {
    const errors = [];
    if (entry.requiredFields.missing.length > 0) {
      errors.push(`${entry.file}: missing required fields: ${entry.requiredFields.missing.join(", ")}`);
    }
    if (entry.r3Extensions.missing.length > 0) {
      errors.push(`${entry.file}: missing or invalid capture extensions: ${entry.r3Extensions.missing.join(", ")}`);
    }
    return errors;
  });
}

async function main() {
  const report = buildReadiness();
  if (process.argv.includes("--write")) writeFileSync(outputPath, `${JSON.stringify(report, null, 2)}\n`);
  process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
  if (process.argv.includes("--require-ready") && report.status !== "READY") {
    process.exitCode = 1;
  }
  if (process.argv.includes("--require-capture")) {
    const errors = requiredCaptureErrors(report);
    if (errors.length > 0) {
      console.error(`r3-readiness: capture contract failed (${errors.length} errors)`);
      for (const error of errors) console.error(`- ${error}`);
      process.exitCode = 1;
    }
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await main();
