import test from "node:test";
import assert from "node:assert/strict";
import { parseResults, resultsExportText, resultsTable, validateResults } from "../website/workbench-results.js";

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
const payloadLine = (text) => ["out", "RESULTS " + text];

test("a single RESULTS line is parsed, validated and keeps its exact payload text", () => {
  const text = JSON.stringify(doc());
  const parsed = parseResults([["out", "log text"], payloadLine(text), ["out", "all checks passed"]]);
  assert.equal(parsed.ok, true);
  assert.equal(parsed.document.rows.length, 2);
  assert.equal(parsed.payload, text, "the payload is the exact text after the prefix");
});

test("a run without structured results is reported as such, not guessed from text", () => {
  const parsed = parseResults([["out", "c = 1: root 1.6"], ["out", "all checks passed"]]);
  assert.equal(parsed.ok, false);
  assert.match(parsed.reason, /no structured results/);
});

test("more than one RESULTS line is refused", () => {
  assert.match(parseResults([payloadLine(JSON.stringify(doc())), payloadLine(JSON.stringify(doc()))]).reason, /more than one/);
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
  assert.match(validateResults(doc([row(1, { root: "1.5" })])).join(";"), /root must be a finite number/);
  const missing = row(1);
  delete missing.residual_bound;
  assert.match(validateResults(doc([missing])).join(";"), /missing residual_bound/);
});

test("non-finite numbers are rejected: an out-of-range exponent such as 1e999 parses to Infinity and must not pass", () => {
  // The text is valid JSON; the number it denotes is not finite.
  const overflow = '{"schema":"ostrin.results/v1","experiment":"sweep","rows":[{"c":1,"status":"converged","root":1e999,"residual":0,"residual_bound":1e-11,"analytic_derivative":2.5,"numeric_derivative":2.5,"passed":true,"message":""}]}';
  const parsed = parseResults([payloadLine(overflow)]);
  assert.equal(parsed.ok, false, "a document with an infinite root is not a valid result");
  assert.match(parsed.reason, /root must be a finite number/);
  assert.equal("payload" in parsed, false, "a rejected document carries no export payload");
});

test("negative overflow and null in a numeric field are rejected, not converted to zero or null", () => {
  const negative = '{"schema":"ostrin.results/v1","experiment":"sweep","rows":[{"c":-1e999,"status":"converged","root":1,"residual":0,"residual_bound":1,"analytic_derivative":1,"numeric_derivative":1,"passed":true,"message":""}]}';
  assert.match(parseResults([payloadLine(negative)]).reason, /c must be a finite number/);
  assert.match(validateResults(doc([row(1, { residual: null })])).join(";"), /residual must be a finite number/);
});

test("the table shows the program's own values, in row order", () => {
  const table = resultsTable(doc([row(1, { residual: -1e-15 }), row(3)]));
  assert.deepEqual(table.headers.slice(0, 4), ["c", "status", "root", "residual"]);
  assert.equal(table.rows[0][3], "-1e-15", "a negative residual is shown as written, not made absolute");
  assert.equal(table.rows[1][0], "3");
  assert.equal(table.rows[0][7], "yes");
});

test("the export is the exact RESULTS payload, not a re-serialized copy", () => {
  // A payload written with a number that JSON.stringify would print differently ("2.50" vs "2.5").
  const text = '{"schema":"ostrin.results/v1","experiment":"sweep","rows":[{"c":5,"status":"converged","root":2.0945514815423265,"residual":0,"residual_bound":1e-11,"analytic_derivative":2.50,"numeric_derivative":2.5,"passed":true,"message":""}]}';
  const parsed = parseResults([payloadLine(text)]);
  assert.equal(parsed.ok, true);
  assert.equal(resultsExportText(parsed), text, "the exported bytes are the bytes printed on the line");
  assert.notEqual(resultsExportText(parsed), JSON.stringify(parsed.document), "a re-serialized copy would differ here");
});

test("only a validated result can be exported", () => {
  assert.throws(() => resultsExportText({ ok: false, reason: "x" }), /only a validated RESULTS payload/);
  assert.throws(() => resultsExportText(undefined), /only a validated RESULTS payload/);
});
