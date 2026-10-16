#!/usr/bin/env node

import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { validateEffectCapture } from "./experiment-bundle.mjs";

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
  if (!validFileReference(runtime?.compiler_lockfile)) {
    errors.push("provenance.runtime.compiler_lockfile");
  }
  const host = runtime?.host;
  if (!isObject(host)
    || host.schema !== "ostrin.host/v1"
    || typeof host.source !== "string" || host.source.length === 0
    || typeof host.runner_os !== "string" || host.runner_os === "unknown"
    || typeof host.runner_arch !== "string" || host.runner_arch === "unknown"
    || typeof host.runner_environment !== "string" || host.runner_environment === "unknown"
    || !isObject(host.image) || typeof host.image.os !== "string" || host.image.os === "unknown"
    || typeof host.image.version !== "string" || host.image.version === "unknown"
    || typeof host.node !== "string" || host.node.length === 0
    || !isObject(host.hardware) || typeof host.hardware.arch !== "string"
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
    } else if (!effects.effect_coverage.complete
      || (symbolicCallCount ?? 0) > 0
      || Object.values(symbolicCalls ?? {}).some((calls) => calls.length > 0)) {
      incomplete.push("provenance.effects.effect_coverage");
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
    effectCoverage: effectCoverage && typeof effectCoverage === "object"
      ? { ...effectCoverage }
      : null,
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
  const unresolved = isObject(effects?.unresolved_calls) ? effects.unresolved_calls : {};
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
  return {
    referenceCount,
    uniqueOperationNames: operationReferences.size,
    bySyntax,
    operations: operationReferences,
  };
}

function buildGapDiagnostics(records) {
  const missingFields = new Map();
  const incompleteFields = new Map();
  const effectOperationReferences = new Map();
  const effectSyntaxCounts = { identifier: 0, memberPath: 0, qualifiedPath: 0, other: 0 };
  let effectOperationReferenceCount = 0;

  for (const { entry, effectCalls } of records) {
    const bundle = entry.id ?? entry.file;
    for (const field of entry.r3Extensions.missing) {
      if (!missingFields.has(field)) missingFields.set(field, new Set());
      missingFields.get(field).add(bundle);
    }
    for (const field of entry.r3Extensions.incomplete) {
      if (!incompleteFields.has(field)) incompleteFields.set(field, new Set());
      incompleteFields.get(field).add(bundle);
    }
    effectOperationReferenceCount += effectCalls.referenceCount;
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
      interpretation: "Syntax groups are descriptive only. An identifier may be a builtin, local function or callback; a member path may be a method, field or host API. This report does not determine type resolution or prove purity.",
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
    return { entry, effectCalls: diagnoseEffectCalls(bundle?.provenance?.effects) };
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
    incompleteEffectBundleCount: entries.filter((entry) => entry.r3Extensions.incomplete.includes("provenance.effects.effect_coverage")).length,
    unresolvedEffectCallCount: entries.reduce((total, entry) => total + (entry.effectCoverage?.unresolved_calls ?? 0), 0),
    gapDiagnostics: buildGapDiagnostics(records),
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
