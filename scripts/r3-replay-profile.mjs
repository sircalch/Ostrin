import { createHash } from "node:crypto";

const schema = "ostrin.r3-wasi-host-replay-receipt/v1";
const digestPattern = /^sha256:[a-f0-9]{64}$/;
const commitPattern = /^[a-f0-9]{40}$/;
const reproducibilityLevels = ["R0", "R1", "R2"];
const artifactPaths = [
  "validation/r3-readiness.json",
  "validation/quality-scorecard.json",
  "website/status-data.js",
  "target/r3-ci/surface-replay-receipt.json",
  "target/r3-ci/wasi-file-read-evidence.json",
  "target/r3-ci/wasi-random-evidence.json",
];

export function sha256(bytes) {
  return `sha256:${createHash("sha256").update(bytes).digest("hex")}`;
}

export function canonicalJson(value) {
  return `${JSON.stringify(value, null, 2)}\n`;
}

function requireCondition(condition, message) {
  if (!condition) throw new Error(`R3 WASI host replay profile: ${message}`);
}

function sortedKeys(value) {
  return Object.keys(value ?? {}).sort((left, right) => left < right ? -1 : left > right ? 1 : 0);
}

function parseJsonFile(files, relativePath) {
  try {
    return JSON.parse(files[relativePath].bytes.toString("utf8"));
  } catch (error) {
    throw new Error(`R3 WASI host replay profile: ${relativePath} is invalid JSON (${error.message})`);
  }
}

function validateReportCounts(readiness, scorecard, expectedCount, expectedLevelCounts) {
  requireCondition(readiness?.status === "READY", "R3 readiness report is not READY");
  requireCondition(readiness.inventory?.status === "READY"
    && readiness.inventory.expectedCount === expectedCount
    && readiness.inventory.discoveredCount === expectedCount
    && readiness.inventory.missing?.length === 0
    && readiness.inventory.unexpected?.length === 0,
  "R3 readiness does not certify the complete registered inventory");
  for (const field of ["bundleCount", "readyCount", "captureReadyCount"]) {
    requireCondition(readiness[field] === expectedCount, `readiness ${field} does not cover every fixture`);
  }
  requireCondition(readiness.openCount === 0 && readiness.captureOpenCount === 0,
    "R3 readiness reports open bundles");
  requireCondition(readiness.closedEntryEffectBundleCount === expectedCount,
    "closed entry effect rows do not cover every fixture");

  const r3 = scorecard?.r3;
  requireCondition(scorecard?.reproducibility?.R3 === 0,
    "the run-bound profile must not promote static fixture maturity to R3");
  requireCondition(r3?.status === "READY" && r3.bundleCount === expectedCount
    && r3.readyCount === expectedCount && r3.captureReadyCount === expectedCount
    && r3.openCount === 0 && r3.captureOpenCount === 0,
  "quality scorecard R3 capture summary is not complete");
  const levelTotal = reproducibilityLevels.reduce((total, level) => total + (scorecard?.reproducibility?.[level] ?? 0), 0);
  requireCondition(levelTotal === expectedCount,
    "static R0/R1/R2 fixture labels do not reconcile with the complete inventory");
  for (const level of reproducibilityLevels) {
    requireCondition(scorecard.reproducibility[level] === expectedLevelCounts[level],
      `static ${level} count differs from the archived fixture inventory`);
  }
}

