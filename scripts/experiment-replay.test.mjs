import test from "node:test";
import assert from "node:assert/strict";
import { bundleErrors, extractSvg, replay } from "./experiment-replay.mjs";

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
    seeded: "changed-output",
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
    seeded: "changed-output",
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
    seeded: "changed-output",
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
    seeded: "changed-output",
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
    seeded: "changed-output",
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
    seeded: "changed-output",
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
    seeded: "changed-output",
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
    seeded: "changed-output",
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
