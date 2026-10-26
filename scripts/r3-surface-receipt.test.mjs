import test from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { listExecutableReplayIds, readArchivedReplayBundle, replay } from "./experiment-replay.mjs";
import { bundlePath, createSurfaceReceipt, verifySurfaceReceipt } from "./r3-surface-receipt.mjs";
import { buildExperimentBundle, outputPathFor, repositoryRoot, verifyExperimentBundle } from "./experiment-bundle.mjs";

const sha256 = (bytes) => `sha256:${createHash("sha256").update(bytes).digest("hex")}`;
const canonicalJson = (value) => `${JSON.stringify(value, null, 2)}\n`;
const expectedRun = { runId: "1234", testedSha: "b".repeat(40) };
const fakeWasm = Buffer.from("test-only WASM bytes");

function runFor(bundle) {
  const text = canonicalJson(bundle);
  return {
    ...expectedRun,
    replayBundles: {
      surface: { level: "R2", bundleSha256: sha256(Buffer.from(text, "utf8")) },
    },
  };
}

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
  assert.ok(upload.includes('node scripts/experiment-replay.mjs --replay-artifact-all target/r3-ci/downloaded-host-evidence --run-id "$GITHUB_RUN_ID" --tested-sha "$GITHUB_SHA"'),
    "CI must independently execute every archived R1/R2 package from the downloaded candidate before publishing verified evidence");
  assert.ok(upload.includes('node scripts/experiment-replay.mjs --replay-artifact-all . --run-id "$GITHUB_RUN_ID" --tested-sha "$GITHUB_SHA"'),
    "CI must generate the run-bound WASI comparison reports before packaging the candidate");
  assert.ok(upload.split(/\r?\n/).includes("          target/r3-ci/wasi-file-read-evidence.json"),
    "CI must upload validated file-read evidence from the archived replay");
  assert.ok(upload.split(/\r?\n/).includes("          target/r3-ci/wasi-random-evidence.json"),
    "CI must upload the controlled WASI random input and replay hashes");
});

test("offline artifact bundle replay requires a matching host capture and a closed entry row", () => {
  const root = mkdtempSync(path.join(repositoryRoot, "target", "r3-replay-artifact-"));
  try {
    const bundle = capturedSurface();
    const file = path.join(root, "website/assets/experiments/surface.ostrin-experiment.json");
    mkdirSync(path.dirname(file), { recursive: true });
    writeFileSync(file, canonicalJson(bundle));
    const accepted = readArchivedReplayBundle(root, "surface", runFor(bundle));
    assert.equal(JSON.parse(accepted.bundleText).id, "surface");
    assert.match(accepted.path, /surface\.ostrin-experiment\.json$/);

    const wrongRun = structuredClone(bundle);
    wrongRun.provenance.runtime.host.ci_run.run_id = "9876";
    refreshProvenance(wrongRun);
    writeFileSync(file, canonicalJson(wrongRun));
    assert.throws(() => readArchivedReplayBundle(root, "surface", runFor(bundle)), /requested CI run and tested SHA|tested run inventory/);

    const openRow = structuredClone(bundle);
    openRow.provenance.effects.entry_effect_signature.row_closed = false;
    openRow.provenance.effects.entry_effect_signature.open_calls = ["unknown_callback"];
    refreshProvenance(openRow);
    writeFileSync(file, canonicalJson(openRow));
    assert.throws(() => readArchivedReplayBundle(root, "surface", runFor(bundle)), /main effect row is not closed|structural capture is OPEN/);

    const downgraded = structuredClone(bundle);
    downgraded.reproducibility.level = "R0";
    downgraded.provenance.level = "R0";
    downgraded.provenance.reproducibility.level = "R0";
    refreshProvenance(downgraded);
    writeFileSync(file, canonicalJson(downgraded));
    assert.throws(() => readArchivedReplayBundle(root, "surface", runFor(bundle)), /differs from the tested run inventory/);

    const wrongEntry = capturedSurface();
    wrongEntry.provenance.effects.entry = "callback";
    wrongEntry.provenance.effects.entry_effect_signature.function = "callback";
    refreshProvenance(wrongEntry);
    writeFileSync(file, canonicalJson(wrongEntry));
    assert.throws(() => readArchivedReplayBundle(root, "surface", runFor(bundle)), /main effect row|entry effect signature must name/);
  } finally {
    assert.ok(root.startsWith(path.join(repositoryRoot, "target", "r3-replay-artifact-")));
    rmSync(root, { recursive: true, force: true });
  }
});

test("archived R0 bundles are accepted only with matching host and bundle inventory", () => {
  const root = mkdtempSync(path.join(repositoryRoot, "target", "r3-replay-artifact-"));
  try {
    const bundle = JSON.parse(readFileSync(outputPathFor("animation"), "utf8"));
    bundle.provenance.runtime.host = structuredClone(capturedSurface().provenance.runtime.host);
    bundle.provenance.runtime.missing = bundle.provenance.runtime.missing
      .filter((item) => item !== "captured host and hardware metadata");
    refreshProvenance(bundle);
    const text = canonicalJson(bundle);
    const file = path.join(root, "website/assets/experiments/animation.ostrin-experiment.json");
    mkdirSync(path.dirname(file), { recursive: true });
    writeFileSync(file, text);
    const run = {
      ...expectedRun,
      replayBundles: {
        animation: { level: "R0", bundleSha256: sha256(Buffer.from(text, "utf8")) },
      },
    };
    const archived = readArchivedReplayBundle(root, "animation", run);
    assert.equal(archived.bundle.reproducibility.level, "R0");

    run.replayBundles.animation.bundleSha256 = sha256(Buffer.from("different bundle", "utf8"));
    assert.throws(() => readArchivedReplayBundle(root, "animation", run), /tested run inventory/);
  } finally {
    assert.ok(root.startsWith(path.join(repositoryRoot, "target", "r3-replay-artifact-")));
    rmSync(root, { recursive: true, force: true });
  }
});

