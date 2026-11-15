import test from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { comparability, compareRuns, createHistory, figureOf, findRun, isSha256Hex, recordRun, runRecord, validateMetadata, verifyEntry } from "../website/workbench-history.js";

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
const sha = (text) => createHash("sha256").update(text, "utf8").digest("hex");
const digestOf = async (text) => sha(text);
const H = (c) => c.repeat(64);
const digests = () => ({ sources: { "main.ostrin": H("a") }, stdout: H("b"), stderr: H("c"), figure: H("d"), manifest: H("e") });
const run = (id, results, overrides = {}) =>
  runRecord({
    id, projectId: "sweep-project", projectName: "Parameter sweep", entry: "main.ostrin", command: "--run main.ostrin",
    status: "completed", exitCode: 0, compilerIdentity: "ostrinc 0.2.0", startedAt: "2026-11-10T10:00:00Z", durationMs: 300,
    results, digests: digests(), ...overrides,
  });

// ---- recording and figures -------------------------------------------------------------------------------------------

test("a run is recorded once; a duplicate id is refused", () => {
  let history = createHistory();
  history = recordRun(history, run("r1", doc([1, 3])), "<svg>a</svg>");
  assert.throws(() => recordRun(history, run("r1", doc([1, 3])), "<svg>a</svg>"), /already recorded/);
  assert.equal(findRun(history, "r1").id, "r1");
});

test("a figure is stored once per digest; the same content is accepted, different content under the same digest is refused", () => {
  let history = createHistory();
  history = recordRun(history, run("r1", doc([1]), { digests: { ...digests(), figure: H("f") } }), "<svg>same</svg>");
  history = recordRun(history, run("r2", doc([1]), { digests: { ...digests(), figure: H("f") } }), "<svg>same</svg>");
  assert.equal(Object.keys(history.figures).length, 1);
  assert.throws(
    () => recordRun(history, run("r3", doc([1]), { digests: { ...digests(), figure: H("f") } }), "<svg>different</svg>"),
    /refusing to overwrite/,
  );
  assert.equal(figureOf(history, findRun(history, "r2")), "<svg>same</svg>", "the stored figure is unchanged");
});

test("the digests must be SHA-256 hex; a non-hash value is refused, never accepted as a digest", () => {
  assert.throws(() => run("r1", doc([1]), { digests: { ...digests(), sources: { "main.ostrin": "not-a-hash" } } }), /not SHA-256 hex/);
  assert.throws(() => run("r1", doc([1]), { digests: { ...digests(), stdout: "b".repeat(63) } }), /stdout digest is not SHA-256 hex/);
  assert.equal(isSha256Hex(H("a")), true);
  assert.equal(isSha256Hex("A".repeat(64)), false, "uppercase hex is not the canonical form");
});

test("an unknown status is refused, and the timeout state is named as the controller names it", () => {
  assert.throws(() => run("r1", doc([1]), { status: "maybe" }), /unknown status/);
  assert.throws(() => run("r1", null, { status: "timeout" }), /unknown status/, "the controller uses timed-out");
  const timed = run("r1", null, { status: "timed-out" });
  assert.equal(timed.status, "timed-out");
});

// ---- immutability ----------------------------------------------------------------------------------------------------

test("a recorded entry shares no mutable object with its caller: changing the source afterwards changes nothing", () => {
  const results = doc([1, 3]);
  const sources = { "main.ostrin": H("a") };
  const entry = run("r1", results, { digests: { sources, stdout: H("b"), stderr: null, figure: null, manifest: null } });
  results.rows[0].root = 999;
  sources["main.ostrin"] = H("9");
  assert.equal(entry.results.rows[0].root, 1.5, "the recorded result is unchanged");
  assert.equal(entry.digests.sources["main.ostrin"], H("a"), "the recorded digest is unchanged");
  assert.throws(() => { entry.results.rows[0].root = 0; }, TypeError, "a recorded entry is frozen");
});

// ---- hashes: verification and discrepancies --------------------------------------------------------------------------

