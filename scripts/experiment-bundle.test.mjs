import test from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import {
  buildExperimentBundle,
  experimentFixtures,
  outputPath,
  outputPathFor,
  renderExperimentBundle,
  verifyBundleFile,
  verifyExperimentBundle,
} from "./experiment-bundle.mjs";

test("the v0 experiment bundle has calculated hashes and explicit R0/R1 labels", () => {
  const result = verifyBundleFile(outputPath);
  assert.deepEqual(result.errors, []);
  const bundle = JSON.parse(readFileSync(outputPath, "utf8"));
  assert.equal(bundle.schema, "ostrin.experiment/v0");
  assert.equal(bundle.reproducibility.level, "R0");
  assert.match(bundle.reproducibility.next, /^R1/);
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