function validateBundleAndReplay({ id, level, bundle, bundleText, result, expectedRun, compilerWasmSha256 }) {
  requireCondition(bundle?.schema === "ostrin.experiment/v0" && bundle.id === id,
    `${id}: bundle identity or schema differs from the replay inventory`);
  requireCondition(reproducibilityLevels.includes(level)
    && bundle.reproducibility?.level === level
    && bundle.provenance?.level === level
    && bundle.provenance?.reproducibility?.level === level,
  `${id}: static reproducibility labels disagree`);
  requireCondition(result?.id === id
    && result?.bundleSha256 === sha256(Buffer.from(bundleText ?? "", "utf8")),
    `${id}: replay result does not identify the archived bundle`);
  requireCondition(result?.wasmSha256 === compilerWasmSha256,
    `${id}: replay loaded a different compiler WASM`);
  requireCondition(result?.baseline === "match", `${id}: baseline replay did not match`);
  requireCondition(level === "R0"
    ? result.repeatability === "same-output"
    : result.sensitivity === "changed-output",
  `${id}: replay did not meet the recorded level's execution contract`);
  const seedEvidence = bundle.provenance?.randomness?.status === "input-driven-r1";
  if (seedEvidence) {
    requireCondition(result.seeded === "same-seed-and-changed-output",
      `${id}: recorded seeded replay contract was not verified`);
  }

  const ciRun = bundle.provenance?.runtime?.host?.ci_run;
  requireCondition(bundle.provenance?.runtime?.host?.source === "github-actions"
    && ciRun?.run_id === expectedRun.runId
    && ciRun?.tested_sha === expectedRun.testedSha,
  `${id}: host capture does not identify this CI run and tested revision`);
  const signature = bundle.provenance?.effects?.entry_effect_signature;
  requireCondition(signature?.row_closed === true
    && Array.isArray(signature.variables) && signature.variables.length === 0
    && Array.isArray(signature.open_calls) && signature.open_calls.length === 0,
  `${id}: entry effect row is not closed`);

  const manifest = bundle.manifest?.files;
  const fileNames = sortedKeys(bundle.files);
  requireCondition(Array.isArray(manifest) && manifest.length === fileNames.length
    && JSON.stringify([...manifest].map((entry) => entry.path).sort()) === JSON.stringify(fileNames),
  `${id}: bundle manifest does not cover the archived files`);
  const manifestFiles = {};
  for (const entry of manifest) {
    const contents = bundle.files[entry.path];
    const contentsBytes = Buffer.from(contents ?? "", "utf8");
    requireCondition(typeof contents === "string"
      && entry.bytes === contentsBytes.byteLength
      && entry.sha256 === sha256(contentsBytes),
    `${id}: ${entry.path} differs from its manifest digest`);
    manifestFiles[entry.path] = { bytes: entry.bytes, sha256: entry.sha256 };
  }
  requireCondition(manifestFiles["source.ostrin"] && manifestFiles["figure.svg"]
    && manifestFiles["provenance.json"], `${id}: source, output, or provenance is absent`);

  return {
    id,
    recordedLevel: level,
    sourceRevision: bundle.provenance.commit,
    bundleSha256: result.bundleSha256,
    files: manifestFiles,
    entryEffectSignatureSha256: sha256(Buffer.from(canonicalJson(signature), "utf8")),
    replay: {
      baseline: result.baseline,
      repeatability: result.repeatability ?? null,
      sensitivity: result.sensitivity ?? null,
      seeded: result.seeded ?? null,
    },
  };
}

