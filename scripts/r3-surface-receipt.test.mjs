import test from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import path from "node:path";
import { replay } from "./experiment-replay.mjs";
import { bundlePath, createSurfaceReceipt, verifySurfaceReceipt } from "./r3-surface-receipt.mjs";
import { buildExperimentBundle, repositoryRoot, verifyExperimentBundle } from "./experiment-bundle.mjs";

const sha256 = (bytes) => `sha256:${createHash("sha256").update(bytes).digest("hex")}`;
const canonicalJson = (value) => `${JSON.stringify(value, null, 2)}\n`;
const expectedRun = { runId: "1234", testedSha: "b".repeat(40) };
const fakeWasm = Buffer.from("test-only WASM bytes");

function capturedSurface() {
  const bundle = JSON.parse(readFileSync(bundlePath, "utf8"));
  bundle.provenance.runtime.host = {
    schema: "ostrin.host/v1",
    source: "github-actions",
    ci_run: {
      schema: "ostrin.ci-run/v1",
      repository: "sircalch/Ostrin",
      workflow: "Ostrin CI",
      run_id: expectedRun.runId,
      run_attempt: "1",
      event: "pull_request",
      ref: "refs/pull/999/merge",
      head_sha: "a".repeat(40),
      tested_sha: expectedRun.testedSha,
      url: `https://github.com/sircalch/Ostrin/actions/runs/${expectedRun.runId}`,
    },
    runner_os: "Linux",
    runner_arch: "X64",
    runner_environment: "github-hosted",
    image: { os: "ubuntu24", version: "20261001.1.0" },
    node: "v24.0.0",
    hardware: { arch: "x64", cpu_count: 2, endianness: "LE" },
    wasi: "preview1",
    captured: true,
  };
  bundle.provenance.runtime.missing = bundle.provenance.runtime.missing
    .filter((item) => item !== "captured host and hardware metadata");
  refreshProvenance(bundle);
  return bundle;
}

function refreshProvenance(bundle) {
  bundle.files["provenance.json"] = canonicalJson(bundle.provenance);
  const record = bundle.manifest.files.find((entry) => entry.path === "provenance.json");
  record.bytes = Buffer.byteLength(bundle.files["provenance.json"], "utf8");
  record.sha256 = sha256(Buffer.from(bundle.files["provenance.json"], "utf8"));
}

function replayProof(bundleText) {
  const bundle = JSON.parse(bundleText);
  return {
    id: "surface",
    baseline: "match",
    sensitivity: "changed-output",
    wasmSha256: sha256(fakeWasm),
    bundleSha256: sha256(Buffer.from(bundleText, "utf8")),
    fileHashes: Object.fromEntries(["source.ostrin", "data.json", "figure.svg", "provenance.json"]
      .map((name) => [name, sha256(Buffer.from(bundle.files[name], "utf8"))])),
  };
}

test("a surface receipt binds one captured run, bundle, entry row and WASM bytes", () => {
  const text = canonicalJson(capturedSurface());
  const receipt = createSurfaceReceipt(text, replayProof(text));
  assert.deepEqual(verifySurfaceReceipt(receipt, text, fakeWasm, expectedRun), []);
  assert.equal(receipt.fixtureSourceRevision, JSON.parse(text).provenance.commit);
  assert.equal(receipt.compilerWasmSha256, sha256(fakeWasm));
});

test("a receipt cannot be issued before a successful replay of the exact bundle", () => {
  const text = canonicalJson(capturedSurface());
  assert.throws(() => createSurfaceReceipt(text, null), /successful bound replay/);
  assert.throws(() => createSurfaceReceipt(text, { ...replayProof(text), bundleSha256: sha256("other") }), /exact bundle/);
  assert.throws(() => createSurfaceReceipt(text, { ...replayProof(text), wasmSha256: undefined }), /successful bound replay/);
});

test("the verifier rejects missing or mutated source, input, output and receipt hashes", () => {
  const text = canonicalJson(capturedSurface());
  const receipt = createSurfaceReceipt(text, replayProof(text));
  assert.match(verifySurfaceReceipt(null, text, fakeWasm, expectedRun).join("; "), /receipt schema/);
  for (const name of ["source.ostrin", "data.json", "figure.svg", "provenance.json"]) {
    const altered = JSON.parse(text);
    altered.files[name] += "tampered";
    assert.notDeepEqual(verifySurfaceReceipt(receipt, canonicalJson(altered), fakeWasm, expectedRun), [], name);
  }
  const forged = structuredClone(receipt);
  forged.files["data.json"] = sha256("forged");
  assert.match(verifySurfaceReceipt(forged, text, fakeWasm, expectedRun).join("; "), /data.json differs/);
  assert.match(verifySurfaceReceipt(receipt, text, Buffer.from("different module"), expectedRun).join("; "), /WASM bytes differ/);
});

