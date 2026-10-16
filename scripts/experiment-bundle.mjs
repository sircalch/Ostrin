// Build and verify recorded Ostrin experiment contracts.
//
// The v0 bundles carry source, declared inputs, recorded figures and
// machine-readable provenance with hashes. Website CI replays every source
// with the same WASI compiler and compares the complete SVG byte-for-byte;
// input-driven seeded bundles retain an R1 algorithm/partition prerequisite after
// their seed sensitivity contract is verified; embedded external-input bundles can
// claim R2 after their snapshot, capability and sensitivity contract is verified.
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

export const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
export const schema = "ostrin.experiment/v0";
export const defaultFixtureId = "provenance";

// A fixture is complete only when all three recorded inputs are present. Keep
// this registry source-backed so a future bundle cannot silently grow a second
// set of hand-written source, data or figure content.
export const experimentFixtures = Object.freeze({
  lines: Object.freeze({
    id: "lines",
    title: "Lines, bands and legends",
    sourcePath: "examples/viz_lines.ostrin",
    inputPath: "experiments/lines.inputs.json",
    figurePath: "website/assets/viz/lines.svg",
    outputPath: "website/assets/experiments/lines.ostrin-experiment.json",
    sourceRevision: "a773387e2e7ade15a8baffa48e642f5f619b7a9b",
  }),
  surface: Object.freeze({
    id: "surface",
    title: "Shaded 3D surface",
    sourcePath: "examples/viz_surface.ostrin",
    inputPath: "experiments/surface.inputs.json",
    figurePath: "website/assets/viz/surface.svg",
    outputPath: "website/assets/experiments/surface.ostrin-experiment.json",
    sourceRevision: "6235c3fc42179ec790e79fcee7e423624a45a674",
  }),
  heatmap: Object.freeze({
    id: "heatmap",
    title: "Heatmap and contours",
    sourcePath: "examples/viz_heatmap.ostrin",
    inputPath: "experiments/heatmap.inputs.json",
    figurePath: "website/assets/viz/heatmap.svg",
    outputPath: "website/assets/experiments/heatmap.ostrin-experiment.json",
    sourceRevision: "a773387e2e7ade15a8baffa48e642f5f619b7a9b",
  }),
  contourf: Object.freeze({
    id: "contourf",
    title: "Filled contour bands",
    sourcePath: "examples/viz_contourf.ostrin",
    inputPath: "experiments/contourf.inputs.json",
    figurePath: "website/assets/viz/contourf.svg",
    outputPath: "website/assets/experiments/contourf.ostrin-experiment.json",
    sourceRevision: "a773387e2e7ade15a8baffa48e642f5f619b7a9b",
  }),
  quiver: Object.freeze({
    id: "quiver",
    title: "2D vector field",
    sourcePath: "examples/viz_quiver.ostrin",
    inputPath: "experiments/quiver.inputs.json",
    figurePath: "website/assets/viz/quiver.svg",
    outputPath: "website/assets/experiments/quiver.ostrin-experiment.json",
    sourceRevision: "a773387e2e7ade15a8baffa48e642f5f619b7a9b",
  }),
  streamplot: Object.freeze({
    id: "streamplot",
    title: "2D streamlines",
    sourcePath: "examples/viz_streamplot.ostrin",
    inputPath: "experiments/streamplot.inputs.json",
    figurePath: "website/assets/viz/streamplot.svg",
    outputPath: "website/assets/experiments/streamplot.ostrin-experiment.json",
    sourceRevision: "a773387e2e7ade15a8baffa48e642f5f619b7a9b",
  }),
  provenance: Object.freeze({
    id: "provenance",
    title: "Reproducible provenance",
    sourcePath: "examples/viz_provenance.ostrin",
    inputPath: "experiments/provenance.inputs.json",
    figurePath: "website/assets/viz/provenance.svg",
    outputPath: "website/assets/experiments/provenance.ostrin-experiment.json",
    sourceRevision: "0ecc9c6e81d4a7000904db67446a4a377e9dd20d",
  }),
  lorenz: Object.freeze({
    id: "lorenz",
    title: "3D trajectory",
    sourcePath: "examples/viz_lorenz.ostrin",
    inputPath: "experiments/lorenz.inputs.json",
    figurePath: "website/assets/viz/lorenz.svg",
    outputPath: "website/assets/experiments/lorenz.ostrin-experiment.json",
    sourceRevision: "a773387e2e7ade15a8baffa48e642f5f619b7a9b",
  }),
  histogram: Object.freeze({
    id: "histogram",
    title: "Histogram and density",
    sourcePath: "examples/viz_histogram.ostrin",
    inputPath: "experiments/histogram.inputs.json",
    figurePath: "website/assets/viz/histogram.svg",
    outputPath: "website/assets/experiments/histogram.ostrin-experiment.json",
    sourceRevision: "a773387e2e7ade15a8baffa48e642f5f619b7a9b",
  }),
  boxplot: Object.freeze({
    id: "boxplot",
    title: "Grouped boxplots",
    sourcePath: "examples/viz_boxplot.ostrin",
    inputPath: "experiments/boxplot.inputs.json",
    figurePath: "website/assets/viz/boxplot.svg",
    outputPath: "website/assets/experiments/boxplot.ostrin-experiment.json",
    sourceRevision: "a773387e2e7ade15a8baffa48e642f5f619b7a9b",
  }),
  violin: Object.freeze({
    id: "violin",
    title: "Kernel-density violins",
    sourcePath: "examples/viz_violin.ostrin",
    inputPath: "experiments/violin.inputs.json",
    figurePath: "website/assets/viz/violin.svg",
    outputPath: "website/assets/experiments/violin.ostrin-experiment.json",
    sourceRevision: "a773387e2e7ade15a8baffa48e642f5f619b7a9b",
  }),
  hexbin: Object.freeze({
    id: "hexbin",
    title: "Hexbin density",
    sourcePath: "examples/viz_hexbin.ostrin",
    inputPath: "experiments/hexbin.inputs.json",
    figurePath: "website/assets/viz/hexbin.svg",
    outputPath: "website/assets/experiments/hexbin.ostrin-experiment.json",
    sourceRevision: "a773387e2e7ade15a8baffa48e642f5f619b7a9b",
  }),
  "point-cloud": Object.freeze({
    id: "point-cloud",
    title: "3D point cloud",
    sourcePath: "examples/viz_point_cloud.ostrin",
    inputPath: "experiments/point-cloud.inputs.json",
    figurePath: "website/assets/viz/point-cloud.svg",
    outputPath: "website/assets/experiments/point-cloud.ostrin-experiment.json",
    sourceRevision: "a773387e2e7ade15a8baffa48e642f5f619b7a9b",
  }),
  "vector-field": Object.freeze({
    id: "vector-field",
    title: "3D vector field",
    sourcePath: "examples/viz_vector_field.ostrin",
    inputPath: "experiments/vector-field.inputs.json",
    figurePath: "website/assets/viz/vector-field.svg",
    outputPath: "website/assets/experiments/vector-field.ostrin-experiment.json",
    sourceRevision: "a773387e2e7ade15a8baffa48e642f5f619b7a9b",
  }),
  "volume-slices": Object.freeze({
    id: "volume-slices",
    title: "3D volume slices",
    sourcePath: "examples/viz_volume_slices.ostrin",
    inputPath: "experiments/volume-slices.inputs.json",
    figurePath: "website/assets/viz/volume-slices.svg",
    outputPath: "website/assets/experiments/volume-slices.ostrin-experiment.json",
    sourceRevision: "a773387e2e7ade15a8baffa48e642f5f619b7a9b",
  }),
  isosurface: Object.freeze({
    id: "isosurface",
    title: "3D isosurface",
    sourcePath: "examples/viz_isosurface.ostrin",
    inputPath: "experiments/isosurface.inputs.json",
    figurePath: "website/assets/viz/isosurface.svg",
    outputPath: "website/assets/experiments/isosurface.ostrin-experiment.json",
    sourceRevision: "a773387e2e7ade15a8baffa48e642f5f619b7a9b",
  }),
  "scatter-fit": Object.freeze({
    id: "scatter-fit",
    title: "Scatter and fit",
    sourcePath: "examples/viz_scatter_fit.ostrin",
    inputPath: "experiments/scatter-fit.inputs.json",
    figurePath: "website/assets/viz/scatter-fit.svg",
    outputPath: "website/assets/experiments/scatter-fit.ostrin-experiment.json",
    sourceRevision: "a773387e2e7ade15a8baffa48e642f5f619b7a9b",
  }),
  units: Object.freeze({
    id: "units",
    title: "Unit-aware axes",
    sourcePath: "examples/viz_units.ostrin",
    inputPath: "experiments/units.inputs.json",
    figurePath: "website/assets/viz/units.svg",
    outputPath: "website/assets/experiments/units.ostrin-experiment.json",
    sourceRevision: "a773387e2e7ade15a8baffa48e642f5f619b7a9b",
  }),
  bars: Object.freeze({
    id: "bars",
    title: "Bars with error bars",
    sourcePath: "examples/viz_bars.ostrin",
    inputPath: "experiments/bars.inputs.json",
    figurePath: "website/assets/viz/bars.svg",
    outputPath: "website/assets/experiments/bars.ostrin-experiment.json",
    sourceRevision: "a773387e2e7ade15a8baffa48e642f5f619b7a9b",
  }),
  ode: Object.freeze({
    id: "ode",
    title: "Adaptive ODE solver",
    sourcePath: "examples/viz_ode.ostrin",
    inputPath: "experiments/ode.inputs.json",
    figurePath: "website/assets/viz/ode.svg",
    outputPath: "website/assets/experiments/ode.ostrin-experiment.json",
    sourceRevision: "a773387e2e7ade15a8baffa48e642f5f619b7a9b",
  }),
  fft: Object.freeze({
    id: "fft",
    title: "Spectrum with the FFT",
    sourcePath: "examples/viz_fft.ostrin",
    inputPath: "experiments/fft.inputs.json",
    figurePath: "website/assets/viz/fft.svg",
    outputPath: "website/assets/experiments/fft.ostrin-experiment.json",
    sourceRevision: "a773387e2e7ade15a8baffa48e642f5f619b7a9b",
  }),
  spline: Object.freeze({
    id: "spline",
    title: "Cubic spline",
    sourcePath: "examples/viz_spline.ostrin",
    inputPath: "experiments/spline.inputs.json",
    figurePath: "website/assets/viz/spline.svg",
    outputPath: "website/assets/experiments/spline.ostrin-experiment.json",
    sourceRevision: "a773387e2e7ade15a8baffa48e642f5f619b7a9b",
  }),
  "double-pendulum": Object.freeze({
    id: "double-pendulum",
    title: "Double pendulum (animated)",
    sourcePath: "examples/viz_double_pendulum.ostrin",
    inputPath: "experiments/double-pendulum.inputs.json",
    figurePath: "website/assets/viz/double-pendulum.svg",
    outputPath: "website/assets/experiments/double-pendulum.ostrin-experiment.json",
    sourceRevision: "a773387e2e7ade15a8baffa48e642f5f619b7a9b",
  }),
  orbits: Object.freeze({
    id: "orbits",
    title: "Kepler orbits (animated)",
    sourcePath: "examples/viz_orbits.ostrin",
    inputPath: "experiments/orbits.inputs.json",
    figurePath: "website/assets/viz/orbits.svg",
    outputPath: "website/assets/experiments/orbits.ostrin-experiment.json",
    sourceRevision: "a773387e2e7ade15a8baffa48e642f5f619b7a9b",
  }),
  string: Object.freeze({
    id: "string",
    title: "Plucked string (animated)",
    sourcePath: "examples/viz_string.ostrin",
    inputPath: "experiments/string.inputs.json",
    figurePath: "website/assets/viz/string.svg",
    outputPath: "website/assets/experiments/string.ostrin-experiment.json",
    sourceRevision: "a773387e2e7ade15a8baffa48e642f5f619b7a9b",
  }),
  animation: Object.freeze({
    id: "animation",
    title: "Animation",
    sourcePath: "examples/viz_animation.ostrin",
    inputPath: "experiments/animation.inputs.json",
    figurePath: "website/assets/viz/animation.svg",
    outputPath: "website/assets/experiments/animation.ostrin-experiment.json",
    sourceRevision: "a773387e2e7ade15a8baffa48e642f5f619b7a9b",
  }),
  table: Object.freeze({
    id: "table",
    title: "Data table",
    sourcePath: "examples/viz_table.ostrin",
    inputPath: "experiments/table.inputs.json",
    figurePath: "website/assets/viz/table.svg",
    outputPath: "website/assets/experiments/table.ostrin-experiment.json",
    sourceRevision: "a773387e2e7ade15a8baffa48e642f5f619b7a9b",
  }),
  "linked-data": Object.freeze({
    id: "linked-data",
    title: "Linked data selection",
    sourcePath: "examples/viz_linked_data.ostrin",
    inputPath: "experiments/linked-data.inputs.json",
    figurePath: "website/assets/viz/linked-data.svg",
    outputPath: "website/assets/experiments/linked-data.ostrin-experiment.json",
    sourceRevision: "a773387e2e7ade15a8baffa48e642f5f619b7a9b",
  }),
  dashboard: Object.freeze({
    id: "dashboard",
    title: "Multi-panel layout",
    sourcePath: "examples/viz_dashboard.ostrin",
    inputPath: "experiments/dashboard.inputs.json",
    figurePath: "website/assets/viz/dashboard.svg",
    outputPath: "website/assets/experiments/dashboard.ostrin-experiment.json",
    sourceRevision: "a773387e2e7ade15a8baffa48e642f5f619b7a9b",
  }),
});

