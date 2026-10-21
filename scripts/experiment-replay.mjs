#!/usr/bin/env node

// Execute an input-file experiment bundle in an isolated WASI filesystem.
//
// R0 bundles remain the byte-for-byte publication contract. This command is a
// executable input-driven slice: the source reads data.json, the host verifies
// the bundle hashes, and a sensitivity run proves that a declared parameter
// changes the rendered figure. It deliberately does not claim the
// seeded-randomness R1 guarantee for fixtures that have not opted in.
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, realpathSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { experimentFixtures, normalizeFigureMetadata, outputPathFor, repositoryRoot, validateEffectCapture, validateEntryEffectSignature } from "./experiment-bundle.mjs";
import { runWasmWithFilesystemEvidence } from "./lab-data.mjs";
import { createSurfaceReceipt, readArchivedFile, receiptPath, verifySurfaceArtifact } from "./r3-surface-receipt.mjs";
import { inspectBundle } from "./r3-readiness.mjs";

const schema = "ostrin.experiment/v0";
const filesystemEvidenceSchema = "ostrin.wasi-file-read-evidence/v1";

function sha256(text) {
  return `sha256:${createHash("sha256").update(Buffer.from(text, "utf8")).digest("hex")}`;
}

function bytes(text) {
  return Buffer.byteLength(text, "utf8");
}

export function validateWasiReadEvidence(id, events, files, requiredPaths = []) {
  const errors = [];
  const seen = new Set();
  if (!Array.isArray(events)) return [`${id}: WASI filesystem read evidence is missing`];
  for (const event of events) {
    if (event.kind !== "file-read") continue;
    if (event.pathStatus !== "relative" || typeof event.path !== "string") {
      errors.push(`${id}: WASI read path is ${event.pathStatus ?? "unknown"}`);
      continue;
    }
    const expected = files[event.path];
    if (typeof expected !== "string") {
      errors.push(`${id}: WASI attempted to read ${event.path} outside the supplied filesystem`);
      continue;
    }
    if (event.openStatus !== "opened") {
      errors.push(`${id}: declared file ${event.path} could not be opened (${event.openStatus})`);
      continue;
    }
    if (event.readStatus) {
      errors.push(`${id}: WASI read from ${event.path} was incomplete (${event.readStatus})`);
      continue;
    }
    const expectedBytes = Buffer.from(expected, "utf8");
    const expectedHash = `sha256:${createHash("sha256").update(expectedBytes).digest("hex")}`;
    if (event.bytesRead !== expectedBytes.byteLength || event.contentSha256 !== expectedHash) {
      errors.push(`${id}: bytes read from ${event.path} differ from the supplied file`);
      continue;
    }
    seen.add(event.path);
  }
  for (const file of requiredPaths) {
    if (!seen.has(file)) errors.push(`${id}: WASI execution did not read required file ${file}`);
  }
  return errors;
}

function sanitizeWasiReadEvents(events) {
  return events.map((event) => ({
    kind: event.kind,
    ...(event.path === null ? {} : { path: event.path }),
    ...(event.pathHash ? { pathSha256: event.pathHash } : {}),
    pathStatus: event.pathStatus,
    openStatus: event.openStatus,
    ...(event.openErrno == null ? {} : { openErrno: event.openErrno }),
    bytesRead: event.bytesRead,
    contentSha256: event.contentSha256,
    ...(event.readStatus ? { readStatus: event.readStatus } : {}),
  }));
}

export function createWasiFileReadEvidenceReport(results, expectedRun, compilerWasmSha256, replayBundles) {
  if (!/^\d+$/.test(expectedRun?.runId ?? "") || !/^[a-f0-9]{40}$/.test(expectedRun?.testedSha ?? "")) {
    throw new Error("WASI file-read evidence requires a run ID and tested commit SHA");
  }
  if (!/^sha256:[a-f0-9]{64}$/.test(compilerWasmSha256 ?? "")) {
    throw new Error("WASI file-read evidence requires the tested compiler WASM digest");
  }
  const resultById = new Map(results.map((result) => [result.id, result]));
  const ids = Object.keys(replayBundles ?? {}).sort();
  if (ids.length === 0 || results.length !== ids.length || resultById.size !== ids.length || ids.some((id) => !resultById.has(id))) {
    throw new Error("WASI file-read evidence does not cover the complete replay inventory");
  }
  const bundles = Object.fromEntries(ids.map((id) => {
    const result = resultById.get(id);
    const inventory = replayBundles[id];
    if (!result.filesystemReadEvidence || result.filesystemReadEvidence.schema !== filesystemEvidenceSchema
      || !Array.isArray(result.filesystemReadEvidence.executions) || result.filesystemReadEvidence.executions.length === 0) {
      throw new Error(`${id}: replay is missing runtime WASI file-read evidence`);
    }
    if (result.wasmSha256 !== compilerWasmSha256 || result.bundleSha256 !== inventory.bundleSha256) {
      throw new Error(`${id}: replay evidence does not match the tested compiler or bundle inventory`);
    }
    return [id, {
      level: inventory.level,
      bundleSha256: result.bundleSha256,
      executions: result.filesystemReadEvidence.executions,
    }];
  }));
  return {
    schema: filesystemEvidenceSchema,
    ciRun: { run_id: expectedRun.runId, tested_sha: expectedRun.testedSha },
    compilerWasmSha256,
    scope: "regular files read through WASI Preview 1 path_open/fd_read only",
    limitations: [
      "Does not observe directory enumeration or filesystem metadata queries.",
      "Does not cover native host filesystem access, network, clock, environment variables, or other WASI interfaces.",
      "A completed read trace does not establish general R3 closure or cross-platform equivalence.",
    ],
    replayBundles: bundles,
  };
}