export function createR3WasiHostReplayReceipt({
  runId,
  testedSha,
  replayBundles,
  replayResults,
  bundleRecords,
  files,
}) {
  requireCondition(/^\d+$/.test(runId ?? "") && commitPattern.test(testedSha ?? ""),
    "run ID or tested revision is invalid");
  const ids = sortedKeys(replayBundles);
  requireCondition(ids.length > 0, "replay inventory is empty");
  const resultById = new Map(replayResults.map((result) => [result.id, result]));
  const recordById = new Map(bundleRecords.map((record) => [record.id, record]));
  requireCondition(resultById.size === ids.length && recordById.size === ids.length
    && ids.every((id) => resultById.has(id) && recordById.has(id)),
  "replay results do not cover the complete registered inventory");
  requireCondition(ids.every((id) => Object.hasOwn(replayBundles, id)),
    "replay inventory is malformed");

  const surfaceBundle = recordById.get("surface")?.bundle;
  requireCondition(surfaceBundle?.id === "surface", "surface must anchor the compiler WASM and run identity");
  const host = surfaceBundle.provenance?.runtime?.host;
  const ciRun = host?.ci_run;
  requireCondition(host?.source === "github-actions"
    && host.wasi === "preview1"
    && typeof host.runner_os === "string" && host.runner_os !== "unknown"
    && typeof host.runner_arch === "string" && host.runner_arch !== "unknown"
    && ciRun?.run_id === runId && ciRun?.tested_sha === testedSha,
  "surface host capture does not identify this CI run and tested revision");
  const compilerWasmSha256 = recordById.get("surface")?.result?.wasmSha256;
  requireCondition(digestPattern.test(compilerWasmSha256 ?? ""), "compiler WASM digest is invalid");
  const surfaceReceipt = parseJsonFile(files, "target/r3-ci/surface-replay-receipt.json");
  requireCondition(surfaceReceipt.schema === "ostrin.r3-replay-receipt/v1"
    && surfaceReceipt.fixture === "surface"
    && JSON.stringify(surfaceReceipt.ciRun) === JSON.stringify(ciRun)
    && surfaceReceipt.compilerWasmSha256 === compilerWasmSha256,
  "surface replay receipt is not bound to this run and compiler");
  requireCondition(JSON.stringify(surfaceReceipt.replayBundles) === JSON.stringify(replayBundles),
    "surface receipt inventory differs from the replayed inventory");

  const readiness = parseJsonFile(files, "validation/r3-readiness.json");
  const scorecard = parseJsonFile(files, "validation/quality-scorecard.json");
  const fileReadEvidence = parseJsonFile(files, "target/r3-ci/wasi-file-read-evidence.json");
  const randomEvidence = parseJsonFile(files, "target/r3-ci/wasi-random-evidence.json");
  const counts = ids.reduce((total, id) => {
    total[replayBundles[id].level] += 1;
    return total;
  }, { R0: 0, R1: 0, R2: 0 });
  validateReportCounts(readiness, scorecard, ids.length, counts);
  requireCondition(Array.isArray(readiness.entries) && readiness.entries.length === ids.length
    && JSON.stringify(sortedKeys(Object.fromEntries(readiness.entries.map((entry) => [entry.id, true]))))
    === JSON.stringify(ids)
    && readiness.entries.every((entry) => entry.status === "READY"),
  "R3 readiness entries do not cover the exact replay inventory");
  const developmentEvidence = scorecard.developmentEvidence;
  requireCondition(developmentEvidence?.schema === "ostrin.ci-evidence/v1"
    && developmentEvidence.repository === ciRun.repository
    && developmentEvidence.workflow === ciRun.workflow
    && String(developmentEvidence.runId) === runId
    && String(developmentEvidence.runAttempt) === ciRun.run_attempt
    && developmentEvidence.headSha === ciRun.head_sha
    && developmentEvidence.testedSha === testedSha
    && developmentEvidence.ref === ciRun.ref
    && developmentEvidence.runUrl === ciRun.url,
  "quality scorecard CI evidence does not identify this run and tested revision");
  for (const [name, evidence] of [["file-read", fileReadEvidence], ["random", randomEvidence]]) {
    requireCondition(evidence.ciRun?.run_id === runId
      && evidence.ciRun?.tested_sha === testedSha
      && evidence.compilerWasmSha256 === compilerWasmSha256
      && JSON.stringify(sortedKeys(evidence.replayBundles)) === JSON.stringify(ids),
    `${name} evidence does not cover the exact run, compiler, and fixture inventory`);
    requireCondition(ids.every((id) => evidence.replayBundles[id]?.bundleSha256 === replayBundles[id].bundleSha256
      && evidence.replayBundles[id]?.level === replayBundles[id].level),
    `${name} evidence contains a mismatched bundle or static level`);
  }

  const replayInventory = Object.fromEntries(ids.map((id) => {
    const inventory = replayBundles[id];
    const record = recordById.get(id);
    const result = resultById.get(id);
    requireCondition(inventory.level === record.level && inventory.bundleSha256 === result.bundleSha256,
      `${id}: replay inventory, bundle, and result disagree`);
    return [id, validateBundleAndReplay({
      id,
      level: inventory.level,
      bundle: record.bundle,
      bundleText: record.bundleText,
      result,
      expectedRun: { runId, testedSha },
      compilerWasmSha256,
    })];
  }));
  const evidenceHashes = Object.fromEntries(artifactPaths.map((relativePath) => {
    const artifact = files[relativePath];
    requireCondition(Buffer.isBuffer(artifact?.bytes) && artifact.bytes.length > 0,
      `required artifact file ${relativePath} is missing`);
    return [relativePath, sha256(artifact.bytes)];
  }));
  const receipt = {
    schema,
    status: "VERIFIED",
    profile: {
      id: "wasi-preview1-host-bound-replay",
      version: 1,
      scope: "all registered visualization fixtures replayed from the exact archived artifact under Node WASI Preview 1 on one declared GitHub Actions runner",
      claims: [
        "The archived bundle bytes, embedded file manifests, compiler WASM digest, run identity, and tested revision are linked.",
        "R0 outputs matched twice; R1/R2 baseline and declared sensitivity checks matched; seeded contracts were replayed with the recorded deterministic test input.",
        "The checked-in fixture maturity labels remain R0/R1/R2; this per-run profile does not promote reproducibility.R3.",
      ],
      limitations: [
        "One Node WASI Preview 1 runner only; no cross-platform numerical equivalence is established.",
        "GitHub Actions metadata is run-bound but is not a cryptographic attestation or an independently signed build provenance statement.",
        "The experimental effect analyzer and observed WASI file-read trace do not prove complete dynamic effect or external-input enumeration.",
        "Replay agreement does not independently validate the scientific correctness of each figure or model.",
        "The deterministic random_get provider is test-only and is not suitable for cryptographic use.",
      ],
    },
    ciRun: {
      repository: ciRun.repository,
      workflow: ciRun.workflow,
      event: ciRun.event,
      ref: ciRun.ref,
      run_id: ciRun.run_id,
      run_attempt: ciRun.run_attempt,
      head_sha: ciRun.head_sha,
      tested_sha: ciRun.tested_sha,
      url: ciRun.url,
    },
    host: {
      runner_os: host.runner_os,
      runner_arch: host.runner_arch,
      runner_environment: host.runner_environment,
      image: host.image,
      node: host.node,
      hardware: host.hardware,
      wasi: host.wasi,
    },
    compiler: { sourceRevision: testedSha, wasmSha256: compilerWasmSha256 },
    effectInventory: {
      knownCalls: readiness.knownEffectCallCount,
      symbolicCalls: readiness.symbolicEffectCallCount,
      unresolvedCalls: readiness.unresolvedEffectCallCount,
      closedEntryRows: readiness.closedEntryEffectBundleCount,
    },
    fixtureMaturity: { ...counts, R3: scorecard.reproducibility.R3 },
    inventory: { count: ids.length, ids, replayBundles },
    replayInventorySha256: sha256(Buffer.from(canonicalJson(replayBundles), "utf8")),
    evidenceHashes,
    bundles: replayInventory,
  };
  validateR3WasiHostReplayReceipt(receipt, { runId, testedSha });
  return receipt;
}