// Short alias for callers that only need to enumerate the registry.
export const fixtures = experimentFixtures;

function fixtureFor(fixtureId = defaultFixtureId) {
  const fixture = experimentFixtures[fixtureId];
  if (!fixture) {
    const available = Object.keys(experimentFixtures).join(", ");
    throw new Error(`unknown experiment fixture ${JSON.stringify(fixtureId)} (available: ${available})`);
  }
  return fixture;
}

export function outputPathFor(fixtureId = defaultFixtureId, root = repositoryRoot) {
  return path.join(root, fixtureFor(fixtureId).outputPath);
}

export const outputPath = outputPathFor();

function normalizedText(root, relativePath) {
  return readFileSync(path.join(root, relativePath), "utf8").replaceAll("\r\n", "\n");
}

function sha256(value) {
  return `sha256:${createHash("sha256").update(Buffer.from(value, "utf8")).digest("hex")}`;
}

function isStringArrayMap(value) {
  return value != null
    && typeof value === "object"
    && !Array.isArray(value)
    && Object.values(value).every((items) =>
      Array.isArray(items) && items.every((item) => typeof item === "string"));
}

export function validateEffectCapture(effects) {
  const errors = [];
  if (!effects || typeof effects !== "object" || Array.isArray(effects)) {
    return ["effects capture must be an object"];
  }
  if (effects.schema !== "ostrin.effect-report/v0" || effects.status !== "captured") {
    errors.push("effects capture schema/status is missing or unsupported");
  }
  if (typeof effects.experimental !== "boolean" || typeof effects.conservative !== "boolean") {
    errors.push("effects capture must preserve experimental/conservative flags");
  }
  if (!Array.isArray(effects.kinds) || !effects.kinds.every((kind) => typeof kind === "string")) {
    errors.push("effects capture kinds must be an array of strings");
  }
  if (!isStringArrayMap(effects.functions)) errors.push("effects capture functions must map names to effect arrays");
  if (!isStringArrayMap(effects.function_calls)) errors.push("effects capture function_calls must map names to call arrays");
  if (!isStringArrayMap(effects.unresolved_calls)) errors.push("effects capture unresolved_calls must map names to call arrays");
  if (!isStringArrayMap(effects.function_effect_variables)) errors.push("effects capture function_effect_variables must map names to effect-variable arrays");
  if (!isStringArrayMap(effects.symbolic_effect_calls)) errors.push("effects capture symbolic_effect_calls must map names to callback arrays");

  const coverage = effects.effect_coverage;
  if (coverage?.count_unit !== "distinct_function_operation_edges") {
    errors.push("effects capture effect_coverage.count_unit must be distinct_function_operation_edges");
    return errors;
  }
  if (!coverage || typeof coverage !== "object" || Array.isArray(coverage)
    || !Number.isInteger(coverage.known_calls) || coverage.known_calls < 0
    || !Number.isInteger(coverage.symbolic_calls) || coverage.symbolic_calls < 0
    || !Number.isInteger(coverage.unresolved_calls) || coverage.unresolved_calls < 0
    || typeof coverage.complete !== "boolean") {
    errors.push("effects capture effect_coverage is missing or malformed");
    return errors;
  }
  if (!isStringArrayMap(effects.function_calls) || !isStringArrayMap(effects.unresolved_calls)
    || !isStringArrayMap(effects.function_effect_variables) || !isStringArrayMap(effects.symbolic_effect_calls)) return errors;

  const callCount = Object.values(effects.function_calls).reduce((total, calls) => total + calls.length, 0);
  let unresolvedCount = 0;
  for (const [caller, calls] of Object.entries(effects.unresolved_calls)) {
    const recordedCalls = new Set(effects.function_calls[caller] ?? []);
    for (const call of calls) {
      unresolvedCount++;
      if (!recordedCalls.has(call)) errors.push(`unresolved call ${caller}.${call} is absent from function_calls`);
    }
  }
  let symbolicCount = 0;
  for (const [caller, calls] of Object.entries(effects.symbolic_effect_calls)) {
    const recordedCalls = new Set(effects.function_calls[caller] ?? []);
    const unresolvedCalls = new Set(effects.unresolved_calls[caller] ?? []);
    const variables = new Set(effects.function_effect_variables[caller] ?? []);
    for (const call of calls) {
      symbolicCount++;
      if (!recordedCalls.has(call)) errors.push(`symbolic callback ${caller}.${call} is absent from function_calls`);
      if (!variables.has(call) && !variables.has(`ε_${call}`)) errors.push(`symbolic callback ${caller}.${call} has no effect variable`);
      if (unresolvedCalls.has(call)) errors.push(`symbolic callback ${caller}.${call} is also marked unresolved`);
    }
  }
  if (unresolvedCount > callCount
    || coverage.known_calls + symbolicCount + unresolvedCount !== callCount
    || coverage.symbolic_calls !== symbolicCount
    || coverage.unresolved_calls !== unresolvedCount
    || coverage.complete !== (unresolvedCount === 0)) {
    errors.push("effects capture effect_coverage does not match concrete/symbolic/unresolved call categories");
  }
  return errors;
}

