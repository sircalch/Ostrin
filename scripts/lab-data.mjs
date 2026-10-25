// Builds website/lab-data.js for the homepage Scientific Lab and the Cookbook.
//
// Every demo is an Ostrin program versioned under examples/. This script copies its sources
// (and, for projects, its path dependencies) into the data file and records the output of
// running them with website/ostrinc.wasm — the same compiler the browser runs — under Node
// WASI. The page shows that recorded output until the visitor presses Run, which recomputes
// it live in the browser. `--check` fails when the sources or the recorded outputs drift.
//
//   node scripts/lab-data.mjs --write   regenerate website/lab-data.js
//   node scripts/lab-data.mjs           check that website/lab-data.js is current
import { WASI } from "node:wasi";
import { createHash } from "node:crypto";
import { closeSync, existsSync, mkdirSync, mkdtempSync, openSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { spawnSync } from "node:child_process";
import { collectSiteFacts, repositoryRoot } from "./site-facts.mjs";
import { experimentFixtures, metadataForFixture, normalizeFigureMetadata, outputPathFor } from "./experiment-bundle.mjs";

const repository = "https://github.com/sircalch/Ostrin";
const wasmPath = path.join(repositoryRoot, "website", "ostrinc.wasm");
const outputPath = path.join(repositoryRoot, "website", "lab-data.js");

// Every demo runs live in the browser. Anything the compiler cannot do yet is described in
// `limits` and on the roadmap instead of being simulated.
export const LAB = [
  {
    id: "plot",
    title: "Plot",
    headline: "A damped oscillator, drawn by Ostrin.",
    file: "examples/lab_plot.ostrin",
    render: "svg",
    params: [
      { name: "damping", label: "Damping", min: 0, max: 1, step: 0.05 },
      { name: "frequency", label: "Frequency", min: 0.5, max: 5, step: 0.25 },
    ],
    how: "Ostrin computes x(t) with array operations (linspace, exp, cos) and std.viz, the visualization library written in Ostrin, lays out the axes, ticks, band, line and legend and writes the SVG text. The page only displays that SVG as an image.",
    docs: { label: "std.viz source", href: `${repository}/blob/main/compiler/std/viz.ostrin` },
    limits: "std.viz renders SVG: hover tooltips and looping animations (viz.animate) work without scripts; the Lab and Viz explorer rewrite declared source parameters and rerun Ostrin, while `.bind(channel, event)` links indexed marks across nested panels. The explorer can download SVG, rasterize the current frame to PNG, print a vector PDF and encode animated GIF/WebM when the browser supports the required APIs.",
  },
  {
    id: "surface",
    title: "3D",
    headline: "A shaded 3D surface, projected by Ostrin.",
    file: "examples/lab_surface.ostrin",
    render: "svg",
    params: [
      { name: "waves", label: "Wave number k", min: 0.4, max: 3, step: 0.1 },
      { name: "azimuth", label: "Azimuth (°)", min: -90, max: 0, step: 5 },
      { name: "elevation", label: "Elevation (°)", min: 10, max: 70, step: 5 },
    ],
    how: "Ostrin samples z = cos(k r) exp(-r/4) on a 28 × 28 grid; std.viz rotates it with an orthographic camera, sorts 1458 triangles back to front, shades each one with a directional light and maps its height to the magma colormap.",
    docs: { label: "std.viz source", href: `${repository}/blob/main/compiler/std/viz.ostrin` },
    limits: "Rendering is CPU-side SVG (painter's algorithm), fine for thousands of triangles. A WebGPU backend for large meshes and volumes is planned.",
  },
  {
    id: "ode",
    title: "ODE",
    headline: "A driven pendulum, integrated and animated by Ostrin.",
    file: "examples/lab_ode.ostrin",
    render: "svg",
    params: [
      { name: "damping", label: "Damping", min: 0, max: 0.6, step: 0.05 },
      { name: "drive", label: "Drive amplitude", min: 0, max: 1.6, step: 0.1 },
      { name: "start_angle", label: "Start angle (rad)", min: -3, max: 3, step: 0.1 },
    ],
    how: "std.numeric.rk45, an adaptive Dormand–Prince integrator written in Ostrin, solves θ'' = −c θ' − sin θ + A cos(0.8 t) with a lambda that captures the three parameters. interp_all resamples the solution at 25 points per second, and std.viz turns them into an animation: the pendulum swings on the left while the phase portrait (θ, ω) draws itself on the right. The browser only interpolates between positions Ostrin computed (SVG <animate>, no scripts).",
    docs: { label: "std.numeric source", href: `${repository}/blob/main/compiler/std/numeric.ostrin` },
    limits: "Explicit solvers only (rk4, rk45): stiff systems need an implicit method, which is planned. The Viz explorer provides scrubbing and finite play-once loops; the Lab preview remains a recorded/live SVG result.",
  },
  {
    id: "linear-algebra",
    title: "Linear Algebra",
    headline: "Solve a spring system, inspect eigenvectors, complex systems and dense factorizations.",
    file: "examples/lab_linear_algebra.ostrin",
    render: "text",
    params: [{ name: "coupling", label: "Spring coupling", min: 0.25, max: 4, step: 0.25 }],
    how: "The stiffness matrix is an Array<Float>; det, solve, norm, @ and eigvals are compiler built-ins. std.numeric.eigen adds ascending eigenvalues, orthonormal eigenvectors and reconstruction for symmetric matrices. std.numeric.lu adds partial-pivoting factors and a triangular solve, std.numeric.qr adds modified Gram–Schmidt diagnostics, std.numeric.svd adds thin singular values with reconstruction, rank, condition-number diagnostics and least-squares solving, and std.numeric.cholesky adds a symmetric positive-definite factor. The same tab now solves a dense complex system with split real/imaginary storage, conjugate transpose and complex matrix-vector multiplication. All paths produce the same result in the interpreter, native C and browser.",
    docs: { label: "numeric hierarchy and arrays", href: `${repository}/blob/main/docs/design/19-jerarquia-numerica-y-arrays.md` },
    limits: "Dense matrices only; real eigenpairs require a symmetric square matrix, LU requires a nonsingular square matrix, QR currently requires full column rank, SVD uses a deterministic Jacobi solve of AᵀA, Cholesky requires a symmetric positive-definite matrix, and complex matrices use split real/imaginary arrays with Gaussian elimination. Parametric Array<Complex>, sparse matrices, complex decompositions and WebGPU remain on the roadmap.",
  },
  {
    id: "complex",
    title: "Complex",
    headline: "Typed complex arithmetic for scientific models.",
    file: "examples/lab_complex.ostrin",
    render: "text",
    params: [{ name: "angle", label: "Rotation angle (rad)", min: -3.14, max: 3.14, step: 0.1 }],
    how: "std.numeric's experimental Complex record carries real and imaginary components through overloaded addition, subtraction, multiplication and division. Polar construction, conjugation, magnitude and integer powers run in Ostrin, so the interpreter, native compiler and browser expose the same values.",
    docs: { label: "numeric hierarchy and arrays", href: `${repository}/blob/main/docs/design/19-jerarquia-numerica-y-arrays.md` },
    limits: "Complex<Float> is currently a library record backed by Float64. Parametric Complex arrays, promotion across Float32/Float64 and complex matrix decompositions beyond dense solve/matmul are planned; the FFT keeps its stable real/imaginary Spectrum API.",
  },
  {
    id: "statistics",
    title: "Statistics",
    headline: "Summaries, a histogram and a calibration fit.",
    file: "examples/lab_statistics.ostrin",
    render: "bars",
    chart: { prefix: "bin", x: "value", series: ["count"] },
    params: [
      { name: "seed", label: "Seed", min: 1, max: 99, step: 1 },
      { name: "samples", label: "Samples", min: 200, max: 5000, step: 200 },
    ],
    how: "A seeded rng draws the sample; mean, std, median, percentile, histogram and linfit run in Ostrin. The bars are drawn from the \"bin\" lines Ostrin prints.",
    docs: { label: "statistics example", href: `${repository}/blob/main/examples/statistics.ostrin` },
    limits: "Descriptive statistics and least-squares fits are built in; distributions beyond the normal and hypothesis tests are planned.",
  },
  {
    id: "monte-carlo",
    title: "Monte Carlo",
    headline: "Estimate π from reproducible random points.",
    file: "examples/lab_monte_carlo.ostrin",
    render: "series",
    chart: { prefix: "estimate", x: "samples", series: ["π estimate"] },
    params: [
      { name: "seed", label: "Seed", min: 1, max: 9999, step: 1 },
      { name: "samples", label: "Samples", min: 2000, max: 40000, step: 2000 },
    ],
    how: "rng(seed).rand draws the points and Ostrin counts the hits, printing the running estimate at ten checkpoints. The same seed gives the same digits in the browser, the interpreter and native code.",
    docs: { label: "random numbers example", href: `${repository}/blob/main/examples/random.ostrin` },
    limits: "The generator is deterministic and seedable; parallel random streams are planned.",
  },
  {
    id: "autodiff",
    title: "Autodiff",
    headline: "Exact derivatives with dual numbers, then Newton's method.",
    project: "examples/autodiff_project/lab",
    render: "series",
    chart: { prefix: "point", x: "x", series: ["f(x)", "f′(x)"] },
    params: [{ name: "start", label: "Newton start", min: 1, max: 6, step: 0.5 }],
    how: "The autodiff package, written in Ostrin, overloads + - * / for a Dual record, so f and f' come from one evaluation. Newton's method uses those derivatives.",
    docs: { label: "autodiff package source", href: `${repository}/blob/main/examples/autodiff_project/autodiff/autodiff.ostrin` },
    limits: "Experimental forward mode for scalar functions. Reverse mode and array-valued functions are planned.",
  },
  {
    id: "units",
    title: "Units",
    headline: "Projectile motion that cannot mix up its units.",
    file: "examples/lab_units.ostrin",
    render: "text",
    params: [
      { name: "launch_speed", label: "Launch speed (m/s)", min: 5, max: 40, step: 1 },
      { name: "angle_degrees", label: "Angle (°)", min: 5, max: 85, step: 5 },
    ],
    how: "Quantities carry their dimension in the type: flight_time only accepts a speed and an acceleration, and dividing by 1 m only compiles for a length. as converts between compatible units.",
    docs: { label: "quantities and units", href: "language.html#quantities" },
    limits: "as converts to compound units (km/h, m/s^2), derived units print simplified (kg*m^2/s^2) Array<Quantity<D>> keeps one unit per array, and programs declare their own units with unit/dimension/define. Affine units (°C) are not supported yet.",
  },
  {
    id: "measurements",
    title: "Measurements",
    headline: "Track correlated uncertainty through scalar and dimensioned arithmetic.",
    file: "examples/measurement_lab.ostrin",
    render: "text",
    params: [],
    how: "std.measurements keeps explicit source identities with values and their uncertainty state. For dimensional values, exact_quantity, unknown_quantity and from_quantity_source construct exact, unknown and source-backed states without hand-written records. Shared IDs preserve correlation through sums, products and same- or cross-dimension ratios; independent IDs combine through the linearized variance rule. The dimensional smoke and Lab have matching interpreter/native output, live_allocations=0, IR-generated main functions, zero AST fallback and zero generic type divergences for the measured cases. The smoke also emits C for WASI. The browser runs the Lab through the interpreter/WASM path. This evidence covers those examples only; it does not establish complete native support for every Measurement<Quantity<D>> program.",
    docs: { label: "measurement design", href: `${repository}/blob/main/docs/design/25-mediciones-e-incertidumbre.md` },
    limits: "The direct full-surface fixture now has native IR/interpreter parity, emits WASI C and ends with live_allocations=0 for its measured cases. Equal dimensions in div_quantity reach the explicit experimental boundary error; use div_quantity_same for a dimensionless ratio. This evidence does not establish general Measurement<Quantity<D>> support. One-dimensional Array<Measurement<Float>> has an experimental structural profile for construction, indexing, get, slicing, boolean-mask selection, to_list and display, with ownership checks for the covered cases. Numeric array operations, reductions, covariance, Monte Carlo and coverage intervals remain planned. Unknown uncertainty is never treated as zero.",
  },
  {
    id: "data",
    title: "Data",
    headline: "Parse CSV and summarize each group.",
    file: "examples/lab_data.ostrin",
    render: "text",
    params: [],
    how: "parse_csv returns List<List<String>>; String.to_float returns a Result that match handles explicitly; a Map groups values and Array computes the statistics.",
    docs: { label: "tables package (DataFrame)", href: `${repository}/tree/main/examples/data_project` },
    limits: "The tables package is a minimal DataFrame. Reading large files and columnar storage are planned.",
  },
  {
    id: "concurrency",
    title: "Concurrency",
    headline: "Split an integral across tasks and join the parts.",
    file: "examples/lab_concurrency.ostrin",
    render: "text",
    params: [{ name: "workers", label: "Tasks", min: 1, max: 8, step: 1 }],
    how: "Each spawn integrates a slice of sin(x) and sends its part through a channel. The browser uses the deterministic cooperative scheduler, and compiled programs can use --native-threads with the same code.",
    docs: { label: "concurrency design", href: "language.html#concurrency" },
    limits: "The browser playground runs the cooperative scheduler, not OS threads. GPU execution is not planned before the core stabilizes.",
  },
];

// Figures of the Viz gallery (website/viz.html). Each program prints one SVG; the recorded SVG
// is written to website/assets/viz/<id>.svg and checked for drift like the Lab outputs.
export const GALLERY = [
  { id: "lines", title: "Lines, bands and legends", file: "examples/viz_lines.ostrin", blurb: "Three damped oscillators, the envelope as a shaded band and a reference line.", bundle: "assets/experiments/lines.ostrin-experiment.json" },
  { id: "surface", title: "Shaded 3D surface", file: "examples/viz_surface.ostrin", blurb: "peaks(x, y) on a 36 × 36 grid: 2450 triangles sorted back to front and lit.", controls: [{ name: "scale", label: "Spatial scale", min: 0.5, max: 2, step: 0.1 }], bundle: "assets/experiments/surface.ostrin-experiment.json" },
  { id: "heatmap", title: "Heatmap and contours", file: "examples/viz_heatmap.ostrin", blurb: "A 48 × 48 field with a colorbar and ten marching-squares contour levels.", bundle: "assets/experiments/heatmap.ostrin-experiment.json" },
  { id: "contourf", title: "Filled contour bands", file: "examples/viz_contourf.ostrin", blurb: "A 48 × 48 scalar field rendered as nine discrete filled contour bands with isolines.", bundle: "assets/experiments/contourf.ostrin-experiment.json" },
  { id: "quiver", title: "2D vector field", file: "examples/viz_quiver.ostrin", blurb: "A sampled rotational velocity field rendered as 221 deterministic arrows with tooltips.", bundle: "assets/experiments/quiver.ostrin-experiment.json" },
  { id: "streamplot", title: "2D streamlines", file: "examples/viz_streamplot.ostrin", blurb: "63 seeded paths integrated bidirectionally through a bilinearly interpolated velocity field.", bundle: "assets/experiments/streamplot.ostrin-experiment.json" },
  { id: "provenance", title: "Reproducible provenance", file: "examples/viz_provenance.ostrin", blurb: "A publication-ready figure carrying source and data hashes, a seed and compiler identity in its SVG metadata.", bundle: "assets/experiments/provenance.ostrin-experiment.json" },
  { id: "lorenz", title: "3D trajectory", file: "examples/viz_lorenz.ostrin", blurb: "The Lorenz attractor integrated in Ostrin and colored by time.", controls: [{ name: "rho", label: "ρ (Rayleigh parameter)", min: 20, max: 40, step: 1 }], bundle: "assets/experiments/lorenz.ostrin-experiment.json" },
  { id: "histogram", title: "Histogram and density", file: "examples/viz_histogram.ostrin", blurb: "20 000 seeded normal samples with the scaled N(4, 1.5²) density on top.", bundle: "assets/experiments/histogram.ostrin-experiment.json" },
  { id: "boxplot", title: "Grouped boxplots", file: "examples/viz_boxplot.ostrin", blurb: "Three seeded cohorts summarized by their whiskers, quartiles and median, computed in Ostrin.", bundle: "assets/experiments/boxplot.ostrin-experiment.json" },
  { id: "violin", title: "Kernel-density violins", file: "examples/viz_violin.ostrin", blurb: "Three seeded cohorts rendered as deterministic Gaussian KDE shapes with median markers.", bundle: "assets/experiments/violin.ostrin-experiment.json" },
  { id: "hexbin", title: "Hexbin density", file: "examples/viz_hexbin.ostrin", blurb: "1 200 seeded bivariate observations counted into color-mapped hexagonal cells in Ostrin.", bundle: "assets/experiments/hexbin.ostrin-experiment.json" },
  { id: "point-cloud", title: "3D point cloud", file: "examples/viz_point_cloud.ostrin", blurb: "Three seeded Gaussian clusters, depth-sorted and colored by height.", bundle: "assets/experiments/point-cloud.ostrin-experiment.json" },
  { id: "vector-field", title: "3D vector field", file: "examples/viz_vector_field.ostrin", blurb: "A sampled rotational field with depth-sorted arrowheads and SVG tooltips.", bundle: "assets/experiments/vector-field.ostrin-experiment.json" },
  { id: "volume-slices", title: "3D volume slices", file: "examples/viz_volume_slices.ostrin", blurb: "Three orthogonal scalar-field cuts, extracted from an Array<Float> volume and depth-sorted as interactive SVG cells.", bundle: "assets/experiments/volume-slices.ostrin-experiment.json" },
  { id: "isosurface", title: "3D isosurface", file: "examples/viz_isosurface.ostrin", blurb: "A scalar shell polygonized from a 3D Float volume with deterministic marching tetrahedra and depth-sorted triangles.", bundle: "assets/experiments/isosurface.ostrin-experiment.json" },
  { id: "scatter-fit", title: "Scatter and fit", file: "examples/viz_scatter_fit.ostrin", blurb: "Calibration data, a least-squares line and its ±2σ band.", bundle: "assets/experiments/scatter-fit.ostrin-experiment.json" },
  { id: "units", title: "Unit-aware axes", file: "examples/viz_units.ostrin", blurb: "Quantities converted to km/h: the axis labels come from the units in the data.", bundle: "assets/experiments/units.ostrin-experiment.json" },
  { id: "bars", title: "Bars with error bars", file: "examples/viz_bars.ostrin", blurb: "Group means with standard uncertainties projected from Measurement<Float> sources.", bundle: "assets/experiments/bars.ostrin-experiment.json" },
  { id: "ode", title: "Adaptive ODE solver", file: "examples/viz_ode.ostrin", blurb: "A pendulum solved with std.numeric.rk45: angle over time and the phase portrait, side by side.", bundle: "assets/experiments/ode.ostrin-experiment.json" },
  { id: "fft", title: "Spectrum with the FFT", file: "examples/viz_fft.ostrin", blurb: "A seeded noisy two-tone signal and its amplitude spectrum from std.numeric.fft: peaks at 50 Hz and 120 Hz.", bundle: "assets/experiments/fft.ostrin-experiment.json" },
  { id: "spline", title: "Cubic spline", file: "examples/viz_spline.ostrin", blurb: "Eight measurements, a natural cubic spline through them and the area under it by Simpson's rule.", bundle: "assets/experiments/spline.ostrin-experiment.json" },
  { id: "double-pendulum", title: "Double pendulum (animated)", file: "examples/viz_double_pendulum.ostrin", blurb: "Chaos in real time: rk45 integrates 12 s with energy conserved to 1e-6, and the rods and masses move smoothly with SVG <animate>.", bundle: "assets/experiments/double-pendulum.ostrin-experiment.json" },
  { id: "orbits", title: "Kepler orbits (animated)", file: "examples/viz_orbits.ostrin", blurb: "Earth, Mars and a comet around the Sun; the comet visibly speeds up at perihelion, as Kepler's second law says.", bundle: "assets/experiments/orbits.ostrin-experiment.json" },
  { id: "string", title: "Plucked string (animated)", file: "examples/viz_string.ostrin", blurb: "25 standing waves summed in Ostrin; viz.morph animates the curve's shape through one period.", bundle: "assets/experiments/string.ostrin-experiment.json" },
  { id: "animation", title: "Animation", file: "examples/viz_animation.ostrin", blurb: "24 frames of a spreading wave packet, combined by viz.animate into one SVG that loops with CSS alone.", bundle: "assets/experiments/animation.ostrin-experiment.json" },
  { id: "table", title: "Data table", file: "examples/viz_table.ostrin", blurb: "A formatted ODE solver comparison rendered as a tooltip-rich SVG table.", bundle: "assets/experiments/table.ostrin-experiment.json" },
  { id: "linked-data", title: "Linked data selection", file: "examples/viz_linked_data.ostrin", blurb: "The same Ostrin samples feed a scatter plot and a table; click either side to select the matching row.", bundle: "assets/experiments/linked-data.ostrin-experiment.json" },
  { id: "dashboard", title: "Multi-panel layout", file: "examples/viz_dashboard.ostrin", blurb: "A seeded four-panel composition of 2D and 3D figures assembled with viz.grid into one SVG.", bundle: "assets/experiments/dashboard.ostrin-experiment.json" },
];

// Curated paths through the gallery. These are discovery metadata only: every step points to
// an existing source-backed figure, so the page can explain a scientific workflow without
// inventing a second set of results or maintaining a separate demo implementation.
export const WORKFLOWS = [
  {
    id: "simulate",
    title: "Simulate a system",
    status: "available",
    summary: "Start with an adaptive ODE, inspect its motion and carry the result into a reproducible figure.",
    steps: [
      { figure: "ode", kind: "figure", title: "Solve and plot", detail: "rk45 integrates the pendulum and renders the trajectory with a phase portrait." },
      { figure: "double-pendulum", kind: "animation", title: "Watch the dynamics", detail: "The same visualization layer exposes continuous SVG motion with scrubbing and video export." },
      { figure: "provenance", kind: "provenance", title: "Record the evidence", detail: "Inspect source and data hashes, seed and compiler identity in the SVG." },
    ],
  },
  {
    id: "analyze",
    title: "Analyze data",
    status: "experimental",
    summary: "Move from observations to a fitted figure, a sortable table and explicit uncertainty evidence.",
    steps: [
      { figure: "scatter-fit", kind: "figure", title: "Fit the observations", detail: "Calibration points, a least-squares line and its ±2σ band are computed in Ostrin." },
      { figure: "table", kind: "table", title: "Inspect the rows", detail: "Filter and sort an SVG table while keeping the values visible and source-linked." },
      { figure: "bars", kind: "figure", title: "Show uncertainty", detail: "Measurement-derived standard uncertainties become error bars in the figure." },
      { figure: "provenance", kind: "provenance", title: "Record the evidence", detail: "Inspect the source and data hashes, seed and compiler identity carried by a figure." },
    ],
  },
  {
    id: "explore-3d",
    title: "Explore 3D fields",
    status: "experimental",
    summary: "Change a parameter, move the camera and compare geometric views of a scalar or vector field.",
    steps: [
      { figure: "surface", kind: "figure", title: "Rotate a surface", detail: "The camera and spatial scale rerun the Ostrin source in the browser." },
      { figure: "lorenz", kind: "animation", title: "Change a trajectory", detail: "Adjust ρ and inspect how the 3D trajectory changes before exporting a frame or animation." },
      { figure: "volume-slices", kind: "figure", title: "Slice a volume", detail: "Orthogonal slices expose a sampled scalar volume with inspectable SVG cells." },
    ],
  },
];

// A tiny function shown through every compiler stage on the homepage.
export const PIPELINE = { file: "examples/lab_pipeline.ostrin", function: "kinetic" };

// The program shown in the homepage hero.
export const HERO = "examples/lab_hero.ostrin";

// Capability tags are derived from the source program and the recorded SVG. They are
// discovery metadata, not maturity claims: the gallery remains the source-backed
// evidence for every item shown on the public Viz page.
function vizCapabilities(source, svg) {
  const capabilities = [/\bviz\.scene3d\s*\(/.test(source) ? "3d" : "2d"];
  if (svg.includes('data-ostrin-motion="true"')
    || /\bviz\.animate\s*\(|\.morph\s*\(|\bmoving_point\s*\(|\brod\s*\(/.test(source)) {
    capabilities.push("animation");
  }
  if (svg.includes('data-ostrin-table="true"') || /\bviz\.table\s*\(/.test(source)) capabilities.push("tables");
  if (svg.includes("<ostrin-provenance")) capabilities.push("reproducibility");
  if (/\bQuantity\b|\bunit_line\b|\bunit_scatter\b/.test(source)) capabilities.push("units");
  return capabilities;
}

function readText(relativePath) {
  return readFileSync(path.join(repositoryRoot, relativePath), "utf8").replaceAll("\r\n", "\n");
}

function projectFiles(projectDir) {
  // Collects a project and its path dependencies, keyed relative to the project's parent.
  const files = {};
  const base = path.posix.dirname(projectDir);
  const visit = (dir) => {
    for (const name of readdirSync(path.join(repositoryRoot, dir)).sort()) {
      if (name === "ostrin.toml" || name.endsWith(".ostrin")) {
        files[path.posix.relative(base, `${dir}/${name}`)] = readText(`${dir}/${name}`);
      }
    }
    const manifest = readText(`${dir}/ostrin.toml`);
    for (const [, dependency] of manifest.matchAll(/path\s*=\s*"([^"]+)"/g)) {
      const target = path.posix.normalize(`${dir}/${dependency}`);
      if (path.posix.dirname(target) !== base) throw new Error(`${dir}: dependency ${dependency} must be a sibling directory`);
      visit(target);
    }
  };
  visit(projectDir);
  return files;
}

export function demoInputs(demo) {
  if (demo.project) {
    const name = path.posix.basename(demo.project);
    const main = `${name}/${readText(`${demo.project}/ostrin.toml`).match(/entry\s*=\s*"([^"]+)"/)[1]}`;
    return { files: projectFiles(demo.project), main, args: ["--run", "--project", name], source: `${demo.project}/${main.slice(name.length + 1)}` };
  }
  return { files: { "main.ostrin": readText(demo.file) }, main: "main.ostrin", args: ["--run", "main.ostrin"], source: demo.file };
}

const SOURCE_NUMBER = "-?(?:\\d+(?:\\.\\d*)?|\\.\\d+)(?:[eE][+-]?\\d+)?";
export const DEFAULT_WASI_CHILD_TIMEOUT_MS = 120_000;

export function spawnBoundedChild(command, args, options = {}, timeoutMs = DEFAULT_WASI_CHILD_TIMEOUT_MS) {
  if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1) {
    throw new RangeError(`child timeout must be a positive integer, got ${timeoutMs}`);
  }
  const child = spawnSync(command, args, {
    ...options,
    timeout: timeoutMs,
    windowsHide: true,
  });
  if (child.error?.code === "ETIMEDOUT") {
    throw new Error(`${command} timed out after ${timeoutMs} ms`);
  }
  if (child.error) throw new Error(`${command} could not be started: ${child.error.message}`);
  return child;
}

function sourceAssignmentPattern(name) {
  const escaped = String(name).replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return new RegExp(`^\\s*${escaped}\\s*=\\s*${SOURCE_NUMBER}$`, "m");
}

// Runs ostrinc.wasm on `files` in a bounded child Node process and returns its stdout lines.
// Each run gets a fresh process: WebAssembly memories of finished instances are not reliably
// released within one process, and a few dozen heavy Viz programs used to crash Node.
function spawnWasmChild(files, args, withEvidence = false, compilerWasmPath = wasmPath, captureFilesystem = false, deterministicRandomSeed = null) {
  const child = spawnBoundedChild(process.execPath, ["--no-warnings", fileURLToPath(import.meta.url), "--wasm-child"], {
    input: JSON.stringify({ files, args, withEvidence, compilerWasmPath, captureFilesystem, deterministicRandomSeed }),
    maxBuffer: 256 * 1024 * 1024,
    encoding: "utf8",
  });
  if (child.status !== 0) throw new Error(child.stderr || `ostrinc ${args.join(" ")} crashed (signal ${child.signal})`);
  return JSON.parse(child.stdout);
}

export async function runWasm(module, files, args) {
  return spawnWasmChild(files, args);
}

// The child returns the digest of the exact WASM bytes it compiled. Hashing a
// parent-side module would not bind the receipt to the separately loaded child.
export async function runWasmWithEvidence(files, args, compilerWasmPath = wasmPath) {
  return spawnWasmChild(files, args, true, compilerWasmPath);
}

// Observe regular-file reads at the WASI boundary and hash the bytes returned to
// the module. This is stronger than scanning source text for read_file calls,
// but deliberately does not claim to cover directory enumeration or metadata
// queries.
export async function runWasmWithFilesystemEvidence(files, args, compilerWasmPath = wasmPath, deterministicRandomSeed) {
  if (!/^[a-f0-9]{64}$/.test(deterministicRandomSeed ?? "")) {
    throw new Error("WASI replay evidence requires a deterministic 32-byte random seed");
  }
  return spawnWasmChild(files, args, true, compilerWasmPath, true, deterministicRandomSeed);
}

function wasiPathLabel(bytes) {
  const pathHash = `sha256:${createHash("sha256").update(bytes).digest("hex")}`;
  let value;
  try {
    value = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    return { path: null, pathHash, pathStatus: "non-utf8" };
  }
  const parts = value.split("/");
  if (value.startsWith("/") || value.includes("\0") || parts.includes("..")) {
    return { path: null, pathHash, pathStatus: "unsafe" };
  }
  const normalized = parts.filter((part) => part && part !== ".").join("/");
  if (!normalized) return { path: ".", pathHash, pathStatus: "directory" };
  return { path: normalized, pathHash, pathStatus: "relative" };
}

export function captureWasiFileReads(importObject) {
  const wasi = importObject.wasi_snapshot_preview1;
  const originals = {
    path_open: wasi.path_open,
    fd_read: wasi.fd_read,
    fd_close: wasi.fd_close,
  };
  let memory = null;
  const openFiles = new Map();
  const events = [];
  const emptyHash = () => createHash("sha256");
  const decodePath = (pointer, length) => {
    if (!memory || !Number.isInteger(pointer) || !Number.isInteger(length)
      || pointer < 0 || length < 0 || pointer + length > memory.buffer.byteLength) {
      return { path: null, pathHash: null, pathStatus: "invalid-memory-range" };
    }
    return wasiPathLabel(new Uint8Array(memory.buffer, pointer, length));
  };
  const startRead = (label, openStatus, openErrno = null) => {
    const event = {
      kind: "file-read",
      ...label,
      openStatus,
      ...(openErrno == null ? {} : { openErrno }),
      bytesRead: 0,
      contentSha256: null,
    };
    const record = { event, hash: emptyHash(), finalized: false };
    events.push(event);
    return record;
  };
  const finish = (record) => {
    if (!record || record.finalized) return;
    record.event.contentSha256 = `sha256:${record.hash.digest("hex")}`;
    record.finalized = true;
  };

  wasi.path_open = function (...args) {
    const label = decodePath(args[2], args[3]);
    const rights = typeof args[5] === "bigint" ? args[5] : BigInt(args[5]);
    const requestsRead = (rights & 2n) !== 0n; // WASI FD_READ
    const errno = Reflect.apply(originals.path_open, this, args);
    if (!requestsRead) return errno;
    if (errno !== 0) {
      startRead(label, "open-failed", errno);
      return errno;
    }
    if (!memory || !Number.isInteger(args[8]) || args[8] < 0 || args[8] + 4 > memory.buffer.byteLength) {
      startRead(label, "trace-failed");
      return errno;
    }
    const descriptor = new DataView(memory.buffer).getInt32(args[8], true);
    const prior = openFiles.get(descriptor);
    finish(prior);
    openFiles.set(descriptor, startRead(label, "opened"));
    return errno;
  };

  wasi.fd_read = function (...args) {
    const errno = Reflect.apply(originals.fd_read, this, args);
    const descriptor = args[0];
    let record = openFiles.get(descriptor);
    if (!record && descriptor === 0 && errno === 0 && memory
      && Number.isInteger(args[3]) && args[3] >= 0 && args[3] + 4 <= memory.buffer.byteLength) {
      const amount = new DataView(memory.buffer).getUint32(args[3], true);
      if (amount > 0) {
        record = startRead({ path: "<stdin>", pathHash: null, pathStatus: "standard-input" }, "opened");
        openFiles.set(descriptor, record);
      }
    }
    if (!record || !memory) return errno;
    if (errno !== 0) {
      record.event.readStatus = `fd_read_errno_${errno}`;
      return errno;
    }
    if (!Number.isInteger(args[1]) || !Number.isInteger(args[2]) || !Number.isInteger(args[3])
      || args[1] < 0 || args[2] < 0 || args[3] < 0
      || args[1] + args[2] * 8 > memory.buffer.byteLength || args[3] + 4 > memory.buffer.byteLength) {
      record.event.readStatus = "invalid-memory-range";
      return errno;
    }
    const view = new DataView(memory.buffer);
    let remaining = view.getUint32(args[3], true);
    for (let index = 0; index < args[2] && remaining > 0; index++) {
      const entry = args[1] + index * 8;
      const pointer = view.getUint32(entry, true);
      const length = view.getUint32(entry + 4, true);
      if (pointer + length > memory.buffer.byteLength) {
        record.event.readStatus = "invalid-memory-range";
        return errno;
      }
      const consumed = Math.min(remaining, length);
      record.hash.update(new Uint8Array(memory.buffer, pointer, consumed));
      record.event.bytesRead += consumed;
      remaining -= consumed;
    }
    if (remaining > 0) record.event.readStatus = "short-iovec-accounting";
    return errno;
  };

  wasi.fd_close = function (...args) {
    const errno = Reflect.apply(originals.fd_close, this, args);
    finish(openFiles.get(args[0]));
    openFiles.delete(args[0]);
    return errno;
  };

  return {
    attachInstance(instance) {
      memory = instance.exports.memory;
    },
    finish() {
      for (const record of openFiles.values()) finish(record);
      openFiles.clear();
      return events.map((event) => ({ ...event }));
    },
  };
}

const wasiRandomEvidenceSchema = "ostrin.wasi-random-evidence/v1";
const wasiRandomAlgorithm = "sha256-counter-test-only/v1";
const wasiErrnoInvalid = 28;

// WASI replay must not depend on ambient host entropy. This deterministic test
// provider is deliberately unsuitable for cryptographic use; the bundle-bound
// seed and each returned buffer hash make the replay input explicit.
export function captureWasiRandom(importObject, deterministicRandomSeed) {
  if (!/^[a-f0-9]{64}$/.test(deterministicRandomSeed ?? "")) {
    throw new Error("WASI random capture requires a deterministic 32-byte seed");
  }
  const wasi = importObject.wasi_snapshot_preview1;
  const original = wasi.random_get;
  if (typeof original !== "function") throw new Error("WASI random_get import is missing");
  const seed = Buffer.from(deterministicRandomSeed, "hex");
  const requests = [];
  let memory = null;
  let blockCounter = 0n;
  wasi.random_get = function (pointer, length) {
    if (!memory || !Number.isInteger(pointer) || !Number.isInteger(length)
      || pointer < 0 || length < 0 || pointer + length > memory.buffer.byteLength) {
      requests.push({ byteLength: Number.isInteger(length) && length >= 0 ? length : null, status: "invalid-memory-range" });
      return wasiErrnoInvalid;
    }
    const target = new Uint8Array(memory.buffer, pointer, length);
    let offset = 0;
    while (offset < length) {
      const counter = Buffer.alloc(8);
      counter.writeBigUInt64BE(blockCounter++);
      const block = createHash("sha256")
        .update("ostrin.wasi.random.sha256-counter-test-only.v1\0", "utf8")
        .update(seed)
        .update(counter)
        .digest();
      const count = Math.min(block.length, length - offset);
      target.set(block.subarray(0, count), offset);
      offset += count;
    }
    requests.push({
      byteLength: length,
      contentSha256: `sha256:${createHash("sha256").update(target).digest("hex")}`,
    });
    return 0;
  };
  return {
    attachInstance(instance) {
      memory = instance.exports.memory;
    },
    finish() {
      return {
        schema: wasiRandomEvidenceSchema,
        algorithm: wasiRandomAlgorithm,
        seed: `sha256:${createHash("sha256").update(seed).digest("hex")}`,
        requests: requests.map((request) => ({ ...request })),
      };
    },
  };
}

// Runs ostrinc.wasm under Node WASI in a scratch copy of `files`; returns stdout lines.
async function runWasmHere(module, files, args, captureFilesystem = false, deterministicRandomSeed = null) {
  const scratch = mkdtempSync(path.join(os.tmpdir(), "ostrin-lab-"));
  const stdoutPath = path.join(scratch, ".stdout");
  const stderrPath = path.join(scratch, ".stderr");
  const work = path.join(scratch, "work");
  let stdoutFd;
  let stderrFd;
  try {
    mkdirSync(work, { recursive: true });
    for (const [name, text] of Object.entries(files)) {
      mkdirSync(path.join(work, path.dirname(name)), { recursive: true });
      writeFileSync(path.join(work, name), text);
    }
    stdoutFd = openSync(stdoutPath, "w");
    stderrFd = openSync(stderrPath, "w");
    const wasi = new WASI({ version: "preview1", args: ["ostrinc", ...args], env: {}, preopens: { ".": work }, stdout: stdoutFd, stderr: stderrFd, returnOnExit: true });
    const importObject = wasi.getImportObject();
    const filesystemCapture = captureFilesystem ? captureWasiFileReads(importObject) : null;
    const randomCapture = captureFilesystem ? captureWasiRandom(importObject, deterministicRandomSeed) : null;
    const instance = await WebAssembly.instantiate(module, importObject);
    filesystemCapture?.attachInstance(instance);
    randomCapture?.attachInstance(instance);
    const code = wasi.start(instance);
    closeSync(stdoutFd);
    closeSync(stderrFd);
    stdoutFd = stderrFd = undefined;
    const stdout = readFileSync(stdoutPath, "utf8").replaceAll("\r\n", "\n");
    const stderr = readFileSync(stderrPath, "utf8");
    if (code !== 0) throw new Error(`ostrinc ${args.join(" ")} exited with ${code}: ${stderr}${stdout}`);
    const lines = stdout.replace(/\n$/, "").split("\n");
    return filesystemCapture
      ? { lines, filesystemReads: filesystemCapture.finish(), wasiRandomEvidence: randomCapture.finish() }
      : lines;
  } finally {
    if (stdoutFd !== undefined) closeSync(stdoutFd);
    if (stderrFd !== undefined) closeSync(stderrFd);
    rmSync(scratch, { recursive: true, force: true });
  }
}

function extractFunction(cSource, name) {
  const start = cSource.search(new RegExp(`^\\S[^\\n;]*\\bostrin_fn_${name}\\([^)]*\\) \\{$`, "m"));
  if (start < 0) throw new Error(`emitted C has no definition for ${name}`);
  const end = cSource.indexOf("\n}\n", start);
  return cSource.slice(start, end + 2);
}

function extractBlock(text, header) {
  // HIR/IR print one function per paragraph.
  const block = text.split(/\n\s*\n/).find((part) => part.trimStart().startsWith(header));
  if (!block) throw new Error(`missing '${header}' in compiler output`);
  return block.trim();
}

export async function buildLabData() {
  if (!existsSync(wasmPath)) throw new Error("website/ostrinc.wasm is missing; run tests/browser/prepare-runtime.mjs or build wasm32-wasip1");
  const module = await WebAssembly.compile(readFileSync(wasmPath));
  const demos = [];
  for (const demo of LAB) {
    const { files, main, args, source } = demoInputs(demo);
    for (const param of demo.params) {
      if (!sourceAssignmentPattern(param.name).test(files[main])) {
        throw new Error(`${source}: parameter ${param.name} needs a 'name = number' line`);
      }
    }
    const output = await runWasm(module, files, args);
    if (demo.chart) {
      const points = output.filter((line) => line.startsWith(`${demo.chart.prefix} `));
      if (points.length < 2 || points.some((line) => line.split(" ").length !== demo.chart.series.length + 2)) {
        throw new Error(`${source}: expected '${demo.chart.prefix} x ${demo.chart.series.map(() => "y").join(" ")}' lines for the chart`);
      }
    }
    demos.push({ ...demo, files, main, args, source, sourceUrl: `${repository}/blob/main/${source}`, output });
  }

  const pipelineSource = readText(PIPELINE.file);
  const stage = (flags) => runWasm(module, { "main.ostrin": pipelineSource }, [...flags, "main.ostrin"]).then((lines) => lines.join("\n"));
  const pipeline = {
    source: PIPELINE.file,
    sourceUrl: `${repository}/blob/main/${PIPELINE.file}`,
    code: pipelineSource.trim(),
    hir: extractBlock(await stage(["--hir"]), `fn ${PIPELINE.function}(`),
    ir: extractBlock(await stage(["--ir"]), `ir fn ${PIPELINE.function}(`),
    c: extractFunction(await stage(["--emit-c"]), PIPELINE.function),
    output: await stage(["--run"]),
  };

  const heroSource = readText(HERO);
  const hero = {
    source: HERO,
    sourceUrl: `${repository}/blob/main/${HERO}`,
    code: heroSource.split("\n").filter((line) => !line.startsWith("//")).join("\n").trim(),
    output: await runWasm(module, { "main.ostrin": heroSource }, ["--run", "main.ostrin"]),
  };

  const gallery = [];
  const figures = {};
  for (const figure of GALLERY) {
    const source = readText(figure.file);
    const fixture = Object.values(experimentFixtures).find((candidate) => candidate.sourcePath === figure.file);
    let reproducibility;
    if (fixture) {
      try {
        reproducibility = JSON.parse(readFileSync(outputPathFor(fixture.id), "utf8")).reproducibility;
      } catch (error) {
        throw new Error(`${figure.file}: experiment bundle maturity is unavailable (${error.message})`);
      }
    }
    for (const param of figure.controls ?? []) {
      if (!sourceAssignmentPattern(param.name).test(source)) {
        throw new Error(`${figure.file}: Viz control ${param.name} needs a 'name = number' line`);
      }
      if (!(Number.isFinite(param.min) && Number.isFinite(param.max) && Number.isFinite(param.step) && param.min < param.max && param.step > 0)) {
        throw new Error(`${figure.file}: Viz control ${param.name} has invalid range metadata`);
      }
    }
    const output = await runWasm(module, {
      "main.ostrin": source,
      ...(fixture ? { "data.json": readText(fixture.inputPath) } : {}),
    }, ["--run", "main.ostrin"]);
    const start = output.findIndex((line) => line.startsWith("<svg"));
    const end = output.findIndex((line, index) => index >= start && line === "</svg>");
    if (start < 0 || end < 0) throw new Error(`${figure.file}: expected one SVG figure in the output`);
    const svgPath = `assets/viz/${figure.id}.svg`;
    const rawSvg = `${output.slice(start, end + 1).join("\n")}\n`;
    // Experiment fixtures use the same calculated metadata as their downloadable bundle.
    // Replay verification below removes drift between the live WASM output and
    // the published surface while preserving the fixture's recorded R0/R1/R2 maturity contract.
    const svg = fixture
      ? normalizeFigureMetadata(rawSvg, metadataForFixture(fixture.id))
      : rawSvg;
    figures[svgPath] = svg;
    const provenanceAttributes = svg.match(/<ostrin-provenance\s+([^>]+?)\s*\/>/)?.[1] ?? "";
    const provenance = Object.fromEntries([...provenanceAttributes.matchAll(/([a-z-]+)="([^"]*)"/g)].map(([, key, value]) => [key, value]));
    gallery.push({
      ...figure,
      source: figure.file,
      sourceUrl: `${repository}/blob/main/${figure.file}`,
      code: source,
      svg: svgPath,
      capabilities: vizCapabilities(source, svg),
      provenance,
      reproducibility,
      printed: [...output.slice(0, start), ...output.slice(end + 1)],
    });
  }
  const galleryIds = new Set(gallery.map((figure) => figure.id));
  const workflows = WORKFLOWS.map((workflow) => ({
    ...workflow,
    steps: workflow.steps.map((step) => {
      if (!galleryIds.has(step.figure)) throw new Error(`${workflow.id}: unknown gallery figure ${step.figure}`);
      const figure = gallery.find((candidate) => candidate.id === step.figure);
      return {
        ...step,
        source: figure.source,
        sourceUrl: figure.sourceUrl,
      };
    }),
  }));

  return { compiler: collectSiteFacts().version, recordedWith: "ostrinc.wasm (wasm32-wasip1) under Node WASI", hero, demos, pipeline, gallery, workflows, figures };
}

/**
 * Compare one live WASM replay with the figure stored in its source-backed
 * experiment bundle. This is intentionally byte-for-byte: a renderer change
 * must update the recorded artifact instead of being silently tolerated.
 */
export function compareReplayFigure(id, replayedSvg, bundle) {
  const errors = [];
  if (typeof replayedSvg !== "string" || replayedSvg.length === 0) {
    errors.push(`${id}: replay did not produce an SVG`);
  }
  if (typeof bundle?.files?.["figure.svg"] !== "string") {
    errors.push(`${id}: bundle has no figure.svg`);
  } else if (bundle.files["figure.svg"] !== replayedSvg) {
    errors.push(`${id}: replayed SVG differs from the bundled figure`);
  }
  const replay = bundle?.provenance?.replay;
  if (!['R0', 'R1', 'R2'].includes(replay?.level) || replay?.status !== "verified" || replay?.backend !== "ostrinc.wasm"
    || replay?.target !== "wasm32-wasip1" || replay?.compares !== "figure.svg") {
    errors.push(`${id}: bundle is missing the verified R0/R1/R2 replay contract`);
  }
  return errors;
}

/**
 * Verify every registered publication fixture against the freshly executed
 * WASM figures returned by buildLabData().
 */
export function verifyFixtureReplays(data, root = repositoryRoot) {
  const errors = [];
  for (const id of Object.keys(experimentFixtures)) {
    const figure = data?.figures?.[`assets/viz/${id}.svg`];
    const file = outputPathFor(id, root);
    if (!existsSync(file)) {
      errors.push(`${id}: missing ${path.relative(root, file)}`);
      continue;
    }
    let bundle;
    try {
      bundle = JSON.parse(readFileSync(file, "utf8"));
    } catch (error) {
      errors.push(`${id}: invalid bundle JSON (${error.message})`);
      continue;
    }
    errors.push(...compareReplayFigure(id, figure, bundle));
  }
  return { ok: errors.length === 0, errors };
}

export function renderLabData(data) {
  const { figures, ...rest } = data;
  return "// Generated by scripts/lab-data.mjs from examples/. Do not edit by hand.\n" +
    `globalThis.OSTRIN_LAB = Object.freeze(${JSON.stringify(rest, null, 2)});\n`;
}

function decodeHtml(text) {
  return text.replace(/<[^>]+>/g, "").replaceAll("&lt;", "<").replaceAll("&gt;", ">").replaceAll("&quot;", "\"").replaceAll("&#39;", "'").replaceAll("&amp;", "&");
}

// Program output shown as static text on a page must come from a real program: every
// <pre data-output-source="examples/..."> line has to appear in that program's output ("…" marks
// omitted lines), and output blocks without a source are rejected.
export async function verifyOutputEvidence() {
  const module = await WebAssembly.compile(readFileSync(wasmPath));
  const websiteRoot = path.join(repositoryRoot, "website");
  const failures = [];
  let verified = 0;
  for (const page of readdirSync(websiteRoot).filter((name) => name.endsWith(".html")).sort()) {
    const html = readFileSync(path.join(websiteRoot, page), "utf8").replaceAll("\r\n", "\n");
    for (const match of html.matchAll(/<pre\b([^>]*)>([\s\S]*?)<\/pre>/g)) {
      const attributes = match[1];
      const source = attributes.match(/data-output-source="([^"]+)"/)?.[1];
      if (!source) {
        if (/class="[^"]*showcase-output/.test(attributes)) failures.push(`${page}: a showcase output has no data-output-source`);
        continue;
      }
      const absolute = path.join(repositoryRoot, source);
      if (!existsSync(absolute)) { failures.push(`${page}: ${source} does not exist`); continue; }
      const isProject = existsSync(path.join(absolute, "ostrin.toml"));
      const inputs = isProject
        ? { files: projectFiles(source), args: ["--run", "--project", path.posix.basename(source)] }
        : { files: { "main.ostrin": readText(source) }, args: ["--run", "main.ostrin"] };
      const output = new Set(await runWasm(module, inputs.files, inputs.args));
      for (const line of decodeHtml(match[2]).split("\n")) {
        if (line !== "…" && !output.has(line)) failures.push(`${page}: "${line}" is not printed by ${source}`);
      }
      verified += 1;
    }
  }
  // Every flag the Reference lists must exist in the real `ostrinc --help`.
  const help = (await runWasm(module, {}, ["--help"])).join("\n");
  const reference = readFileSync(path.join(websiteRoot, "reference.html"), "utf8");
  for (const [, flag] of reference.matchAll(/<code data-cli-flag>([^<]+)<\/code>/g)) {
    if (!new RegExp(`(^|\\s)${flag}(\\s|,|$)`, "m").test(help)) failures.push(`reference.html: ${flag} is not in ostrinc --help`);
  }
  if (failures.length) throw new Error(`static output evidence drifted:\n${failures.join("\n")}`);
  return verified;
}

async function main() {
  const verified = await verifyOutputEvidence();
  console.log(`lab-data: ${verified} static page outputs match their Ostrin programs`);
  const data = await buildLabData();
  if (process.argv.includes("--verify-replays")) {
    const replay = verifyFixtureReplays(data);
    if (!replay.ok) throw new Error(`Experiment replay verification failed:\n${replay.errors.join("\n")}`);
    console.log(`lab-data: ${Object.keys(experimentFixtures).length} experiment replays match their bundled SVG figures`);
  }
  const expected = renderLabData(data);
  const figureDir = path.join(repositoryRoot, "website", "assets", "viz");
  if (process.argv.includes("--write")) {
    writeFileSync(outputPath, expected, "utf8");
    mkdirSync(figureDir, { recursive: true });
    for (const name of readdirSync(figureDir)) {
      if (!data.figures[`assets/viz/${name}`]) rmSync(path.join(figureDir, name));
    }
    for (const [relative, svg] of Object.entries(data.figures)) writeFileSync(path.join(repositoryRoot, "website", relative), svg, "utf8");
    console.log(`lab-data: wrote website/lab-data.js and ${Object.keys(data.figures).length} figures in website/assets/viz`);
  } else {
    const staleFigures = Object.entries(data.figures).filter(([relative, svg]) => {
      const file = path.join(repositoryRoot, "website", relative);
      return !existsSync(file) || readFileSync(file, "utf8").replaceAll("\r\n", "\n") !== svg;
    });
    if (!existsSync(outputPath) || readFileSync(outputPath, "utf8").replaceAll("\r\n", "\n") !== expected || staleFigures.length) {
      console.error(`lab-data: website/lab-data.js or ${staleFigures.map(([name]) => name).join(", ") || "its figures"} is stale (sources or recorded outputs changed); run node scripts/lab-data.mjs --write`);
      process.exitCode = 1;
    } else {
      console.log(`lab-data: ok (${LAB.length} demos and ${GALLERY.length} Viz figures recorded with ostrinc.wasm)`);
    }
  }
}

async function wasmChild() {
  const { files, args, withEvidence, captureFilesystem, deterministicRandomSeed, compilerWasmPath = wasmPath } = JSON.parse(readFileSync(0, "utf8"));
  const wasmBytes = readFileSync(compilerWasmPath);
  const wasmSha256 = `sha256:${createHash("sha256").update(wasmBytes).digest("hex")}`;
  const module = await WebAssembly.compile(wasmBytes);
  const result = await runWasmHere(module, files, args, captureFilesystem, deterministicRandomSeed);
  const lines = Array.isArray(result) ? result : result.lines;
  if (withEvidence || captureFilesystem) {
    process.stdout.write(JSON.stringify({
      lines,
      ...(withEvidence ? { wasmSha256 } : {}),
      ...(captureFilesystem ? { filesystemReads: result.filesystemReads } : {}),
      ...(captureFilesystem ? { wasiRandomEvidence: result.wasiRandomEvidence } : {}),
    }));
  } else {
    process.stdout.write(JSON.stringify(lines));
  }
}

if (process.argv.includes("--wasm-child")) {
  wasmChild().catch((error) => {
    process.stderr.write(error.message);
    process.exitCode = 1;
  });
} else if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  main().catch((error) => {
    console.error(`lab-data: ${error.message}`);
    process.exitCode = 1;
  });
}
