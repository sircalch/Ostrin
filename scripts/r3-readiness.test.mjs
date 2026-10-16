import test from "node:test";
import assert from "node:assert/strict";
import { after } from "node:test";
import { runtimeSnapshotFor } from "./experiment-bundle.mjs";
import { buildReadiness, diagnoseEffectCalls, inspectBundle, requiredCaptureErrors } from "./r3-readiness.mjs";

const originalCaptureHost = process.env.OSTRIN_CAPTURE_HOST;
delete process.env.OSTRIN_CAPTURE_HOST;
after(() => {
  if (originalCaptureHost === undefined) delete process.env.OSTRIN_CAPTURE_HOST;
  else process.env.OSTRIN_CAPTURE_HOST = originalCaptureHost;
});

const validFile = (name) => ({ path: name, sha256: `sha256:${"a".repeat(64)}` });
const validRuntime = () => ({
  schema: "ostrin.runtime/v1",
  target: "wasm32-wasip1",
  compiler_lockfile: validFile("compiler/Cargo.lock"),
  host: {
    schema: "ostrin.host/v1",
    source: "github-actions",
    runner_os: "Linux",
    runner_arch: "X64",
    runner_environment: "github-hosted",
    image: { os: "ubuntu", version: "24.04" },
    node: "v24.0.0",
    hardware: { arch: "x64", cpu_count: 2, endianness: "LE" },
    wasi: "preview1",
    captured: true,
  },
  dependencies: {
    compiler_lockfile: validFile("compiler/Cargo.lock"),
    project_manifest: validFile("ostrin.toml"),
    project_lockfile: validFile("ostrin.lock"),
  },
  scheduler: {
    mode: "single-threaded-wasi-replay",
    concurrency: "not used by fixture",
    status: "declared",
  },
});
const validEffects = () => ({
  schema: "ostrin.effect-report/v0",
  status: "captured",
  experimental: true,
  conservative: true,
  kinds: [],
  functions: { main: [] },
  function_calls: { main: [] },
  unresolved_calls: {},
  effect_coverage: { known_calls: 0, unresolved_calls: 0, complete: true },
});

test("R3 readiness requires host, dependencies, scheduler and effects", () => {
  const bundle = {
    provenance: {
      commit: "commit",
      compiler: "ostrinc 0.1.0",
      target: "wasm32-wasip1",
      runtime: validRuntime(),
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
      runtime: validRuntime(),
      inputs: [{ id: "input" }], outputs: [{ id: "output" }], parameters: { scale: 1 },
      execution: { mode: "recorded" }, figure_metadata: { source_hash: "fixture" }, effects,
    },
  };
  assert.equal(inspectBundle(bundle, "fixture.json").status, "READY");
  delete bundle.provenance.runtime.host;
  assert.equal(inspectBundle(bundle, "fixture.json").status, "OPEN");
});

test("R3 accepts the host and dependency capture emitted by the bundle generator", () => {
  const names = ["OSTRIN_CAPTURE_HOST", "RUNNER_OS", "RUNNER_ARCH", "RUNNER_ENVIRONMENT", "ImageOS", "ImageVersion"];
  const previous = Object.fromEntries(names.map((name) => [name, process.env[name]]));
  Object.assign(process.env, {
    OSTRIN_CAPTURE_HOST: "1",
    RUNNER_OS: "Linux",
    RUNNER_ARCH: "X64",
    RUNNER_ENVIRONMENT: "github-hosted",
    ImageOS: "ubuntu24",
    ImageVersion: "20261001.1.0",
  });
  try {
    const bundle = {
      provenance: {
        commit: "fixture-commit",
        compiler: "ostrinc fixture",
        target: "wasm32-wasip1",
        runtime: runtimeSnapshotFor(undefined, "examples/viz_lines.ostrin"),
        inputs: [{ id: "input" }],
        outputs: [{ id: "output" }],
        parameters: { scale: 1 },
        execution: { mode: "recorded" },
        figure_metadata: { source_hash: "fixture" },
        effects: validEffects(),
      },
    };
    assert.equal(inspectBundle(bundle, "generator.json").status, "READY");
  } finally {
    for (const name of names) {
      if (previous[name] === undefined) delete process.env[name];
      else process.env[name] = previous[name];
    }
  }
});