export function validateEntryEffectSignature(signature, expectedFunction = "main", effectReport) {
  if (!signature || typeof signature !== "object" || Array.isArray(signature)) {
    return ["entry effect signature must be an object"];
  }
  const errors = [];
  if (signature.function !== expectedFunction || typeof signature.function !== "string") {
    errors.push("entry effect signature must name the recorded entry function");
  }
  if (effectReport != null && (!isStringArrayMap(effectReport.functions)
    || !Object.hasOwn(effectReport.functions, expectedFunction))) {
    errors.push("entry function must have a recorded effect inventory row");
  }
  for (const field of ["effects", "variables", "open_calls"]) {
    if (!Array.isArray(signature[field]) || !signature[field].every((value) => typeof value === "string")) {
      errors.push(`entry effect signature ${field} must be a string array`);
    }
  }
  if (typeof signature.row_closed !== "boolean") {
    errors.push("entry effect signature row_closed must be boolean");
  }
  if (signature.purity_proven !== false) {
    errors.push("entry effect signature must not claim purity proof");
  }
  if (Array.isArray(signature.variables) && Array.isArray(signature.open_calls)
    && typeof signature.row_closed === "boolean"
    && signature.row_closed !== (signature.variables.length === 0 && signature.open_calls.length === 0)) {
    errors.push("entry effect signature row_closed disagrees with its variables/open_calls");
  }
  if (effectReport != null && Array.isArray(signature.effects)
    && isStringArrayMap(effectReport.functions)
    && Object.hasOwn(effectReport.functions, expectedFunction)
    && !sameStringSet(signature.effects, effectReport.functions[expectedFunction])) {
    errors.push("entry effect signature effects disagree with the function effect row");
  }
  if (effectReport != null && Array.isArray(signature.variables)
    && isStringArrayMap(effectReport.function_effect_variables)) {
    const entryVariables = effectReport.function_effect_variables[expectedFunction] ?? [];
    if (!Array.isArray(entryVariables) || !sameStringSet(signature.variables, entryVariables)) {
      errors.push("entry effect signature variables disagree with the function effect variables");
    }
  }
  if (effectReport != null && Array.isArray(signature.open_calls)
    && isStringArrayMap(effectReport.unresolved_calls)) {
    const openCalls = new Set(signature.open_calls);
    for (const call of effectReport.unresolved_calls[expectedFunction] ?? []) {
      if (!openCalls.has(call)) errors.push("entry effect signature omits a direct unresolved call");
    }
  }
  return errors;
}

