#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { repositoryRoot } from "./experiment-bundle.mjs";
import { buildReadiness as buildR3Readiness } from "./r3-readiness.mjs";
import { readinessPath } from "./v0.2-readiness-check.mjs";

function text(file) {
  return readFileSync(path.join(repositoryRoot, file), "utf8");
}

function git(args) {
  return execFileSync("git", args, { cwd: repositoryRoot, encoding: "utf8" }).trim();
}

const generatedEvidencePath = (filePath) => {
  const normalized = filePath.replaceAll("\\", "/");
  return normalized === "validation/r3-readiness.json"
    || normalized === "validation/quality-scorecard.json"
    || normalized === "website/status-data.js"
    || (normalized.startsWith("website/assets/experiments/") && normalized.endsWith(".ostrin-experiment.json"));
};

export function hasDirtySourceChanges(changedPaths) {
  return changedPaths.some((filePath) => !generatedEvidencePath(filePath));
}

function workingTreeSourcePaths() {
  const tracked = git(["diff", "--name-only", "HEAD"]).split(/\r?\n/).filter(Boolean);
  const untracked = git(["ls-files", "--others", "--exclude-standard"]).split(/\r?\n/).filter(Boolean);
  return [...tracked, ...untracked];
}

function filesUnder(directory) {
  const root = path.join(repositoryRoot, directory);
  const files = [];
  function visit(current) {
    for (const entry of readdirSync(current, { withFileTypes: true })) {
      const fullPath = path.join(current, entry.name);
      if (entry.isDirectory()) visit(fullPath);
      else files.push(fullPath);
    }
  }
  visit(root);
  return files;
}