test("offline replay registry reads fixture contracts without building the native compiler", () => {
  const ids = listExecutableReplayIds();
  assert.equal(ids.length, 12);
  assert.ok(ids.includes("surface"));
  assert.ok(ids.every((id) => ["lines", "surface", "histogram", "boxplot", "scatter-fit", "violin", "hexbin", "bars", "point-cloud", "fft", "dashboard", "pendulum-calibration"].includes(id)));
});

test("archived replay maturity and membership are bound to the tested run inventory", () => {
  const root = mkdtempSync(path.join(repositoryRoot, "target", "r3-replay-artifact-"));
  try {
    const bundle = capturedSurface();
    const file = path.join(root, "website/assets/experiments/surface.ostrin-experiment.json");
    mkdirSync(path.dirname(file), { recursive: true });
    writeFileSync(file, canonicalJson(bundle));
    const run = runFor(bundle);
    const accepted = readArchivedReplayBundle(root, "surface", run);
    assert.equal(accepted.bundle.id, "surface");

    const altered = structuredClone(bundle);
    altered.reproducibility.level = "R0";
    altered.provenance.level = "R0";
    altered.provenance.reproducibility.level = "R0";
    refreshProvenance(altered);
    writeFileSync(file, canonicalJson(altered));
    assert.throws(() => readArchivedReplayBundle(root, "surface", run), /tested run inventory|tested run inventory/);

    assert.throws(() => readArchivedReplayBundle(root, "surface", {
      ...run,
      replayBundles: { "removed-in-current-checkout": { level: "R2", bundleSha256: "sha256:" + "a".repeat(64) } },
    }), /does not register this fixture as an executable R0\/R1\/R2 bundle/);
  } finally {
    assert.ok(root.startsWith(path.join(repositoryRoot, "target", "r3-replay-artifact-")));
    rmSync(root, { recursive: true, force: true });
  }
});

test("an archived bundle can replay after its fixture is retired from the current registry", async () => {
  const root = mkdtempSync(path.join(repositoryRoot, "target", "r3-replay-artifact-"));
  try {
    const bundle = capturedSurface();
    bundle.id = "retired-fixture";
    refreshProvenance(bundle);
    const text = canonicalJson(bundle);
    const file = path.join(root, "website/assets/experiments/retired-fixture.ostrin-experiment.json");
    mkdirSync(path.dirname(file), { recursive: true });
    writeFileSync(file, text);
    const run = {
      ...expectedRun,
      replayBundles: {
        "retired-fixture": { level: "R2", bundleSha256: sha256(Buffer.from(text, "utf8")) },
      },
    };
    const archived = readArchivedReplayBundle(root, "retired-fixture", run);
    const result = await replay("retired-fixture", { bundleText: archived.bundleText });
    assert.equal(result.id, "retired-fixture");
    assert.equal(result.baseline, "match");
    assert.equal(result.sensitivity, "changed-output");
  } finally {
    assert.ok(root.startsWith(path.join(repositoryRoot, "target", "r3-replay-artifact-")));
    rmSync(root, { recursive: true, force: true });
  }
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

test("a surface receipt binds the run and every archived R0/R1/R2 bundle", () => {
  const text = canonicalJson(capturedSurface());
  const receipt = createSurfaceReceipt(text, replayProof(text));
  assert.deepEqual(verifySurfaceReceipt(receipt, text, fakeWasm, expectedRun), []);
  assert.equal(receipt.fixtureSourceRevision, JSON.parse(text).provenance.commit);
  assert.equal(receipt.compilerWasmSha256, sha256(fakeWasm));
  assert.equal(Object.keys(receipt.replayBundles).length, 30);
  assert.equal(Object.values(receipt.replayBundles).filter((item) => item.level === "R0").length, 18);
  assert.equal(Object.values(receipt.replayBundles).filter((item) => item.level === "R2").length, 12);
  assert.equal(receipt.replayBundles.surface.bundleSha256, receipt.bundleSha256);

  const historicalInventory = {
    surface: receipt.replayBundles.surface,
    "retired-fixture": { level: "R1", bundleSha256: "sha256:" + "c".repeat(64) },
  };
  const replayedHistorical = createSurfaceReceipt(text, replayProof(text), historicalInventory);
  assert.deepEqual(replayedHistorical.replayBundles, historicalInventory,
    "offline surface replay must preserve the already verified historical inventory");
  assert.deepEqual(verifySurfaceReceipt(replayedHistorical, text, fakeWasm, expectedRun), []);

  const legacyReceipt = structuredClone(receipt);
  delete legacyReceipt.replayBundles;
  assert.deepEqual(verifySurfaceReceipt(legacyReceipt, text, fakeWasm, expectedRun), [],
    "surface-only verification remains compatible with v1 receipts created before bundle inventories");
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