function sameStringSet(left, right) {
  if (!Array.isArray(left) || !Array.isArray(right) || left.length !== right.length) return false;
  const sortedLeft = [...left].sort();
  const sortedRight = [...right].sort();
  return sortedLeft.every((value, index) => value === sortedRight[index]);
}

function bytes(value) {
  return Buffer.byteLength(value, "utf8");
}

function canonicalJson(value) {
  return `${JSON.stringify(value, null, 2)}\n`;
}

function compilerVersion(root) {
  return normalizedText(root, "compiler/Cargo.toml").match(/^version\s*=\s*"([^"]+)"/m)?.[1] ?? "unknown";
}

function ancestorProjectFile(root, sourcePath, fileName) {
  const rootAbsolute = path.resolve(root);
  let directory = path.dirname(path.resolve(root, sourcePath));
  while (directory === rootAbsolute || directory.startsWith(`${rootAbsolute}${path.sep}`)) {
    const candidate = path.join(directory, fileName);
    if (existsSync(candidate)) return candidate;
    if (directory === rootAbsolute) break;
    directory = path.dirname(directory);
  }
  return null;
}

function runtimeFile(root, absolutePath) {
  if (!absolutePath) return null;
  const relativePath = path.relative(root, absolutePath).replaceAll("\\", "/");
  return {
    path: relativePath,
    sha256: sha256(normalizedText(root, relativePath)),
  };
}

export function runtimeSnapshotFor(root = repositoryRoot, sourcePath = "") {
  const compilerLockfile = path.join(root, "compiler", "Cargo.lock");
  const projectManifest = ancestorProjectFile(root, sourcePath, "ostrin.toml");
  const projectLockfile = ancestorProjectFile(root, sourcePath, "ostrin.lock");
  const host = process.env.OSTRIN_CAPTURE_HOST === "1" ? {
    schema: "ostrin.host/v1",
    source: "github-actions",
    runner_os: process.env.RUNNER_OS ?? "unknown",
    runner_arch: process.env.RUNNER_ARCH ?? "unknown",
    runner_environment: process.env.RUNNER_ENVIRONMENT ?? "unknown",
    image: {
      os: process.env.ImageOS ?? "unknown",
      version: process.env.ImageVersion ?? "unknown",
    },
    node: process.version,
    hardware: {
      arch: process.arch,
      cpu_count: os.cpus().length,
      endianness: os.endianness(),
    },
    wasi: "preview1",
    captured: true,
  } : undefined;
  const snapshot = {
    schema: "ostrin.runtime/v1",
    status: "partial",
    target: "wasm32-wasip1",
    wasi: "preview1",
    runner: "WASI host",
    compiler_lockfile: runtimeFile(root, compilerLockfile),
    dependencies: {
      compiler_lockfile: runtimeFile(root, compilerLockfile),
      project_manifest: runtimeFile(root, projectManifest),
      project_lockfile: runtimeFile(root, projectLockfile),
    },
    scheduler: {
      mode: "single-threaded-wasi-replay",
      concurrency: "not used by gallery fixture",
      status: "declared",
    },
    ...(host ? { host } : {}),
    project: {
      manifest: runtimeFile(root, projectManifest),
      lockfile: runtimeFile(root, projectLockfile),
    },
    platform_policy: {
      output: "byte equality is verified only against the recorded figure in the CI replay configuration; this is not a cross-platform guarantee",
      numeric: "cross-platform numerical comparison is not performed or claimed; before comparing, declare absolute and relative tolerances per metric with units, reference method, and applicable input domain; no universal epsilon is assumed",
    },
    missing: [
      "input snapshots",
      "output hashes from this invocation",
      "captured host and hardware metadata",
    ],
  };
  if (host) snapshot.missing = snapshot.missing.filter((item) => item !== "captured host and hardware metadata");
  return snapshot;
}

