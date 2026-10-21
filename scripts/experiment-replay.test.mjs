import test from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { bundleErrors, createWasiFileReadEvidenceReport, extractSvg, listExecutableReplayIds, replay, replayRecordedR0, validateWasiReadEvidence } from "./experiment-replay.mjs";
import { buildExperimentBundle, experimentFixtures } from "./experiment-bundle.mjs";

test("the executable replay registry includes every source-backed R1/R2 bundle", () => {
  const expected = Object.keys(experimentFixtures).filter((id) => {
    const level = buildExperimentBundle(id).reproducibility.level;
    return level === "R1" || level === "R2";
  });
  assert.deepEqual(listExecutableReplayIds(), expected);
  assert.equal(expected.length, 11);
});

test("WASI read evidence requires declared files and exact bytes actually read", () => {
  const files = { "main.ostrin": "print(1)\n", "data.json": "{\"x\":1}\n" };
  const read = (file) => ({
    kind: "file-read",
    path: file,
    pathStatus: "relative",
    openStatus: "opened",
    bytesRead: Buffer.byteLength(files[file] ?? "", "utf8"),
    contentSha256: `sha256:${createHash("sha256").update(Buffer.from(files[file] ?? "", "utf8")).digest("hex")}`,
  });
  assert.deepEqual(
    validateWasiReadEvidence("fixture", [read("main.ostrin"), read("data.json")], files, ["main.ostrin", "data.json"]),
    [],
  );
  assert.match(
    validateWasiReadEvidence("fixture", [read("main.ostrin"), read("secret.txt")], files, ["main.ostrin", "data.json"]).join("\n"),
    /outside the supplied filesystem/,
  );
  const mismatch = { ...read("data.json"), contentSha256: `sha256:${"0".repeat(64)}` };
  assert.match(
    validateWasiReadEvidence("fixture", [read("main.ostrin"), mismatch], files, ["main.ostrin", "data.json"]).join("\n"),
    /bytes read from data\.json differ/,
  );
  const missing = { ...read("data.json"), openStatus: "open-failed" };
  assert.match(
    validateWasiReadEvidence("fixture", [read("main.ostrin"), missing], files, ["main.ostrin", "data.json"]).join("\n"),
    /could not be opened/,
  );
  const failedRead = { ...read("data.json"), readStatus: "fd_read_errno_5" };
  assert.match(
    validateWasiReadEvidence("fixture", [read("main.ostrin"), failedRead], files, ["main.ostrin", "data.json"]).join("\n"),
    /WASI read from data\.json was incomplete/,
  );
  assert.match(
    validateWasiReadEvidence("fixture", [read("main.ostrin")], files, ["main.ostrin", "data.json"]).join("\n"),
    /did not read required file data\.json/,
  );
});

test("run-bound WASI evidence binds every archived replay to one run, compiler and bundle inventory", () => {
  const compilerWasmSha256 = `sha256:${"a".repeat(64)}`;
  const bundleSha256 = `sha256:${"b".repeat(64)}`;
  const inventory = { surface: { level: "R2", bundleSha256 } };
  const result = {
    id: "surface",
    wasmSha256: compilerWasmSha256,
    bundleSha256,
    filesystemReadEvidence: {
      schema: "ostrin.wasi-file-read-evidence/v1",
      executions: [{ scenario: "baseline", reads: [] }],
    },
  };
  const report = createWasiFileReadEvidenceReport(
    [result],
    { runId: "1234", testedSha: "c".repeat(40) },
    compilerWasmSha256,
    inventory,
  );
  assert.equal(report.ciRun.run_id, "1234");
  assert.equal(report.ciRun.tested_sha, "c".repeat(40));
  assert.equal(report.replayBundles.surface.bundleSha256, bundleSha256);
  assert.equal(report.replayBundles.surface.executions.length, 1);
  assert.ok(report.limitations.some((item) => item.includes("does not establish general R3 closure")));
  assert.throws(
    () => createWasiFileReadEvidenceReport([], { runId: "1234", testedSha: "c".repeat(40) }, compilerWasmSha256, inventory),
    /complete replay inventory/,
  );
});

test("an R0 recorded-parameter bundle replays twice from its archived source", async () => {
  const result = await replayRecordedR0("animation");
  assert.equal(result.id, "animation");
  assert.equal(result.level, "R0");
  assert.equal(result.baseline, "match");
  assert.equal(result.repeatability, "same-output");
  assert.match(result.wasmSha256, /^sha256:[a-f0-9]{64}$/);
  assert.match(result.bundleSha256, /^sha256:[a-f0-9]{64}$/);
});

test("R0 archive replay rejects mismatched provenance hashes and undeclared external effects", async () => {
  const hashMismatch = buildExperimentBundle("animation");
  hashMismatch.provenance.program.source_hash = `sha256:${"0".repeat(64)}`;
  await assert.rejects(
    replayRecordedR0("animation", { bundleText: JSON.stringify(hashMismatch) }),
    /R0 source, parameter-only execution or recorded output contract is incomplete/,
  );

  const externalEffect = buildExperimentBundle("animation");
  externalEffect.provenance.effects.entry_effect_signature.effects.push("network");
  const provenanceText = `${JSON.stringify(externalEffect.provenance, null, 2)}\n`;
  externalEffect.files["provenance.json"] = provenanceText;
  const provenanceEntry = externalEffect.manifest.files.find((entry) => entry.path === "provenance.json");
  provenanceEntry.bytes = Buffer.byteLength(provenanceText, "utf8");
  provenanceEntry.sha256 = `sha256:${createHash("sha256").update(Buffer.from(provenanceText, "utf8")).digest("hex")}`;
  await assert.rejects(
    replayRecordedR0("animation", { bundleText: JSON.stringify(externalEffect) }),
    /R0 replay requires a closed entry row/,
  );
});