function canonicalJson(value) {
  return `${JSON.stringify(value, null, 2)}\n`;
}

function fixtureArgument(argv) {
  const inline = argv.find((argument) => argument.startsWith("--fixture="));
  if (inline) return inline.slice("--fixture=".length);
  const index = argv.indexOf("--fixture");
  if (index >= 0 && argv[index + 1]) return argv[index + 1];
  return "";
}

export function listExecutableReplayIds() {
  return Object.keys(experimentFixtures).filter((id) => {
    const fixture = experimentFixtures[id];
    const input = JSON.parse(readFileSync(path.join(repositoryRoot, fixture.inputPath), "utf8"));
    const level = input.reproducibility?.level ?? "R0";
    return level === "R1" || level === "R2";
  });
}

function extractSvg(lines, id) {
  const start = lines.findIndex((line) => line.startsWith("<svg"));
  const end = lines.findIndex((line, index) => index >= start && line === "</svg>");
  if (start < 0 || end < 0) throw new Error(`${id}: executable replay did not print one complete SVG`);
  return `${lines.slice(start, end + 1).join("\n")}\n`;
}

function bundleErrors(bundle, id) {
  const errors = [];
  if (!bundle || typeof bundle !== "object") return [`${id}: bundle is not an object`];
  if (bundle.schema !== schema) errors.push(`${id}: expected ${schema}`);
  if (bundle.id !== id) errors.push(`${id}: bundle id does not match the requested fixture`);
  if (!["R0", "R1", "R2"].includes(bundle.reproducibility?.level)) errors.push(`${id}: executable replay requires an explicit R0, R1 or R2 bundle`);
  if (!bundle.files || typeof bundle.files !== "object") errors.push(`${id}: files object is missing`);
  if (!Array.isArray(bundle.manifest?.files) || bundle.manifest.files.length !== 4) errors.push(`${id}: manifest must contain four files`);
  if (bundle.provenance?.runtime?.schema !== "ostrin.runtime/v1"
    || bundle.provenance?.runtime?.status !== "partial"
    || bundle.provenance?.runtime?.target !== "wasm32-wasip1"
    || bundle.provenance?.runtime?.wasi !== "preview1") {
    errors.push(`${id}: bundle is missing the partial ostrin.runtime/v1 snapshot`);
  }
  const invocation = bundle.provenance?.runtime?.invocation;
  const invocationOutput = invocation?.outputs?.find((entry) => entry.path === "figure.svg");
  const figureEntry = bundle.manifest?.files?.find((entry) => entry.path === "figure.svg");
  if (invocation?.command !== "node scripts/lab-data.mjs --verify-replays"
    || invocation?.status !== "verified-in-ci"
    || invocationOutput?.bytes !== figureEntry?.bytes
    || invocationOutput?.sha256 !== figureEntry?.sha256) {
    errors.push(`${id}: runtime invocation output hash is missing or differs from the bundled figure`);
  }
  if (bundle.reproducibility?.level === "R2") {
    const execution = bundle.provenance?.execution;
    const snapshot = execution?.snapshot;
    if (!Array.isArray(execution?.capabilities)
      || !execution.capabilities.includes(`filesystem.read:${execution?.input_file}`)) {
      errors.push(`${id}: R2 bundle is missing the filesystem capability for its input snapshot`);
    }
    if (!snapshot || snapshot.path !== execution?.input_file || snapshot.embedded !== true) {
      errors.push(`${id}: R2 bundle is missing an embedded input snapshot`);
    }
    const runtimeSnapshot = bundle.provenance?.runtime?.input_snapshots?.find((entry) => entry.path === snapshot?.path);
    const inputEntry = bundle.manifest?.files?.find((entry) => entry.path === execution?.input_file);
    if (!runtimeSnapshot || runtimeSnapshot.sha256 !== inputEntry?.sha256) {
      errors.push(`${id}: R2 input snapshot hash does not match the bundled data`);
    }
  }
  for (const entry of bundle.manifest?.files ?? []) {
    const content = bundle.files?.[entry.path];
    if (typeof content !== "string") {
      errors.push(`${id}: ${entry.path} is missing from the bundle`);
      continue;
    }
    if (entry.bytes !== bytes(content)) errors.push(`${id}: ${entry.path} byte count differs from the manifest`);
    if (entry.sha256 !== sha256(content)) errors.push(`${id}: ${entry.path} hash differs from the manifest`);
  }
  let embedded;
  try {
    embedded = JSON.parse(bundle.files?.["provenance.json"] ?? "null");
  } catch {
    embedded = null;
  }
  if (JSON.stringify(embedded) !== JSON.stringify(bundle.provenance)) errors.push(`${id}: provenance.json is not the embedded provenance object`);
  const execution = bundle.provenance?.execution;
  if (!execution || execution.mode !== "input-file" || execution.input_file !== "data.json") {
    errors.push(`${id}: bundle does not declare an input-file executable replay contract`);
  }
  if (execution?.status !== "verified-in-ci") errors.push(`${id}: executable replay contract is not CI-verified`);
  if (!Array.isArray(execution?.consumes) || execution.consumes.length === 0) errors.push(`${id}: executable replay does not declare consumed inputs`);
  const sensitivity = execution?.sensitivity;
  if (!sensitivity || typeof sensitivity.path !== "string" || sensitivity.path.length === 0) {
    errors.push(`${id}: executable replay does not declare a sensitivity path`);
  }
  if (!Number.isFinite(sensitivity?.alternate)) errors.push(`${id}: executable replay sensitivity must declare a finite alternate value`);
  if (Array.isArray(execution?.consumes) && typeof sensitivity?.path === "string" && !execution.consumes.includes(sensitivity.path)) {
    errors.push(`${id}: executable replay sensitivity path is not listed in consumed inputs`);
  }
  const seeded = bundle.reproducibility?.level === "R1"
    || bundle.provenance?.randomness?.status === "input-driven-r1";
  if (seeded) {
    const randomness = execution?.randomness;
    const evidence = bundle.provenance?.randomness;
    if (!randomness || randomness.seed_path !== sensitivity?.path) {
      errors.push(`${id}: seeded replay must declare a randomness seed path equal to the sensitivity path`);
    }
    if (typeof randomness?.seed_path !== "string" || !execution?.consumes?.includes(randomness.seed_path)) {
      errors.push(`${id}: seeded randomness seed path must be listed in consumed inputs`);
    }
    if (randomness?.algorithm !== evidence?.algorithm || randomness?.version !== evidence?.version) {
      errors.push(`${id}: seeded randomness algorithm/version does not match provenance evidence`);
    }
    if (randomness?.partition !== evidence?.partition) {
      errors.push(`${id}: seeded randomness partition does not match provenance evidence`);
    }
    if (evidence?.status !== "input-driven-r1" || evidence?.seed_consumption !== `data.json:${randomness?.seed_path}`) {
      errors.push(`${id}: provenance does not prove input-driven seed consumption`);
    }
  }
  return errors;
}

