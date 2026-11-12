import test from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import {
  addCell,
  attachResource,
  cellHashes,
  createNotebook,
  csvRows,
  dependencyOrder,
  freshness,
  LIMITS,
  moveCell,
  NotebookError,
  outputsFromLines,
  parseNotebook,
  planRun,
  removeCell,
  serializeNotebook,
  setDeps,
  setSource,
  validateNotebook,
} from "../website/workbench-notebook.js";

const sha = (text) => createHash("sha256").update(text, "utf8").digest("hex");
const digestOf = async (text) => sha(text);
const now = "2026-11-11T10:00:00.000Z";

// t1: a text cell. c1: definitions only. c2: the program, which imports c1 and depends on it.
function sample() {
  let doc = createNotebook({ title: "sample", now });
  doc = addCell(doc, "text", { source: "Notes." });
  doc = addCell(doc, "code", { source: "fn helper() -> Int {\n    2\n}\n" });
  doc = addCell(doc, "code", { source: "import c1\nfn main() -> Void {\n    print(c1.helper().to_string())\n}\n" });
  return setDeps(doc, "c2", ["c1"]);
}

const validRun = (hash) => ({
  hash,
  state: "completed",
  startedAt: now,
  durationMs: 12,
  exitCode: 0,
  stdout: "2\n",
  stderr: "",
  tables: [],
  figures: [],
  results: null,
  resultsRejected: null,
  error: null,
});

test("a new notebook gets stable ids and the operations keep it valid", () => {
  const doc = sample();
  assert.deepEqual(validateNotebook(doc), []);
  assert.deepEqual(doc.cells.map((c) => c.id), ["t1", "c1", "c2"]);
  const moved = moveCell(doc, "c2", -1);
  assert.deepEqual(moved.cells.map((c) => c.id), ["t1", "c2", "c1"]);
  assert.equal(moveCell(doc, "t1", -1), doc, "moving the first cell up changes nothing");
  const removed = removeCell(doc, "c1");
  assert.deepEqual(removed.cells.map((c) => c.id), ["t1", "c2"]);
  assert.deepEqual(removed.cells.find((c) => c.id === "c2").deps, [], "a removed dependency is also removed from deps");
  const grown = addCell(doc, "code", { afterId: "c1", source: "fn other() -> Int { 3 }" });
  assert.deepEqual(grown.cells.map((c) => c.id), ["t1", "c1", "c3", "c2"]);
});

test("serialization round-trips exactly and refuses an invalid document", () => {
  const doc = sample();
  const text = serializeNotebook(doc);
  assert.deepEqual(parseNotebook(text), JSON.parse(text));
  assert.throws(() => parseNotebook("{ not json"), (e) => e instanceof NotebookError && /not valid JSON/.test(e.problems[0]));
  assert.throws(() => parseNotebook(JSON.stringify({ ...doc, version: 2 })), (e) => /unsupported version/.test(e.problems[0]));
  assert.throws(() => parseNotebook(JSON.stringify({ ...doc, format: "other" })), (e) => /format must be/.test(e.problems.join(";")));
});

test("validation names each defect: ids, dependencies, cycles and unknown fields", () => {
  const doc = sample();
  const duplicated = { ...doc, cells: [...doc.cells, { ...doc.cells[1] }] };
  assert.ok(validateNotebook(duplicated).some((p) => /appears twice/.test(p)));
  assert.ok(validateNotebook(setDeps(doc, "c1", ["nope"])).some((p) => /not a code cell/.test(p)));
  assert.ok(validateNotebook(setDeps(doc, "c1", ["c1"])).some((p) => /itself/.test(p)));
  assert.ok(validateNotebook(setDeps(setDeps(doc, "c1", ["c2"]), "c2", ["c1"])).some((p) => /cycle/.test(p)));
  const textDeps = { ...doc, cells: doc.cells.map((c) => (c.kind === "text" ? { ...c, deps: ["c1"] } : c)) };
  assert.ok(validateNotebook(textDeps).some((p) => /cannot have dependencies/.test(p)));
  assert.ok(validateNotebook({ ...doc, extra: 1 }).some((p) => /unknown field extra/.test(p)));
  assert.ok(validateNotebook({ ...doc, cells: [{ id: "bad id", kind: "code", source: "", deps: [] }] }).some((p) => /invalid id/.test(p)));
});

