// Tests of the classification of native-sanitize rows and of the exit status derived from it.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, existsSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { classifyRow, missingExpectedRejections, summarise, CATEGORIES, FAILING_CATEGORIES } from "./native-sanitize-classify.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const expected = JSON.parse(readFileSync(join(here, "native-sanitize-expected-rejections.json"), "utf8"));
const MESSAGE = "cannot determine the type of the function value 'f'";

const typeRow = (route, detail = `compiler refused: error: ${MESSAGE}: the native backend cannot infer it`) => ({
  program: "untyped_closure_parameter",
  route,
  verdict: "REJECTED_TYPE",
  detail,
});
const verified = (program, route) => ({ program, route, verdict: "VERIFIED", detail: "", liveAllocations: 0 });

test("a verified row is VERIFIED", () => {
  assert.equal(classifyRow(verified("p", "ir -O0")), "VERIFIED");
});

test("a type rejection matching the expected list is EXPECTED_TYPE_REJECTION", () => {
  for (const route of ["ir -O0", "hir -O0", "ast -O0", "ir -O1", "hir -O1", "ast -O1"]) {
    assert.equal(classifyRow(typeRow(route), expected), "EXPECTED_TYPE_REJECTION", route);
  }
});

test("a type rejection with a different message is UNEXPECTED_TYPE_REJECTION", () => {
  const row = typeRow("ir -O0", "compiler refused: error: cannot infer the type of 'g'");
  assert.equal(classifyRow(row, expected), "UNEXPECTED_TYPE_REJECTION");
});

test("a type rejection on a route or level not in the list is UNEXPECTED_TYPE_REJECTION", () => {
  const narrowed = [{ ...expected[0], routes: ["ir"] }];
  assert.equal(classifyRow(typeRow("hir -O0"), narrowed), "UNEXPECTED_TYPE_REJECTION");
  const onlyO0 = [{ ...expected[0], opts: ["-O0"] }];
  assert.equal(classifyRow(typeRow("ir -O1"), onlyO0), "UNEXPECTED_TYPE_REJECTION");
});

test("a type rejection with no list at all is UNEXPECTED_TYPE_REJECTION", () => {
  assert.equal(classifyRow(typeRow("ir -O0"), []), "UNEXPECTED_TYPE_REJECTION");
});

test("other compiler refusals are COMPILER_REFUSAL and never expected", () => {
  const row = { program: "p", route: "ir -O0", verdict: "REJECTED_COMPILER", detail: "parse error" };
  assert.equal(classifyRow(row, expected), "COMPILER_REFUSAL");
});

test("failed rows are categorised by their reason", () => {
  assert.equal(classifyRow({ program: "p", route: "ir -O0", verdict: "FAILED", reason: "memory", detail: "x" }), "MEMORY_FAILURE");
  assert.equal(classifyRow({ program: "p", route: "ir -O0", verdict: "FAILED", reason: "build", detail: "x" }), "BUILD_FAILURE");
  assert.equal(classifyRow({ program: "p", route: "ir -O0", verdict: "FAILED", reason: "execution", detail: "x" }), "EXECUTION_FAILURE");
  // A FAILED row with no reason is never a milder category.
  assert.equal(classifyRow({ program: "p", route: "ir -O0", verdict: "FAILED", detail: "x" }), "EXECUTION_FAILURE");
});

test("blocked rows are BLOCKED", () => {
  assert.equal(classifyRow({ program: "p", route: "ir -O0", verdict: "BLOCKED", detail: "x" }), "BLOCKED");
});

test("an unknown verdict is an error, not a silent pass", () => {
  assert.throws(() => classifyRow({ program: "p", route: "ir -O0", verdict: "MAYBE" }), /unknown verdict/);
});

test("every expected rejection of a program in the run must be observed", () => {
  const rows = [typeRow("ir -O0"), typeRow("hir -O0"), typeRow("ast -O0")];
  const missing = missingExpectedRejections(rows, expected);
  // -O1 rows and nothing else are missing for the three routes.
  assert.equal(missing.length, 3);
  assert.ok(missing.every((m) => m.opt === "-O1"));
});

test("an expected program that is not in the run is not required", () => {
  assert.deepEqual(missingExpectedRejections([verified("other", "ir -O0")], expected), []);
});

test("an expected program that now compiles is a missing expected rejection", () => {
  const rows = [verified("untyped_closure_parameter", "ir -O0")];
  assert.ok(missingExpectedRejections(rows, expected).length > 0);
});

test("exit status is 0 only when every row is verified or expected, and every expectation is met", () => {
  const all = [...["ir", "hir", "ast"].flatMap((r) => ["-O0", "-O1"].map((o) => typeRow(`${r} ${o}`))), verified("p", "ir -O0")];
  const result = summarise(all, expected);
  assert.equal(result.exitCode, 0);
  assert.equal(result.counts.EXPECTED_TYPE_REJECTION, 6);
  assert.equal(result.counts.VERIFIED, 1);
  assert.equal(result.failing.length, 0);
});

test("an unexpected type rejection makes the exit status non-zero", () => {
  const rows = [verified("p", "ir -O0"), { program: "q", route: "ir -O0", verdict: "REJECTED_TYPE", detail: "cannot infer x" }];
  const result = summarise(rows, expected);
  assert.equal(result.exitCode, 1);
  assert.equal(result.counts.UNEXPECTED_TYPE_REJECTION, 1);
});

test("a memory or execution failure makes the exit status non-zero", () => {
  const memory = summarise([{ program: "p", route: "ir -O0", verdict: "FAILED", reason: "memory", detail: "LeakSanitizer" }], expected);
  const execution = summarise([{ program: "p", route: "ir -O0", verdict: "FAILED", reason: "execution", detail: "exit 1" }], expected);
  assert.equal(memory.exitCode, 1);
  assert.equal(memory.counts.MEMORY_FAILURE, 1);
  assert.equal(execution.exitCode, 1);
  assert.equal(execution.counts.EXECUTION_FAILURE, 1);
});

test("counts add up to the number of rows, with one category per row", () => {
  const rows = [verified("p", "ir -O0"), typeRow("ir -O0"), { program: "q", route: "ir -O0", verdict: "BLOCKED", detail: "" }];
  const result = summarise(rows, expected);
  const sum = CATEGORIES.reduce((acc, c) => acc + (c === "MISSING_EXPECTED_REJECTION" ? 0 : result.counts[c]), 0);
  assert.equal(sum, rows.length);
});

test("failing categories are the ones that make the exit status non-zero", () => {
  assert.ok(FAILING_CATEGORIES.includes("UNEXPECTED_TYPE_REJECTION"));
  assert.ok(FAILING_CATEGORIES.includes("MEMORY_FAILURE"));
  assert.ok(!FAILING_CATEGORIES.includes("EXPECTED_TYPE_REJECTION"));
  assert.ok(!FAILING_CATEGORIES.includes("VERIFIED"));
});

test("the expected message is the one the compiler prints for the untyped fixture", () => {
  const compiler = join(here, "..", "compiler", "target", "release", "ostrinc.exe");
  const fixture = join(here, "..", "compiler", "tests", "fixtures", "ownership", "untyped_closure_parameter.ostrin");
  if (!existsSync(compiler) || !existsSync(fixture)) return;
  const r = spawnSync(compiler, ["--emit-c", "--out", join(process.env.TEMP ?? "/tmp", "untyped-check.c"), fixture], { encoding: "utf8" });
  assert.notEqual(r.status, 0);
  assert.ok((r.stderr + r.stdout).includes(MESSAGE), "the compiler message no longer contains the expected text");
});
