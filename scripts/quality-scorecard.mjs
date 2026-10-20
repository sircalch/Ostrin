#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { repositoryRoot } from "./experiment-bundle.mjs";
import { buildReadiness as buildR3Readiness } from "./r3-readiness.mjs";
import { readinessPath } from "./v0.2-readiness-check.mjs";
import { canonicalTextSha256, checkTolerance, directDft } from "./numeric-validation.mjs";

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
    || normalized === "validation/release-candidate-v0.2.0.json"
    || normalized === "docs/release-readiness-v0.2.json"
    || normalized === "docs/review-map-v0.2.md"
    || normalized === "validation/numeric/windows-local.json"
    || (normalized.startsWith("website/assets/experiments/") && normalized.endsWith(".ostrin-experiment.json"));
};

export function hasDirtySourceChanges(changedPaths) {
  return changedPaths.some((filePath) => !generatedEvidencePath(filePath));
}

export function buildDevelopmentEvidence(environment = process.env) {
  if (environment.OSTRIN_CAPTURE_HOST !== "1") return null;

  const repository = environment.GITHUB_REPOSITORY ?? "";
  const runId = environment.GITHUB_RUN_ID ?? "";
  const testedSha = environment.GITHUB_SHA ?? "";
  const headSha = environment.OSTRIN_EVIDENCE_HEAD_SHA ?? testedSha;
  const ref = environment.GITHUB_REF ?? "";
  const workflow = environment.GITHUB_WORKFLOW ?? "";
  const server = environment.GITHUB_SERVER_URL ?? "https://github.com";
  if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repository)
    || !/^\d+$/.test(runId)
    || !/^[a-f0-9]{40}$/.test(testedSha)
    || !/^[a-f0-9]{40}$/.test(headSha)
    || !/^refs\/.+/.test(ref)
    || workflow.trim().length === 0) return null;

  let serverUrl;
  try {
    serverUrl = new URL(server);
  } catch {
    return null;
  }
  if (serverUrl.protocol !== "https:" || serverUrl.username || serverUrl.password
    || serverUrl.search || serverUrl.hash) return null;

  const runUrl = `${serverUrl.origin}/${repository}/actions/runs/${runId}`;
  const artifactUrl = environment.OSTRIN_EVIDENCE_ARTIFACT_URL ?? null;
  if (artifactUrl !== null) {
    let parsedArtifactUrl;
    try {
      parsedArtifactUrl = new URL(artifactUrl);
    } catch {
      return null;
    }
    const expectedArtifactPath = `/${repository}/actions/runs/${runId}/artifacts/`;
    const artifactId = parsedArtifactUrl.pathname.startsWith(expectedArtifactPath)
      ? parsedArtifactUrl.pathname.slice(expectedArtifactPath.length)
      : "";
    if (parsedArtifactUrl.origin !== serverUrl.origin
      || parsedArtifactUrl.username || parsedArtifactUrl.password
      || parsedArtifactUrl.search || parsedArtifactUrl.hash
      || !/^\d+$/.test(artifactId)
      || parsedArtifactUrl.href !== artifactUrl) return null;
  }
  return {
    schema: "ostrin.ci-evidence/v1",
    repository,
    workflow,
    runId,
    ref,
    headSha,
    testedSha,
    runUrl,
    artifactUrl,
    artifactName: `ostrin-r3-host-evidence-${runId}`,
  };
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

function countNumericLeaves(value) {
  if (typeof value === "number") return Number.isFinite(value) ? 1 : 0;
  if (Array.isArray(value)) return value.reduce((total, item) => total + countNumericLeaves(item), 0);
  if (value && typeof value === "object") {
    return Object.values(value).reduce((total, item) => total + countNumericLeaves(item), 0);
  }
  return 0;
}

function hasExactKeys(value, names) {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    && Object.keys(value).length === names.length
    && names.every((name) => Object.hasOwn(value, name));
}

function finiteArray(value, length) {
  return Array.isArray(value) && value.length === length
    && value.every((item) => typeof item === "number" && Number.isFinite(item));
}

function completeResidual(value, testCase, field) {
  if (!hasExactKeys(value, ["value", "limit", "scale"])) return false;
  if (![value.value, value.limit, value.scale].every((item) => typeof item === "number" && Number.isFinite(item) && item >= 0)) return false;
  const expectedLimit = field === "relativeBackwardError"
    ? testCase.relative
    : testCase.absolute + testCase.relative * value.scale;
  return value.limit === expectedLimit && value.value <= expectedLimit;
}

const matrixSummaryFields = Object.freeze({
  lu: ["solution", "absoluteResidual", "relativeBackwardError", "maximumForwardError", "relativeFactorizationResidual", "triangularStructureResidual"],
  cholesky: ["solution", "absoluteResidual", "relativeBackwardError", "maximumForwardError", "relativeFactorizationResidual", "triangularStructureResidual"],
  qr: ["relativeReconstructionResidual", "orthogonalityResidual", "triangularStructureResidual"],
  svd: ["relativeReconstructionResidual", "uOrthogonalityResidual", "vOrthogonalityResidual", "singularValues"],
  eigen: ["relativeEigenResidual", "orthogonalityResidual", "eigenvalues"],
});