function inputValue(input, dottedPath, id) {
  const parts = dottedPath.split(".").filter(Boolean);
  if (parts.length === 0) throw new Error(`${id}: sensitivity path is empty`);
  let value = input;
  for (const part of parts) {
    if (!value || typeof value !== "object" || !(part in value)) {
      throw new Error(`${id}: sensitivity path ${dottedPath} is missing from data.json`);
    }
    value = value[part];
  }
  return { parts, value };
}

function setInputValue(input, parts, value) {
  const parent = parts.slice(0, -1).reduce((current, part) => current[part], input);
  parent[parts.at(-1)] = value;
}

function alternateInput(bundle, id) {
  let input;
  try {
    input = JSON.parse(bundle.files["data.json"]);
  } catch (error) {
    throw new Error(`${id}: data.json is invalid JSON (${error.message})`);
  }
  const execution = bundle.provenance.execution;
  const sensitivity = execution.sensitivity;
  const { parts, value: original } = inputValue(input, sensitivity.path, id);
  if (!Number.isFinite(original)) throw new Error(`${id}: sensitivity path ${sensitivity.path} must resolve to a finite number`);
  const alternate = structuredClone(input);
  setInputValue(alternate, parts, sensitivity.alternate);
  if (sensitivity.alternate === original) throw new Error(`${id}: sensitivity alternate must differ from the recorded value`);
  return canonicalJson(alternate);
}

