import test from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import path from "node:path";
import {
  buildExperimentBundle,
  experimentFixtures,
  metadataForFixture,
  normalizeFigureMetadata,
  outputPath,
  repositoryRoot,
  outputPathFor,
  renderExperimentBundle,
  runtimeSnapshotFor,
  verifyBundleFile,
  verifyExperimentBundle,
} from "./experiment-bundle.mjs";

test("the v0 experiment bundle has calculated hashes and an explicit maturity boundary", () => {
  const result = verifyBundleFile(outputPath);
  assert.deepEqual(result.errors, []);
  const bundle = JSON.parse(readFileSync(outputPath, "utf8"));
  assert.equal(bundle.schema, "ostrin.experiment/v0");
  assert.equal(bundle.reproducibility.level, "R0");
  assert.equal(bundle.reproducibility.label, "R0 · exact replay verified");
  assert.equal(bundle.reproducibility.next, "R1 · seeded replay (planned verification)");
  assert.equal(bundle.provenance.runtime.schema, "ostrin.runtime/v1");
  assert.equal(bundle.provenance.runtime.status, "partial");
  assert.equal(bundle.provenance.runtime.target, "wasm32-wasip1");
  assert.equal(bundle.provenance.runtime.wasi, "preview1");
  assert.deepEqual(bundle.provenance.runtime.platform_policy, {
    output: "byte equality is verified only against the recorded figure in the CI replay configuration; this is not a cross-platform guarantee",
    numeric: "cross-platform numerical comparison is not performed or claimed; before comparing, declare absolute and relative tolerances per metric with units, reference method, and applicable input domain; no universal epsilon is assumed",
  });
  assert.match(bundle.provenance.runtime.compiler_lockfile.sha256, /^sha256:[0-9a-f]{64}$/);
  assert.equal(bundle.provenance.runtime.invocation.command, "node scripts/lab-data.mjs --verify-replays");
  assert.equal(bundle.provenance.runtime.invocation.status, "verified-in-ci");
  assert.deepEqual(bundle.provenance.runtime.invocation.outputs, [{
    path: "figure.svg",
    bytes: bundle.manifest.files.find((entry) => entry.path === "figure.svg").bytes,
    sha256: bundle.manifest.files.find((entry) => entry.path === "figure.svg").sha256,
  }]);
  assert.deepEqual(bundle.provenance.replay, {
    level: "R0",
    status: "verified",
    backend: "ostrinc.wasm",
    target: "wasm32-wasip1",
    command: "node scripts/lab-data.mjs --verify-replays",
    compares: "figure.svg",
    note: "Website CI compares the replayed SVG byte-for-byte with this bundled figure; R1 requires seeded-randomness evidence.",
  });
  assert.equal(bundle.provenance.figure_metadata.policy,
    "The bundle generator normalizes the recorded SVG metadata to these calculated hashes; bundle manifest hashes are authoritative.");
  assert.match(bundle.provenance.commit, /^[0-9a-f]{40}$/);
  assert.match(bundle.files["figure.svg"], new RegExp(`source-hash="${bundle.provenance.figure_metadata.source_hash}"`));
  assert.match(bundle.files["figure.svg"], new RegExp(`data-hash="${bundle.provenance.figure_metadata.data_hash}"`));
  assert.equal(bundle.manifest.files.length, 4);
  for (const entry of bundle.manifest.files) {
    assert.equal(entry.sha256, `sha256:${createHash("sha256").update(Buffer.from(bundle.files[entry.path], "utf8")).digest("hex")}`);
    assert.equal(entry.bytes, Buffer.byteLength(bundle.files[entry.path], "utf8"));
  }
});

