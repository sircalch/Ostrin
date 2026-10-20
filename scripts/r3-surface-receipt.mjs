#!/usr/bin/env node

// A run-bound replay receipt for one R2 fixture. This is narrower than an R3
// maturity claim: it ties an actual WASI replay to the captured CI host, the
// bundled inputs and output, the closed entry effect row, and the WASM bytes
// loaded by the child process. It is never checked into the repository.
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, realpathSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { experimentFixtures, outputPathFor, validateEffectCapture, validateEntryEffectSignature, verifyBundleFile } from "./experiment-bundle.mjs";
import { inspectBundle, repositoryRoot } from "./r3-readiness.mjs";

export const receiptPath = path.join(repositoryRoot, "target/r3-ci/surface-replay-receipt.json");
export const bundlePath = path.join(repositoryRoot, "website/assets/experiments/surface.ostrin-experiment.json");
const wasmPath = path.join(repositoryRoot, "website/ostrinc.wasm");
const schema = "ostrin.r3-replay-receipt/v1";
const fileNames = ["source.ostrin", "data.json", "figure.svg", "provenance.json"];
const shaPattern = /^sha256:[a-f0-9]{64}$/;

function sha256(bytes) {
  return `sha256:${createHash("sha256").update(bytes).digest("hex")}`;
}

function canonicalJson(value) {
  return `${JSON.stringify(value, null, 2)}\n`;
}

function parseBundle(bundleText) {
  try {
    return JSON.parse(bundleText);
  } catch (error) {
    throw new Error(`surface bundle is not JSON: ${error.message}`);
  }
}

function fileHashes(bundle) {
  return Object.fromEntries(fileNames.map((name) => [name, sha256(Buffer.from(bundle.files?.[name] ?? "", "utf8"))]));
}

function executableReplayInventory(surfaceBundleText) {
  return Object.fromEntries(Object.entries(experimentFixtures).flatMap(([id, fixture]) => {
    const input = JSON.parse(readFileSync(path.join(repositoryRoot, fixture.inputPath), "utf8"));
    const level = input.reproducibility?.level ?? "R0";
    if (!(["R1", "R2"].includes(level))) return [];
    const bundle = id === "surface" ? Buffer.from(surfaceBundleText, "utf8") : readFileSync(outputPathFor(id));
    return [[id, { level, bundleSha256: sha256(bundle) }]];
  }));
}

export function createSurfaceReceipt(bundleText, replayResult) {
  const bundle = parseBundle(bundleText);
  const errors = validateSurfaceEvidence(bundle);
  if (errors.length) throw new Error(`surface receipt cannot be issued: ${errors.join("; ")}`);
  if (replayResult?.id !== "surface" || replayResult?.baseline !== "match"
    || replayResult?.sensitivity !== "changed-output" || !shaPattern.test(replayResult?.wasmSha256 ?? "")
    || replayResult?.bundleSha256 !== sha256(Buffer.from(bundleText, "utf8"))) {
    throw new Error("surface receipt requires a successful bound replay of this exact bundle");
  }
  const hashes = fileHashes(bundle);
  for (const name of fileNames) {
    if (replayResult.fileHashes?.[name] !== hashes[name]) {
      throw new Error(`surface replay used different ${name} bytes`);
    }
  }
  return {
    schema,
    fixture: "surface",
    scope: "single-run Node WASI Preview 1; no cross-platform equivalence claim",
    result: { baseline: "match", sensitivity: "changed-output" },
    ciRun: { ...bundle.provenance.runtime.host.ci_run },
    fixtureSourceRevision: bundle.provenance.commit,
    fixtureSourcePath: experimentFixtures.surface.sourcePath,
    replayBundles: executableReplayInventory(bundleText),
    bundleSha256: replayResult.bundleSha256,
    files: hashes,
    compilerWasmSha256: replayResult.wasmSha256,
    entryEffectSignatureSha256: sha256(Buffer.from(canonicalJson(bundle.provenance.effects.entry_effect_signature), "utf8")),
  };
}