function completeMatrixSummary(name, summary, testCase) {
  const fields = matrixSummaryFields[name];
  if (!fields || !hasExactKeys(summary, fields)) return false;
  for (const field of fields) {
    if (field === "solution") {
      if (!finiteArray(summary.solution, testCase.solution?.length)) return false;
    } else if (field === "singularValues") {
      if (!finiteArray(summary.singularValues, testCase.singular_values?.length)) return false;
    } else if (field === "eigenvalues") {
      if (!finiteArray(summary.eigenvalues, testCase.eigenvalues?.length)) return false;
    } else if (field === "maximumForwardError") {
      if (typeof summary.maximumForwardError !== "number" || !Number.isFinite(summary.maximumForwardError)
        || summary.maximumForwardError < 0) return false;
    } else if (!completeResidual(summary[field], testCase, field)) return false;
  }
  return true;
}

export function hasCompleteNumericRuns(report, contract) {
  const cases = contract?.cases;
  const caseNames = [...Object.keys(matrixSummaryFields), "fft"];
  if (!hasExactKeys(cases, caseNames)
    || !Number.isSafeInteger(contract.expected_report_values) || contract.expected_report_values <= 0
    || !Number.isSafeInteger(contract.expected_emitted_values) || contract.expected_emitted_values <= 0
    || report.parity?.comparedValues !== contract.expected_report_values
    || report.emittedOutputParity?.comparedValues !== contract.expected_emitted_values) return false;
  return ["interpreter", "native"].every((backend) => {
    const run = report.runs?.[backend];
    return hasExactKeys(run, caseNames)
      && Object.keys(matrixSummaryFields).every((name) => completeMatrixSummary(name, run[name], cases[name]))
      && hasCompleteFftCases(run.fft, cases.fft)
      && countNumericLeaves(run) === contract.expected_report_values;
  });
}

export function hasMatchingNumericCi(report, currentRevision) {
  return report.ci?.provider === "github-actions"
    && report.ci.commit === currentRevision
    && /^[0-9a-f]{40}$/.test(currentRevision)
    && /^\d+$/.test(String(report.ci.run_id ?? ""))
    && report.ci.run_url === `https://github.com/sircalch/Ostrin/actions/runs/${report.ci.run_id}`;
}

export function hasMatchingNumericInputs(report, root = repositoryRoot) {
  const contractFile = path.join(root, "validation/numeric/matrix_algorithms.json");
  const fixtureFile = path.join(root, "validation/numeric/matrix_algorithms.ostrin");
  return report.inputs?.contractSha256 === canonicalTextSha256(readFileSync(contractFile, "utf8"))
    && report.inputs?.fixtureSha256 === canonicalTextSha256(readFileSync(fixtureFile, "utf8"))
    && /^[0-9a-f]{64}$/.test(report.inputs?.compilerSha256 ?? "");
}

export function hasCompleteFftCases(fftReport, fftContract) {
  const cases = fftContract?.cases ?? {};
  const names = Object.keys(cases);
  if (!fftReport || names.length === 0 || Object.keys(fftReport).length !== names.length) return false;
  try {
    return names.every((name) => {
      const testCase = cases[name];
      const result = fftReport[name];
      if (!hasExactKeys(result, ["spectralResidual", "maximumReconstructionError", "parsevalResidual", "inputEnergy", "spectralEnergy"])
        || !Array.isArray(testCase.signal) || testCase.signal.length === 0) return false;
      const reference = directDft(testCase.signal);
      const spectralScale = Math.max(0, ...reference.re.map(Math.abs), ...reference.im.map(Math.abs));
      const reconstructionScale = Math.max(0, ...testCase.signal.map(Math.abs));
      const inputEnergy = testCase.signal.reduce((sum, value) => sum + value ** 2, 0);
      if (result.inputEnergy !== inputEnergy
        || !Number.isFinite(result.spectralEnergy) || result.spectralEnergy < 0
        || result.parsevalResidual !== Math.abs(result.spectralEnergy / testCase.signal.length - inputEnergy)) return false;
      checkTolerance(`${name} spectral summary`, result.spectralResidual,
        spectralScale, testCase.absolute, testCase.relative);
      checkTolerance(`${name} reconstruction summary`, result.maximumReconstructionError,
        reconstructionScale, testCase.absolute, testCase.relative);
      checkTolerance(`${name} Parseval summary`, result.parsevalResidual,
        inputEnergy, testCase.absolute, testCase.relative);
      return true;
    });
  } catch {
    return false;
  }
}