test("R3 capture rejects present but malformed host, dependency and scheduler metadata", () => {
  const runtime = validRuntime();
  runtime.host = { schema: "ostrin.host/v1" };
  runtime.dependencies.compiler_lockfile = {};
  runtime.scheduler = { mode: "single-threaded-wasi-replay" };
  const bundle = { provenance: { runtime, effects: validEffects() } };
  const report = inspectBundle(bundle, "malformed.json");
  assert.equal(report.status, "OPEN");
  assert.deepEqual(report.r3Extensions.missing, [
    "provenance.runtime.host",
    "provenance.runtime.dependencies",
    "provenance.runtime.scheduler",
  ]);
  assert.ok(requiredCaptureErrors({ entries: [report] }).length > 0);
});

test("R3 capture rejects invalid dependency hashes and unknown runner metadata", () => {
  const runtime = validRuntime();
  runtime.compiler_lockfile.sha256 = "sha256:not-a-hash";
  runtime.dependencies.project_lockfile.sha256 = "sha256:not-a-hash";
  runtime.host.runner_os = "unknown";
  const report = inspectBundle({ provenance: { runtime, effects: validEffects() } }, "invalid-values.json");
  assert.deepEqual(report.r3Extensions.missing, [
    "provenance.runtime.compiler_lockfile",
    "provenance.runtime.host",
    "provenance.runtime.dependencies",
  ]);
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
      runtime: validRuntime(),
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

test("effect-gap diagnostics group operation syntax without inferring resolution or purity", () => {
  const diagnostics = diagnoseEffectCalls({
    unresolved_calls: {
      main: ["array", "data.max", "std::math::exp", "host?"],
      wrapper: ["array", "callback"],
    },
  });

  assert.equal(diagnostics.referenceCount, 6);
  assert.equal(diagnostics.uniqueOperationNames, 5);
  assert.deepEqual(diagnostics.bySyntax, {
    identifier: 3,
    memberPath: 1,
    qualifiedPath: 1,
    other: 1,
  });
  assert.equal(diagnostics.operations.get("array"), 2);
});

test("effect-gap diagnostics recognize Unicode identifiers accepted by the Ostrin lexer", () => {
  const diagnostics = diagnoseEffectCalls({
    unresolved_calls: {
      main: ["λ", "变量", "callback٢", "_callback1", "host?"],
    },
  });

  assert.equal(diagnostics.referenceCount, 5);
  assert.equal(diagnostics.uniqueOperationNames, 5);
  assert.deepEqual(diagnostics.bySyntax, {
    identifier: 4,
    memberPath: 0,
    qualifiedPath: 0,
    other: 1,
  });
});

test("R3 gap summary reconciles with per-bundle missing fields and effect coverage", () => {
  const report = buildReadiness();
  const { captureGaps, effectInventory, notAssessed } = report.gapDiagnostics;
  const hostGap = captureGaps.missingExtensionFields.find(({ field }) => field === "provenance.runtime.host");
  const effectGap = captureGaps.incompleteExtensionFields.find(({ field }) => field === "provenance.effects.effect_coverage");

  assert.equal(hostGap?.bundleCount ?? 0, report.captureOpenCount);
  assert.equal(effectGap?.bundleCount ?? 0, report.incompleteEffectBundleCount);
  assert.equal(effectInventory.operationReferences, report.unresolvedEffectCallCount);
  assert.equal(Object.values(effectInventory.bySyntax).reduce((sum, count) => sum + count, 0),
    effectInventory.operationReferences);
  assert.equal(effectInventory.uniqueOperationNames > 0, report.unresolvedEffectCallCount > 0);
  assert.equal(effectInventory.topOperations.length > 0, report.unresolvedEffectCallCount > 0);
  assert.match(effectInventory.interpretation, /does not determine type resolution or prove purity/i);
  assert.ok(notAssessed.includes("Whether numerical tolerances are scientifically justified"));
});
