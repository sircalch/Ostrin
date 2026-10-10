#!/usr/bin/env node

import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

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

export function inspectBundle(bundle, file) {
  const missing = requiredPaths.filter((field) => !present(getPath(bundle, field)));
  const runtime = bundle?.provenance?.runtime;
  const missingR3Extensions = [];
  if (!present(runtime?.host)) missingR3Extensions.push("provenance.runtime.host");
  if (!present(runtime?.dependencies)) missingR3Extensions.push("provenance.runtime.dependencies");
  if (!present(runtime?.scheduler)) missingR3Extensions.push("provenance.runtime.scheduler");
  if (!present(bundle?.provenance?.effects)) missingR3Extensions.push("provenance.effects");
  return {
    file,
    id: bundle?.id ?? null,
    currentLevel: bundle?.provenance?.level ?? bundle?.reproducibility?.level ?? null,
    status: missing.length === 0 && missingR3Extensions.length === 0 ? "READY" : "OPEN",
    requiredFields: { missing },
    r3Extensions: { missing: missingR3Extensions },
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
    extensionReadyCount: entries.filter((entry) => entry.r3Extensions.missing.length === 0).length,
    readyCount: entries.filter((entry) => entry.status === "READY").length,
    openCount: entries.filter((entry) => entry.status !== "READY").length,
    entries,
  };
}

async function main() {
  const report = buildReadiness();
  if (process.argv.includes("--write")) writeFileSync(outputPath, `${JSON.stringify(report, null, 2)}\n`);
  process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
  if (process.argv.includes("--require-ready") && report.status !== "READY") {
    process.exitCode = 1;
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await main();
