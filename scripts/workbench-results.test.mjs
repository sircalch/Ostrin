import test from "node:test";
import assert from "node:assert/strict";
import { parseResults, resultsJsonText, resultsTable, validateResults } from "../website/workbench-results.js";

const row = (c, extra = {}) => ({
  c,
  status: "converged",
  root: 1.5,
  residual: 0,
  residual_bound: 1e-11,
  analytic_derivative: 2.5,
  numeric_derivative: 2.5000000001,
  passed: true,
  message: "",
  ...extra,
});
const doc = (rows = [row(1), row(3)]) => ({ schema: "ostrin.results/v1", experiment: "sweep", rows });
const line = (document) => ["out", "RESULTS " + JSON.stringify(document)];

test("a single RESULTS line is parsed and validated", () => {
  const parsed = parseResults([["out", "log text"], line(doc()), ["out", "all checks passed"]]);
  assert.equal(parsed.ok, true);
  assert.equal(parsed.document.rows.length, 2);
});

test("a run without structured results is reported as such, not guessed from text", () => {
  const parsed = parseResults([["out", "c = 1: root 1.6"], ["out", "all checks passed"]]);
  assert.equal(parsed.ok, false);
  assert.match(parsed.reason, /no structured results/);
});

test("more than one RESULTS line is refused", () => {
  assert.match(parseResults([line(doc()), line(doc())]).reason, /more than one/);
});

test("stderr never provides results", () => {
  assert.equal(parseResults([["err", "RESULTS " + JSON.stringify(doc())]]).ok, false);
});

test("invalid JSON after the prefix is refused", () => {
  assert.match(parseResults([["out", "RESULTS {not json"]]).reason, /not valid JSON/);
});

test("the schema, rows and field types are validated", () => {
  assert.match(validateResults({ ...doc(), schema: "other" }).join(";"), /schema must be/);
  assert.match(validateResults(doc([])).join(";"), /non-empty/);
  assert.match(validateResults(doc([row(1, { status: "maybe" })])).join(";"), /unknown status/);
  assert.match(validateResults(doc([row(1, { passed: "yes" })])).join(";"), /passed must be a boolean/);
  assert.match(validateResults(doc([row(1, { root: "1.5" })])).join(";"), /root must be a number/);
  const missing = row(1);
  delete missing.residual_bound;
  assert.match(validateResults(doc([missing])).join(";"), /missing residual_bound/);
});

test("the table shows the program's own values, in row order", () => {
  const table = resultsTable(doc([row(1, { residual: -1e-15 }), row(3)]));
  assert.deepEqual(table.headers.slice(0, 4), ["c", "status", "root", "residual"]);
  assert.equal(table.rows[0][3], "-1e-15", "a negative residual is shown as written, not made absolute");
  assert.equal(table.rows[1][0], "3");
  assert.equal(table.rows[0][7], "yes");
});

test("the exported text keeps every number unrounded", () => {
  const exact = doc([row(5, { root: 2.0945514815423265 })]);
  const text = resultsJsonText(exact);
  assert.equal(JSON.parse(text).rows[0].root, 2.0945514815423265);
  assert.match(text, /2\.0945514815423265/);
});