function validateSurfaceEvidence(bundle) {
  const errors = [];
  if (bundle?.schema !== "ostrin.experiment/v0" || bundle?.id !== "surface") {
    errors.push("expected the surface experiment bundle");
  }
  if (bundle?.reproducibility?.level !== "R2" || bundle?.provenance?.level !== "R2") {
    errors.push("surface must retain its recorded R2 maturity");
  }
  const execution = bundle?.provenance?.execution;
  if (execution?.mode !== "input-file" || execution?.input_file !== "data.json"
    || JSON.stringify(execution?.capabilities) !== JSON.stringify(["filesystem.read:data.json"])
    || execution?.snapshot?.path !== "data.json" || execution?.snapshot?.embedded !== true) {
    errors.push("surface input snapshot and isolated filesystem capability are incomplete");
  }
  const entry = inspectBundle(bundle, "website/assets/experiments/surface.ostrin-experiment.json");
  if (entry.status !== "READY") errors.push(`structural capture is ${entry.status}`);
  const effects = bundle?.provenance?.effects;
  errors.push(...validateEffectCapture(effects));
  errors.push(...validateEntryEffectSignature(effects?.entry_effect_signature, "main", effects));
  if (effects?.entry_effect_signature?.row_closed !== true
    || effects?.entry_effect_signature?.variables?.length !== 0
    || effects?.entry_effect_signature?.open_calls?.length !== 0) {
    errors.push("main effect row is not closed");
  }
  const manifest = bundle?.manifest?.files;
  if (!Array.isArray(manifest) || manifest.length !== fileNames.length
    || !fileNames.every((name) => manifest.some((entry) => entry.path === name))) {
    errors.push("bundle manifest must contain exactly the four recorded files");
  }
  for (const name of fileNames) {
    const text = bundle?.files?.[name];
    const record = manifest?.find((entry) => entry.path === name);
    if (typeof text !== "string" || record?.bytes !== Buffer.byteLength(text, "utf8")
      || record?.sha256 !== sha256(Buffer.from(text ?? "", "utf8"))) {
      errors.push(`${name} does not match its manifest hash and size`);
    }
  }
  let embeddedProvenance;
  try { embeddedProvenance = JSON.parse(bundle?.files?.["provenance.json"] ?? "null"); }
  catch { embeddedProvenance = null; }
  if (JSON.stringify(embeddedProvenance) !== JSON.stringify(bundle?.provenance)) {
    errors.push("embedded provenance differs from the bundle");
  }
  const dataRecord = manifest?.find((entry) => entry.path === "data.json");
  const figureRecord = manifest?.find((entry) => entry.path === "figure.svg");
  if (bundle?.provenance?.runtime?.input_snapshots?.find((item) => item.path === "data.json")?.sha256 !== dataRecord?.sha256
    || bundle?.provenance?.runtime?.invocation?.outputs?.find((item) => item.path === "figure.svg")?.sha256 !== figureRecord?.sha256) {
    errors.push("runtime snapshot or output hash differs from the manifest");
  }
  if (bundle?.provenance?.commit !== experimentFixtures.surface.sourceRevision) {
    errors.push("fixture source revision differs from the registered source-backed revision");
  } else if (typeof bundle?.files?.["source.ostrin"] === "string") {
    try {
      const sourceAtRevision = execFileSync("git", [
        "show", `${experimentFixtures.surface.sourceRevision}:${experimentFixtures.surface.sourcePath}`,
      ], { cwd: repositoryRoot, windowsHide: true });
      if (sha256(sourceAtRevision) !== sha256(Buffer.from(bundle.files["source.ostrin"], "utf8"))) {
        errors.push("embedded source differs from the registered fixture source revision");
      }
    } catch {
      errors.push("registered fixture source revision cannot be verified in this checkout");
    }
  }
  return errors;
}

export function verifySurfaceReceipt(receipt, bundleText, wasmBytes, expectedRun) {
  const bundle = parseBundle(bundleText);
  const errors = validateSurfaceEvidence(bundle);
  if (!receipt || receipt.schema !== schema || receipt.fixture !== "surface") {
    errors.push("receipt schema or fixture is invalid");
    return errors;
  }
  if (receipt.scope !== "single-run Node WASI Preview 1; no cross-platform equivalence claim"
    || receipt.result?.baseline !== "match" || receipt.result?.sensitivity !== "changed-output") {
    errors.push("receipt replay scope or result is invalid");
  }
  const capturedRun = bundle?.provenance?.runtime?.host?.ci_run;
  if (JSON.stringify(receipt.ciRun) !== JSON.stringify(capturedRun)
    || receipt.ciRun?.run_id !== expectedRun?.runId
    || receipt.ciRun?.tested_sha !== expectedRun?.testedSha) {
    errors.push("receipt and host capture do not identify the current tested CI run");
  }
  if (receipt.fixtureSourceRevision !== bundle?.provenance?.commit) {
    errors.push("receipt fixture source revision differs from the bundle");
  }
  if (receipt.fixtureSourcePath !== experimentFixtures.surface.sourcePath) {
    errors.push("receipt fixture source path differs from the registered source");
  }
  const replayBundles = receipt.replayBundles;
  if (!replayBundles || typeof replayBundles !== "object" || Array.isArray(replayBundles)
    || !Object.hasOwn(replayBundles, "surface")
    || Object.entries(replayBundles).some(([id, item]) => !Object.hasOwn(experimentFixtures, id)
      || !["R1", "R2"].includes(item?.level)
      || !shaPattern.test(item?.bundleSha256 ?? ""))) {
    errors.push("receipt executable replay inventory is missing or invalid");
  } else if (replayBundles.surface?.bundleSha256 !== sha256(Buffer.from(bundleText, "utf8"))
    || replayBundles.surface?.level !== bundle?.reproducibility?.level) {
    errors.push("surface bundle differs from the tested run replay inventory");
  }
  if (receipt.bundleSha256 !== sha256(Buffer.from(bundleText, "utf8"))) errors.push("bundle bytes differ from the replayed bundle");
  const hashes = fileHashes(bundle);
  for (const name of fileNames) {
    if (receipt.files?.[name] !== hashes[name]) errors.push(`${name} differs from the replay receipt`);
  }
  if (!Buffer.isBuffer(wasmBytes) || receipt.compilerWasmSha256 !== sha256(wasmBytes)) {
    errors.push("compiler WASM bytes differ from the child execution digest");
  }
  const signature = bundle?.provenance?.effects?.entry_effect_signature;
  if (receipt.entryEffectSignatureSha256 !== sha256(Buffer.from(canonicalJson(signature), "utf8"))) {
    errors.push("entry effect signature differs from the replay receipt");
  }
  return errors;
}