export function validateR3WasiHostReplayReceipt(receipt, expectedRun = {}) {
  const errors = [];
  if (receipt?.schema !== schema || receipt?.status !== "VERIFIED") errors.push("unsupported schema or non-verified status");
  if (receipt?.profile?.id !== "wasi-preview1-host-bound-replay" || receipt.profile.version !== 1) {
    errors.push("bounded profile identity is invalid");
  }
  if (!/^\d+$/.test(receipt?.ciRun?.run_id ?? "")
    || !/^[1-9][0-9]*$/.test(receipt?.ciRun?.run_attempt ?? "")
    || !commitPattern.test(receipt?.ciRun?.tested_sha ?? "")
    || !commitPattern.test(receipt?.ciRun?.head_sha ?? "")
    || receipt?.compiler?.sourceRevision !== receipt?.ciRun?.tested_sha
    || !commitPattern.test(receipt?.compiler?.sourceRevision ?? "")
    || !digestPattern.test(receipt?.compiler?.wasmSha256 ?? "")) {
    errors.push("run, source revision, or compiler digest is invalid");
  }
  if (expectedRun.runId && receipt?.ciRun?.run_id !== expectedRun.runId) errors.push("run ID differs from the requested run");
  if (expectedRun.testedSha && receipt?.ciRun?.tested_sha !== expectedRun.testedSha) errors.push("tested revision differs from the requested revision");
  const ids = receipt?.inventory?.ids;
  const inventoryIds = sortedKeys(receipt?.inventory?.replayBundles);
  const bundleIds = sortedKeys(receipt?.bundles);
  if (!Array.isArray(ids) || JSON.stringify(ids) !== JSON.stringify(inventoryIds)
    || JSON.stringify(ids) !== JSON.stringify(bundleIds) || ids.length !== receipt?.inventory?.count
    || ids.length === 0 || new Set(ids).size !== ids.length) {
    errors.push("bundle receipt does not cover the complete inventory");
  }
  if (!digestPattern.test(receipt?.replayInventorySha256 ?? "")
    || receipt.replayInventorySha256 !== sha256(Buffer.from(canonicalJson(receipt?.inventory?.replayBundles ?? {}), "utf8"))) {
    errors.push("inventory digest is invalid");
  }
  if (receipt?.fixtureMaturity?.R3 !== 0) errors.push("static fixture maturity must remain R3=0");
  for (const level of reproducibilityLevels) {
    if (!Number.isInteger(receipt?.fixtureMaturity?.[level]) || receipt.fixtureMaturity[level] < 0) {
      errors.push(`static ${level} fixture count is invalid`);
    }
  }
  if (reproducibilityLevels.reduce((sum, level) => sum + (receipt?.fixtureMaturity?.[level] ?? 0), 0) !== ids?.length) {
    errors.push("static fixture maturity counts do not reconcile with the inventory");
  }
  if (receipt?.effectInventory?.closedEntryRows !== ids?.length
    || !Number.isInteger(receipt?.effectInventory?.knownCalls)
    || !Number.isInteger(receipt?.effectInventory?.symbolicCalls)
    || !Number.isInteger(receipt?.effectInventory?.unresolvedCalls)) {
    errors.push("effect inventory summary is incomplete");
  }
  for (const relativePath of artifactPaths) {
    if (!digestPattern.test(receipt?.evidenceHashes?.[relativePath] ?? "")) errors.push(`missing digest for ${relativePath}`);
  }
  for (const id of ids ?? []) {
    const bundle = receipt.bundles?.[id];
    const inventory = receipt.inventory?.replayBundles?.[id];
    if (!bundle || bundle.id !== id || bundle.recordedLevel !== inventory?.level
      || bundle.bundleSha256 !== inventory?.bundleSha256
      || bundle.replay?.baseline !== "match"
      || !digestPattern.test(bundle.bundleSha256 ?? "")
      || !commitPattern.test(bundle.sourceRevision ?? "")
      || !digestPattern.test(bundle.entryEffectSignatureSha256 ?? "")) {
      errors.push(`${id}: bundle-level receipt is incomplete or inconsistent`);
      continue;
    }
    const expectedReplay = bundle.recordedLevel === "R0"
      ? bundle.replay.repeatability === "same-output"
      : bundle.replay.sensitivity === "changed-output";
    if (!expectedReplay) errors.push(`${id}: replay result does not match its recorded level`);
    for (const [filePath, record] of Object.entries(bundle.files ?? {})) {
      if (!/^[a-z0-9._/-]+$/.test(filePath) || !Number.isInteger(record?.bytes) || record.bytes < 0
        || !digestPattern.test(record?.sha256 ?? "")) {
        errors.push(`${id}: file manifest record is invalid`);
      }
    }
  }
  return errors;
}

export function sameR3WasiHostReplayReceipt(actualBytes, expectedReceipt) {
  let actual;
  try { actual = JSON.parse(actualBytes.toString("utf8")); }
  catch (error) { throw new Error(`archived R3 replay profile receipt is invalid JSON (${error.message})`); }
  const errors = validateR3WasiHostReplayReceipt(actual, {
    runId: expectedReceipt.ciRun.run_id,
    testedSha: expectedReceipt.ciRun.tested_sha,
  });
  if (errors.length) throw new Error(`archived R3 replay profile receipt is invalid: ${errors.join("; ")}`);
  if (!actualBytes.equals(Buffer.from(canonicalJson(expectedReceipt), "utf8"))) {
    throw new Error("archived R3 replay profile receipt differs from the independently replayed artifact");
  }
}

export const r3WasiHostReplayReceiptPath = "target/r3-ci/r3-replay-profile-receipt.json";
export const r3WasiHostReplayArtifactPaths = artifactPaths;
