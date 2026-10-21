import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { buildDevelopmentEvidence, buildScorecard, hasCompleteFftCases, hasCompleteNumericRuns, hasDirtySourceChanges, hasMatchingNumericCi, hasMatchingNumericInputs } from "./quality-scorecard.mjs";

const contract = JSON.parse(readFileSync(new URL("../validation/numeric/matrix_algorithms.json", import.meta.url), "utf8"));
const releaseEvidence = JSON.parse(readFileSync(new URL("../validation/release-candidate-v0.2.0.json", import.meta.url), "utf8"));
const numericReport = JSON.parse(readFileSync(new URL("../validation/numeric/windows-local.json", import.meta.url), "utf8"));

test("FFT scorecard requires each residual within its declared tolerance", () => {
  const fft = structuredClone(numericReport.runs.interpreter.fft);
  assert.equal(hasCompleteFftCases(fft, contract.cases.fft), true);
  fft.radix2_n8.spectralResidual = 1e9;
  assert.equal(hasCompleteFftCases(fft, contract.cases.fft), false);
  fft.radix2_n8.spectralResidual = numericReport.runs.interpreter.fft.radix2_n8.spectralResidual;
  fft.radix2_n8.spectralEnergy += 1;
  assert.equal(hasCompleteFftCases(fft, contract.cases.fft), false);
});

test("numeric scorecard ties the local report to the current contract and fixture", () => {
  assert.equal(hasMatchingNumericInputs(numericReport), true);
  const altered = structuredClone(numericReport);
  altered.inputs.fixtureSha256 = "0".repeat(64);
  assert.equal(hasMatchingNumericInputs(altered), false);
});

test("numeric scorecard rejects missing matrix sections and truncated output counts", () => {
  assert.equal(hasCompleteNumericRuns(numericReport, contract), true);
  for (const backend of ["interpreter", "native"]) {
    for (const name of ["lu", "cholesky", "qr", "svd", "eigen"]) {
      const altered = structuredClone(numericReport);
      delete altered.runs[backend][name];
      altered.parity.comparedValues -= 1;
      assert.equal(hasCompleteNumericRuns(altered, contract), false, `${backend}.${name}`);
    }
  }
  const missingResidual = structuredClone(numericReport);
  delete missingResidual.runs.native.lu.absoluteResidual;
  assert.equal(hasCompleteNumericRuns(missingResidual, contract), false);
  const outOfTolerance = structuredClone(numericReport);
  outOfTolerance.runs.interpreter.qr.orthogonalityResidual.value = 1e9;
  assert.equal(hasCompleteNumericRuns(outOfTolerance, contract), false);
  const truncatedOutput = structuredClone(numericReport);
  truncatedOutput.emittedOutputParity.comparedValues -= 1;
  assert.equal(hasCompleteNumericRuns(truncatedOutput, contract), false);
});

test("numeric CI attribution uses the current tested revision, never the historical RC revision", () => {
  const currentRevision = "f".repeat(40);
  const attributed = structuredClone(numericReport);
  attributed.ci = {
    provider: "github-actions",
    commit: currentRevision,
    run_id: "39100071045",
    run_url: "https://github.com/sircalch/Ostrin/actions/runs/39100071045",
  };
  assert.equal(hasMatchingNumericCi(attributed, currentRevision), true);
  assert.equal(hasMatchingNumericCi(attributed, "d0abbad0f674317c95c389b953507365f3fd1a59"), false);
  attributed.ci.run_url = "https://github.com/sircalch/Ostrin/actions/runs/other";
  assert.equal(hasMatchingNumericCi(attributed, currentRevision), false);
});

test("generated website evidence does not mark a clean source snapshot dirty", () => {
  assert.equal(hasDirtySourceChanges([
    "validation/r3-readiness.json",
    "validation/quality-scorecard.json",
    "website/status-data.js",
    "website/assets/experiments/lines.ostrin-experiment.json",
  ]), false);
  assert.equal(hasDirtySourceChanges(["website\\assets\\experiments\\lines.ostrin-experiment.json"]), false);
});

test("source edits and unrelated generated files remain visible as dirty", () => {
  assert.equal(hasDirtySourceChanges(["compiler/src/main.rs"]), true);
  assert.equal(hasDirtySourceChanges(["website/assets/viz/lines.svg"]), true);
  assert.equal(hasDirtySourceChanges(["list_phi_probe.ostrin"]), true);
});

