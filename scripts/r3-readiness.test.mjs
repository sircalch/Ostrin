import test from "node:test";
import assert from "node:assert/strict";
import { after } from "node:test";
import { runtimeSnapshotFor } from "./experiment-bundle.mjs";
import { buildReadiness, diagnoseEffectCalls, diagnoseEntryEffectClosure, inspectBundle, requiredCaptureErrors } from "./r3-readiness.mjs";

const originalCaptureHost = process.env.OSTRIN_CAPTURE_HOST;
delete process.env.OSTRIN_CAPTURE_HOST;
after(() => {
  if (originalCaptureHost === undefined) delete process.env.OSTRIN_CAPTURE_HOST;
  else process.env.OSTRIN_CAPTURE_HOST = originalCaptureHost;
});

const validFile = (name) => ({ path: name, sha256: `sha256:${"a".repeat(64)}` });
const validRuntime = () => ({
  schema: "ostrin.runtime/v1",
  status: "partial",
  target: "wasm32-wasip1",
  wasi: "preview1",
  runner: "WASI host",
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
  function_effect_variables: {},
  symbolic_effect_calls: {},
  entry_effect_signature: {
    function: "main", effects: [], variables: [], open_calls: [], row_closed: true, purity_proven: false,
  },
  effect_coverage: { count_unit: "distinct_function_operation_edges", known_calls: 0, symbolic_calls: 0, unresolved_calls: 0, complete: true },
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
        function_effect_variables: {},
        symbolic_effect_calls: {},
        entry_effect_signature: {
          function: "main", effects: [], variables: [], open_calls: [], row_closed: true, purity_proven: false,
        },
        effect_coverage: { count_unit: "distinct_function_operation_edges", known_calls: 1, symbolic_calls: 0, unresolved_calls: 0, complete: true },
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
    function_effect_variables: {},
    symbolic_effect_calls: {},
    entry_effect_signature: {
      function: "main", effects: [], variables: [], open_calls: [], row_closed: true, purity_proven: false,
    },
    effect_coverage: { count_unit: "distinct_function_operation_edges", known_calls: 0, symbolic_calls: 0, unresolved_calls: 0, complete: true },
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

test("R3 capture rejects a mismatched runtime contract and non-CI host source", () => {
  const runtime = validRuntime();
  runtime.runner = "local process";
  runtime.host.source = "developer workstation";
  const report = inspectBundle({ provenance: { runtime, effects: validEffects() } }, "wrong-origin.json");
  assert.equal(report.status, "OPEN");
  assert.deepEqual(report.r3Extensions.missing, [
    "provenance.runtime.contract",
    "provenance.runtime.host",
  ]);
});

test("R3 stays open while the entry effect signature is open", () => {
  const effects = {
    schema: "ostrin.effect-report/v0",
    status: "captured",
    experimental: true,
    conservative: true,
    kinds: [],
    functions: { main: [] },
    function_calls: { main: ["mystery_operation"] },
    unresolved_calls: { main: ["mystery_operation"] },
    function_effect_variables: {},
    symbolic_effect_calls: {},
    entry_effect_signature: {
      function: "main", effects: [], variables: [], open_calls: ["mystery_operation"],
      row_closed: false, purity_proven: false,
    },
    effect_coverage: { count_unit: "distinct_function_operation_edges", known_calls: 0, symbolic_calls: 0, unresolved_calls: 1, complete: false },
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
  assert.deepEqual(incomplete.r3Extensions.incomplete, ["provenance.effects.entry_effect_signature"]);
  assert.equal(incomplete.effectCoverage.unresolved_calls, 1);
  assert.deepEqual(requiredCaptureErrors({ entries: [incomplete] }), []);

  delete bundle.provenance.effects.unresolved_calls;
  const malformed = inspectBundle(bundle, "fixture.json");
  assert.equal(malformed.status, "OPEN");
  assert.ok(malformed.r3Extensions.missing.includes("provenance.effects.effect_coverage"));
  assert.equal(requiredCaptureErrors({ entries: [malformed] }).length, 1);
});

test("R3 stays open when an otherwise complete effect inventory has symbolic callbacks", () => {
  const effects = {
    ...validEffects(),
    function_calls: { main: ["callback"] },
    function_effect_variables: { main: ["ε_callback"] },
    symbolic_effect_calls: { main: ["callback"] },
    entry_effect_signature: {
      function: "main", effects: [], variables: ["ε_callback"], open_calls: [],
      row_closed: false, purity_proven: false,
    },
    effect_coverage: {
      count_unit: "distinct_function_operation_edges",
      known_calls: 0,
      symbolic_calls: 1,
      unresolved_calls: 0,
      complete: true,
    },
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

  const report = inspectBundle(bundle, "symbolic-callback.json");
  assert.equal(report.effectCoverage.complete, true);
  assert.equal(report.effectCoverage.symbolic_calls, 1);
  assert.equal(report.status, "OPEN");
  assert.deepEqual(report.r3Extensions.incomplete, ["provenance.effects.entry_effect_signature"]);

  delete effects.effect_coverage.symbolic_calls;
  const countOmitted = inspectBundle(bundle, "symbolic-callback.json");
  assert.equal(countOmitted.status, "OPEN");
  assert.deepEqual(countOmitted.r3Extensions.missing, ["provenance.effects.effect_coverage"]);

  effects.effect_coverage.symbolic_calls = -1;
  const malformed = inspectBundle(bundle, "symbolic-callback.json");
  assert.equal(malformed.status, "OPEN");
  assert.deepEqual(malformed.r3Extensions.missing, ["provenance.effects.effect_coverage"]);
});

test("readiness reports module-wide callbacks separately from entry effect closure", () => {
  const report = buildReadiness();
  assert.equal(report.closedEntryEffectBundleCount, report.bundleCount);
  assert.equal(report.incompleteEffectBundleCount, 0);
  assert.ok(report.incompleteEffectBundleCount <= report.bundleCount);
  assert.equal(report.unresolvedEffectCallCount, 0);
  assert.ok(report.knownEffectCallCount > 0);
  assert.ok(report.symbolicEffectCallCount > 0);
  assert.equal(report.effectCoverageCountUnit, "distinct_function_operation_edges");
  assert.equal(report.readyCount + report.openCount, report.bundleCount);
  assert.equal(report.status, report.openCount === 0 ? "READY" : "OPEN");
  assert.equal(report.captureReadyCount + report.captureOpenCount, report.bundleCount);
  assert.ok(report.captureReadyCount <= report.bundleCount);
});

test("R3 requires a structurally valid entry signature and does not infer purity", () => {
  const effects = validEffects();
  effects.entry_effect_signature.effects = ["io"];
  effects.functions.main = ["io"];
  const bundle = { provenance: {
    commit: "fixture-commit", compiler: "ostrinc fixture", target: "wasm32-wasip1",
    runtime: validRuntime(), inputs: [{ id: "input" }], outputs: [{ id: "output" }],
    parameters: { scale: 1 }, execution: { mode: "recorded" },
    figure_metadata: { source_hash: "fixture" }, effects,
  } };
  const nonPureButClosed = inspectBundle(bundle, "known-io.json");
  assert.equal(nonPureButClosed.entryEffectSignature.row_closed, true);
  assert.equal(nonPureButClosed.status, "READY");

  delete effects.entry_effect_signature;
  const missing = inspectBundle(bundle, "missing-entry-signature.json");
  assert.ok(missing.r3Extensions.missing.includes("provenance.effects.entry_effect_signature"));
  assert.equal(missing.status, "OPEN");
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
  assert.equal(diagnostics.symbolicCallbackReferenceCount, 0);
  assert.equal(diagnostics.uniqueSymbolicCallbacks, 0);
});

test("entry closure diagnostics separate the root row from module-wide effect rows", () => {
  const closure = diagnoseEntryEffectClosure({
    entry: "main",
    functions: { main: ["io"] },
    unresolved_calls: { main: [], library_helper: ["host_effect"] },
    symbolic_effect_calls: { main: [], generic_library_helper: ["callback"] },
  });
  assert.deepEqual(closure, {
    entry: "main",
    status: "closed",
    unresolvedCalls: [],
    symbolicCallbacks: [],
  });

  const open = diagnoseEntryEffectClosure({
    entry: "main",
    functions: { main: [] },
    unresolved_calls: { main: ["host_effect"] },
    symbolic_effect_calls: { main: ["callback"] },
  });
  assert.equal(open.status, "open");
  assert.deepEqual(open.unresolvedCalls, ["host_effect"]);
  assert.deepEqual(open.symbolicCallbacks, ["callback"]);

  const unavailable = diagnoseEntryEffectClosure({
    entry: "main",
    functions: { main: [] },
    unresolved_calls: { main: [] },
  });
  assert.deepEqual(unavailable, {
    entry: "main",
    status: "unavailable",
    unresolvedCalls: null,
    symbolicCallbacks: null,
  });

  const missingEntry = diagnoseEntryEffectClosure({
    entry: "main",
    functions: { helper: [] },
    unresolved_calls: {},
    symbolic_effect_calls: {},
  });
  assert.equal(missingEntry.status, "unavailable");
});

test("effect-gap diagnostics report symbolic callbacks separately from unresolved calls", () => {
  const diagnostics = diagnoseEffectCalls({
    unresolved_calls: { main: [] },
    symbolic_effect_calls: {
      "std.viz::grid_of": ["f"],
      "std.numeric::rk45": ["f", "jacobian"],
    },
  });
  assert.equal(diagnostics.referenceCount, 0);
  assert.equal(diagnostics.symbolicCallbackReferenceCount, 3);
  assert.equal(diagnostics.uniqueSymbolicCallbacks, 3);
  assert.equal(diagnostics.symbolicCallbacks.get("std.viz::grid_of.f"), 1);
});

test("effect-gap diagnostics distinguish unavailable symbolic inventory from an empty one", () => {
  const unavailable = diagnoseEffectCalls({
    entry: "main",
    functions: { main: [], helper: [] },
    function_calls: { main: ["helper"], helper: [] },
  });
  assert.equal(unavailable.symbolicInventoryAvailable, false);
  assert.equal(unavailable.directlyReachableFunctionCount, null);
  assert.equal(unavailable.directlyReachableSymbolicCallbacks, null);

  const available = diagnoseEffectCalls({
    entry: "main",
    functions: { main: [], helper: [] },
    function_calls: { main: ["helper"], helper: [] },
    symbolic_effect_calls: {},
  });
  assert.equal(available.symbolicInventoryAvailable, true);
  assert.equal(available.directlyReachableFunctionCount, 2);
  assert.equal(available.directlyReachableSymbolicCallbacks.size, 0);

  const missingReachableRow = diagnoseEffectCalls({
    entry: "main",
    functions: { main: [], helper: [] },
    function_calls: { main: ["helper"] },
    symbolic_effect_calls: {},
  });
  assert.equal(missingReachableRow.directlyReachableFunctionCount, null);
  assert.equal(missingReachableRow.directlyReachableSymbolicCallbacks, null);
});

test("direct reachability follows only exact recorded function edges", () => {
  const diagnostics = diagnoseEffectCalls({
    entry: "main",
    functions: { main: [], helper: [], "std.viz::grid_of": [] },
    function_calls: {
      main: ["helper", "callback", "std.viz::grid_of.f"],
      helper: ["main"],
      "std.viz::grid_of": [],
    },
    symbolic_effect_calls: {
      helper: ["callback"],
      "std.viz::grid_of": ["f"],
    },
  });
  assert.equal(diagnostics.directlyReachableFunctionCount, 2);
  assert.equal(diagnostics.directlyReachableSymbolicCallbacks.get("helper.callback"), 1);
  assert.equal(diagnostics.directlyReachableSymbolicCallbacks.has("std.viz::grid_of.f"), false);
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
  const { captureGaps, effectInventory, notAssessed, reproducibilityEvidence } = report.gapDiagnostics;
  const hostGap = captureGaps.missingExtensionFields.find(({ field }) => field === "provenance.runtime.host");
  const effectGap = captureGaps.incompleteExtensionFields.find(({ field }) => field === "provenance.effects.effect_coverage");

  assert.equal(hostGap?.bundleCount ?? 0, report.captureOpenCount);
  assert.equal(effectGap?.bundleCount ?? 0, report.incompleteEffectBundleCount);
  assert.equal(effectInventory.operationReferences, report.unresolvedEffectCallCount);
  assert.equal(Object.values(effectInventory.bySyntax).reduce((sum, count) => sum + count, 0),
    effectInventory.operationReferences);
  assert.equal(effectInventory.uniqueOperationNames > 0, report.unresolvedEffectCallCount > 0);
  assert.equal(effectInventory.topOperations.length > 0, report.unresolvedEffectCallCount > 0);
  if (effectInventory.symbolicInventoryStatus === "available") {
    assert.equal(effectInventory.symbolicCallbackReferences,
      report.entries.reduce((total, entry) => total + (entry.effectCoverage?.symbolic_calls ?? 0), 0));
  } else {
    assert.equal(effectInventory.symbolicCallbackReferences, null);
    assert.equal(effectInventory.uniqueSymbolicCallbacks, null);
  }
  assert.equal(effectInventory.symbolicInventoryStatus,
    effectInventory.symbolicInventoryUnavailableBundleCount === 0 ? "available"
      : effectInventory.symbolicInventoryBundleCount === 0 ? "unavailable" : "partial");
  assert.equal(effectInventory.directReachability.availableBundleCount
    + effectInventory.directReachability.unavailableBundleCount, report.bundleCount);
  if (effectInventory.directReachability.status !== "available") {
    assert.equal(effectInventory.directReachability.symbolicCallbackReferences, null);
    assert.equal(effectInventory.directReachability.uniqueSymbolicCallbacks, null);
  }
  const entrypoint = report.gapDiagnostics.entrypointEffects;
  assert.equal(entrypoint.closedBundleCount + entrypoint.openBundleCount + entrypoint.unavailableBundleCount,
    report.bundleCount);
  assert.equal(entrypoint.closedBundleCount, report.entries.filter(({ entryEffectClosure }) => entryEffectClosure.status === "closed").length);
  assert.equal(entrypoint.openBundleCount, report.entries.filter(({ entryEffectClosure }) => entryEffectClosure.status === "open").length);
  assert.equal(entrypoint.unavailableBundleCount, report.entries.filter(({ entryEffectClosure }) => entryEffectClosure.status === "unavailable").length);
  assert.match(effectInventory.interpretation, /does not determine type resolution or prove purity/i);
  assert.ok(notAssessed.includes("Whether numerical tolerances are scientifically justified"));
  assert.ok(reproducibilityEvidence.embeddedInputSnapshotBundles > 0);
  assert.ok(reproducibilityEvidence.embeddedInputSnapshotBundles <= report.bundleCount);
  assert.ok(reproducibilityEvidence.declaredEmbeddedInputSnapshotCount > 0);
  assert.equal(reproducibilityEvidence.ciVerifiedExecutionBundles, 29);
  assert.equal(reproducibilityEvidence.recordedVerifiedReplayBundles, report.bundleCount);
  assert.equal(reproducibilityEvidence.numericComparisonNotClaimedBundles, 29);
  assert.equal(reproducibilityEvidence.numericPolicyToleranceMentionBundles, report.bundleCount);
  assert.match(reproducibilityEvidence.interpretation, /do not establish completeness/i);
});
