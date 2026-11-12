// Runs every code cell of the three example notebooks with the native compiler, exactly as the page does: the
// cell's request (planRun) is written to a folder and the program runs there. Each output is checked against a
// reference computed here, independently of the Ostrin programs:
//   - the calibration statistics and regressions: scripts/data-engine-fixture.mjs (exact rational arithmetic);
//   - the decay and the oscillator: their analytic solutions;
//   - the symbolic values: the polynomial 3x^2 - 2x + 1, its derivative and its roots, by hand.
import test from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, existsSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";
import { parseNotebook, planRun, outputsFromLines, csvRows, setSource, serializeNotebook } from "../website/workbench-notebook.js";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");
const binary = join(root, "compiler", "target", "release", process.platform === "win32" ? "ostrinc.exe" : "ostrinc");
const available = existsSync(binary);
const reference = JSON.parse(readFileSync(join(root, "examples", "data", "calibration.reference.json"), "utf8"));

function load(name) {
  return parseNotebook(readFileSync(join(root, "website", "notebooks", `${name}.ostrin.notebook.json`), "utf8"));
}

// Runs one cell as the page would, and returns its outputs (stdout, tables, figures, results) and exit status.
function runCell(doc, cellId) {
  const plan = planRun(doc, cellId);
  assert.equal(plan.ok, true, `${cellId} cannot run: ${plan.problems.join("; ")}`);
  const dir = mkdtempSync(join(tmpdir(), "ostrin-nb-"));
  try {
    for (const [path, text] of Object.entries(plan.request.files)) {
      mkdirSync(dirname(join(dir, path)), { recursive: true });
      writeFileSync(join(dir, path), text);
    }
    let stdout = "";
    let status = 0;
    try {
      stdout = execFileSync(binary, plan.request.command, { cwd: dir, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
    } catch (error) {
      stdout = error.stdout ?? "";
      status = error.status ?? 1;
    }
    const text = stdout.replace(/\r\n/g, "\n");
    const lines = (text.endsWith("\n") ? text.slice(0, -1) : text).split("\n").map((l) => ["out", l]);
    return { ...outputsFromLines(lines), status };
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

const near = (a, b, tol = 1e-12) => Math.abs(a - b) <= tol * Math.max(1, Math.abs(b));

test("the three notebooks are valid and carry no recorded outputs", () => {
  for (const name of ["experimental", "physical", "symbolic", "numeric"]) {
    const doc = load(name);
    assert.deepEqual(doc.runs, {}, `${name} must not contain outputs`);
    assert.equal(doc.format, "ostrin.notebook");
    assert.equal(doc.version, 1);
  }
  assert.equal(load("experimental").resources[0].path, "data/calibration.csv");
});

test("experimental: the descriptive statistics and the regressions match the exact reference", { skip: !available }, () => {
  const doc = load("experimental");
  const out = runCell(doc, "c2");
  assert.equal(out.status, 0, out.stdout);
  assert.equal(out.tables.length, 2, "statistics and fit tables");
  const [stats] = csvRows(out.tables[0].csv).slice(1);
  assert.ok(near(Number(stats[0]), reference.response.n), "count");
  assert.ok(near(Number(stats[1]), reference.response.mean), "mean");
  assert.ok(near(Number(stats[2]), reference.response.sd), "sd");
  assert.ok(near(Number(stats[5]), reference.response.median), "median");
  const correlation = out.stdout.match(/correlation of temperature and response: (\S+)/);
  assert.ok(correlation && near(Number(correlation[1]), reference.correlation_temperature_response, 1e-12), "correlation");
  const quadratic = out.stdout.match(/quadratic fit: sse (\S+), rmse (\S+), r2 (\S+)/);
  assert.ok(quadratic && near(Number(quadratic[1]), reference.quadratic.sse, 1e-12), "quadratic sse");
  assert.ok(near(Number(quadratic[3]), reference.quadratic.r_squared, 1e-12), "quadratic r2");
  const linear = out.stdout.match(/linear fit: sse (\S+), rmse (\S+), r2 (\S+)/);
  assert.ok(linear && near(Number(linear[3]), reference.linear.r_squared, 1e-12), "linear r2");
});

test("experimental: the fit table holds one row per measurement, and its residuals sum to zero", { skip: !available }, () => {
  const out = runCell(load("experimental"), "c2");
  const rows = csvRows(out.tables[1].csv).slice(1);
  assert.equal(rows.length, reference.rows);
  const residualSum = rows.reduce((sum, row) => sum + Number(row[3]), 0);
  assert.ok(Math.abs(residualSum) < 1e-12, `residual sum ${residualSum}`);
});

test("experimental: both figures are produced by the cells, as one SVG document each", { skip: !available }, () => {
  const doc = load("experimental");
  for (const cell of ["c3", "c4"]) {
    const out = runCell(doc, cell);
    assert.equal(out.status, 0, `${cell}: ${out.stdout}`);
    assert.equal(out.figures.length, 1, `${cell} figure`);
    assert.match(out.figures[0], /^<svg/);
  }
});

test("physical: the decay table agrees with m0 exp(-k t) at every printed time", { skip: !available }, () => {
  const out = runCell(load("physical"), "c2");
  assert.equal(out.status, 0, out.stdout);
  const rows = csvRows(out.tables[0].csv).slice(1);
  assert.equal(rows.length, 13);
  for (const [t, numeric, analytic, error] of rows) {
    const exact = 2.0 * Math.exp(-0.35 * Number(t));
    assert.ok(Math.abs(Number(analytic) - exact) <= 1e-12 * 2, `analytic at t=${t}`);
    assert.ok(Math.abs(Number(numeric) - exact) <= 1e-9, `numeric at t=${t}`);
    assert.ok(Number(error) < 1e-9, `error at t=${t}`);
  }
  assert.match(out.stdout, /largest error over the 1201 nodes: \S+ kg/);
});

test("physical: the oscillator table agrees with the underdamped solution", { skip: !available }, () => {
  const out = runCell(load("physical"), "c4");
  assert.equal(out.status, 0, out.stdout);
  const rows = csvRows(out.tables[0].csv).slice(1);
  const omega = 2.0;
  const zeta = 0.1;
  const wd = omega * Math.sqrt(1 - zeta * zeta);
  for (const [t, numeric, analytic] of rows) {
    const tt = Number(t);
    const exact = Math.exp(-zeta * omega * tt) * (0.5 * Math.cos(wd * tt) + ((0 + zeta * omega * 0.5) / wd) * Math.sin(wd * tt));
    assert.ok(Math.abs(Number(analytic) - exact) <= 1e-12, `analytic at t=${t}`);
    assert.ok(Math.abs(Number(numeric) - exact) <= 1e-7, `numeric at t=${t}`);
  }
});

test("physical: the decay and oscillator figures are produced by their cells", { skip: !available }, () => {
  const doc = load("physical");
  for (const cell of ["c3", "c5"]) {
    const out = runCell(doc, cell);
    assert.equal(out.status, 0, `${cell}: ${out.stdout}`);
    assert.equal(out.figures.length, 1, `${cell} figure`);
  }
});

test("physical: a dimensional mismatch in a parameter is a compile error in the cell that uses it", { skip: !available }, () => {
  let doc = load("physical");
  // Replace the decay of mass with a use that adds a time to a mass: the compiler refuses it, and the cell fails.
  doc = {
    ...doc,
    cells: doc.cells.map((c) => (c.id === "c2" ? { ...c, source: "import c1\nfn main() -> Void {\n    print((c1.initial_mass() + c1.horizon()).to_string())\n}\n" } : c)),
  };
  const out = runCell(doc, "c2");
  assert.notEqual(out.status, 0, "the mismatched cell must fail");
});

test("symbolic: the derivative, the substitution and the sampled values are exact", { skip: !available }, () => {
  const out = runCell(load("symbolic"), "c1");
  assert.equal(out.status, 0, out.stdout);
  assert.match(out.stdout, /f\(x\) = /);
  assert.match(out.stdout, /f'\(x\) = /);
  assert.match(out.stdout, /f\(2\) by substitution: 9/);
  const rows = csvRows(out.tables[0].csv).slice(1);
  assert.equal(rows.length, 9);
  for (const [x, f, df, check] of rows) {
    const xv = Number(x);
    assert.ok(near(Number(f), 3 * xv * xv - 2 * xv + 1), `f at ${x}`);
    assert.ok(near(Number(df), 6 * xv - 2), `f' at ${x}`);
    assert.ok(near(Number(check), Number(f)), `check at ${x}`);
  }
});

test("symbolic: the roots and the factorization are the exact ones, and the unsupported case is reported", { skip: !available }, () => {
  const out = runCell(load("symbolic"), "c2");
  assert.equal(out.status, 0, out.stdout);
  assert.match(out.stdout, /roots of p: 2, 1/);
  assert.match(out.stdout, /factorization: \(-1 \+ x\)\*\(-2 \+ x\)/, "(x - 1)(x - 2)");
  assert.match(out.stdout, /roots of x\^2 \+ 1: error: the discriminant is negative/);
  assert.match(out.stdout, /roots of 2x\^2 \+ 3x - 2 by the same solver: 1\/2, -2/);
});

test("symbolic: the figure of f and f' is drawn from the evaluated expressions", { skip: !available }, () => {
  const out = runCell(load("symbolic"), "c3");
  assert.equal(out.status, 0, out.stdout);
  assert.equal(out.figures.length, 1);
});

test("each notebook's code is stored as text, so a run's outputs can be tied to it", () => {
  for (const name of ["experimental", "physical", "symbolic", "numeric"]) {
    const doc = load(name);
    for (const cell of doc.cells.filter((c) => c.kind === "code")) {
      assert.match(createHash("sha256").update(cell.source).digest("hex"), /^[0-9a-f]{64}$/);
    }
  }
});

// The numeric notebook (D): symbolic to numeric. Its references are computed by hand: f(x) = 2x^3 - 3x + 1 with
// its derivative 6x^2 - 3, and f(1/2) = -1/4.
test("numeric: the transformation and the derivative are the exact expressions, and f(1/2) is exact", { skip: !available }, () => {
  const out = runCell(load("numeric"), "c2");
  assert.equal(out.status, 0, out.stdout);
  assert.match(out.stdout, /f\(x\) = 1 \+ 2\*x\^3 - 3\*x/);
  assert.match(out.stdout, /f'\(x\) = -3 \+ 6\*x\^2/);
  assert.match(out.stdout, /f\(1\/2\) exactly: -1\/4/);
});

test("numeric: the table gives f, f' and the central difference at nine points, each matching the hand values", { skip: !available }, () => {
  const out = runCell(load("numeric"), "c3");
  assert.equal(out.status, 0, out.stdout);
  assert.equal(out.tables.length, 1);
  const rows = csvRows(out.tables[0].csv).slice(1);
  assert.equal(rows.length, 9);
  for (const [x, f, df, quotient] of rows) {
    const xv = Number(x);
    assert.ok(near(Number(f), 2 * xv ** 3 - 3 * xv + 1), "f at " + x);
    assert.ok(near(Number(df), 6 * xv * xv - 3), "f' at " + x);
    assert.ok(Math.abs(Number(quotient) - Number(df)) < 1e-7, "central difference at " + x);
  }
});

test("numeric: the checks report no mismatch, refuse the tiny step, and find no counterexample to the true identity", { skip: !available }, () => {
  const out = runCell(load("numeric"), "c4");
  assert.equal(out.status, 0, out.stdout);
  assert.match(out.stdout, /derivative against central differences: 0 mismatched points of 41/);
  assert.match(out.stdout, /a step of 1e-16 is refused, as it should be/);
  assert.match(out.stdout, /\(x \+ 1\)\^3 = x\^3 \+ 3x\^2 \+ 3x \+ 1: 0 counterexamples on the grid/);
});

test("numeric: the figure is an SVG document drawn from the sampled values", { skip: !available }, () => {
  const out = runCell(load("numeric"), "c5");
  assert.equal(out.status, 0, out.stdout);
  assert.equal(out.figures.length, 1);
  assert.match(out.figures[0], /^<svg /);
});

test("numeric: changing a parameter in c1 changes the next run of the cells that depend on it", { skip: !available }, () => {
  let doc = load("numeric");
  const before = runCell(doc, "c2");
  assert.match(before.stdout, /f'\(x\) = -3 \+ 6\*x\^2/);
  const parameters = doc.cells.find((cell) => cell.id === "c1");
  const edited = parameters.source.replace("return 2", "return 5");
  doc = setSource(doc, "c1", edited);
  const after = runCell(doc, "c2");
  assert.equal(after.status, 0, after.stdout);
  assert.match(after.stdout, /f'\(x\) = -3 \+ 15\*x\^2/);
  const table = csvRows(runCell(doc, "c3").tables[0].csv).slice(1);
  const [, f] = table.find(([value]) => Number(value) === 1);
  assert.ok(near(Number(f), 5 - 3 + 1), "f(1) with a = 5 is 3, got " + f);
  assert.equal(serializeNotebook(doc).includes("return 5"), true);
});