test("the verifier rejects a different CI run and an open entry effect row", () => {
  const bundle = capturedSurface();
  const text = canonicalJson(bundle);
  const receipt = createSurfaceReceipt(text, replayProof(text));
  assert.match(verifySurfaceReceipt(receipt, text, fakeWasm, { ...expectedRun, runId: "4321" }).join("; "), /current tested CI run/);
  assert.match(verifySurfaceReceipt(receipt, text, fakeWasm, { ...expectedRun, testedSha: "c".repeat(40) }).join("; "), /current tested CI run/);
  bundle.provenance.effects.entry_effect_signature.row_closed = false;
  bundle.provenance.effects.entry_effect_signature.open_calls = ["unknown_callback"];
  refreshProvenance(bundle);
  assert.match(verifySurfaceReceipt(receipt, canonicalJson(bundle), fakeWasm, expectedRun).join("; "), /effect row is not closed/);
});

test("the verifier rejects malformed host capture and a forged fixture revision", () => {
  const bundle = capturedSurface();
  const text = canonicalJson(bundle);
  const receipt = createSurfaceReceipt(text, replayProof(text));
  bundle.provenance.runtime.host.runner_os = "";
  refreshProvenance(bundle);
  assert.match(verifySurfaceReceipt(receipt, canonicalJson(bundle), fakeWasm, expectedRun).join("; "), /structural capture is OPEN/);
  const forged = capturedSurface();
  forged.provenance.commit = "c".repeat(40);
  refreshProvenance(forged);
  const forgedText = canonicalJson(forged);
  assert.throws(() => createSurfaceReceipt(forgedText, replayProof(forgedText)), /registered source-backed revision/);
  assert.match(verifySurfaceReceipt(receipt, forgedText, fakeWasm, expectedRun).join("; "), /registered source-backed revision/);
});

test("the bound replay reports the WASM bytes loaded by the child process", async () => {
  const result = await replay("surface", { bindWasm: true });
  assert.equal(result.baseline, "match");
  assert.equal(result.sensitivity, "changed-output");
  assert.equal(result.wasmSha256, sha256(readFileSync(path.join(repositoryRoot, "website/ostrinc.wasm"))));
  assert.equal(result.bundleSha256, sha256(readFileSync(bundlePath)));
});

test("a source-backed host-bound surface bundle receives a receipt only after an actual replay", async () => {
  const environment = {
    OSTRIN_CAPTURE_HOST: "1",
    RUNNER_OS: "Linux",
    RUNNER_ARCH: "X64",
    RUNNER_ENVIRONMENT: "github-hosted",
    ImageOS: "ubuntu24",
    ImageVersion: "20261001.1.0",
    GITHUB_REPOSITORY: "sircalch/Ostrin",
    GITHUB_WORKFLOW: "Ostrin CI",
    GITHUB_RUN_ID: expectedRun.runId,
    GITHUB_RUN_ATTEMPT: "1",
    GITHUB_EVENT_NAME: "pull_request",
    GITHUB_REF: "refs/pull/999/merge",
    GITHUB_SHA: expectedRun.testedSha,
    OSTRIN_EVIDENCE_HEAD_SHA: "a".repeat(40),
  };
  const previous = Object.fromEntries(Object.keys(environment).map((name) => [name, process.env[name]]));
  Object.assign(process.env, environment);
  try {
    const bundle = buildExperimentBundle("surface");
    assert.deepEqual(verifyExperimentBundle(bundle, { fixtureId: "surface" }), { ok: true, errors: [] });
    const text = canonicalJson(bundle);
    const replayResult = await replay("surface", { bindWasm: true, bundleText: text });
    const receipt = createSurfaceReceipt(text, replayResult);
    assert.deepEqual(verifySurfaceReceipt(
      receipt,
      text,
      readFileSync(path.join(repositoryRoot, "website/ostrinc.wasm")),
      expectedRun,
    ), []);
  } finally {
    for (const [name, value] of Object.entries(previous)) {
      if (value === undefined) delete process.env[name];
      else process.env[name] = value;
    }
  }
});
