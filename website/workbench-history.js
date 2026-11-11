// Scientific run history for the Workbench.
//
// Pure data code, with one asynchronous helper for verification. The page computes every SHA-256 with Web Crypto
// and passes the lowercase hex digests in; `verifyEntry` recomputes them from the recorded content and reports any
// mismatch. Entries are deep-copied and frozen when recorded, so nothing later can change a record.
//
// Recording rules:
//   - An id is recorded once.
//   - A figure is stored under its digest. A digest that already holds different content is refused; identical
//     content is stored once.
//   - Only a `completed` run that produced validated structured results carries `results`. Cancelled, timed-out and
//     failed runs are recorded as `incomplete`: they carry no results and are never presented as validated.

export const HISTORY_SCHEMA = "ostrin.history/v1";
// The same states as the execution controller (workbench-execution.js) and its run lifecycle.
export const STATUSES = Object.freeze(["completed", "failed", "cancelled", "timed-out"]);
const HEX64 = /^[0-9a-f]{64}$/;

export function createHistory() {
  return { schema: HISTORY_SCHEMA, entries: [], figures: {} };
}

// Deep copy and freeze, so a record shares no mutable object with its caller.
export function snapshot(value) {
  const copy = JSON.parse(JSON.stringify(value));
  return deepFreeze(copy);
}

function deepFreeze(value) {
  if (value && typeof value === "object") {
    for (const child of Object.values(value)) deepFreeze(child);
    Object.freeze(value);
  }
  return value;
}

export function isSha256Hex(text) {
  return typeof text === "string" && HEX64.test(text);
}

// Formal validation of the metadata that decides comparability. Returns a list of problems.
export function validateMetadata(results) {
  const problems = [];
  if (!results || typeof results !== "object") return ["results are missing"];
  if (typeof results.family !== "string" || results.family === "") problems.push("family must be a declared string");
  if (typeof results.method !== "string" || results.method === "") problems.push("method must be a declared string");
  if (typeof results.tolerances !== "string" || results.tolerances === "") problems.push("tolerances must be a declared string");
  const b = results.bracket;
  if (!Array.isArray(b) || b.length !== 2 || !b.every((v) => typeof v === "number" && Number.isFinite(v))) {
    problems.push("bracket must be two finite numbers");
  } else if (!(b[0] < b[1])) {
    problems.push("bracket must be increasing");
  }
  return problems;
}

// The record of one run. `digests` must hold lowercase SHA-256 hex digests computed by the page:
//   { sources: { name: hex }, stdout: hex, stderr: hex, figure: hex | null, manifest: hex | null }.
// `results` is a validated RESULTS document or null. `resultsRejected` is the reason a RESULTS line was refused.
export function runRecord({ id, projectId, projectName, revision = null, entry, command, status, exitCode = null, compilerIdentity = null, startedAt, durationMs, parameters = null, results = null, resultsRejected = null, digests }) {
  if (typeof id !== "string" || id === "") throw new Error("a run needs an id");
  if (!STATUSES.includes(status)) throw new Error(`unknown status ${JSON.stringify(status)}`);
  if (!digests || typeof digests.sources !== "object") throw new Error("a run needs the digests of its sources");
  for (const [name, digest] of Object.entries(digests.sources)) {
    if (!isSha256Hex(digest)) throw new Error(`source ${name} digest is not SHA-256 hex`);
  }
  for (const key of ["stdout", "stderr"]) {
    if (digests[key] !== undefined && digests[key] !== null && !isSha256Hex(digests[key])) throw new Error(`${key} digest is not SHA-256 hex`);
  }
  if (digests.figure !== undefined && digests.figure !== null && !isSha256Hex(digests.figure)) throw new Error("figure digest is not SHA-256 hex");
  const completed = status === "completed";
  const validated = completed && results !== null;
  if (validated) {
    const problems = validateMetadata(results);
    if (problems.length > 0) throw new Error(`results metadata is invalid: ${problems.join("; ")}`);
    if (!Array.isArray(results.rows) || results.rows.length === 0) throw new Error("results have no rows");
  }
  return snapshot({
    id,
    projectId,
    projectName,
    revision,
    entry,
    command,
    status,
    // `complete` only when the run finished and produced a validated document. Anything else is incomplete evidence.
    evidence: validated ? "validated" : "incomplete",
    exitCode,
    compilerIdentity,
    startedAt,
    durationMs,
    parameters,
    results: validated ? results : null,
    resultsRejected: validated ? null : resultsRejected,
    digests: {
      sources: { ...digests.sources },
      stdout: digests.stdout ?? null,
      stderr: digests.stderr ?? null,
      figure: digests.figure ?? null,
      manifest: digests.manifest ?? null,
    },
  });
}

