// Build and verify recorded Ostrin experiment contracts.
//
// The v0 bundles are R0 artifacts with an exact local replay gate: each one
// carries its source, declared inputs, recorded figure and machine-readable
// provenance with hashes. The website CI replays the source with the same
// WASI compiler and compares the complete SVG byte-for-byte. They do not
// claim seeded-randomness R1 or R2/R3 guarantees.
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
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

function bytes(value) {
  return Buffer.byteLength(value, "utf8");
}

function canonicalJson(value) {
  return `${JSON.stringify(value, null, 2)}\n`;
}

function compilerVersion(root) {
  return normalizedText(root, "compiler/Cargo.toml").match(/^version\s*=\s*"([^"]+)"/m)?.[1] ?? "unknown";
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

function seededRandomnessEvidence(root, source, inputs) {
  if (typeof inputs.seed !== "number" || !Number.isFinite(inputs.seed) || !/\brng\s*\(/.test(source)) {
    return undefined;
  }
  const implementationPaths = [
    "compiler/src/interpreter/rng.rs",
    "compiler/src/rng_runtime.c",
  ];
  return {
    contract: "ostrin.rng/v1",
    algorithm: "xoshiro256**",
    seeding: "splitmix64",
    normal: "Marsaglia polar",
    deterministic_log: "detmath::ln",
    implementations: implementationPaths.map((relativePath) => ({
      path: relativePath,
      sha256: sha256(normalizedText(root, relativePath)),
    })),
    parity: "interpreter-native",
    seed_consumption: "source-literal",
    status: "evidence-only-r0",
  };
}

function provenanceFor(root, fixture, files, manifest) {
  const data = files["data.json"];
  const figure = files["figure.svg"];
  const source = files["source.ostrin"];
  const inputs = JSON.parse(data);
  const metadata = metadataForFixture(fixture.id, root);
  const randomness = seededRandomnessEvidence(root, source, inputs);
  return {
    schema: "ostrin.provenance/v0",
    level: "R0",
    levels: {
      R0: "Recorded source, inputs and outputs with calculated hashes.",
      R1: "Replay-ready source, inputs, command and target; local replay must still be verified.",
    },
    program: {
      source: "source.ostrin",
      source_hash: metadata.sourceHash,
      entry: "main",
    },
    inputs: [{
      id: "data.json",
      kind: "parameters",
      sha256: metadata.dataHash,
      seed: inputs.seed,
    }],
    outputs: [{
      id: "figure.svg",
      kind: "figure",
      sha256: sha256(figure),
    }],
    replay: {
      level: "R0",
      status: "verified",
      backend: "ostrinc.wasm",
      target: "wasm32-wasip1",
      command: "node scripts/lab-data.mjs --verify-replays",
      compares: "figure.svg",
      note: "Website CI compares the replayed SVG byte-for-byte with this bundled figure; R1 requires seeded-randomness evidence.",
    },
    figure_metadata: {
      policy: "The bundle generator normalizes the recorded SVG metadata to these calculated hashes; bundle manifest hashes are authoritative.",
      source_hash: metadata.sourceHash,
      data_hash: metadata.dataHash,
      seed: `seed=${inputs.seed}`,
      compiler: metadata.compiler,
    },
    compiler: metadata.compiler,
    commit: fixtureCommit(root, fixture),
    target: "wasm32-wasip1",
    command: "ostrinc --run --target wasm32-wasi source.ostrin",
    parameters: inputs.parameters,
    ...(inputs.camera ? { camera: inputs.camera } : {}),
    seed: inputs.seed,
    ...(randomness ? { randomness } : {}),
    ...(inputs.execution ? { execution: inputs.execution } : {}),
    manifest_schema: manifest.schema,
    limits: "R0 exact SVG replay is verified in website CI. The bundle does not include seeded-randomness evidence for R1, external snapshots, lockfiles, runtime captures or an R2/R3 replay guarantee.",
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
  const provenanceFile = canonicalJson(provenance);
  const allFiles = { ...files, "provenance.json": provenanceFile };
  const completeManifest = manifestFor(allFiles);
  return {
    schema,
    id: fixture.id,
    title: fixture.title,
    reproducibility: {
      level: "R0",
      label: "R0 · exact replay verified",
      next: "R1 · seeded replay (planned verification)",
    },
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
  if (bundle?.reproducibility?.level !== "R0") errors.push("bundle must be explicitly labelled R0");
  if (bundle?.reproducibility?.label !== "R0 · exact replay verified") errors.push("bundle must expose the verified R0 replay label");
  if (bundle?.reproducibility?.next !== "R1 · seeded replay (planned verification)") errors.push("bundle must expose the planned seeded R1 label");
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
  const replay = bundle?.provenance?.replay;
  if (replay?.level !== "R0" || replay?.status !== "verified" || replay?.backend !== "ostrinc.wasm"
    || replay?.target !== "wasm32-wasip1" || replay?.command !== "node scripts/lab-data.mjs --verify-replays"
    || replay?.compares !== "figure.svg" || !/R1 requires seeded-randomness evidence/.test(replay?.note ?? "")) {
    errors.push("provenance replay contract is missing or overclaims R1");
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
    console.log(`experiment-bundle: ok (${schema} · ${fixtureId} · R0)`);
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) main();