test("verification recomputes SHA-256 from the content and accepts exactly the recorded content", async () => {
  const content = { sources: { "main.ostrin": "fn main() -> Void {}\n" }, stdout: "49\n", stderr: "", figure: "<svg/>" };
  const entry = run("r1", doc([1]), {
    digests: { sources: { "main.ostrin": sha(content.sources["main.ostrin"]) }, stdout: sha("49\n"), stderr: sha(""), figure: sha("<svg/>"), manifest: null },
  });
  assert.deepEqual(await verifyEntry(entry, content, digestOf), []);
});

test("verification detects every altered artefact: source, stdout, stderr and figure", async () => {
  const good = { sources: { "main.ostrin": "A" }, stdout: "out", stderr: "err", figure: "<svg/>" };
  const entry = run("r1", doc([1]), { digests: { sources: { "main.ostrin": sha("A") }, stdout: sha("out"), stderr: sha("err"), figure: sha("<svg/>"), manifest: null } });
  assert.deepEqual(await verifyEntry(entry, good, digestOf), []);
  assert.match((await verifyEntry(entry, { ...good, sources: { "main.ostrin": "B" } }, digestOf)).join(";"), /source main.ostrin does not match/);
  assert.match((await verifyEntry(entry, { ...good, stdout: "OUT" }, digestOf)).join(";"), /stdout does not match/);
  assert.match((await verifyEntry(entry, { ...good, stderr: "x" }, digestOf)).join(";"), /stderr does not match/);
  assert.match((await verifyEntry(entry, { ...good, figure: "<svg>tampered</svg>" }, digestOf)).join(";"), /figure does not match/);
});

test("verification reports missing artefacts as incomplete, not as matching", async () => {
  const entry = run("r1", doc([1]), { digests: { sources: { "main.ostrin": sha("A") }, stdout: sha("out"), stderr: null, figure: sha("<svg/>"), manifest: null } });
  const mismatches = (await verifyEntry(entry, { sources: {}, stdout: undefined, figure: null }, digestOf)).join(";");
  assert.match(mismatches, /source main.ostrin is missing/);
  assert.match(mismatches, /stdout is missing/);
  assert.match(mismatches, /figure is missing/);
});

// ---- incomplete and failed runs --------------------------------------------------------------------------------------

test("a cancelled, timed-out or failed run is recorded as incomplete evidence with no results", () => {
  for (const status of ["cancelled", "timed-out", "failed"]) {
    const entry = run("r-" + status, doc([1, 3]), { status, exitCode: null, digests: { ...digests(), figure: null } });
    assert.equal(entry.evidence, "incomplete", status);
    assert.equal(entry.results, null, `${status} keeps no results`);
    assert.equal(entry.status, status);
  }
});

test("an incomplete run is never comparable with anything, and says why", () => {
  const ok = run("A", doc([1, 3]));
  const failed = run("B", doc([1, 3]), { status: "failed", digests: { ...digests(), figure: null } });
  const check = comparability(ok, failed);
  assert.equal(check.comparable, false);
  assert.match(check.reasons.join(";"), /run B is failed, not completed/);
  assert.throws(() => compareRuns(ok, failed), /not comparable/);
});

test("a completed run without a validated document is incomplete, not a scientific result", () => {
  const entry = run("A", null, { resultsRejected: "structured results are not valid JSON" });
  assert.equal(entry.evidence, "incomplete");
  assert.equal(entry.resultsRejected, "structured results are not valid JSON");
});

// ---- metadata --------------------------------------------------------------------------------------------------------