function numericalValidationStatus(currentRevision) {
  const reportPath = path.join(repositoryRoot, "validation/numeric/windows-local.json");
  if (!existsSync(reportPath)) return { local: "missing", ci: "pending" };
  const report = JSON.parse(readFileSync(reportPath, "utf8"));
  const contract = JSON.parse(text("validation/numeric/matrix_algorithms.json"));
  const expectedFftCases = Object.keys(contract.cases?.fft?.cases ?? {});
  const interpreterFft = report.runs?.interpreter?.fft ?? {};
  const nativeFft = report.runs?.native?.fft ?? {};
  const fftCaseNames = Object.keys(interpreterFft);
  const fftCasesComplete = hasCompleteFftCases(interpreterFft, contract.cases?.fft)
    && hasCompleteFftCases(nativeFft, contract.cases?.fft);
  const inputIntegrity = hasMatchingNumericInputs(report);
  const comparedValues = report.parity?.comparedValues ?? 0;
  const emittedValuesCompared = report.emittedOutputParity?.comparedValues ?? 0;
  const localPass = report.leakCheck === "live_allocations=0"
    && hasCompleteNumericRuns(report, contract)
    && report.parity?.maximumAbsoluteDifference === 0
    && report.emittedOutputParity?.maximumAbsoluteDifference === 0
    && inputIntegrity
    && fftCasesComplete;
  const ciMatchesEvidence = localPass && hasMatchingNumericCi(report, currentRevision);
  return {
    local: localPass ? "verified" : "invalid",
    platform: report.environment?.platform ?? "unknown",
    comparedValues: report.parity?.comparedValues ?? 0,
    emittedValuesCompared,
    fftCases: fftCasesComplete ? expectedFftCases.length : fftCaseNames.length,
    fftReference: contract.cases?.fft?.reference ?? "not declared",
    inputIntegrity: inputIntegrity ? "verified" : "invalid",
    maximumAbsoluteDifference: report.parity?.maximumAbsoluteDifference ?? null,
    leakCheck: report.leakCheck ?? "missing",
    ci: ciMatchesEvidence ? "verified" : "unattributed",
    ciRunUrl: ciMatchesEvidence ? report.ci.run_url : null,
  };
}

function effectCoverageStatus() {
  const report = buildR3Readiness();
  return {
    knownCalls: report.knownEffectCallCount,
    symbolicCalls: report.symbolicEffectCallCount,
    unresolvedCalls: report.unresolvedEffectCallCount,
    countUnit: report.effectCoverageCountUnit,
    aggregation: "sum_of_per_bundle_distinct_function_operation_edges",
    bundleCount: report.bundleCount,
    scope: `registered experiment bundles (${report.bundleCount})`,
    complete: report.unresolvedEffectCallCount === 0,
  };
}

export async function buildScorecard() {
  const native = nativeCounts();
  const candidateEvidence = JSON.parse(text("validation/release-candidate-v0.2.0.json"));
  // Website CI intentionally rewrites these evidence outputs before publishing
  // host-bound R3 data. They are not source edits and must not make the public
  // source-integrity indicator permanently dirty.
  const workingTreeDirty = hasDirtySourceChanges(workingTreeSourcePaths());
  return {
    schema: "ostrin.quality-scorecard/v1",
    metricsScope: "current-repository-evidence",
    sourceRevision: candidateEvidence.sourceRevision,
    sourceRevisionScope: "v0.2-release-candidate-artifacts",
    evidenceRevision: candidateEvidence.evidenceRevision,
    releaseTag: candidateEvidence.releaseTag,
    finalReleaseSourceRevision: candidateEvidence.finalReleaseSourceRevision,
    developmentEvidence: buildDevelopmentEvidence(),
    workingTreeDirty,
    sources: {
      native: "compiler/tests/differential.rs",
      ownership: "docs/ownership-matrix.md",
      readiness: readinessPath,
      reproducibility: "scripts/experiment-bundle.mjs",
      numericalValidation: "validation/numeric/windows-local.json",
      tests: "compiler/tests",
    },
    tests: { rustMarkedTests: countTests() },
    nativeBackend: native,
    ownership: ownershipCounts(),
    numericalValidation: numericalValidationStatus(git(["rev-parse", "HEAD"])),
    effectCoverage: effectCoverageStatus(),
    reproducibility: await reproducibilityCounts(),
    r3: (() => {
      const report = buildR3Readiness();
      return {
        status: report.status,
        bundleCount: report.bundleCount,
        baseReadyCount: report.baseReadyCount,
        extensionReadyCount: report.extensionReadyCount,
        readyCount: report.readyCount,
        openCount: report.openCount,
        captureReadyCount: report.captureReadyCount,
        captureOpenCount: report.captureOpenCount,
        incompleteEffectBundleCount: report.incompleteEffectBundleCount,
        closedEntryEffectBundleCount: report.closedEntryEffectBundleCount,
        symbolicEffectCallCount: report.symbolicEffectCallCount,
        unresolvedEffectCallCount: report.unresolvedEffectCallCount,
      };
    })(),
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
