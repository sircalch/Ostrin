import test from "node:test";
import assert from "node:assert/strict";
import { buildReadiness, inspectBundle } from "./r3-readiness.mjs";

test("R3 readiness requires host, dependencies, scheduler and effects", () => {
  const bundle = {
    provenance: {
      commit: "commit",
      compiler: "ostrinc 0.1.0",
      target: "wasm32-wasip1",
      runtime: {
        schema: "ostrin.runtime/v1",
        target: "wasm32-wasip1",
        compiler_lockfile: { sha256: "sha256:test" },
        host: { schema: "ostrin.host/v1" },
        dependencies: { compiler_lockfile: {} },
        scheduler: { mode: "single-threaded-wasi-replay" },
      },
      inputs: [{ id: "data.json" }],
      outputs: [{ id: "figure.svg" }],
      parameters: { scale: 1 },
      execution: { mode: "recorded-parameters" },
      figure_metadata: { source_hash: "source" },
      effects: { schema: "ostrin.effect-report/v0" },
      level: "R0",
    },
  };
  assert.equal(inspectBundle(bundle, "fixture.json").status, "READY");
  delete bundle.provenance.runtime.host;
  assert.deepEqual(inspectBundle(bundle, "fixture.json").r3Extensions.missing, ["provenance.runtime.host"]);
});

test("readiness reflects whether CI host metadata is present", () => {
  const report = buildReadiness();
  if (process.env.OSTRIN_CAPTURE_HOST === "1") {
    assert.equal(report.status, "READY");
    assert.equal(report.readyCount, report.bundleCount);
  } else {
    assert.equal(report.status, "OPEN");
    assert.ok(report.entries.every((entry) => entry.r3Extensions.missing.includes("provenance.runtime.host")));
  }
});