async function replay(id, { bindWasm = false, bundleText: suppliedBundleText, compilerWasmPath } = {}) {
  if (!/^[a-z0-9][a-z0-9-]*$/.test(id)) throw new Error(`invalid executable replay fixture ID ${JSON.stringify(id)}`);
  let bundleText = suppliedBundleText;
  if (bundleText === undefined) {
    if (!experimentFixtures[id]) throw new Error(`unknown executable replay fixture ${JSON.stringify(id)}; available: ${Object.keys(experimentFixtures).join(", ")}`);
    const bundlePath = outputPathFor(id);
    if (!existsSync(bundlePath)) throw new Error(`${id}: missing ${path.relative(repositoryRoot, bundlePath)}`);
    bundleText = readFileSync(bundlePath, "utf8");
  }
  const bundle = JSON.parse(bundleText);
  const errors = bundleErrors(bundle, id);
  if (errors.length) throw new Error(errors.join("\n"));

  const source = bundle.files["source.ostrin"];
  const input = bundle.files["data.json"];
  const inputFile = bundle.provenance.execution.input_file;
  const readPattern = new RegExp(`read_file\\(\\s*"${inputFile.replace(/[.*+?^${}()|[\\]\\]/g, "\\$&")}"\\s*\\)`);
  if (!readPattern.test(source)) throw new Error(`${id}: source does not read ${inputFile}`);

  const modulePath = compilerWasmPath ?? path.join(repositoryRoot, "website", "ostrinc.wasm");
  if (!existsSync(modulePath)) throw new Error(`${id}: compiler WASM is missing at ${modulePath}`);
  if (compilerWasmPath && !bindWasm) throw new Error(`${id}: an archived compiler requires a bound replay`);
  let wasmSha256;
  const filesystemExecutions = [];
  const execute = async (files) => {
    const observed = await runWasmWithFilesystemEvidence(files, ["--run", "main.ostrin"], modulePath);
    if (!Array.isArray(observed?.lines) || !/^sha256:[a-f0-9]{64}$/.test(observed?.wasmSha256 ?? "")) {
      throw new Error(`${id}: WASI child did not return valid execution evidence`);
    }
    if (wasmSha256 && observed.wasmSha256 !== wasmSha256) {
      throw new Error(`${id}: WASM module changed between replay executions`);
    }
    wasmSha256 = observed.wasmSha256;
    const readErrors = validateWasiReadEvidence(
      id,
      observed.filesystemReads,
      files,
      ["main.ostrin", inputFile],
    );
    if (readErrors.length) throw new Error(readErrors.join("\n"));
    filesystemExecutions.push({
      scenario: filesystemExecutions.length === 0 ? "baseline" : "subsequent-run",
      reads: sanitizeWasiReadEvents(observed.filesystemReads),
    });
    return observed.lines;
  };
  const files = { "main.ostrin": source, [inputFile]: input };
  const output = await execute(files);
  const recorded = bundle.provenance.figure_metadata;
  const metadata = {
    sourceHash: recorded.source_hash,
    dataHash: recorded.data_hash,
    seed: String(recorded.seed ?? "").replace(/^seed=/, ""),
    compiler: recorded.compiler,
  };
  const rendered = normalizeFigureMetadata(extractSvg(output, id), metadata);
  if (rendered !== bundle.files["figure.svg"]) throw new Error(`${id}: input-driven replay SVG differs from the published bundle`);
  const seeded = bundle.reproducibility?.level === "R1"
    || bundle.provenance?.randomness?.status === "input-driven-r1";
  if (seeded) {
    const repeatedOutput = await execute(files);
    const repeated = normalizeFigureMetadata(extractSvg(repeatedOutput, id), metadata);
    if (repeated !== rendered) {
      throw new Error(`${id}: same input and seed produced different SVG output on replay`);
    }
  }
  const invocationOutput = bundle.provenance.runtime.invocation.outputs.find((entry) => entry.path === "figure.svg");
  if (bytes(rendered) !== invocationOutput.bytes || sha256(rendered) !== invocationOutput.sha256) {
    throw new Error(`${id}: input-driven replay output hash differs from the runtime invocation record`);
  }

  const changedInput = alternateInput(bundle, id);
  const changedRunIndex = filesystemExecutions.length;
  const changedOutput = await execute({ "main.ostrin": source, [inputFile]: changedInput });
  filesystemExecutions[changedRunIndex].scenario = "changed-input";
  const changedSvg = normalizeFigureMetadata(extractSvg(changedOutput, id), metadata);
  if (changedSvg === rendered) throw new Error(`${id}: changing a declared input did not change the rendered figure`);

  return {
    id,
    input: inputFile,
    consumed: bundle.provenance.execution.consumes,
    baseline: "match",
    sensitivity: "changed-output",
    ...(seeded ? { seeded: "same-seed-and-changed-output" } : {}),
    ...(bindWasm ? { filesystemReadEvidence: {
      schema: filesystemEvidenceSchema,
      scope: "regular files read through WASI Preview 1 path_open/fd_read only",
      executions: filesystemExecutions,
    } } : {}),
    ...(bindWasm ? {
      wasmSha256,
      bundleSha256: sha256(bundleText),
      fileHashes: Object.fromEntries(["source.ostrin", "data.json", "figure.svg", "provenance.json"]
        .map((name) => [name, sha256(bundle.files[name])])),
    } : {}),
  };
}

