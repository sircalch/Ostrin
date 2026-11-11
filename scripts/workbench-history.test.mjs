import test from "node:test";
import assert from "node:assert/strict";
import { comparability, compareRuns, createHistory, figureOf, findRun, recordRun, runRecord } from "../website/workbench-history.js";

const TOL = "x: 1e-12 (Brent); residual <= 10*|f'(root)|*1e-12; derivative: |numeric - analytic| <= 1e-5";
const doc = (cs, overrides = {}) => ({
  schema: "ostrin.results/v1",
  experiment: "cubic-parameter-sweep",
  family: "x^3 - 2x - c",
  bracket: [1.0, 3.0],
  method: "std.numeric.brent",
  tolerances: TOL,
  rows: cs.map((c) => ({ c, status: "converged", root: c + 0.5, residual: 0, residual_bound: 1e-11, analytic_derivative: 1, numeric_derivative: 1, passed: true, message: "" })),
  ...overrides,
});
const digests = (figure = "<svg>a</svg>") => ({ sources: { "main.ostrin": "a".repeat(64) }, stdout: "b".repeat(64), stderr: "c".repeat(64), figure: figure ? "d".repeat(64) : null, manifest: "e".repeat(64) });
const run = (id, results, overrides = {}) =>
  runRecord({
    id,
    projectId: "sweep-project",
    projectName: "Parameter sweep",
    entry: "main.ostrin",
    command: "--run main.ostrin",
    status: "completed",
    exitCode: 0,
    compilerIdentity: "ostrinc 0.2.0",
    startedAt: "2026-11-10T10:00:00Z",
    durationMs: 300,
    results,
    digests: digests(),
    ...overrides,
  });

test("a run is recorded once; a duplicate id is refused", () => {
  let history = createHistory();
  history = recordRun(history, run("r1", doc([1, 3])), "<svg>a</svg>");
  assert.throws(() => recordRun(history, run("r1", doc([1, 3])), "<svg>a</svg>"), /already recorded/);
  assert.equal(history.entries.length, 1);
  assert.equal(findRun(history, "r1").id, "r1");
});

test("a figure is stored once per digest and is shared by runs that produce the same SVG", () => {
  let history = createHistory();
  history = recordRun(history, run("r1", doc([1]), { digests: digests("x") }), "<svg>same</svg>");
  history = recordRun(history, run("r2", doc([1]), { digests: digests("x") }), "<svg>same</svg>");
  assert.equal(Object.keys(history.figures).length, 1, "one stored figure for two runs");
  assert.equal(figureOf(history, findRun(history, "r2")), "<svg>same</svg>");
});

test("a figure without its digest is refused, and an unknown status is refused", () => {
  assert.throws(() => recordRun(createHistory(), run("r1", doc([1]), { digests: digests(null) }), "<svg/>"), /SHA-256 digest/);
  assert.throws(() => run("r1", doc([1]), { status: "maybe" }), /unknown status/);
});

test("the digests must be supplied by the caller; the record never invents one", () => {
  assert.throws(() => runRecord({ id: "r1", status: "completed", projectId: "p", entry: "main.ostrin", command: "x" }), /digests/);
});

test("comparable runs: the same question, method, tolerances and compiler", () => {
  const a = run("A", doc([1, 3, 5, 7]));
  const b = run("B", doc([1, 3, 5, 7]));
  assert.deepEqual(comparability(a, b), { comparable: true, reasons: [] });
});

test("incompatible runs are refused, with the reason for each rule", () => {
  const a = run("A", doc([1, 3]));
  assert.match(comparability(a, run("B", doc([1, 3], { experiment: "other" }))).reasons.join(";"), /different experiments/);
  assert.match(comparability(a, run("B", doc([1, 3], { family: "x^3 - c" }))).reasons.join(";"), /different model family/);
  assert.match(comparability(a, run("B", doc([1, 3], { bracket: [0.5, 3] }))).reasons.join(";"), /different bracket/);
  assert.match(comparability(a, run("B", doc([1, 3], { method: "bisect" }))).reasons.join(";"), /different method/);
  assert.match(comparability(a, run("B", doc([1, 3], { tolerances: "other" }))).reasons.join(";"), /different tolerances/);
  assert.match(comparability(a, run("B", doc([1, 3]), { compilerIdentity: "ostrinc 0.3.0" })).reasons.join(";"), /different compiler identity/);
  assert.throws(() => compareRuns(a, run("B", doc([1, 3], { method: "bisect" }))), /not comparable/);
});

test("a run whose method or tolerances are not declared is never assumed equal to another", () => {
  const undeclared = { ...doc([1, 3]) };
  delete undeclared.method;
  delete undeclared.tolerances;
  const reasons = comparability(run("A", doc([1, 3])), run("B", undeclared)).reasons.join(";");
  assert.match(reasons, /method not declared/);
  assert.match(reasons, /tolerances not declared/);
});

test("runs without validated results or without a compiler identity are not compared", () => {
  assert.match(comparability(run("A", null), run("B", doc([1]))).reasons.join(";"), /no validated structured results/);
  assert.match(comparability(run("A", doc([1]), { compilerIdentity: null }), run("B", doc([1]))).reasons.join(";"), /compiler identity not recorded/);
});

test("comparing two experiments matches rows by c and reports the differences b minus a", () => {
  const a = run("A", doc([1, 3, 5, 7]));
  const b = run("B", doc([3, 5, 7, 9]));
  const cmp = compareRuns(a, b);
  assert.deepEqual(cmp.rows.map((r) => r.c), [3, 5, 7]);
  assert.equal(cmp.rows[0].rootDifference, 0, "same root for the same c");
  assert.deepEqual(cmp.onlyInA, [1]);
  assert.deepEqual(cmp.onlyInB, [9]);
});
