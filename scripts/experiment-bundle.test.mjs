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
  verifyBundleFile,
  verifyExperimentBundle,
} from "./experiment-bundle.mjs";

test("the v0 experiment bundle has calculated hashes and an explicit R0/R1 boundary", () => {
  const result = verifyBundleFile(outputPath);
  assert.deepEqual(result.errors, []);
  const bundle = JSON.parse(readFileSync(outputPath, "utf8"));
  assert.equal(bundle.schema, "ostrin.experiment/v0");
  assert.equal(bundle.reproducibility.level, "R0");
  assert.equal(bundle.reproducibility.label, "R0 · exact replay verified");
  assert.equal(bundle.reproducibility.next, "R1 · seeded replay (planned verification)");
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

test("every registered fixture produces a verified recorded bundle", () => {
  for (const [id, fixture] of Object.entries(experimentFixtures)) {
    const bundle = buildExperimentBundle(id);
    assert.equal(bundle.id, id);
    assert.equal(bundle.title, fixture.title);
    assert.match(bundle.provenance.commit, /^[0-9a-f]{40}$/);
    assert.deepEqual(verifyExperimentBundle(bundle, { fixtureId: id }).errors, []);
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