test("resources accept only data files under data/, and their size is bounded", () => {
  const doc = sample();
  assert.deepEqual(validateNotebook(attachResource(doc, "data/calibration.csv", "a,b\n1,2\n")), []);
  assert.ok(validateNotebook(attachResource(doc, "/abs/path.csv", "x")).some((p) => /resource path/.test(p)));
  assert.ok(validateNotebook(attachResource(doc, "data/../x.csv", "x")).some((p) => /resource path/.test(p)));
  assert.ok(validateNotebook(attachResource(doc, "data/x.exe", "x")).some((p) => /resource path/.test(p)));
  const big = "x".repeat(LIMITS.maxResourceBytes + 1);
  assert.ok(validateNotebook(attachResource(doc, "data/big.csv", big)).some((p) => /more than/.test(p)));
});

test("runs are checked: hash, state, outputs, figures, results and unknown fields", () => {
  const doc = sample();
  const hash = "a".repeat(64);
  const withRun = (run) => ({ ...doc, runs: { c2: run } });
  assert.deepEqual(validateNotebook(withRun(validRun(hash))), []);
  assert.ok(validateNotebook(withRun({ ...validRun("xyz") })).some((p) => /hash/.test(p)));
  assert.ok(validateNotebook(withRun({ ...validRun(hash), state: "done" })).some((p) => /unknown state/.test(p)));
  assert.ok(validateNotebook(withRun({ ...validRun(hash), figures: ["<script>alert(1)</script>"] })).some((p) => /malformed figure/.test(p)));
  assert.ok(validateNotebook(withRun({ ...validRun(hash), extra: true })).some((p) => /unknown field/.test(p)));
  assert.ok(validateNotebook(withRun({ ...validRun(hash), results: { schema: "ostrin.results/v1", experiment: "e", rows: [] } })).some((p) => /results are invalid/.test(p)));
  assert.ok(validateNotebook({ ...doc, runs: { t1: validRun(hash) } }).some((p) => /not a code cell/.test(p)));
});

test("a run plan carries the cell, its dependencies and the data, and refuses a main in a dependency", () => {
  const doc = attachResource(sample(), "data/x.csv", "a\n1\n");
  const plan = planRun(doc, "c2");
  assert.equal(plan.ok, true, plan.problems.join(";"));
  assert.equal(plan.request.entry, "c2.ostrin");
  assert.deepEqual(plan.request.order, ["c1", "c2"]);
  assert.deepEqual(Object.keys(plan.request.files).sort(), ["c1.ostrin", "c2.ostrin", "data/x.csv"]);
  assert.deepEqual(plan.request.command, ["--run", "--json", "c2.ostrin"]);
  const refused = planRun(setSource(doc, "c1", "fn helper() -> Int { 2 }\nfn main() -> Void { print(\"x\") }\n"), "c2");
  assert.equal(refused.ok, false);
  assert.ok(refused.problems.some((p) => /cannot be a dependency/.test(p)));
  assert.equal(planRun(doc, "t1").ok, false, "a text cell has nothing to run");
  assert.ok(planRun(doc, "c1").problems.some((p) => /no fn main/.test(p)), "a definitions-only cell has nothing to run");
});

test("dependency order is topological and each cell appears once", () => {
  let doc = sample();
  doc = addCell(doc, "code", { source: "fn x() -> Int { 1 }" });
  doc = setDeps(doc, "c3", ["c2", "c1"]);
  assert.deepEqual(dependencyOrder(doc, "c3"), ["c1", "c2", "c3"]);
});