test("metadata is validated formally: family, method, tolerances and bracket", () => {
  assert.deepEqual(validateMetadata(doc([1])), []);
  assert.match(validateMetadata(doc([1], { family: "" })).join(";"), /family/);
  assert.match(validateMetadata(doc([1], { method: undefined })).join(";"), /method/);
  assert.match(validateMetadata(doc([1], { tolerances: 3 })).join(";"), /tolerances/);
  assert.match(validateMetadata(doc([1], { bracket: [1] })).join(";"), /bracket must be two finite numbers/);
  assert.match(validateMetadata(doc([1], { bracket: [3, 1] })).join(";"), /increasing/);
  assert.match(validateMetadata(doc([1], { bracket: [1, 1e999] })).join(";"), /two finite numbers/, "an infinite bracket end is refused");
  assert.throws(() => run("A", doc([1], { method: "" })), /results metadata is invalid/);
});

// ---- comparison ------------------------------------------------------------------------------------------------------

test("comparable runs: the same question, method, tolerances and compiler", () => {
  assert.deepEqual(comparability(run("A", doc([1, 3])), run("B", doc([1, 3]))), { comparable: true, reasons: [] });
});

test("incompatible runs are refused, with the reason for each rule", () => {
  const a = run("A", doc([1, 3]));
  assert.deepEqual(comparability(a, run("B", doc([1, 3], { experiment: "other-parameter-set" }))), { comparable: true, reasons: [] }, "a different parameter-set name is not a model difference");
  assert.match(comparability(a, run("B", doc([1, 3], { family: "x^3 - c" }))).reasons.join(";"), /different model family/);
  assert.match(comparability(a, run("B", doc([1, 3], { bracket: [0.5, 3] }))).reasons.join(";"), /different bracket/);
  assert.match(comparability(a, run("B", doc([1, 3], { method: "bisect" }))).reasons.join(";"), /different method/);
  assert.match(comparability(a, run("B", doc([1, 3], { tolerances: "other" }))).reasons.join(";"), /different tolerances/);
  assert.match(comparability(a, run("B", doc([1, 3]), { compilerIdentity: "ostrinc 0.3.0" })).reasons.join(";"), /different compiler identity/);
  assert.match(comparability(a, run("B", doc([1, 3]), { compilerIdentity: null })).reasons.join(";"), /compiler identity not recorded/);
});

test("matching by c: differences only for shared parameters, exclusive values listed, references kept", () => {
  const a = run("A", doc([1, 3, 5, 7]));
  const b = run("B", doc([3, 5, 7, 9]));
  const cmp = compareRuns(a, b);
  assert.deepEqual(cmp.rows.map((r) => r.c), [3, 5, 7]);
  assert.equal(cmp.rows[0].rootDifference, 0);
  assert.deepEqual(cmp.onlyInA, [1]);
  assert.deepEqual(cmp.onlyInB, [9]);
  assert.deepEqual(cmp.references, { a: "A", b: "B" });
});

test("the example from the brief: c = [1, 3, 5, 7] against c = [2, 4, 6, 8] shares no parameter and invents no difference", () => {
  const cmp = compareRuns(run("A", doc([1, 3, 5, 7])), run("B", doc([2, 4, 6, 8])));
  assert.equal(cmp.rows.length, 0, "no shared c, so no numeric difference");
  assert.deepEqual(cmp.onlyInA, [1, 3, 5, 7]);
  assert.deepEqual(cmp.onlyInB, [2, 4, 6, 8]);
});

test("shared parameters with different roots: the difference is b minus a, computed from the recorded values", () => {
  const rowsA = [{ c: 5, status: "converged", root: 2.0, residual: 1e-14, residual_bound: 1e-11, analytic_derivative: 1, numeric_derivative: 1, passed: true, message: "" }];
  const rowsB = [{ c: 5, status: "converged", root: 2.5, residual: 2e-14, residual_bound: 1e-11, analytic_derivative: 1, numeric_derivative: 1, passed: true, message: "" }];
  const cmp = compareRuns(run("A", doc([5], { rows: rowsA })), run("B", doc([5], { rows: rowsB })));
  assert.equal(cmp.rows.length, 1);
  assert.equal(cmp.rows[0].rootDifference, 0.5);
  assert.equal(cmp.rows[0].residualB, 2e-14);
  assert.deepEqual(cmp.references, { a: "A", b: "B" });
});
