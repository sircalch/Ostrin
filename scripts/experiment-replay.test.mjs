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

test("an executable replay rejects a bundle without its input contract", () => {
  const errors = bundleErrors({ schema: "ostrin.experiment/v0", id: "surface", files: {}, manifest: { files: [] } }, "surface");
  assert.ok(errors.some((error) => error.includes("input-file executable replay contract")));
});

test("SVG extraction keeps only one complete figure", () => {
  assert.equal(extractSvg(["log", "<svg>", "body", "</svg>", "tail"], "surface"), "<svg>\nbody\n</svg>\n");
  assert.throws(() => extractSvg(["log"], "surface"), /complete SVG/);
});