function verifyFiles(bundleFile, receiptFile, wasmFile, expectedRun) {
  for (const file of [bundleFile, receiptFile, wasmFile]) {
    if (!existsSync(file)) throw new Error(`missing ${file}`);
  }
  const checked = verifyBundleFile(bundleFile, "surface");
  if (!checked.ok) throw new Error(`source-backed surface bundle failed: ${checked.errors.join("; ")}`);
  const receipt = JSON.parse(readFileSync(receiptFile, "utf8"));
  const errors = verifySurfaceReceipt(
    receipt,
    readFileSync(bundleFile, "utf8"),
    readFileSync(wasmFile),
    expectedRun,
  );
  if (errors.length) throw new Error(errors.join("; "));
  console.log(`r3-surface-receipt: verified surface replay in CI run ${receipt.ciRun.run_id} at ${receipt.ciRun.tested_sha}`);
}

export function readArchivedFile(root, relativePath, maximumBytes) {
  const location = path.join(root, relativePath);
  if (!existsSync(location)) throw new Error(`missing archived ${relativePath}`);
  const resolved = realpathSync(location);
  const relative = path.relative(root, resolved);
  if (relative === ".." || relative.startsWith(`..${path.sep}`) || path.isAbsolute(relative)) {
    throw new Error(`archived ${relativePath} escapes the artifact directory`);
  }
  const file = statSync(resolved);
  if (!file.isFile() || file.size < 1 || file.size > maximumBytes) {
    throw new Error(`archived ${relativePath} is not a regular file within the size limit`);
  }
  return { path: resolved, bytes: readFileSync(resolved) };
}

// This checks archived bytes without regenerating CI host metadata on the
// verifier's machine. It proves integrity, not a fresh execution of the program.
export function verifySurfaceArtifact(directory, expectedRun) {
  const root = realpathSync(directory);
  const bundle = readArchivedFile(root, "website/assets/experiments/surface.ostrin-experiment.json", 16 * 1024 * 1024);
  const receiptFile = readArchivedFile(root, "target/r3-ci/surface-replay-receipt.json", 64 * 1024);
  const wasm = readArchivedFile(root, "website/ostrinc.wasm", 64 * 1024 * 1024);
  const bundleText = bundle.bytes.toString("utf8");
  const receipt = JSON.parse(receiptFile.bytes.toString("utf8"));
  const errors = verifySurfaceReceipt(receipt, bundleText, wasm.bytes, expectedRun);
  if (errors.length) throw new Error(errors.join("; "));
  return { bundleText, receipt, wasmPath: wasm.path };
}

function main() {
  const args = process.argv.slice(2);
  if (args.length === 1 && args[0] === "--verify") {
    verifyFiles(bundlePath, receiptPath, wasmPath,
      { runId: process.env.GITHUB_RUN_ID, testedSha: process.env.GITHUB_SHA });
    return;
  }
  const [mode, directory, runFlag, runId, shaFlag, testedSha] = args;
  if (args.length !== 6 || mode !== "--verify-artifact" || runFlag !== "--run-id"
    || shaFlag !== "--tested-sha" || !/^\d+$/.test(runId ?? "") || !/^[a-f0-9]{40}$/.test(testedSha ?? "")) {
    throw new Error("usage: node scripts/r3-surface-receipt.mjs --verify | --verify-artifact <directory> --run-id <id> --tested-sha <sha>");
  }
  const { receipt } = verifySurfaceArtifact(directory, { runId, testedSha });
  console.log(`r3-surface-receipt: verified surface replay in CI run ${receipt.ciRun.run_id} at ${receipt.ciRun.tested_sha}`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  try { main(); }
  catch (error) { console.error(`r3-surface-receipt: ${error.message}`); process.exitCode = 1; }
}