const effectReports = new Map();

function compilerEffectSummary(root, sourcePath) {
  const key = `${root}:${sourcePath}`;
  if (effectReports.has(key)) return effectReports.get(key);
  const extension = process.platform === "win32" ? ".exe" : "";
  let compiler = path.join(root, "compiler", "target", "release", `ostrinc${extension}`);
  if (!existsSync(compiler)) {
    execFileSync("cargo", ["build", "--release", "--manifest-path", path.join(root, "compiler", "Cargo.toml"), "--bin", "ostrinc"], {
      cwd: root,
      stdio: "pipe",
      windowsHide: true,
    });
  }
  if (!existsSync(compiler)) throw new Error("release compiler binary is missing after cargo build");
  const raw = execFileSync(compiler, ["--effect-report", "--json", path.join(root, sourcePath)], {
    cwd: root,
    encoding: "utf8",
    windowsHide: true,
    maxBuffer: 16 * 1024 * 1024,
  });
  const report = JSON.parse(raw.trim());
  if (report.schema !== "ostrin.effect-report/v0" || !Array.isArray(report.effects)
    || !report.function_effects || typeof report.function_effects !== "object") {
    throw new Error(`unexpected compiler effect report for ${sourcePath}`);
  }
  const summary = {
    schema: report.schema,
    status: "captured",
    source: "ostrinc --effect-report --json",
    experimental: report.experimental === true,
    conservative: report.conservative === true,
    kinds: [...new Set(report.effects)].sort(),
    entry: "main",
    functions: Object.fromEntries(Object.entries(report.function_effects)
      .sort(([left], [right]) => left < right ? -1 : left > right ? 1 : 0)
      .map(([name, kinds]) => [name, [...new Set(kinds)].sort()])),
    function_calls: report.function_calls,
    unresolved_calls: report.unresolved_calls,
    function_effect_variables: report.function_effect_variables,
    symbolic_effect_calls: report.symbolic_effect_calls,
    entry_effect_signature: {
      function: "main",
      ...report.function_effect_signatures?.main,
    },
    effect_coverage: report.effect_coverage,
    analyzer: ["compiler/src/effects.rs", "compiler/src/main.rs"].map((relativePath) => ({
      path: relativePath,
      sha256: sha256(normalizedText(root, relativePath)),
    })),
  };
  const effectErrors = validateEffectCapture(summary);
  effectErrors.push(...validateEntryEffectSignature(summary.entry_effect_signature, summary.entry, summary));
  if (effectErrors.length > 0) {
    throw new Error(`invalid compiler effect report for ${sourcePath}: ${effectErrors.join("; ")}`);
  }
  effectReports.set(key, summary);
  return summary;
}

function fixtureCommit(root, fixture) {
  if (fixture.sourceRevision) return fixture.sourceRevision;
  try {
    // Resolve the revision from the recorded inputs rather than HEAD. This
    // keeps a checked bundle stable when the generator itself changes later.
    const commit = execFileSync("git", [
      "log", "-1", "--format=%H", "--",
      fixture.sourcePath,
      fixture.inputPath,
      fixture.figurePath,
    ], { cwd: root, encoding: "utf8" }).trim();
    return commit || "unknown";
  } catch {
    return "unknown";
  }
}

export function normalizeFigureMetadata(svg, metadata) {
  const attributes = `source-hash="${metadata.sourceHash}" data-hash="${metadata.dataHash}" seed="seed=${metadata.seed}" compiler="${metadata.compiler}"`;
  const normalized = svg
    .replace(/source-hash="[^"]*"/, `source-hash="${metadata.sourceHash}"`)
    .replace(/data-hash="[^"]*"/, `data-hash="${metadata.dataHash}"`)
    .replace(/seed="[^"]*"/, `seed="seed=${metadata.seed}"`)
    .replace(/compiler="[^"]*"/, `compiler="${metadata.compiler}"`);
  if (normalized.includes("<ostrin-provenance ")) return normalized;
  return normalized.replace("</svg>", `  <ostrin-provenance ${attributes} />\n</svg>`);
}

export function metadataForFixture(fixtureId = defaultFixtureId, root = repositoryRoot) {
  const fixture = fixtureFor(fixtureId);
  const source = normalizedText(root, fixture.sourcePath);
  const data = canonicalJson(JSON.parse(normalizedText(root, fixture.inputPath)));
  const inputs = JSON.parse(data);
  return {
    sourceHash: sha256(source),
    dataHash: sha256(data),
    seed: inputs.seed,
    compiler: `ostrinc ${compilerVersion(root)}`,
  };
}

function bundleFiles(root, fixture) {
  const source = normalizedText(root, fixture.sourcePath);
  const data = canonicalJson(JSON.parse(normalizedText(root, fixture.inputPath)));
  const recordedFigure = normalizedText(root, fixture.figurePath);
  const metadata = metadataForFixture(fixture.id, root);
  const figure = normalizeFigureMetadata(recordedFigure, metadata);
  return {
    "source.ostrin": source,
    "data.json": data,
    "figure.svg": figure,
  };
}

function manifestFor(files) {
  const entries = Object.entries(files)
    // Use code-unit ordering so output does not depend on the host locale.
    .sort(([left], [right]) => left < right ? -1 : left > right ? 1 : 0)
    .map(([filePath, content]) => ({
      path: filePath,
      kind: filePath === "source.ostrin" ? "source" : filePath === "figure.svg" ? "figure" : filePath === "provenance.json" ? "provenance" : "input",
      media_type: filePath.endsWith(".ostrin") ? "text/x-ostrin" : filePath.endsWith(".svg") ? "image/svg+xml" : "application/json",
      bytes: bytes(content),
      sha256: sha256(content),
    }));
  return {
    schema: "ostrin.manifest/v0",
    files: entries,
  };
}

function reproducibilityFor(inputs) {
  const declared = inputs.reproducibility;
  if (!declared) {
    return {
      level: "R0",
      label: "R0 · exact replay verified",
      next: "R1 · seeded replay (planned verification)",
    };
  }
  return {
    level: declared.level,
    label: declared.label,
    next: declared.next,
  };
}

