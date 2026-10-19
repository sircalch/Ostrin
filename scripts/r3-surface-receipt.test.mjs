import test from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { replay } from "./experiment-replay.mjs";
import { bundlePath, createSurfaceReceipt, verifySurfaceReceipt } from "./r3-surface-receipt.mjs";
import { buildExperimentBundle, repositoryRoot, verifyExperimentBundle } from "./experiment-bundle.mjs";

const sha256 = (bytes) => `sha256:${createHash("sha256").update(bytes).digest("hex")}`;
const canonicalJson = (value) => `${JSON.stringify(value, null, 2)}\n`;
const expectedRun = { runId: "1234", testedSha: "b".repeat(40) };
const fakeWasm = Buffer.from("test-only WASM bytes");

test("Pages deploys when any surface replay evidence producer changes", () => {
  const workflow = readFileSync(path.join(repositoryRoot, ".github/workflows/pages.yml"), "utf8");
  for (const source of [
    "scripts/r3-surface-receipt.mjs",
    "scripts/experiment-replay.mjs",
    "scripts/lab-data.mjs",
  ]) {
    assert.ok(workflow.split(/\r?\n/).includes(`      - '${source}'`), `${source} is missing from Pages push paths`);
  }
  const upload = readFileSync(path.join(repositoryRoot, ".github/actions/website-verify/action.yml"), "utf8");
  assert.ok(upload.split(/\r?\n/).includes("          website/ostrinc.wasm"),
    "the archived receipt needs the exact CI-built WASM bytes");
  assert.ok(upload.includes('node scripts/experiment-replay.mjs --replay-artifact . --run-id "$GITHUB_RUN_ID" --tested-sha "$GITHUB_SHA"'),
    "CI must execute the downloaded-package replay contract before uploading evidence");
});

test("a downloaded evidence package can be verified with its archived WASM", () => {
  const root = mkdtempSync(path.join(repositoryRoot, "target", "r3-receipt-artifact-"));
  try {
    const bundleText = canonicalJson(capturedSurface());
    const receipt = createSurfaceReceipt(bundleText, replayProof(bundleText));
    const bundleFile = path.join(root, "website/assets/experiments/surface.ostrin-experiment.json");
    const receiptFile = path.join(root, "target/r3-ci/surface-replay-receipt.json");
    const wasmFile = path.join(root, "website/ostrinc.wasm");
    for (const file of [bundleFile, receiptFile, wasmFile]) mkdirSync(path.dirname(file), { recursive: true });
    writeFileSync(bundleFile, bundleText);
    writeFileSync(receiptFile, canonicalJson(receipt));
    writeFileSync(wasmFile, fakeWasm);
    const command = [path.join(repositoryRoot, "scripts/r3-surface-receipt.mjs"),
      "--verify-artifact", root, "--run-id", expectedRun.runId, "--tested-sha", expectedRun.testedSha];
    const result = spawnSync(process.execPath, command, { cwd: repositoryRoot, encoding: "utf8" });
    assert.equal(result.status, 0, result.stderr);
    writeFileSync(wasmFile, Buffer.from("changed WASM bytes"));
    const altered = spawnSync(process.execPath, command, { cwd: repositoryRoot, encoding: "utf8" });
    assert.notEqual(altered.status, 0);
    assert.match(altered.stderr, /WASM bytes differ/);
    writeFileSync(receiptFile, "x".repeat(64 * 1024 + 1));
    const oversized = spawnSync(process.execPath, command, { cwd: repositoryRoot, encoding: "utf8" });
    assert.notEqual(oversized.status, 0);
    assert.match(oversized.stderr, /regular file within the size limit/);
  } finally {
    assert.ok(root.startsWith(path.join(repositoryRoot, "target", "r3-receipt-artifact-")));
    rmSync(root, { recursive: true, force: true });
  }
});

test("offline replay rejects a self-consistent receipt with a forged figure", () => {
  const root = mkdtempSync(path.join(repositoryRoot, "target", "r3-receipt-artifact-"));
  try {
    const wasmBytes = readFileSync(path.join(repositoryRoot, "website/ostrinc.wasm"));
    const bundleFile = path.join(root, "website/assets/experiments/surface.ostrin-experiment.json");
    const receiptFile = path.join(root, "target/r3-ci/surface-replay-receipt.json");
    const wasmFile = path.join(root, "website/ostrinc.wasm");
    for (const file of [bundleFile, receiptFile, wasmFile]) mkdirSync(path.dirname(file), { recursive: true });
    writeFileSync(wasmFile, wasmBytes);
    const verify = [path.join(repositoryRoot, "scripts/r3-surface-receipt.mjs"),
      "--verify-artifact", root, "--run-id", expectedRun.runId, "--tested-sha", expectedRun.testedSha];
    const execute = [path.join(repositoryRoot, "scripts/experiment-replay.mjs"),
      "--replay-artifact", root, "--run-id", expectedRun.runId, "--tested-sha", expectedRun.testedSha];
    const original = capturedSurface();
    const originalText = canonicalJson(original);
    writeFileSync(bundleFile, originalText);
    writeFileSync(receiptFile, canonicalJson(createSurfaceReceipt(originalText, replayProof(originalText, wasmBytes))));
    const realReplay = spawnSync(process.execPath, execute, { cwd: repositoryRoot, encoding: "utf8" });
    assert.equal(realReplay.status, 0, realReplay.stderr);

    const forged = capturedSurface();
    forged.files["figure.svg"] = forged.files["figure.svg"].replace("</svg>", "<!-- forged output -->\n</svg>");
    const figure = forged.manifest.files.find((entry) => entry.path === "figure.svg");
    figure.bytes = Buffer.byteLength(forged.files["figure.svg"], "utf8");
    figure.sha256 = sha256(Buffer.from(forged.files["figure.svg"], "utf8"));
    const recorded = forged.provenance.runtime.invocation.outputs.find((entry) => entry.path === "figure.svg");
    recorded.bytes = figure.bytes;
    recorded.sha256 = figure.sha256;
    refreshProvenance(forged);
    const forgedText = canonicalJson(forged);
    writeFileSync(bundleFile, forgedText);
    writeFileSync(receiptFile, canonicalJson(createSurfaceReceipt(forgedText, replayProof(forgedText, wasmBytes))));
    const integrity = spawnSync(process.execPath, verify, { cwd: repositoryRoot, encoding: "utf8" });
    assert.equal(integrity.status, 0, integrity.stderr);
    const falseReplay = spawnSync(process.execPath, execute, { cwd: repositoryRoot, encoding: "utf8" });
    assert.notEqual(falseReplay.status, 0);
    assert.match(falseReplay.stderr, /input-driven replay SVG differs from the published bundle/);
  } finally {
    assert.ok(root.startsWith(path.join(repositoryRoot, "target", "r3-receipt-artifact-")));
    rmSync(root, { recursive: true, force: true });
  }
});

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

function replayProof(bundleText, wasmBytes = fakeWasm) {
  const bundle = JSON.parse(bundleText);
  return {
    id: "surface",
    baseline: "match",
    sensitivity: "changed-output",
    wasmSha256: sha256(wasmBytes),
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
