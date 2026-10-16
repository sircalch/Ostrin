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
  let entryEffectSignature = null;
  if (!present(effects)) {
    missingR3Extensions.push("provenance.effects");
  } else {
    const effectErrors = validateEffectCapture(effects);
    if (effectErrors.length > 0) {
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
  };
}

export function buildReadiness(root = repositoryRoot) {
  const directory = path.join(root, "website/assets/experiments");
  const files = readdirSync(directory).filter((file) => file.endsWith(".ostrin-experiment.json")).sort();
  const entries = files.map((file) => {
    const absolute = path.join(directory, file);
    return inspectBundle(JSON.parse(readFileSync(absolute, "utf8")), path.relative(root, absolute).replaceAll("\\", "/"));
  });
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
