import test from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import {
  canonicalJson,
  createR3WasiHostReplayReceipt,
  r3WasiHostReplayArtifactPaths,
  sameR3WasiHostReplayReceipt,
  validateR3WasiHostReplayReceipt,
} from "./r3-replay-profile.mjs";
import { buildExperimentBundle } from "./experiment-bundle.mjs";

function digest(bytes) {
  return `sha256:${createHash("sha256").update(bytes).digest("hex")}`;
}

function makeProfileFixture() {
  const runId = "1234567890";
  const testedSha = "a".repeat(40);
  const wasmSha256 = `sha256:${"b".repeat(64)}`;
  const ciRun = {
    schema: "ostrin.ci-run/v1",
    repository: "sircalch/Ostrin",
    workflow: "Deploy Ostrin website",
    run_id: runId,
    run_attempt: "1",
    event: "push",
    ref: "refs/heads/main",
    head_sha: testedSha,
    tested_sha: testedSha,
    url: `https://github.com/sircalch/Ostrin/actions/runs/${runId}`,
  };
  const bundle = buildExperimentBundle("surface");
  bundle.provenance.runtime.host = {
    schema: "ostrin.host/v1",
    source: "github-actions",
    ci_run: ciRun,
    runner_os: "Linux",
    runner_arch: "X64",
    runner_environment: "github-hosted",
    image: { os: "ubuntu", version: "24.04" },
    node: "v24.0.0",
    hardware: { arch: "x64", cpu_count: 2, endianness: "LE" },
    wasi: "preview1",
    captured: true,
  };
  const provenanceText = canonicalJson(bundle.provenance);
  bundle.files["provenance.json"] = provenanceText;
  const provenanceEntry = bundle.manifest.files.find((entry) => entry.path === "provenance.json");
  provenanceEntry.bytes = Buffer.byteLength(provenanceText, "utf8");
  provenanceEntry.sha256 = digest(Buffer.from(provenanceText, "utf8"));
  const bundleText = canonicalJson(bundle);
  const bundleSha256 = digest(Buffer.from(bundleText, "utf8"));
  const replayBundles = { surface: { level: "R2", bundleSha256 } };
  const result = {
    id: "surface",
    baseline: "match",
    sensitivity: "changed-output",
    bundleSha256,
    wasmSha256,
  };
  const readiness = {
    status: "READY",
    inventory: { status: "READY", expectedCount: 1, discoveredCount: 1, missing: [], unexpected: [] },
    entries: [{ id: "surface", status: "READY" }],
    bundleCount: 1,
    readyCount: 1,
    captureReadyCount: 1,
    openCount: 0,
    captureOpenCount: 0,
    closedEntryEffectBundleCount: 1,
    knownEffectCallCount: 20,
    symbolicEffectCallCount: 0,
    unresolvedEffectCallCount: 0,
  };
  const scorecard = {
    reproducibility: { R0: 0, R1: 0, R2: 1, R3: 0 },
    r3: {
      status: "READY",
      bundleCount: 1,
      readyCount: 1,
      captureReadyCount: 1,
      openCount: 0,
      captureOpenCount: 0,
    },
    developmentEvidence: {
      schema: "ostrin.ci-evidence/v1",
      repository: ciRun.repository,
      workflow: ciRun.workflow,
      runId,
      runAttempt: ciRun.run_attempt,
      headSha: ciRun.head_sha,
      testedSha,
      ref: ciRun.ref,
      runUrl: ciRun.url,
    },
  };
  const fileReadEvidence = {
    ciRun: { run_id: runId, tested_sha: testedSha },
    compilerWasmSha256: wasmSha256,
    replayBundles: { surface: { level: "R2", bundleSha256 } },
  };
  const randomEvidence = structuredClone(fileReadEvidence);
  const surfaceReceipt = {
    schema: "ostrin.r3-replay-receipt/v1",
    fixture: "surface",
    ciRun,
    compilerWasmSha256: wasmSha256,
    replayBundles,
  };
  const files = Object.fromEntries(r3WasiHostReplayArtifactPaths.map((relativePath) => {
    let contents;
    if (relativePath === "validation/r3-readiness.json") contents = canonicalJson(readiness);
    else if (relativePath === "validation/quality-scorecard.json") contents = canonicalJson(scorecard);
    else if (relativePath === "website/status-data.js") contents = "export const status = {};\n";
    else if (relativePath === "target/r3-ci/surface-replay-receipt.json") contents = canonicalJson(surfaceReceipt);
    else if (relativePath === "target/r3-ci/wasi-file-read-evidence.json") contents = canonicalJson(fileReadEvidence);
    else contents = canonicalJson(randomEvidence);
    return [relativePath, { bytes: Buffer.from(contents, "utf8") }];
  }));
  return {
    runId,
    testedSha,
    replayBundles,
    replayResults: [result],
    bundleRecords: [{ id: "surface", level: "R2", bundle, bundleText, result }],
    files,
    wasmSha256,
  };
}

test("bounded R3 host replay receipt binds a complete run without changing fixture maturity", () => {
  const fixture = makeProfileFixture();
  const receipt = createR3WasiHostReplayReceipt(fixture);

  assert.equal(receipt.status, "VERIFIED");
  assert.equal(receipt.profile.id, "wasi-preview1-host-bound-replay");
  assert.equal(receipt.ciRun.run_id, fixture.runId);
  assert.equal(receipt.compiler.sourceRevision, fixture.testedSha);
  assert.equal(receipt.fixtureMaturity.R2, 1);
  assert.equal(receipt.fixtureMaturity.R3, 0);
  assert.equal(receipt.bundles.surface.replay.sensitivity, "changed-output");
  assert.deepEqual(validateR3WasiHostReplayReceipt(receipt, fixture), []);
  assert.ok(receipt.profile.limitations.some((item) => item.includes("cross-platform")));
});

test("bounded R3 host replay receipt rejects an incomplete or misbound inventory", () => {
  const fixture = makeProfileFixture();
  assert.throws(() => createR3WasiHostReplayReceipt({
    ...fixture,
    replayResults: [],
  }), /complete registered inventory/);

  const mismatched = makeProfileFixture();
  mismatched.replayResults[0] = { ...mismatched.replayResults[0], wasmSha256: `sha256:${"c".repeat(64)}` };
  assert.throws(() => createR3WasiHostReplayReceipt(mismatched), /different compiler WASM/);

  const promoted = createR3WasiHostReplayReceipt(fixture);
  promoted.fixtureMaturity.R3 = 1;
  assert.match(validateR3WasiHostReplayReceipt(promoted).join("\n"), /must remain R3=0/);
});

test("offline receipt comparison rejects a changed archived replay claim", () => {
  const fixture = makeProfileFixture();
  const receipt = createR3WasiHostReplayReceipt(fixture);
  const receiptBytes = Buffer.from(canonicalJson(receipt), "utf8");
  assert.doesNotThrow(() => sameR3WasiHostReplayReceipt(receiptBytes, receipt));

  const changed = structuredClone(receipt);
  changed.bundles.surface.replay.sensitivity = "not-verified";
  assert.throws(
    () => sameR3WasiHostReplayReceipt(Buffer.from(canonicalJson(changed), "utf8"), receipt),
    /replay result does not match its recorded level/,
  );
});