test("the surface bundle executes from its declared data.json input", async () => {
  const result = await replay("surface");
  assert.deepEqual(result, {
    id: "surface",
    input: "data.json",
    consumed: ["parameters.scale"],
    baseline: "match",
    sensitivity: "changed-output",
  });
});

test("the lines bundle executes from a generic declared sensitivity path", async () => {
  const result = await replay("lines");
  assert.deepEqual(result, {
    id: "lines",
    input: "data.json",
    consumed: ["parameters.samples"],
    baseline: "match",
    sensitivity: "changed-output",
  });
});

test("the histogram bundle verifies input-driven seeded replay", async () => {
  const result = await replay("histogram");
  assert.deepEqual(result, {
    id: "histogram",
    input: "data.json",
    consumed: ["seed", "parameters.samples", "parameters.bins"],
    baseline: "match",
    sensitivity: "changed-output",
    seeded: "same-seed-and-changed-output",
  });
});

test("the boxplot bundle verifies a second input-driven seeded replay", async () => {
  const result = await replay("boxplot");
  assert.deepEqual(result, {
    id: "boxplot",
    input: "data.json",
    consumed: ["seed", "parameters.samples_per_group"],
    baseline: "match",
    sensitivity: "changed-output",
    seeded: "same-seed-and-changed-output",
  });
});

test("the scatter-fit bundle verifies a third input-driven seeded replay", async () => {
  const result = await replay("scatter-fit");
  assert.deepEqual(result, {
    id: "scatter-fit",
    input: "data.json",
    consumed: ["seed", "parameters.samples"],
    baseline: "match",
    sensitivity: "changed-output",
    seeded: "same-seed-and-changed-output",
  });
});

test("the violin bundle verifies a fourth input-driven seeded replay", async () => {
  const result = await replay("violin");
  assert.deepEqual(result, {
    id: "violin",
    input: "data.json",
    consumed: ["seed", "parameters.samples_per_group", "parameters.bandwidth_samples"],
    baseline: "match",
    sensitivity: "changed-output",
    seeded: "same-seed-and-changed-output",
  });
});

test("the hexbin bundle verifies a fifth input-driven seeded replay", async () => {
  const result = await replay("hexbin");
  assert.deepEqual(result, {
    id: "hexbin",
    input: "data.json",
    consumed: ["seed", "parameters.samples", "parameters.bins_x", "parameters.bins_y"],
    baseline: "match",
    sensitivity: "changed-output",
    seeded: "same-seed-and-changed-output",
  });
});

test("the bars bundle verifies a sixth input-driven seeded replay", async () => {
  const result = await replay("bars");
  assert.deepEqual(result, {
    id: "bars",
    input: "data.json",
    consumed: ["seed", "parameters.samples_per_group"],
    baseline: "match",
    sensitivity: "changed-output",
    seeded: "same-seed-and-changed-output",
  });
});

test("the point-cloud bundle verifies a seventh input-driven seeded replay", async () => {
  const result = await replay("point-cloud");
  assert.deepEqual(result, {
    id: "point-cloud",
    input: "data.json",
    consumed: ["seed", "parameters.clusters", "parameters.points_per_cluster"],
    baseline: "match",
    sensitivity: "changed-output",
    seeded: "same-seed-and-changed-output",
  });
});

test("the fft bundle verifies an eighth input-driven seeded replay", async () => {
  const result = await replay("fft");
  assert.deepEqual(result, {
    id: "fft",
    input: "data.json",
    consumed: ["seed", "parameters.samples", "parameters.duration_seconds", "parameters.noise_scale"],
    baseline: "match",
    sensitivity: "changed-output",
    seeded: "same-seed-and-changed-output",
  });
});

test("the dashboard bundle verifies a ninth input-driven seeded replay", async () => {
  const result = await replay("dashboard");
  assert.deepEqual(result, {
    id: "dashboard",
    input: "data.json",
    consumed: ["seed", "parameters.random_samples", "parameters.grid"],
    baseline: "match",
    sensitivity: "changed-output",
    seeded: "same-seed-and-changed-output",
  });
});

test("an executable replay rejects a bundle without its input contract", () => {
  const errors = bundleErrors({ schema: "ostrin.experiment/v0", id: "surface", files: {}, manifest: { files: [] } }, "surface");
  assert.ok(errors.some((error) => error.includes("input-file executable replay contract")));
});

test("SVG extraction keeps only one complete figure", () => {
  assert.equal(extractSvg(["log", "<svg>", "body", "</svg>", "tail"], "surface"), "<svg>\nbody\n</svg>\n");
  assert.throws(() => extractSvg(["log"], "surface"), /complete SVG/);
});