function seededRandomnessEvidence(root, source, inputs) {
  if (typeof inputs.seed !== "number" || !Number.isFinite(inputs.seed) || !/\brng\s*\(/.test(source)) {
    return undefined;
  }
  const inputDriven = inputs.execution?.randomness?.seed_path === "seed"
    && inputs.execution?.consumes?.includes("seed")
    && ["R1", "R2"].includes(reproducibilityFor(inputs).level);
  const implementationPaths = [
    "compiler/src/interpreter/rng.rs",
    "compiler/src/rng_runtime.c",
  ];
  return {
    contract: "ostrin.rng/v1",
    algorithm: "xoshiro256**",
    version: "ostrin.rng/v1",
    seeding: "splitmix64",
    normal: "Marsaglia polar",
    deterministic_log: "detmath::ln",
    partition: inputs.execution?.randomness?.partition ?? "single sequential stream",
    implementations: implementationPaths.map((relativePath) => ({
      path: relativePath,
      sha256: sha256(normalizedText(root, relativePath)),
    })),
    parity: "interpreter-native",
    seed_consumption: inputDriven ? "data.json:seed" : "source-literal",
    status: inputDriven ? "input-driven-r1" : "evidence-only-r0",
  };
}

function provenanceFor(root, fixture, files, manifest) {
  const data = files["data.json"];
  const figure = files["figure.svg"];
  const source = files["source.ostrin"];
  const inputs = JSON.parse(data);
  const metadata = metadataForFixture(fixture.id, root);
  const reproducibility = reproducibilityFor(inputs);
  const randomness = seededRandomnessEvidence(root, source, inputs);
  const effects = compilerEffectSummary(root, fixture.sourcePath);
  const execution = inputs.execution ?? {
    mode: "recorded-parameters",
    input_file: "data.json",
    consumes: Object.keys(inputs.parameters ?? {}).map((key) => `parameters.${key}`),
    capabilities: [],
    command: "node scripts/lab-data.mjs --verify-replays",
    target: "wasm32-wasip1",
    wasi: "preview1",
    status: "verified-in-ci",
  };
  const runtime = runtimeSnapshotFor(root, fixture.sourcePath);
  if (execution.snapshot) {
    runtime.input_snapshots = [{
      ...execution.snapshot,
      sha256: metadata.dataHash,
    }];
    runtime.missing = runtime.missing.filter((item) => item !== "input snapshots");
  }
  runtime.invocation = {
    command: "node scripts/lab-data.mjs --verify-replays",
    status: "verified-in-ci",
    outputs: [{
      path: "figure.svg",
      bytes: bytes(figure),
      sha256: sha256(figure),
    }],
  };
  runtime.missing = runtime.missing.filter((item) => item !== "output hashes from this invocation");
  return {
    schema: "ostrin.provenance/v0",
    level: reproducibility.level,
    reproducibility,
    levels: {
      R0: "Recorded source, inputs and outputs with calculated hashes.",
      R1: "Seeded replay with an input-driven seed, algorithm/version and stream partition; CI must verify seed sensitivity.",
      R2: "Replay of an embedded external input snapshot with declared capabilities and parameters; CI must verify the snapshot and sensitivity contract.",
    },
    program: {
      source: "source.ostrin",
      source_hash: metadata.sourceHash,
      entry: "main",
    },
    inputs: [{
      id: "data.json",
      kind: execution.snapshot ? "external-snapshot" : "parameters",
      sha256: metadata.dataHash,
      seed: inputs.seed,
      ...(execution.snapshot ? { snapshot: execution.snapshot } : {}),
    }],
    outputs: [{
      id: "figure.svg",
      kind: "figure",
      sha256: sha256(figure),
    }],
    replay: {
      level: reproducibility.level,
      status: "verified",
      backend: "ostrinc.wasm",
      target: "wasm32-wasip1",
      command: "node scripts/lab-data.mjs --verify-replays",
      compares: "figure.svg",
      note: reproducibility.level === "R2" && randomness
        ? "Website CI replays the embedded data.json snapshot under its declared filesystem capability, changes the declared seed and verifies the seeded xoshiro256** stream and changed SVG."
        : reproducibility.level === "R2"
        ? "Website CI replays the embedded data.json snapshot under its declared filesystem capability and verifies that changing the declared parameter changes the figure."
        : reproducibility.level === "R1"
        ? "Website CI compares the replayed SVG byte-for-byte and changes the declared data.json seed; the seeded xoshiro256** stream is verified for R1."
        : "Website CI compares the replayed SVG byte-for-byte with this bundled figure; R1 requires seeded-randomness evidence.",
    },
    figure_metadata: {
      policy: "The bundle generator normalizes the recorded SVG metadata to these calculated hashes; bundle manifest hashes are authoritative.",
      source_hash: metadata.sourceHash,
      data_hash: metadata.dataHash,
      seed: `seed=${inputs.seed}`,
      compiler: metadata.compiler,
    },
    compiler: metadata.compiler,
    runtime,
    commit: fixtureCommit(root, fixture),
    target: "wasm32-wasip1",
    command: "ostrinc --run --target wasm32-wasi source.ostrin",
    parameters: inputs.parameters,
    ...(inputs.camera ? { camera: inputs.camera } : {}),
    seed: inputs.seed,
    ...(randomness ? { randomness } : {}),
    execution,
    effects,
    manifest_schema: manifest.schema,
    limits: reproducibility.level === "R2" && randomness
      ? "R2 external snapshot replay and the seeded xoshiro256** sensitivity contract are verified in website CI. Compiler effects are captured by the experimental conservative analyzer; host capture and a complete R3 auditable artifact guarantee remain pending."
      : reproducibility.level === "R2"
      ? "R2 external snapshot replay is verified in website CI for the embedded data.json, declared filesystem capability and parameter sensitivity. Compiler effects are captured by the experimental conservative analyzer; host capture and a complete R3 auditable artifact guarantee remain pending."
      : reproducibility.level === "R1"
      ? "R1 seeded replay is verified in website CI for the declared seed and algorithm contract. Compiler effects are captured by the experimental conservative analyzer; host capture, external snapshots and complete R2/R3 guarantees remain pending."
      : "R0 exact SVG replay is verified in website CI. Compiler effects are captured by the experimental conservative analyzer; host capture, seeded-randomness evidence for R1, external snapshots and complete R2/R3 guarantees remain pending.",
  };
}

export function buildExperimentBundle(fixtureId = defaultFixtureId, root = repositoryRoot) {
  const fixture = fixtureFor(fixtureId);
  for (const relativePath of [fixture.sourcePath, fixture.inputPath, fixture.figurePath]) {
    if (!existsSync(path.join(root, relativePath))) throw new Error(`missing experiment input ${relativePath}`);
  }
  const files = bundleFiles(root, fixture);
  const manifest = manifestFor(files);
  const provenance = provenanceFor(root, fixture, files, manifest);
  const inputData = JSON.parse(files["data.json"]);
  const reproducibility = reproducibilityFor(inputData);
  const provenanceFile = canonicalJson(provenance);
  const allFiles = { ...files, "provenance.json": provenanceFile };
  const completeManifest = manifestFor(allFiles);
  return {
    schema,
    id: fixture.id,
    title: fixture.title,
    reproducibility,
    experiment: {
      source: "source.ostrin",
      inputs: "data.json",
      figure: "figure.svg",
      provenance: "provenance.json",
    },
    manifest: completeManifest,
    provenance,
    files: allFiles,
  };
}

export function renderExperimentBundle(bundle) {
  return canonicalJson(bundle);
}

function expectedText(fixtureId = defaultFixtureId, root = repositoryRoot) {
  return renderExperimentBundle(buildExperimentBundle(fixtureId, root));
}

function checkHash(entry, content, errors) {
  if (!entry || typeof entry.path !== "string") {
    errors.push("manifest entry is missing path");
    return;
  }
  if (entry.bytes !== bytes(content)) errors.push(`${entry.path}: byte count does not match manifest`);
  if (entry.sha256 !== sha256(content)) errors.push(`${entry.path}: sha256 does not match manifest`);
}

export function verifyExperimentBundle(bundle, { expected, fixtureId } = {}) {
  const errors = [];
  let fixture;
  try {
    fixture = fixtureFor(fixtureId ?? bundle?.id ?? defaultFixtureId);
  } catch (error) {
    errors.push(error.message);
    fixture = fixtureFor(defaultFixtureId);
  }
  expected ??= buildExperimentBundle(fixture.id);
  if (!bundle || typeof bundle !== "object") errors.push("bundle is not an object");
  if (bundle?.schema !== schema) errors.push(`schema must be ${schema}`);
  if (bundle?.id !== fixture.id) errors.push(`id must be ${fixture.id}`);
  const maturity = expected?.reproducibility;
  if (!maturity || !["R0", "R1", "R2"].includes(maturity.level)) errors.push("bundle reproducibility level must be R0, R1 or R2");
  if (JSON.stringify(bundle?.reproducibility) !== JSON.stringify(maturity)) errors.push("bundle reproducibility maturity drifted from the calculated fixture contract");
  const files = bundle?.files;
  const entries = bundle?.manifest?.files;
  if (!files || typeof files !== "object") errors.push("files object is missing");
  if (!Array.isArray(entries) || entries.length !== 4) errors.push("manifest must contain four bundled files");
  for (const entry of entries ?? []) checkHash(entry, files?.[entry.path] ?? "", errors);
  for (const required of ["source.ostrin", "data.json", "figure.svg", "provenance.json"]) {
    if (typeof files?.[required] !== "string") errors.push(`${required}: bundled content is missing`);
  }
  const expectedManifest = expected?.manifest?.files ?? [];
  if (JSON.stringify(entries) !== JSON.stringify(expectedManifest)) errors.push("manifest drifted from calculated fixture hashes");
  let provenanceFile;
  try { provenanceFile = JSON.parse(files?.["provenance.json"] ?? "null"); } catch { provenanceFile = null; }
  if (JSON.stringify(bundle?.provenance) !== JSON.stringify(provenanceFile)) errors.push("provenance.json is not the embedded provenance object");
  const effects = bundle?.provenance?.effects;
  for (const error of validateEffectCapture(effects)) errors.push(`provenance ${error}`);
  for (const error of validateEntryEffectSignature(effects?.entry_effect_signature, effects?.entry, effects)) {
    errors.push(`provenance ${error}`);
  }
  const replay = bundle?.provenance?.replay;
  const replayNote = maturity?.level === "R2"
    ? /embedded data\.json snapshot under its declared filesystem capability/.test(replay?.note ?? "")
    : maturity?.level === "R1"
    ? /seeded xoshiro256\*\* stream is verified for R1/.test(replay?.note ?? "")
    : /R1 requires seeded-randomness evidence/.test(replay?.note ?? "");
  if (replay?.level !== maturity?.level || replay?.status !== "verified" || replay?.backend !== "ostrinc.wasm"
    || replay?.target !== "wasm32-wasip1" || replay?.command !== "node scripts/lab-data.mjs --verify-replays"
    || replay?.compares !== "figure.svg" || !replayNote) {
    errors.push("provenance replay contract is missing or overclaims its reproducibility level");
  }
  if (bundle?.provenance?.level !== maturity?.level || JSON.stringify(bundle?.provenance?.reproducibility) !== JSON.stringify(maturity)) {
    errors.push("provenance reproducibility maturity drifted from the bundle contract");
  }
  const seeded = maturity?.level === "R1" || (
    maturity?.level === "R2" && bundle?.provenance?.randomness?.status === "input-driven-r1"
  );
  if (seeded) {
    const execution = bundle?.provenance?.execution;
    const randomness = execution?.randomness;
    const evidence = bundle?.provenance?.randomness;
    if (!randomness || randomness.seed_path !== execution?.sensitivity?.path) {
      errors.push("seeded replay must declare a randomness seed path equal to the sensitivity path");
    }
    if (typeof randomness?.seed_path !== "string" || !execution?.consumes?.includes(randomness.seed_path)) {
      errors.push("seeded randomness seed path must be listed in consumed inputs");
    }
    if (randomness?.algorithm !== evidence?.algorithm || randomness?.version !== evidence?.version) {
      errors.push("seeded randomness algorithm/version does not match provenance evidence");
    }
    if (randomness?.partition !== evidence?.partition) {
      errors.push("seeded randomness partition does not match provenance evidence");
    }
    if (evidence?.status !== "input-driven-r1" || evidence?.seed_consumption !== `data.json:${randomness?.seed_path}`) {
      errors.push("provenance does not prove input-driven seed consumption");
    }
  }
  const runtime = bundle?.provenance?.runtime;
  const expectedRuntime = expected?.provenance?.runtime;
  if (JSON.stringify(runtime) !== JSON.stringify(expectedRuntime)) errors.push("runtime snapshot drifted from the source-backed contract");
  if (runtime?.schema !== "ostrin.runtime/v1" || runtime?.status !== "partial"
    || runtime?.target !== "wasm32-wasip1" || runtime?.wasi !== "preview1"
    || runtime?.runner !== "WASI host") {
    errors.push("runtime snapshot is missing the partial WASI contract");
  }
  const invocation = runtime?.invocation;
  const invocationOutput = invocation?.outputs?.find((entry) => entry.path === "figure.svg");
  const figureEntry = entries?.find((entry) => entry.path === "figure.svg");
  if (invocation?.command !== "node scripts/lab-data.mjs --verify-replays"
    || invocation?.status !== "verified-in-ci"
    || invocationOutput?.bytes !== figureEntry?.bytes
    || invocationOutput?.sha256 !== figureEntry?.sha256) {
    errors.push("runtime invocation output hash is missing or differs from the bundled figure");
  }
  if (maturity?.level === "R2") {
    const execution = bundle?.provenance?.execution;
    const snapshot = execution?.snapshot;
    if (!Array.isArray(execution?.capabilities)
      || !execution.capabilities.includes(`filesystem.read:${execution?.input_file}`)) {
      errors.push("R2 replay contract is missing the filesystem capability for its input snapshot");
    }
    if (!snapshot || snapshot.path !== execution?.input_file || snapshot.embedded !== true) {
      errors.push("R2 replay contract is missing an embedded input snapshot");
    }
    const snapshotRecord = runtime?.input_snapshots?.find((entry) => entry.path === snapshot?.path);
    const inputEntry = entries?.find((entry) => entry.path === execution?.input_file);
    if (!snapshotRecord || snapshotRecord.sha256 !== inputEntry?.sha256) {
      errors.push("R2 runtime snapshot hash does not match the bundled input");
    }
    if (!bundle?.provenance?.parameters || typeof bundle.provenance.parameters !== "object") {
      errors.push("R2 replay contract is missing declared parameters");
    }
  }
  if (JSON.stringify(bundle?.experiment) !== JSON.stringify(expected?.experiment)) errors.push("experiment file map drifted");
  if (bundle?.provenance?.commit !== expected?.provenance?.commit) errors.push("provenance commit drifted from the source-backed fixture revision");
  const embedded = files?.["figure.svg"]?.match(/<ostrin-provenance\s+([^>]+?)\s*\/>/)?.[1] ?? "";
  const embeddedMetadata = Object.fromEntries([...embedded.matchAll(/([a-z-]+)="([^"]*)"/g)].map(([, key, value]) => [key, value]));
  const figureMetadata = bundle?.provenance?.figure_metadata;
  if (!figureMetadata || figureMetadata.policy !== "The bundle generator normalizes the recorded SVG metadata to these calculated hashes; bundle manifest hashes are authoritative.") {
    errors.push("figure metadata authority policy is missing");
  } else {
    if (embeddedMetadata["source-hash"] !== figureMetadata.source_hash) errors.push("figure.svg source-hash is not the calculated bundle hash");
    if (embeddedMetadata["data-hash"] !== figureMetadata.data_hash) errors.push("figure.svg data-hash is not the calculated bundle hash");
    if (embeddedMetadata.seed !== figureMetadata.seed) errors.push("figure.svg seed metadata is not the declared bundle seed");
    if (embeddedMetadata.compiler !== figureMetadata.compiler) errors.push("figure.svg compiler metadata is not the declared bundle compiler");
  }
  return { ok: errors.length === 0, errors };
}

export function verifyBundleFile(filePath = outputPath, fixtureId) {
  if (!existsSync(filePath)) return { ok: false, errors: [`missing ${path.relative(repositoryRoot, filePath)}`] };
  let bundle;
  try {
    bundle = JSON.parse(readFileSync(filePath, "utf8"));
  } catch (error) {
    return { ok: false, errors: [`invalid JSON: ${error.message}`] };
  }
  const selectedFixtureId = fixtureId ?? bundle?.id ?? defaultFixtureId;
  let result;
  try {
    result = verifyExperimentBundle(bundle, { fixtureId: selectedFixtureId });
  } catch (error) {
    result = { ok: false, errors: [error.message] };
  }
  try {
    const actual = readFileSync(filePath, "utf8").replaceAll("\r\n", "\n");
    if (actual !== expectedText(selectedFixtureId)) result.errors.push("bundle is stale; run node scripts/experiment-bundle.mjs --write");
  } catch (error) {
    result.errors.push(error.message);
  }
  return { ok: result.errors.length === 0, errors: result.errors };
}

function fixtureArgument(argv) {
  const inline = argv.find((argument) => argument.startsWith("--fixture="));
  if (inline) return inline.slice("--fixture=".length);
  const index = argv.indexOf("--fixture");
  if (index >= 0 && argv[index + 1]) return argv[index + 1];
  return defaultFixtureId;
}

function main() {
  if (process.argv.includes("--write-all")) {
    for (const fixtureId of Object.keys(experimentFixtures)) {
      const target = outputPathFor(fixtureId);
      mkdirSync(path.dirname(target), { recursive: true });
      writeFileSync(target, expectedText(fixtureId), "utf8");
    }
    console.log(`experiment-bundle: wrote ${Object.keys(experimentFixtures).length} fixtures`);
    return;
  }
  const fixtureId = fixtureArgument(process.argv.slice(2));
  const target = outputPathFor(fixtureId);
  if (process.argv.includes("--write")) {
    mkdirSync(path.dirname(target), { recursive: true });
    writeFileSync(target, expectedText(fixtureId), "utf8");
    console.log(`experiment-bundle: wrote ${path.relative(repositoryRoot, target)}`);
    return;
  }
  const result = verifyBundleFile(target, fixtureId);
  if (!result.ok) {
    console.error(result.errors.map((error) => `experiment-bundle: ${error}`).join("\n"));
    process.exitCode = 1;
  } else {
    const bundle = JSON.parse(readFileSync(target, "utf8"));
    console.log(`experiment-bundle: ok (${schema} · ${fixtureId} · ${bundle.reproducibility.level})`);
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) main();


