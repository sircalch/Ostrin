// Structured results contract for the Workbench.
//
// A program reports structured results by printing exactly one stdout line that starts with "RESULTS " and
// continues with a JSON document of schema "ostrin.results/v1". The Workbench never infers a table from
// free text: it reads only this line, validates it, and shows it. Any other output stays a log.
//
// Document shape (v1):
//   { schema: "ostrin.results/v1", experiment: string, family?: string, bracket?: [number, number],
//     rows: [ { c: number, status: "converged" | "failed", root: number, residual: number,
//               residual_bound: number, analytic_derivative: number, numeric_derivative: number,
//               passed: boolean, message: string } ] }

export const RESULTS_SCHEMA = "ostrin.results/v1";
export const RESULTS_PREFIX = "RESULTS ";
const ROW_KEYS = ["c", "status", "root", "residual", "residual_bound", "analytic_derivative", "numeric_derivative", "passed", "message"];
const STATUSES = ["converged", "failed"];

// Returns { ok: true, document } or { ok: false, reason }. `lines` is the [stream, text] list of a run.
export function parseResults(lines) {
  const hits = lines.filter(([stream, text]) => stream === "out" && text.startsWith(RESULTS_PREFIX));
  if (hits.length === 0) return { ok: false, reason: "no structured results in this run" };
  if (hits.length > 1) return { ok: false, reason: "more than one structured results line" };
  let document;
  try {
    document = JSON.parse(hits[0][1].slice(RESULTS_PREFIX.length));
  } catch (error) {
    return { ok: false, reason: `structured results are not valid JSON: ${error.message}` };
  }
  const problems = validateResults(document);
  if (problems.length > 0) return { ok: false, reason: problems.join("; ") };
  return { ok: true, document, payload: hits[0][1].slice(RESULTS_PREFIX.length) };
}

export function validateResults(document) {
  const problems = [];
  if (!document || typeof document !== "object" || Array.isArray(document)) return ["results must be an object"];
  if (document.schema !== RESULTS_SCHEMA) problems.push(`schema must be ${RESULTS_SCHEMA}`);
  if (typeof document.experiment !== "string" || document.experiment === "") problems.push("experiment must be a name");
  if (!Array.isArray(document.rows) || document.rows.length === 0) {
    problems.push("rows must be a non-empty list");
    return problems;
  }
  const seen = new Set();
  document.rows.forEach((row, index) => {
    if (!row || typeof row !== "object") {
      problems.push(`row ${index} is not an object`);
      return;
    }
    for (const key of ROW_KEYS) {
      if (!(key in row)) problems.push(`row ${index} is missing ${key}`);
    }
    if (!STATUSES.includes(row.status)) problems.push(`row ${index} has an unknown status`);
    if (typeof row.message !== "string") problems.push(`row ${index} message must be a string`);
    for (const key of ["c", "root", "residual", "residual_bound", "analytic_derivative", "numeric_derivative"]) {
      // JSON can carry out-of-range exponents such as 1e999, which parse to Infinity: only finite numbers pass.
      if (key in row && !(typeof row[key] === "number" && Number.isFinite(row[key]))) problems.push(`row ${index} ${key} must be a finite number`);
    }
    if (typeof row.passed !== "boolean") problems.push(`row ${index} passed must be a boolean`);
    // A parameter value appears once: two rows for the same c would make a comparison silently keep one of them.
    if (typeof row.c === "number" && Number.isFinite(row.c)) {
      if (seen.has(row.c)) problems.push(`row ${index} repeats c = ${row.c}`);
      seen.add(row.c);
    }
  });
  return problems;
}

// Table model for display: headers and cell texts. Cell texts are the numbers as the program wrote them.
export function resultsTable(document) {
  const headers = ["c", "status", "root", "residual", "residual bound", "f'(root) analytic", "f'(root) numeric", "passed"];
  const rows = document.rows.map((row) => [
    String(row.c),
    row.status,
    String(row.root),
    String(row.residual),
    String(row.residual_bound),
    String(row.analytic_derivative),
    String(row.numeric_derivative),
    row.passed ? "yes" : "no",
  ]);
  return { headers, rows };
}

// The exported text of a validated run: the exact payload text of its RESULTS line. It is not re-serialized, so
// the bytes are the ones the program printed on that line. A re-serialized copy would not be the same bytes.
export function resultsExportText(parsed) {
  if (!parsed || !parsed.ok) throw new Error("only a validated RESULTS payload can be exported");
  return parsed.payload;
}