test("development scorecard links the exact captured CI run and artifact", () => {
  const evidence = buildDevelopmentEvidence({
    OSTRIN_CAPTURE_HOST: "1",
    GITHUB_REPOSITORY: "sircalch/Ostrin",
    GITHUB_RUN_ID: "39362875573",
    GITHUB_SHA: "69c53dad1ba0e5005ad4bf39ef2f4086682651b4",
    OSTRIN_EVIDENCE_HEAD_SHA: "69c53dad1ba0e5005ad4bf39ef2f4086682651b4",
    GITHUB_REF: "refs/heads/main",
    GITHUB_WORKFLOW: "Deploy Ostrin website",
    GITHUB_SERVER_URL: "https://github.com",
    OSTRIN_EVIDENCE_ARTIFACT_URL: "https://github.com/sircalch/Ostrin/actions/runs/39362875573/artifacts/123456789",
  });
  assert.deepEqual(evidence, {
    schema: "ostrin.ci-evidence/v1",
    repository: "sircalch/Ostrin",
    workflow: "Deploy Ostrin website",
    runId: "39362875573",
    ref: "refs/heads/main",
    headSha: "69c53dad1ba0e5005ad4bf39ef2f4086682651b4",
    testedSha: "69c53dad1ba0e5005ad4bf39ef2f4086682651b4",
    runUrl: "https://github.com/sircalch/Ostrin/actions/runs/39362875573",
    artifactUrl: "https://github.com/sircalch/Ostrin/actions/runs/39362875573/artifacts/123456789",
    artifactName: "ostrin-r3-host-evidence-39362875573",
  });
  assert.equal(buildDevelopmentEvidence({ OSTRIN_CAPTURE_HOST: "0" }), null);
  assert.equal(buildDevelopmentEvidence({
    OSTRIN_CAPTURE_HOST: "1",
    GITHUB_REPOSITORY: "sircalch/Ostrin",
    GITHUB_RUN_ID: "39362875573",
    GITHUB_SHA: "69c53dad1ba0e5005ad4bf39ef2f4086682651b4",
    GITHUB_REF: "refs/heads/main",
    GITHUB_WORKFLOW: "Deploy Ostrin website",
    OSTRIN_EVIDENCE_ARTIFACT_URL: "https://github.com/sircalch/Ostrin/actions/runs/39362875573/artifacts",
  }), null);
  assert.equal(buildDevelopmentEvidence({
    OSTRIN_CAPTURE_HOST: "1",
    GITHUB_REPOSITORY: "sircalch/Ostrin",
    GITHUB_RUN_ID: "39362875573",
    GITHUB_SHA: "invalid",
    GITHUB_REF: "refs/heads/main",
    GITHUB_WORKFLOW: "Deploy Ostrin website",
  }), null);
});

test("scorecard separates closed entry rows from module-wide symbolic effect inventory", async () => {
  const scorecard = await buildScorecard();
  assert.equal(scorecard.metricsScope, "current-repository-evidence");
  assert.equal(scorecard.sourceRevisionScope, "v0.2-release-candidate-artifacts");
  assert.equal(scorecard.sourceRevision, releaseEvidence.sourceRevision);
  assert.equal(scorecard.evidenceRevision, releaseEvidence.evidenceRevision);
  assert.equal(scorecard.finalReleaseSourceRevision, releaseEvidence.finalReleaseSourceRevision);
  assert.equal(scorecard.sources.numericalValidation, "validation/numeric/windows-local.json");
  assert.equal(scorecard.r3.readyCount + scorecard.r3.openCount, scorecard.r3.bundleCount);
  assert.equal(scorecard.r3.status,
    scorecard.r3.readyCount === scorecard.r3.bundleCount ? "READY" : "OPEN");
  assert.equal(scorecard.r3.captureReadyCount + scorecard.r3.captureOpenCount, scorecard.r3.bundleCount);
  assert.equal(scorecard.r3.closedEntryEffectBundleCount, scorecard.r3.bundleCount);
  assert.equal(scorecard.r3.incompleteEffectBundleCount, 0);
  assert.equal(scorecard.r3.unresolvedEffectCallCount, 0);
  assert.ok(scorecard.r3.symbolicEffectCallCount > 0);
  assert.deepEqual(scorecard.r3.sourceReadAudit, {
    sourceBundleCount: scorecard.r3.bundleCount,
    bundlesWithDirectReadFileCalls: 11,
    directReadFileCallCount: 21,
    literalReadPathCallCount: 21,
    dynamicReadCallCount: 0,
    readFileReferenceCount: 0,
    undeclaredLiteralPathCount: 0,
    declaredSnapshotWithoutDirectReadCount: 0,
    sourceReadsWithoutFilesystemCapabilityCount: 0,
    filesystemCapabilitiesWithoutDirectReadCount: 0,
  });
  assert.ok(scorecard.effectCoverage.knownCalls > 0);
  assert.ok(scorecard.effectCoverage.symbolicCalls > 0);
  assert.equal(scorecard.effectCoverage.unresolvedCalls, 0);
  assert.equal(scorecard.effectCoverage.countUnit, "distinct_function_operation_edges");
  assert.equal(scorecard.effectCoverage.aggregation, "sum_of_per_bundle_distinct_function_operation_edges");
  assert.equal(scorecard.effectCoverage.bundleCount, scorecard.r3.bundleCount);
  assert.equal(scorecard.effectCoverage.scope, `registered experiment bundles (${scorecard.r3.bundleCount})`);
  assert.equal(scorecard.effectCoverage.complete, true);
  assert.equal(scorecard.numericalValidation.local, "verified");
  assert.equal(scorecard.numericalValidation.comparedValues, 80);
  assert.equal(scorecard.numericalValidation.emittedValuesCompared, 136);
  assert.equal(scorecard.numericalValidation.fftCases, 4);
  assert.equal(scorecard.numericalValidation.inputIntegrity, "verified");
  assert.match(scorecard.numericalValidation.fftReference, /direct O\(n\^2\) DFT/i);
  assert.equal(scorecard.numericalValidation.ci, "unattributed");
  assert.equal(scorecard.numericalValidation.ciRunUrl, null);
});