export async function replayAll() {
  const ids = listExecutableReplayIds();
  if (ids.length === 0) throw new Error("no R1/R2 experiment bundles are registered for executable replay");
  const results = [];
  for (const id of ids) results.push(await replay(id));
  return results;
}

// R0 fixtures use recorded parameters compiled into source. Replaying them from
// an archived bundle proves that the archived source and WASM reproduce the
// recorded output under the same WASI host; it does not claim cross-platform
// equivalence or scientific validity.
export async function replayRecordedR0(id, { bundleText: suppliedBundleText, compilerWasmPath } = {}) {
  if (!/^[a-z0-9][a-z0-9-]*$/.test(id)) throw new Error(`invalid archived R0 fixture ID ${JSON.stringify(id)}`);
  const bundleText = suppliedBundleText ?? readFileSync(outputPathFor(id), "utf8");
  let bundle;
  try {
    bundle = JSON.parse(bundleText);
  } catch (error) {
    throw new Error(`${id}: archived bundle is invalid JSON: ${error.message}`);
  }
  if (bundle?.schema !== schema || bundle?.id !== id
    || bundle?.reproducibility?.level !== "R0"
    || bundle?.provenance?.level !== "R0"
    || bundle?.provenance?.reproducibility?.level !== "R0") {
    throw new Error(`${id}: archived fixture is not a consistent R0 bundle`);
  }
  const execution = bundle.provenance.execution;
  const source = bundle.files?.["source.ostrin"];
  const data = bundle.files?.["data.json"];
  const recordedFigure = bundle.files?.["figure.svg"];
  const sourceHash = sha256(source ?? "");
  const dataHash = sha256(data ?? "");
  const figureHash = sha256(recordedFigure ?? "");
  const manifest = bundle.manifest?.files;
  const expectedManifestPaths = ["data.json", "figure.svg", "provenance.json", "source.ostrin"];
  const manifestPaths = Array.isArray(manifest)
    ? manifest.map((entry) => entry.path).sort()
    : [];
  const manifestValid = Array.isArray(manifest) && manifest.length === expectedManifestPaths.length
    && JSON.stringify(manifestPaths) === JSON.stringify(expectedManifestPaths);
  const manifestHashesValid = manifestValid && manifest.every((entry) => {
    const content = bundle.files?.[entry.path];
    return typeof content === "string" && entry.bytes === bytes(content) && entry.sha256 === sha256(content);
  });
  const figureEntry = manifest?.find((entry) => entry.path === "figure.svg");
  let embeddedProvenance;
  try { embeddedProvenance = JSON.parse(bundle.files?.["provenance.json"] ?? "null"); }
  catch { embeddedProvenance = null; }
  const experiment = bundle.experiment;
  const inputRecord = bundle.provenance.inputs?.find((entry) => entry.id === "data.json");
  const outputRecord = bundle.provenance.outputs?.find((entry) => entry.id === "figure.svg");
  const figureMetadata = bundle.provenance.figure_metadata;
  const embeddedMetadataText = recordedFigure?.match(/<ostrin-provenance\s+([^>]+?)\s*\/>/)?.[1] ?? "";
  const embeddedMetadata = Object.fromEntries(
    [...embeddedMetadataText.matchAll(/([a-z-]+)="([^"]*)"/g)].map(([, key, value]) => [key, value]),
  );
  const invocationOutput = execution?.mode === "recorded-parameters"
    ? bundle.provenance.runtime?.invocation?.outputs?.find((entry) => entry.path === "figure.svg")
    : null;
  if (execution?.mode !== "recorded-parameters"
    || !Array.isArray(execution.capabilities) || execution.capabilities.length !== 0
    || typeof source !== "string" || typeof data !== "string" || typeof recordedFigure !== "string"
    || !manifestHashesValid
    || JSON.stringify(embeddedProvenance) !== JSON.stringify(bundle.provenance)
    || experiment?.source !== "source.ostrin" || experiment?.inputs !== "data.json"
    || experiment?.figure !== "figure.svg" || experiment?.provenance !== "provenance.json"
    || bundle.provenance.program?.source !== "source.ostrin" || bundle.provenance.program?.entry !== "main"
    || /\bread_file\s*\(/.test(source)
    || bundle.provenance.program?.source_hash !== sourceHash
    || figureMetadata?.source_hash !== sourceHash
    || inputRecord?.sha256 !== dataHash
    || figureMetadata?.data_hash !== dataHash
    || outputRecord?.sha256 !== figureHash
    || embeddedMetadata["source-hash"] !== sourceHash
    || embeddedMetadata["data-hash"] !== dataHash
    || embeddedMetadata.seed !== figureMetadata?.seed
    || embeddedMetadata.compiler !== figureMetadata?.compiler
    || !invocationOutput || invocationOutput.sha256 !== figureEntry?.sha256
    || invocationOutput.bytes !== figureEntry?.bytes) {
    throw new Error(`${id}: R0 source, parameter-only execution or recorded output contract is incomplete`);
  }
  const effects = bundle.provenance.effects;
  const entryEffects = effects?.entry_effect_signature?.effects;
  const effectErrors = [
    ...validateEffectCapture(effects),
    ...validateEntryEffectSignature(effects?.entry_effect_signature, "main", effects),
  ];
  if (effectErrors.length || effects?.entry_effect_signature?.row_closed !== true
    || effects.entry_effect_signature.variables?.length !== 0
    || effects.entry_effect_signature.open_calls?.length !== 0
    || !Array.isArray(entryEffects)
    || entryEffects.some((effect) => !["io", "state(local)"].includes(effect))) {
    throw new Error(`${id}: R0 replay requires a closed entry row with only output I/O and local state: ${effectErrors.join("; ")}`);
  }
  const modulePath = compilerWasmPath ?? path.join(repositoryRoot, "website", "ostrinc.wasm");
  if (!existsSync(modulePath)) throw new Error(`${id}: compiler WASM is missing at ${modulePath}`);
  const metadata = {
    sourceHash: bundle.provenance.figure_metadata?.source_hash,
    dataHash: bundle.provenance.figure_metadata?.data_hash,
    seed: String(bundle.provenance.figure_metadata?.seed ?? "").replace(/^seed=/, ""),
    compiler: bundle.provenance.figure_metadata?.compiler,
  };
  let wasmSha256;
  const filesystemExecutions = [];
  let executionCount = 0;
  const execute = async () => {
    const files = { "main.ostrin": source };
    const observed = await runWasmWithFilesystemEvidence(files, ["--run", "main.ostrin"], modulePath);
    if (!Array.isArray(observed?.lines) || !/^sha256:[a-f0-9]{64}$/.test(observed?.wasmSha256 ?? "")) {
      throw new Error(`${id}: WASI child did not return a valid module execution digest`);
    }
    if (wasmSha256 && observed.wasmSha256 !== wasmSha256) {
      throw new Error(`${id}: WASM module changed between R0 replay executions`);
    }
    wasmSha256 = observed.wasmSha256;
    const readErrors = validateWasiReadEvidence(id, observed.filesystemReads, files, ["main.ostrin"]);
    if (readErrors.length) throw new Error(readErrors.join("\n"));
    filesystemExecutions.push({
      scenario: executionCount++ === 0 ? "baseline" : "repeatability",
      reads: sanitizeWasiReadEvents(observed.filesystemReads),
    });
    const rendered = normalizeFigureMetadata(extractSvg(observed.lines, id), metadata);
    if (rendered !== recordedFigure) throw new Error(`${id}: archived R0 source replay differs from its recorded SVG`);
    if (bytes(rendered) !== invocationOutput.bytes || sha256(rendered) !== invocationOutput.sha256) {
      throw new Error(`${id}: replayed output hash differs from the archived invocation record`);
    }
    return rendered;
  };
  const first = await execute();
  const second = await execute();
  if (first !== second) throw new Error(`${id}: identical R0 inputs produced different outputs`);
  return {
    id,
    level: "R0",
    baseline: "match",
    repeatability: "same-output",
    wasmSha256,
    bundleSha256: sha256(bundleText),
    filesystemReadEvidence: {
      schema: filesystemEvidenceSchema,
      scope: "regular files read through WASI Preview 1 path_open/fd_read only",
      executions: filesystemExecutions,
    },
  };
}

export function readArchivedReplayBundle(directory, id, expectedRun) {
  if (!/^[a-z0-9][a-z0-9-]*$/.test(id)) throw new Error(`invalid archived replay fixture ID ${JSON.stringify(id)}`);
  const root = realpathSync(directory);
  const relativePath = `website/assets/experiments/${id}.ostrin-experiment.json`;
  const archived = readArchivedFile(root, relativePath, 16 * 1024 * 1024);
  const bundleText = archived.bytes.toString("utf8");
  let bundle;
  try {
    bundle = JSON.parse(bundleText);
  } catch (error) {
    throw new Error(`${id}: archived bundle is invalid JSON: ${error.message}`);
  }
  if (!bundle || typeof bundle !== "object" || Array.isArray(bundle)) {
    throw new Error(`${id}: archived bundle is not an object`);
  }
  const inventory = expectedRun?.replayBundles?.[id];
  const expectedLevel = inventory?.level;
  const errors = expectedLevel === "R0" ? [] : bundleErrors(bundle, id);
  if (!inventory || !["R0", "R1", "R2"].includes(expectedLevel)) {
    errors.push(`${id}: archived run does not register this fixture as an executable R0/R1/R2 bundle`);
  }
  if (bundle.reproducibility?.level !== expectedLevel
    || bundle.provenance?.level !== expectedLevel
    || bundle.provenance?.reproducibility?.level !== expectedLevel) {
    errors.push(`${id}: archived reproducibility level differs from the tested run inventory`);
  }
  if (inventory?.bundleSha256 !== sha256(bundleText)) {
    errors.push(`${id}: archived bundle bytes differ from the tested run inventory`);
  }
  const manifest = bundle?.manifest?.files;
  if (!Array.isArray(manifest) || manifest.length !== 4) {
    errors.push(`${id}: archived manifest must contain exactly four files`);
  } else {
    for (const entry of manifest) {
      const content = bundle.files?.[entry.path];
      if (typeof content !== "string" || entry.bytes !== bytes(content) || entry.sha256 !== sha256(content)) {
        errors.push(`${id}: archived ${entry.path} differs from its manifest`);
      }
    }
  }
  let embeddedProvenance;
  try { embeddedProvenance = JSON.parse(bundle.files?.["provenance.json"] ?? "null"); }
  catch { embeddedProvenance = null; }
  if (JSON.stringify(embeddedProvenance) !== JSON.stringify(bundle.provenance)) {
    errors.push(`${id}: embedded provenance differs from the archived bundle`);
  }
  const effects = bundle.provenance?.effects;
  errors.push(...validateEffectCapture(effects).map((error) => `${id}: ${error}`));
  errors.push(...validateEntryEffectSignature(effects?.entry_effect_signature, "main", effects).map((error) => `${id}: ${error}`));
  if (effects?.entry_effect_signature?.row_closed !== true
    || effects?.entry_effect_signature?.variables?.length !== 0
    || effects?.entry_effect_signature?.open_calls?.length !== 0) {
    errors.push(`${id}: main effect row is not closed`);
  }
  const readiness = inspectBundle(bundle, relativePath);
  if (readiness.status !== "READY") {
    errors.push(`${id}: archived bundle structural capture is ${readiness.status}`);
  }
  const ciRun = bundle.provenance?.runtime?.host?.ci_run;
  if (ciRun?.run_id !== expectedRun?.runId || ciRun?.tested_sha !== expectedRun?.testedSha) {
    errors.push(`${id}: host capture does not identify the requested CI run and tested SHA`);
  }
  if (errors.length) throw new Error(errors.join("; "));
  return { bundleText, bundle, path: archived.path };
}

async function main() {
  const args = process.argv.slice(2);
  if (args[0] === "--replay-artifact-all") {
    const [mode, directory, runFlag, runId, shaFlag, testedSha] = args;
    if (args.length !== 6 || mode !== "--replay-artifact-all" || runFlag !== "--run-id"
      || shaFlag !== "--tested-sha" || !/^\d+$/.test(runId ?? "") || !/^[a-f0-9]{40}$/.test(testedSha ?? "")) {
      throw new Error("usage: node scripts/experiment-replay.mjs --replay-artifact-all <directory> --run-id <id> --tested-sha <sha>");
    }
    const expectedRun = { runId, testedSha };
    const surface = verifySurfaceArtifact(directory, expectedRun);
    const ids = Object.keys(surface.receipt.replayBundles ?? {});
    expectedRun.replayBundles = surface.receipt.replayBundles;
    if (ids.length === 0) throw new Error("archived receipt is missing the tested run's executable replay inventory");
    if (!ids.includes("surface")) throw new Error("surface must be in the executable replay registry to bind the archived WASM");
    const moduleSha256 = surface.receipt.compilerWasmSha256;
    const replayResults = [];
    for (const id of ids) {
      const artifact = id === "surface" ? surface : readArchivedReplayBundle(directory, id, expectedRun);
      const level = expectedRun.replayBundles[id]?.level;
      const result = level === "R0"
        ? await replayRecordedR0(id, {
          bundleText: artifact.bundleText,
          compilerWasmPath: surface.wasmPath,
        })
        : await replay(id, {
          bindWasm: true,
          bundleText: artifact.bundleText,
          compilerWasmPath: surface.wasmPath,
        });
      if (result.wasmSha256 !== moduleSha256) {
        throw new Error(`${id}: archived replay loaded WASM bytes that differ from the run-bound surface receipt`);
      }
      replayResults.push(result);
      if (id === "surface"
        && JSON.stringify(createSurfaceReceipt(artifact.bundleText, result, surface.receipt.replayBundles))
          !== JSON.stringify(surface.receipt)) {
        throw new Error("surface: fresh replay differs from the archived receipt");
      }
      const replayDetails = level === "R0"
        ? `${result.baseline}; ${result.repeatability}`
        : `${result.baseline}; ${result.sensitivity}${result.seeded ? `; ${result.seeded}` : ""}`;
      console.log(`experiment-replay: independently replayed ${id} (${level}) from archived WASM in CI run ${runId} (${replayDetails})`);
    }
    const readEvidence = createWasiFileReadEvidenceReport(replayResults, expectedRun, moduleSha256, expectedRun.replayBundles);
    const evidencePath = path.join(repositoryRoot, "target/r3-ci/wasi-file-read-evidence.json");
    mkdirSync(path.dirname(evidencePath), { recursive: true });
    writeFileSync(evidencePath, `${JSON.stringify(readEvidence, null, 2)}\n`, "utf8");
    console.log(`experiment-replay: wrote run-bound runtime file-read evidence for ${ids.length} bundles`);
    console.log(`experiment-replay: independently replayed all ${ids.length} bundles in the archived R0/R1/R2 inventory`);
    return;
  }
  if (args[0] === "--replay-artifact") {
    const [mode, directory, runFlag, runId, shaFlag, testedSha] = args;
    if (args.length !== 6 || mode !== "--replay-artifact" || runFlag !== "--run-id"
      || shaFlag !== "--tested-sha" || !/^\d+$/.test(runId ?? "") || !/^[a-f0-9]{40}$/.test(testedSha ?? "")) {
      throw new Error("usage: node scripts/experiment-replay.mjs --replay-artifact <directory> --run-id <id> --tested-sha <sha>");
    }
    const artifact = verifySurfaceArtifact(directory, { runId, testedSha });
    const replayed = await replay("surface", {
      bindWasm: true,
      bundleText: artifact.bundleText,
      compilerWasmPath: artifact.wasmPath,
    });
    if (JSON.stringify(createSurfaceReceipt(artifact.bundleText, replayed)) !== JSON.stringify(artifact.receipt)) {
      throw new Error("surface: fresh replay differs from the archived receipt");
    }
    console.log(`experiment-replay: independently replayed surface from archived WASM in CI run ${runId} (${replayed.baseline}; ${replayed.sensitivity})`);
    return;
  }
  if (args.includes("--all")) {
    if (args.includes("--write-r3-receipt")) throw new Error("--write-r3-receipt requires --fixture surface");
    if (args.some((argument) => argument === "--fixture" || argument.startsWith("--fixture="))) {
      throw new Error("--all cannot be combined with --fixture");
    }
    const results = await replayAll();
    for (const result of results) {
      const seeded = result.seeded ? `; ${result.seeded}` : "";
      console.log(`experiment-replay: verified ${result.id} (${result.input}; ${result.sensitivity}${seeded})`);
    }
    console.log(`experiment-replay: verified all ${results.length} R1/R2 bundles`);
    return;
  }
  const id = fixtureArgument(args);
  if (!args.includes("--fixture") && !args.some((argument) => argument.startsWith("--fixture="))) {
    throw new Error("usage: node scripts/experiment-replay.mjs --fixture <id> | --all");
  }
  const writeR3Receipt = args.includes("--write-r3-receipt");
  if (writeR3Receipt && id !== "surface") throw new Error("--write-r3-receipt is supported only for surface");
  const result = await replay(id, { bindWasm: writeR3Receipt });
  if (writeR3Receipt) {
    const receipt = createSurfaceReceipt(readFileSync(outputPathFor(id), "utf8"), result);
    mkdirSync(path.dirname(receiptPath), { recursive: true });
    writeFileSync(receiptPath, `${JSON.stringify(receipt, null, 2)}\n`, "utf8");
    console.log(`experiment-replay: wrote ${path.relative(repositoryRoot, receiptPath)}`);
  }
  const seeded = result.seeded ? `; ${result.seeded}` : "";
  console.log(`experiment-replay: verified ${result.id} (${result.input}; ${result.sensitivity}${seeded})`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  main().catch((error) => {
    console.error(`experiment-replay: ${error.message}`);
    process.exitCode = 1;
  });
}

export { bundleErrors, extractSvg, replay };
