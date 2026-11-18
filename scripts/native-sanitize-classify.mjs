// Classification of the rows produced by scripts/native-sanitize.mjs, and the exit status derived from it.
//
// Every row is one program on one codegen route at one optimisation level. Its category is:
//   VERIFIED                     the binary ran, no sanitizer report, live_allocations=0, output equal to the interpreter.
//   EXPECTED_TYPE_REJECTION      the compiler refused the program for a type-inference limit, and the program, route and
//                                message match an entry of the expected-rejection list. The program was never compiled to C,
//                                so this is not a memory result.
//   UNEXPECTED_TYPE_REJECTION    a type-inference refusal that the expected-rejection list does not cover.
//   COMPILER_REFUSAL             any other refusal by the compiler before C is emitted.
//   BUILD_FAILURE                the generated C does not compile under the sanitizer flags.
//   EXECUTION_FAILURE            the binary exits non-zero, or its stdout differs from the interpreter.
//   MEMORY_FAILURE               a sanitizer report, or a leak report with live_allocations different from 0.
//   BLOCKED                      the environment prevented the check (for example, the output was not captured).
//
// Exit status of native-sanitize.mjs: 0 only when every row is VERIFIED or EXPECTED_TYPE_REJECTION, and every expected
// rejection of a program that was run was observed. Any other category, or a missing expected rejection, makes it non-zero.
// The expected-rejection list is scripts/native-sanitize-expected-rejections.json.

export const CATEGORIES = Object.freeze([
  "VERIFIED",
  "EXPECTED_TYPE_REJECTION",
  "UNEXPECTED_TYPE_REJECTION",
  "COMPILER_REFUSAL",
  "BUILD_FAILURE",
  "EXECUTION_FAILURE",
  "MEMORY_FAILURE",
  "BLOCKED",
  "MISSING_EXPECTED_REJECTION",
]);

export const FAILING_CATEGORIES = Object.freeze([
  "UNEXPECTED_TYPE_REJECTION",
  "COMPILER_REFUSAL",
  "BUILD_FAILURE",
  "EXECUTION_FAILURE",
  "MEMORY_FAILURE",
  "BLOCKED",
  "MISSING_EXPECTED_REJECTION",
]);

// The route name of a row is "ir -O0", "hir -O1", ...; the expected list names routes without the optimisation level.
const routeName = (route) => route.split(" ")[0];
const routeOpt = (route) => route.split(" ")[1];

// row: { program, route, verdict, detail, reason? }. reason is set by the runner for FAILED rows: "memory", "execution" or "build".
export function classifyRow(row, expected = []) {
  if (row.verdict === "VERIFIED") return "VERIFIED";
  if (row.verdict === "REJECTED_TYPE") {
    const matched = expected.some(
      (entry) =>
        entry.program === row.program &&
        entry.verdict === "REJECTED_TYPE" &&
        (entry.routes ?? []).includes(routeName(row.route)) &&
        (entry.opts ?? ["-O0", "-O1"]).includes(routeOpt(row.route)) &&
        (row.detail ?? "").includes(entry.message),
    );
    return matched ? "EXPECTED_TYPE_REJECTION" : "UNEXPECTED_TYPE_REJECTION";
  }
  if (row.verdict === "REJECTED_COMPILER") return "COMPILER_REFUSAL";
  if (row.verdict === "BLOCKED") return "BLOCKED";
  if (row.verdict === "FAILED") {
    if (row.reason === "memory") return "MEMORY_FAILURE";
    if (row.reason === "build") return "BUILD_FAILURE";
    if (row.reason === "execution") return "EXECUTION_FAILURE";
    // A FAILED row without a reason is not classified as anything milder than a failure.
    return "EXECUTION_FAILURE";
  }
  throw new Error(`unknown verdict ${row.verdict} for ${row.program} ${row.route}`);
}

// Every expected rejection must be observed for every route and optimisation level of a program that the run included.
export function missingExpectedRejections(rows, expected = []) {
  const programs = new Set(rows.map((r) => r.program));
  const missing = [];
  for (const entry of expected) {
    if (!programs.has(entry.program)) continue;
    for (const route of entry.routes ?? []) {
      for (const opt of entry.opts ?? ["-O0", "-O1"]) {
        const row = rows.find((r) => r.program === entry.program && r.route === `${route} ${opt}`);
        const ok = row && classifyRow(row, expected) === "EXPECTED_TYPE_REJECTION";
        if (!ok) missing.push({ program: entry.program, route, opt, message: entry.message });
      }
    }
  }
  return missing;
}

// Summary of a run: category counts, the rows that make the run fail, and the exit status.
export function summarise(rows, expected = []) {
  const categorised = rows.map((row) => ({ ...row, category: classifyRow(row, expected) }));
  const counts = Object.fromEntries(CATEGORIES.map((c) => [c, 0]));
  for (const row of categorised) counts[row.category] += 1;
  const missing = missingExpectedRejections(rows, expected);
  counts.MISSING_EXPECTED_REJECTION = missing.length;
  const failing = categorised.filter((r) => FAILING_CATEGORIES.includes(r.category));
  const exitCode = failing.length || missing.length ? 1 : 0;
  return { total: rows.length, counts, failing: failing.map((r) => ({ program: r.program, route: r.route, category: r.category, detail: r.detail })), missing, exitCode, rows: categorised };
}