test("a change to a cell or to its data makes its outputs and its dependents' outputs stale", async () => {
  let doc = sample();
  const first = await cellHashes(doc, digestOf);
  doc = { ...doc, runs: { c1: validRun(first.c1), c2: validRun(first.c2) } };
  assert.equal(freshness(doc, first, "c1"), "current");
  assert.equal(freshness(doc, first, "c2"), "current");
  const edited = setSource(doc, "c1", "fn helper() -> Int {\n    3\n}\n");
  const second = await cellHashes(edited, digestOf);
  assert.equal(freshness(edited, second, "c1"), "stale", "the edited cell is stale");
  assert.equal(freshness(edited, second, "c2"), "stale", "its dependent is stale");
  const withData = attachResource(doc, "data/x.csv", "a\n1\n");
  const third = await cellHashes(withData, digestOf);
  assert.equal(freshness(withData, third, "c1"), "stale", "changed data makes every run stale");
  assert.deepEqual(await cellHashes(doc, digestOf), first, "hashes are deterministic");
  assert.equal(freshness(doc, first, "c2"), "current");
});

test("outputs are split into text, tables, a figure and a structured result", () => {
  const svg = "<svg xmlns=\"http://www.w3.org/2000/svg\"><circle r=\"1\"/></svg>";
  const lines = [
    ["out", "hello"],
    ["out", "OSTRIN-TABLE fit"],
    ["out", "x,y"],
    ["out", "1,\"a, b\""],
    ["out", "OSTRIN-TABLE-END"],
    ["out", svg],
    ["err", "warning"],
    ["out", "done"],
  ];
  const out = outputsFromLines(lines);
  assert.equal(out.tables.length, 1);
  assert.equal(out.tables[0].title, "fit");
  assert.equal(out.figures.length, 1);
  assert.equal(out.stdout, "hello\ndone\n");
  assert.equal(out.stderr, "warning\n");
  assert.deepEqual(csvRows(out.tables[0].csv), [["x", "y"], ["1", "a, b"]]);
});

test("a valid RESULTS line becomes a result and a rejected one keeps its reason", () => {
  const good = { schema: "ostrin.results/v1", experiment: "e", family: "f", bracket: [1, 3], method: "m", tolerances: "t",
    rows: [{ c: 1, status: "converged", root: 1.5, residual: 0, residual_bound: 1, analytic_derivative: 1, numeric_derivative: 1, passed: true, message: "" }] };
  const ok = outputsFromLines([["out", "RESULTS " + JSON.stringify(good)]]);
  assert.equal(ok.results.experiment, "e");
  assert.equal(ok.stdout, "");
  const bad = outputsFromLines([["out", "RESULTS {\"schema\":\"ostrin.results/v1\",\"experiment\":\"e\",\"rows\":[{\"c\":\"x\"}]}"]]);
  assert.equal(bad.results, null);
  assert.ok(bad.resultsRejected);
});

test("csvRows reads quotes, escaped quotes and line breaks inside quoted fields", () => {
  assert.deepEqual(csvRows("a,\"b\"\"c\",\"d\ne\"\n"), [["a", "b\"c", "d\ne"]]);
});

test("the limits are the ones the document promises", () => {
  assert.equal(LIMITS.maxCells, 200);
  assert.equal(LIMITS.maxFigureBytes, 2 * 1024 * 1024);
});

test("a compiler diagnostic printed as JSON is shown as a readable error, not as raw text", () => {
  const json = JSON.stringify({ severity: "error", code: null, message: "Error OSTRIN-E1080\n'x' is private", file: "c2.ostrin", line: 3, column: null });
  const out = outputsFromLines([["out", json], ["out", "plain output"]]);
  assert.equal(out.stdout, "plain output\n");
  assert.match(out.stderr, /^error: Error OSTRIN-E1080/m);
  assert.match(out.stderr, /\(line 3\)/);
});