function countTests() {
  return filesUnder("compiler/tests").filter((file) => statSync(file).isFile()).reduce((total, file) => {
    const source = readFileSync(file, "utf8");
    return total + (source.match(/^\s*#\[test\]/gm) ?? []).length;
  }, 0);
}

function nativeCounts() {
  const source = text("compiler/tests/differential.rs");
  const detail = source.match(/(\d[\d,]*)\s+AST\s+fallbacks?\s+\((\d[\d,]*)\s+HIR\/IR-generated:\s*(\d[\d,]*)\s+IR and\s+(\d[\d,]*)\s+HIR functions\)/);
  const ir = Number((detail?.[3] ?? "0").replaceAll(",", ""));
  const hir = Number((detail?.[4] ?? "0").replaceAll(",", ""));
  const ast = Number(source.match(/const MAX_AST_FALLBACK_FUNCTIONS: usize = (\d+)/)?.[1] ?? 0);
  return { ir, hir, ast, total: ir + hir };
}

function ownershipCounts() {
  const source = text("docs/ownership-matrix.md");
  const rows = source
    .split(/\r?\n/)
    .filter((line) =>
      line.startsWith("| `")
      || line.startsWith("| Record")
      || line.startsWith("| Enum")
      || line.startsWith("| Closure")
      || line.startsWith("| Iterator")
      || line.startsWith("| Agregado")
    );
  const counts = { tested: 0, supported: 0, notApplicable: 0, knownLimitation: 0 };
  for (const row of rows) {
    for (const cell of row.split("|").slice(2, 17)) {
      const value = cell.trim();
      if (value === "T") counts.tested++;
      else if (value === "S") counts.supported++;
      else if (value === "A") counts.notApplicable++;
      else if (value === "K") counts.knownLimitation++;
    }
  }
  return counts;
}

async function reproducibilityCounts() {
  const { experimentFixtures, buildExperimentBundle } = await import("./experiment-bundle.mjs");
  const counts = { R0: 0, R1: 0, R2: 0, R3: 0 };
  for (const id of Object.keys(experimentFixtures)) {
    const level = buildExperimentBundle(id).reproducibility.level;
    if (level in counts) counts[level]++;
  }
  return counts;
}

function readiness() {
  const manifest = JSON.parse(text(readinessPath));
  const criteria = Object.fromEntries(manifest.criteria.map((criterion) => [criterion.id, criterion.status]));
  return { release: manifest.release, status: manifest.status, criteria };
}

function linguistStatus() {
  const usagePath = path.join(repositoryRoot, "validation", "linguist-usage.json");
  if (!existsSync(usagePath)) {
    return { preparation: "READY", usage: "MISSING_SNAPSHOT", indexedPublicFiles: null, threshold: 2000, readyForUpstreamPullRequest: false };
  }
  const usage = JSON.parse(readFileSync(usagePath, "utf8"));
  return {
    preparation: "READY",
    usage: usage.status,
    indexedPublicFiles: usage.indexedPublicFiles,
    threshold: usage.threshold?.requiredFiles ?? 2000,
    readyForUpstreamPullRequest: usage.readyForUpstreamPullRequest === true,
    generatedAt: usage.generatedAt,
    queryUrl: usage.queryUrl,
  };
}

function numericalValidationStatus() {
  const reportPath = path.join(repositoryRoot, "validation/numeric/windows-local.json");
  if (!existsSync(reportPath)) return { local: "missing", ci: "pending" };
  const report = JSON.parse(readFileSync(reportPath, "utf8"));
  const localPass = report.leakCheck === "live_allocations=0"
    && report.parity?.comparedValues === 60
    && report.parity?.maximumAbsoluteDifference === 0;
  return {
    local: localPass ? "verified" : "invalid",
    platform: report.environment?.platform ?? "unknown",
    comparedValues: report.parity?.comparedValues ?? 0,
    maximumAbsoluteDifference: report.parity?.maximumAbsoluteDifference ?? null,
    leakCheck: report.leakCheck ?? "missing",
    ci: report.ci?.run_url ? "verified" : "pending",
    ciRunUrl: report.ci?.run_url ?? null,
  };
}

function effectCoverageStatus(report) {
  return {
    countUnit: report.effectCoverageCountUnit,
    knownCalls: report.knownEffectCallCount,
    symbolicCalls: report.symbolicEffectCallCount,
    unresolvedCalls: report.unresolvedEffectCallCount,
    complete: report.incompleteEffectBundleCount === 0,
    scope: `${report.bundleCount} registered R3 experiment bundles`,
  };
}

export async function buildScorecard() {
  const native = nativeCounts();
  const r3Readiness = buildR3Readiness();
  // Website CI intentionally rewrites these evidence outputs before publishing
  // host-bound R3 data. They are not source edits and must not make the public
  // source-integrity indicator permanently dirty.
  const workingTreeDirty = hasDirtySourceChanges(workingTreeSourcePaths());
  return {
    schema: "ostrin.quality-scorecard/v1",
    sourceRevision: git(["rev-parse", "HEAD"]),
    workingTreeDirty,
    sources: {
      native: "compiler/tests/differential.rs",
      ownership: "docs/ownership-matrix.md",
      readiness: readinessPath,
      reproducibility: "scripts/experiment-bundle.mjs",
      tests: "compiler/tests",
    },
    tests: { rustMarkedTests: countTests() },
    nativeBackend: native,
    ownership: ownershipCounts(),
    numericalValidation: numericalValidationStatus(),
    effectCoverage: effectCoverageStatus(r3Readiness),
    reproducibility: await reproducibilityCounts(),
    r3: {
      status: r3Readiness.status,
      bundleCount: r3Readiness.bundleCount,
      baseReadyCount: r3Readiness.baseReadyCount,
      extensionReadyCount: r3Readiness.extensionReadyCount,
      readyCount: r3Readiness.readyCount,
      openCount: r3Readiness.openCount,
      incompleteEffectBundleCount: r3Readiness.incompleteEffectBundleCount,
      knownEffectCallCount: r3Readiness.knownEffectCallCount,
      symbolicEffectCallCount: r3Readiness.symbolicEffectCallCount,
      unresolvedEffectCallCount: r3Readiness.unresolvedEffectCallCount,
    },
    linguist: linguistStatus(),
    releaseReadiness: readiness(),
    artifacts: {
      numericValidationScript: existsSync(path.join(repositoryRoot, "scripts/numeric-validation.mjs")),
      numericValidationWindowsReport: existsSync(path.join(repositoryRoot, "validation/numeric/windows-local.json")),
      branchHygieneScript: existsSync(path.join(repositoryRoot, "scripts/branch-hygiene-check.mjs")),
      astFallbackLedger: existsSync(path.join(repositoryRoot, "validation/ast-fallback-ledger.json")),
      linguistDocumentation: existsSync(path.join(repositoryRoot, "docs/linguist.md")),
      linguistUsageSnapshot: existsSync(path.join(repositoryRoot, "validation/linguist-usage.json")),
      websiteAudit: existsSync(path.join(repositoryRoot, "docs/website-audit.md")),
      reproductionGuide: existsSync(path.join(repositoryRoot, "README-reproduce.md")),
    },
  };
}

async function main() {
  const scorecard = await buildScorecard();
  const serialized = `${JSON.stringify(scorecard, null, 2)}\n`;
  if (process.argv.includes("--write")) {
    writeFileSync(path.join(repositoryRoot, "validation/quality-scorecard.json"), serialized);
  }
  process.stdout.write(serialized);
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) await main();