// Adds a run. Refuses a duplicate id. A figure is stored under its digest; a digest already holding different
// content is refused. Returns a new history object.
export function recordRun(history, entry, figureSvg = null) {
  if (history.entries.some((e) => e.id === entry.id)) throw new Error(`run ${entry.id} is already recorded`);
  const figures = { ...history.figures };
  if (figureSvg !== null) {
    const digest = entry.digests.figure;
    if (!digest) throw new Error("a figure must be recorded with its SHA-256 digest");
    if (figures[digest] !== undefined && figures[digest] !== figureSvg) {
      throw new Error(`figure digest ${digest} already holds different content; refusing to overwrite it`);
    }
    figures[digest] = figureSvg;
  }
  return { ...history, entries: [...history.entries, entry], figures };
}

// Recomputes the digests of an entry from the content supplied and returns the list of mismatches (empty when
// everything matches). `content` = { sources: {name: text}, stdout, stderr, figure }. Digests use Web Crypto.
export async function verifyEntry(entry, content, digestOf) {
  const mismatches = [];
  for (const [name, digest] of Object.entries(entry.digests.sources)) {
    if (!(name in content.sources)) mismatches.push(`source ${name} is missing`);
    else if ((await digestOf(content.sources[name])) !== digest) mismatches.push(`source ${name} does not match its digest`);
  }
  for (const key of ["stdout", "stderr"]) {
    if (entry.digests[key] === null) continue;
    if (content[key] === undefined) mismatches.push(`${key} is missing`);
    else if ((await digestOf(content[key])) !== entry.digests[key]) mismatches.push(`${key} does not match its digest`);
  }
  if (entry.digests.figure !== null) {
    if (content.figure === undefined || content.figure === null) mismatches.push("figure is missing");
    else if ((await digestOf(content.figure)) !== entry.digests.figure) mismatches.push("figure does not match its digest");
  }
  return mismatches;
}

export function findRun(history, id) {
  return history.entries.find((e) => e.id === id) ?? null;
}

export function figureOf(history, entry) {
  return entry.digests.figure ? history.figures[entry.digests.figure] ?? null : null;
}

// Two runs are comparable only when every rule holds. Otherwise the reasons are listed and no number is compared.
export function comparability(a, b) {
  const reasons = [];
  for (const run of [a, b]) {
    if (run.status !== "completed") reasons.push(`run ${run.id} is ${run.status}, not completed`);
    else if (run.evidence !== "validated") reasons.push(`run ${run.id} has no validated structured results`);
  }
  if (a.evidence === "validated" && b.evidence === "validated") {
    if (a.results.experiment !== b.results.experiment) reasons.push("different experiments");
    if (a.results.family !== b.results.family) reasons.push("different model family");
    if (JSON.stringify(a.results.bracket) !== JSON.stringify(b.results.bracket)) reasons.push("different bracket");
    if (a.results.method !== b.results.method) reasons.push("different method");
    if (a.results.tolerances !== b.results.tolerances) reasons.push("different tolerances");
  }
  if (!a.compilerIdentity || !b.compilerIdentity) reasons.push("compiler identity not recorded for a run");
  else if (a.compilerIdentity !== b.compilerIdentity) reasons.push("different compiler identity");
  return { comparable: reasons.length === 0, reasons };
}

// Rows for the scenarios present in both runs, matched by the parameter c. Differences are b minus a. Parameters that
// are not shared produce no difference: they are listed as exclusive to one run. Refuses incomparable runs.
export function compareRuns(a, b) {
  const check = comparability(a, b);
  if (!check.comparable) throw new Error(`runs are not comparable: ${check.reasons.join("; ")}`);
  const byC = new Map(b.results.rows.map((r) => [r.c, r]));
  const aCs = new Set(a.results.rows.map((r) => r.c));
  const rows = [];
  for (const left of a.results.rows) {
    const right = byC.get(left.c);
    if (!right) continue;
    rows.push({
      c: left.c,
      rootA: left.root,
      rootB: right.root,
      rootDifference: right.root - left.root,
      residualA: left.residual,
      residualB: right.residual,
      passedA: left.passed,
      passedB: right.passed,
    });
  }
  return {
    rows,
    onlyInA: a.results.rows.map((r) => r.c).filter((c) => !byC.has(c)),
    onlyInB: b.results.rows.map((r) => r.c).filter((c) => !aCs.has(c)),
    references: { a: a.id, b: b.id },
  };
}