test("shared fixture metadata stays identical across normalized SVG and bundle", () => {
  for (const id of Object.keys(experimentFixtures)) {
    const metadata = metadataForFixture(id);
    const bundle = buildExperimentBundle(id);
    const normalized = normalizeFigureMetadata("<svg><ostrin-provenance source-hash=\"old\" data-hash=\"old\" seed=\"old\" compiler=\"old\"/></svg>", metadata);
    const embedded = Object.fromEntries([...normalized.matchAll(/([a-z-]+)=\"([^\"]*)\"/g)].map(([, key, value]) => [key, value]));
    assert.deepEqual(embedded, {
      "source-hash": metadata.sourceHash,
      "data-hash": metadata.dataHash,
      seed: `seed=${metadata.seed}`,
      compiler: metadata.compiler,
    });
    assert.equal(bundle.provenance.figure_metadata.source_hash, metadata.sourceHash);
    assert.equal(bundle.provenance.figure_metadata.data_hash, metadata.dataHash);
    assert.equal(bundle.provenance.figure_metadata.seed, `seed=${metadata.seed}`);
    assert.equal(bundle.provenance.figure_metadata.compiler, metadata.compiler);

    const visible = readFileSync(path.join(repositoryRoot, experimentFixtures[id].figurePath), "utf8");
    const visibleAttributes = visible.match(/<ostrin-provenance\s+([^>]+?)\s*\/>/)?.[1] ?? "";
    const visibleMetadata = Object.fromEntries([...visibleAttributes.matchAll(/([a-z-]+)=\"([^\"]*)\"/g)].map(([, key, value]) => [key, value]));
    assert.deepEqual(visibleMetadata, {
      "source-hash": bundle.provenance.figure_metadata.source_hash,
      "data-hash": bundle.provenance.figure_metadata.data_hash,
      seed: bundle.provenance.figure_metadata.seed,
      compiler: bundle.provenance.figure_metadata.compiler,
    });
  }
});

test("seeded RNG bundles expose source-backed algorithm evidence inside the R2 replay contract", () => {
  const histogram = buildExperimentBundle("histogram");
  const boxplot = buildExperimentBundle("boxplot");
  const scatterFit = buildExperimentBundle("scatter-fit");
  const violin = buildExperimentBundle("violin");
  const hexbin = buildExperimentBundle("hexbin");
  const bars = buildExperimentBundle("bars");
  const pointCloud = buildExperimentBundle("point-cloud");
  const fft = buildExperimentBundle("fft");
  const dashboard = buildExperimentBundle("dashboard");
  const randomness = histogram.provenance.randomness;
  for (const bundle of [histogram, boxplot, scatterFit, violin, hexbin, bars, pointCloud, fft, dashboard]) {
    assert.equal(bundle.reproducibility.level, "R2");
    assert.equal(bundle.reproducibility.label, "R2 · external snapshot replay verified");
    assert.equal(bundle.provenance.level, "R2");
    assert.equal(bundle.provenance.replay.level, "R2");
    assert.equal(bundle.reproducibility.next, "R3 · auditable artifact (planned verification)");
    assert.deepEqual(bundle.provenance.execution.capabilities, ["filesystem.read:data.json"]);
    assert.deepEqual(bundle.provenance.execution.snapshot, { path: "data.json", kind: "json", embedded: true });
    assert.equal(bundle.provenance.randomness.seed_consumption, "data.json:seed");
    assert.equal(bundle.provenance.randomness.status, "input-driven-r1");
  }
  assert.equal(randomness.contract, "ostrin.rng/v1");
  assert.equal(randomness.algorithm, "xoshiro256**");
  assert.equal(randomness.version, "ostrin.rng/v1");
  assert.equal(randomness.seeding, "splitmix64");
  assert.equal(randomness.normal, "Marsaglia polar");
  assert.equal(randomness.deterministic_log, "detmath::ln");
  assert.equal(randomness.partition, "single sequential stream");
  assert.deepEqual(randomness.implementations.map(({ path }) => path), [
    "compiler/src/interpreter/rng.rs",
    "compiler/src/rng_runtime.c",
  ]);
  for (const implementation of randomness.implementations) assert.match(implementation.sha256, /^sha256:[0-9a-f]{64}$/);
  assert.equal(randomness.parity, "interpreter-native");
  assert.equal(randomness.seed_consumption, "data.json:seed");
  assert.equal(randomness.status, "input-driven-r1");
  const lines = buildExperimentBundle("lines");
  assert.equal(lines.provenance.randomness, undefined);
});

test("input-file bundles expose the R2 snapshot and capability contract", () => {
  for (const id of ["surface", "lines", "histogram", "boxplot", "scatter-fit", "violin", "hexbin", "bars", "point-cloud", "fft", "dashboard"]) {
    const bundle = buildExperimentBundle(id);
    const execution = bundle.provenance.execution;
    assert.equal(bundle.reproducibility.level, "R2");
    assert.equal(bundle.reproducibility.label, "R2 · external snapshot replay verified");
    assert.equal(bundle.provenance.level, "R2");
    assert.equal(bundle.provenance.replay.level, "R2");
    assert.equal(bundle.reproducibility.next, "R3 · auditable artifact (planned verification)");
    assert.deepEqual(execution.capabilities, ["filesystem.read:data.json"]);
    assert.deepEqual(execution.snapshot, { path: "data.json", kind: "json", embedded: true });
    assert.deepEqual(bundle.provenance.runtime.input_snapshots, [{
      path: "data.json",
      kind: "json",
      embedded: true,
      sha256: bundle.manifest.files.find((entry) => entry.path === "data.json").sha256,
    }]);
    assert.ok(!bundle.provenance.runtime.missing.includes("output hashes from this invocation"));
    assert.equal(bundle.provenance.inputs[0].kind, "external-snapshot");
  }
});

test("the fixture registry stays source-backed and parameterizable", () => {
  const fixture = experimentFixtures.provenance;
  assert.ok(fixture);
  for (const path of [fixture.sourcePath, fixture.inputPath, fixture.figurePath]) assert.match(path, /^(examples|experiments|website)\//);
  assert.equal(outputPathFor("provenance"), outputPath);
  const bundle = buildExperimentBundle("provenance");
  assert.equal(bundle.id, fixture.id);
  assert.match(fixture.sourceRevision, /^[0-9a-f]{40}$/);
  assert.equal(bundle.provenance.commit, fixture.sourceRevision);
  assert.throws(() => buildExperimentBundle("missing"), /unknown experiment fixture/);
});

test("the runtime snapshot follows the source project boundary", () => {
  const root = repositoryRoot;
  const snapshot = runtimeSnapshotFor(root, "examples/data_project/app/main.ostrin");
  assert.equal(snapshot.project.manifest.path, "examples/data_project/app/ostrin.toml");
  assert.equal(snapshot.project.lockfile.path, "examples/data_project/app/ostrin.lock");
  assert.match(snapshot.project.manifest.sha256, /^sha256:[0-9a-f]{64}$/);
  assert.match(snapshot.project.lockfile.sha256, /^sha256:[0-9a-f]{64}$/);
  const rootLockfile = runtimeSnapshotFor(root, "examples/viz_histogram.ostrin").project.lockfile;
  assert.equal(rootLockfile.path, "ostrin.lock");
  assert.match(rootLockfile.sha256, /^sha256:[0-9a-f]{64}$/);
});

test("CI host capture is explicit and excludes private machine identity", () => {
  const previous = process.env.OSTRIN_CAPTURE_HOST;
  const previousOs = process.env.RUNNER_OS;
  process.env.OSTRIN_CAPTURE_HOST = "1";
  process.env.RUNNER_OS = "Linux";
  try {
    const host = buildExperimentBundle("provenance").provenance.runtime.host;
    assert.equal(host.schema, "ostrin.host/v1");
    assert.equal(host.source, "github-actions");
    assert.equal(host.runner_os, "Linux");
    assert.equal(host.captured, true);
    assert.equal(host.hardware.arch, process.arch);
    assert.ok(Number.isInteger(host.hardware.cpu_count) && host.hardware.cpu_count > 0);
    assert.match(host.hardware.endianness, /^(LE|BE)$/);
    assert.equal(Object.hasOwn(host, "hostname"), false);
    assert.equal(Object.hasOwn(host, "username"), false);
  } finally {
    if (previous === undefined) delete process.env.OSTRIN_CAPTURE_HOST;
    else process.env.OSTRIN_CAPTURE_HOST = previous;
    if (previousOs === undefined) delete process.env.RUNNER_OS;
    else process.env.RUNNER_OS = previousOs;
  }
});

test("every registered fixture produces a verified recorded bundle", () => {
  for (const [id, fixture] of Object.entries(experimentFixtures)) {
    const bundle = buildExperimentBundle(id);
    assert.equal(bundle.id, id);
    assert.equal(bundle.title, fixture.title);
    assert.match(bundle.provenance.commit, /^[0-9a-f]{40}$/);
    assert.deepEqual(verifyExperimentBundle(bundle, { fixtureId: id }).errors, []);
    assert.equal(bundle.provenance.effects.schema, "ostrin.effect-report/v0");
    assert.equal(bundle.provenance.effects.status, "captured");
    assert.equal(bundle.provenance.effects.source, "ostrinc --effect-report --json");
    assert.ok(Array.isArray(bundle.provenance.effects.kinds));
    assert.ok(Array.isArray(bundle.provenance.effects.functions.main));
    for (const analyzer of bundle.provenance.effects.analyzer) {
      assert.match(analyzer.sha256, /^sha256:[0-9a-f]{64}$/);
    }
    if (id === "surface") {
      assert.deepEqual(bundle.provenance.parameters, { scale: 1 });
      assert.deepEqual(bundle.provenance.camera, { azimuth: -55, elevation: 28 });
    }
    const fileResult = verifyBundleFile(outputPathFor(id), id);
    assert.deepEqual(fileResult.errors, [], `${id}: ${fileResult.errors.join("; ")}`);
  }
});

test("tampering with bundled content is rejected by the hash gate", () => {
  const bundle = JSON.parse(readFileSync(outputPath, "utf8"));
  bundle.files["figure.svg"] += "\n";
  const result = verifyExperimentBundle(bundle);
  assert.equal(result.ok, false);
  assert.ok(result.errors.some((error) => error.includes("figure.svg: sha256 does not match manifest")));
});

test("rendering the checked bundle is deterministic", () => {
  const bundle = JSON.parse(readFileSync(outputPath, "utf8"));
  assert.equal(renderExperimentBundle(bundle), readFileSync(outputPath, "utf8").replaceAll("\r\n", "\n"));
});
