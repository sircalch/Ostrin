import test from "node:test";
import assert from "node:assert/strict";
import { after } from "node:test";
import { buildReadiness, inspectBundle, requiredCaptureErrors } from "./r3-readiness.mjs";

const originalCaptureHost = process.env.OSTRIN_CAPTURE_HOST;
delete process.env.OSTRIN_CAPTURE_HOST;
after(() => {
  if (originalCaptureHost === undefined) delete process.env.OSTRIN_CAPTURE_HOST;
  else process.env.OSTRIN_CAPTURE_HOST = originalCaptureHost;
});

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
      effects: {
        schema: "ostrin.effect-report/v0",
        status: "captured",
        experimental: true,
        conservative: true,
        kinds: [],
        functions: { main: [] },
        function_calls: { main: ["write_file"] },
        unresolved_calls: {},
        effect_coverage: { known_calls: 1, unresolved_calls: 0, complete: true },
      },
      level: "R0",
    },
  };
  assert.equal(inspectBundle(bundle, "fixture.json").status, "READY");
  assert.deepEqual(requiredCaptureErrors({ entries: [inspectBundle(bundle, "fixture.json")] }), []);
  delete bundle.provenance.runtime.host;
  const missingHost = inspectBundle(bundle, "fixture.json");
  assert.deepEqual(missingHost.r3Extensions.missing, ["provenance.runtime.host"]);
  assert.equal(requiredCaptureErrors({ entries: [missingHost] }).length, 1);
});

test("R3 readiness uses the explicit CI host capture contract", () => {
  const effects = {
    schema: "ostrin.effect-report/v0",
    status: "captured",
    experimental: true,
    conservative: true,
    kinds: [],
    functions: { main: [] },
    function_calls: { main: [] },
    unresolved_calls: {},
    effect_coverage: { known_calls: 0, unresolved_calls: 0, complete: true },
  };
  const bundle = {
    provenance: {
      commit: "fixture-commit", compiler: "ostrinc fixture", target: "wasm32-wasip1",
      runtime: {
        schema: "ostrin.runtime/v1", target: "wasm32-wasip1",
        compiler_lockfile: { sha256: "sha256:fixture" },
        host: { schema: "ostrin.host/v1" }, dependencies: { compiler_lockfile: {} },
        scheduler: { mode: "single-threaded-wasi-replay" },
      },
      inputs: [{ id: "input" }], outputs: [{ id: "output" }], parameters: { scale: 1 },
      execution: { mode: "recorded" }, figure_metadata: { source_hash: "fixture" }, effects,
    },
  };
  assert.equal(inspectBundle(bundle, "fixture.json").status, "READY");
  delete bundle.provenance.runtime.host;
  assert.equal(inspectBundle(bundle, "fixture.json").status, "OPEN");
});

test("R3 stays open while valid captured effects are incomplete", () => {
  const effects = {
    schema: "ostrin.effect-report/v0",
    status: "captured",
    experimental: true,
    conservative: true,
    kinds: [],
    functions: { main: [] },
    function_calls: { main: ["mystery_operation"] },
    unresolved_calls: { main: ["mystery_operation"] },
    effect_coverage: { known_calls: 0, unresolved_calls: 1, complete: false },
  };
  const bundle = {
    provenance: {
      commit: "fixture-commit",
      compiler: "ostrinc fixture",
      target: "wasm32-wasip1",
      runtime: {
        schema: "ostrin.runtime/v1",
        target: "wasm32-wasip1",
        compiler_lockfile: { sha256: "sha256:fixture" },
        host: { schema: "ostrin.host/v1" },
        dependencies: { compiler_lockfile: {} },
        scheduler: { mode: "single-threaded-wasi-replay" },
      },
      inputs: [{ id: "input" }],
      outputs: [{ id: "output" }],
      parameters: { scale: 1 },
      execution: { mode: "recorded" },
      figure_metadata: { source_hash: "fixture" },
      effects,
    },
  };

  const incomplete = inspectBundle(bundle, "fixture.json");
  assert.equal(incomplete.status, "OPEN");
  assert.deepEqual(incomplete.r3Extensions.incomplete, ["provenance.effects.effect_coverage"]);
  assert.equal(incomplete.effectCoverage.unresolved_calls, 1);
  assert.deepEqual(requiredCaptureErrors({ entries: [incomplete] }), []);

  delete bundle.provenance.effects.unresolved_calls;
  const malformed = inspectBundle(bundle, "fixture.json");
  assert.equal(malformed.status, "OPEN");
  assert.ok(malformed.r3Extensions.missing.includes("provenance.effects.effect_coverage"));
  assert.equal(requiredCaptureErrors({ entries: [malformed] }).length, 1);
});

test("readiness totals unresolved effects across incomplete bundles", () => {
  const report = buildReadiness();
  assert.ok(report.incompleteEffectBundleCount > 0);
  assert.ok(report.incompleteEffectBundleCount <= report.bundleCount);
  assert.ok(report.unresolvedEffectCallCount > 0);
  assert.equal(report.readyCount, 0);
  assert.equal(report.status, "OPEN");
  assert.equal(report.captureReadyCount + report.captureOpenCount, report.bundleCount);
  assert.ok(report.captureReadyCount <= report.bundleCount);
});

test("readiness totals and status match the inspected bundles", () => {
  const report = buildReadiness();
  assert.equal(report.readyCount + report.openCount, report.bundleCount);
  assert.equal(report.status, report.openCount === 0 ? "READY" : "OPEN");
  assert.equal(report.extensionReadyCount,
    report.entries.filter((entry) =>
      entry.r3Extensions.missing.length === 0 && entry.r3Extensions.incomplete.length === 0).length);
});
