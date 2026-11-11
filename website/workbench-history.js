// Scientific run history for the Workbench: the model of recorded runs, the comparison rules, and the figure store.
//
// Pure data code (no DOM, no storage, no hashing). The page computes every SHA-256 with Web Crypto and passes the
// digests in; this module never invents one. A run is recorded only from its current presentation ticket, so a
// stale run cannot create an entry. Entries are immutable: an id can be recorded once. A figure is stored once per
// SHA-256 and referenced by digest, so repeated runs do not duplicate large SVG text.

export const HISTORY_SCHEMA = "ostrin.history/v1";
export const STATUSES = Object.freeze(["completed", "failed", "cancelled", "timeout"]);

export function createHistory() {
  return { schema: HISTORY_SCHEMA, entries: [], figures: {} };
}

// The record of one run. `digests` holds the SHA-256 values the page computed: { sources: {name: hex}, stdout: hex,
// stderr: hex, figure: hex | null, manifest: hex | null }. `results` is a validated document or null, and
// `resultsRejected` the reason a RESULTS line was refused (null when there was none or it was valid).
export function runRecord({ id, projectId, projectName, entry, command, status, exitCode = null, compilerIdentity = null, startedAt, durationMs, parameters = null, results = null, resultsRejected = null, digests }) {
  if (typeof id !== "string" || id === "") throw new Error("a run needs an id");
  if (!STATUSES.includes(status)) throw new Error(`unknown status ${JSON.stringify(status)}`);
  if (!digests || typeof digests.sources !== "object" || typeof digests.stdout !== "string") {
    throw new Error("a run needs the SHA-256 digests of its sources and stdout");
  }
  if (status === "completed" && results !== null && !results.rows) throw new Error("results must be a validated document");
  return {
    id,
    projectId,
    projectName,
    entry,
    command,
    status,
    exitCode,
    compilerIdentity,
    startedAt,
    durationMs,
    parameters,
    results,
    resultsRejected,
    digests: {
      sources: { ...digests.sources },
      stdout: digests.stdout,
      stderr: digests.stderr ?? null,
      figure: digests.figure ?? null,
      manifest: digests.manifest ?? null,
    },
  };
}

// Adds a run. Refuses a duplicate id. A figure is stored once per digest. Returns a new history object.
export function recordRun(history, entry, figureSvg = null) {
  if (history.entries.some((e) => e.id === entry.id)) throw new Error(`run ${entry.id} is already recorded`);
  const figures = { ...history.figures };
  if (figureSvg !== null) {
    const digest = entry.digests.figure;
    if (!digest) throw new Error("a figure must be recorded with its SHA-256 digest");
    figures[digest] = figureSvg;
  }
  return { ...history, entries: [...history.entries, entry], figures };
}

export function findRun(history, id) {
  return history.entries.find((e) => e.id === id) ?? null;
}

export function figureOf(history, entry) {
  return entry.digests.figure ? history.figures[entry.digests.figure] ?? null : null;
}

// Two runs are comparable only when every rule holds. The reasons are listed; no numbers are compared otherwise.
// Comparable means the same question, the same method and tolerances, and the same compiler, not merely similar text.
export function comparability(a, b) {
  const reasons = [];
  if (!a.results) reasons.push(`run ${a.id} has no validated structured results`);
  if (!b.results) reasons.push(`run ${b.id} has no validated structured results`);
  if (a.results && b.results) {
    if (a.results.experiment !== b.results.experiment) reasons.push("different experiments");
    if (a.results.family !== b.results.family) reasons.push("different model family");
    if (JSON.stringify(a.results.bracket ?? null) !== JSON.stringify(b.results.bracket ?? null)) reasons.push("different bracket");
    // A missing method or tolerance is never assumed equal: both must be declared and must match.
    if (!a.results.method || !b.results.method) reasons.push("method not declared by a run");
    else if (a.results.method !== b.results.method) reasons.push("different method");
    if (!a.results.tolerances || !b.results.tolerances) reasons.push("tolerances not declared by a run");
    else if (a.results.tolerances !== b.results.tolerances) reasons.push("different tolerances");
  }
  if (!a.compilerIdentity || !b.compilerIdentity) reasons.push("compiler identity not recorded for a run");
  else if (a.compilerIdentity !== b.compilerIdentity) reasons.push("different compiler identity");
  return { comparable: reasons.length === 0, reasons };
}

// Rows for the scenarios present in both runs, matched by the parameter c. Differences are b minus a.
// It refuses to run when the runs are not comparable.
export function compareRuns(a, b) {
  const check = comparability(a, b);
  if (!check.comparable) throw new Error(`runs are not comparable: ${check.reasons.join("; ")}`);
  const byC = new Map(b.results.rows.map((r) => [r.c, r]));
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
  const aCs = new Set(a.results.rows.map((r) => r.c));
  return {
    rows,
    onlyInA: a.results.rows.map((r) => r.c).filter((c) => !byC.has(c)),
    onlyInB: b.results.rows.map((r) => r.c).filter((c) => !aCs.has(c)),
  };
}
