// Ostrin Scientific Notebook: the document model, its validation, and the execution contract.
//
// Format `ostrin.notebook`, version 1. A notebook is one JSON document:
//   { format, version, id, title, created, updated, cells, resources, runs }
//   cells      ordered list of { id, kind: "code" | "text", source, deps }
//   resources  attached data files { path: "data/<name>.csv|tsv|txt|json", text }
//   runs       the last recorded run of each code cell, with its outputs and the hash of the code it ran
//
// EXECUTION CONTRACT (there is no shared kernel):
//   - Running a code cell compiles and runs ONE Ostrin program: the cell's source, plus the cells it lists in `deps`
//     (transitively), plus the attached data files. Each run starts from nothing. No variable, value or state
//     produced by one cell is visible to another cell at run time.
//   - To use functions from another cell, the cell lists it in `deps` and imports it by its id (`import c1`).
//     A dependency is a definitions-only cell: it must not define `fn main`. The notebook refuses a run that
//     would import such a cell.
//   - Run All runs the code cells in document order, one after another, each as above.
//   - A run's outputs are tied to the hash of the code that produced them: the cell's source, its deps' hashes and
//     the attached data. When any of these change, the outputs are STALE and are labelled as such. Stale outputs are
//     kept, but they are never shown as current results.
//   - Files a program writes (write_file) go to the run's own virtual file system and are discarded after the run.
//     A notebook that needs a file as output presents its content in stdout or as a figure instead.
//   - Restarting the context clears every recorded output. It does not change the code.
//
// Everything in this module is pure: the digest function is injected, and nothing here runs a program.

import { validateResults, parseResults, RESULTS_SCHEMA } from "./workbench-results.js";

export const NOTEBOOK_FORMAT = "ostrin.notebook";
export const NOTEBOOK_VERSION = 1;
export const RUN_STATES = Object.freeze(["completed", "failed", "cancelled", "timed-out"]);

export const LIMITS = Object.freeze({
  maxDocumentBytes: 4 * 1024 * 1024,
  maxCells: 200,
  maxTitleLength: 200,
  maxSourceBytes: 64 * 1024,
  maxDepsPerCell: 16,
  maxResources: 8,
  maxResourceBytes: 512 * 1024,
  maxOutputBytes: 256 * 1024,
  maxTablesPerRun: 8,
  maxFiguresPerRun: 4,
  maxFigureBytes: 2 * 1024 * 1024,
});

const CELL_ID = /^[A-Za-z][A-Za-z0-9_]{0,31}$/;
const DATA_PATH = /^data\/[A-Za-z0-9_][A-Za-z0-9_-]*\.(csv|tsv|txt|json)$/;
const HASH = /^[0-9a-f]{64}$/;
const ISO = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d{1,3})?Z$/;
const encoder = new TextEncoder();
const MAIN_DEFINITION = /\bfn\s+main\s*\(/;

export class NotebookError extends Error {
  constructor(problems) {
    super(problems.join("; "));
    this.name = "NotebookError";
    this.problems = problems;
  }
}

const isPlainObject = (value) => value !== null && typeof value === "object" && !Array.isArray(value);
const bytes = (text) => encoder.encode(text).length;

function newId(prefix, existing) {
  let n = 1;
  while (existing.has(`${prefix}${n}`)) n += 1;
  return `${prefix}${n}`;
}

export function createNotebook({ title = "Untitled notebook", now = new Date().toISOString(), id = "notebook" } = {}) {
  return {
    format: NOTEBOOK_FORMAT,
    version: NOTEBOOK_VERSION,
    id,
    title: String(title).slice(0, LIMITS.maxTitleLength),
    created: now,
    updated: now,
    cells: [],
    resources: [],
    runs: {},
  };
}

// Returns a new document with a cell added after `afterId` (or at the end). The cell id is new and stable.
export function addCell(doc, kind, { afterId = null, source = "" } = {}) {
  if (kind !== "code" && kind !== "text") throw new NotebookError([`unknown cell kind ${kind}`]);
  if (doc.cells.length >= LIMITS.maxCells) throw new NotebookError([`a notebook holds at most ${LIMITS.maxCells} cells`]);
  const ids = new Set(doc.cells.map((c) => c.id));
  const id = newId(kind === "code" ? "c" : "t", ids);
  const cell = { id, kind, source, deps: [] };
  const at = afterId === null ? doc.cells.length : doc.cells.findIndex((c) => c.id === afterId) + 1;
  const cells = [...doc.cells];
  cells.splice(at, 0, cell);
  return { ...doc, cells };
}

export function removeCell(doc, cellId) {
  const cells = doc.cells.filter((c) => c.id !== cellId).map((c) => ({ ...c, deps: c.deps.filter((d) => d !== cellId) }));
  const runs = { ...doc.runs };
  delete runs[cellId];
  return { ...doc, cells, runs };
}

// Moves a cell one position up (-1) or down (+1). Returns the same document when the move is impossible.
export function moveCell(doc, cellId, direction) {
  const index = doc.cells.findIndex((c) => c.id === cellId);
  const target = index + direction;
  if (index < 0 || target < 0 || target >= doc.cells.length) return doc;
  const cells = [...doc.cells];
  [cells[index], cells[target]] = [cells[target], cells[index]];
  return { ...doc, cells };
}

// Replaces the source of a cell. The recorded outputs are kept (they become stale through the hash).
export function setSource(doc, cellId, source) {
  return { ...doc, cells: doc.cells.map((c) => (c.id === cellId ? { ...c, source } : c)) };
}

export function setDeps(doc, cellId, deps) {
  return { ...doc, cells: doc.cells.map((c) => (c.id === cellId ? { ...c, deps: [...deps] } : c)) };
}

export function attachResource(doc, path, text) {
  const resources = doc.resources.filter((r) => r.path !== path);
  return { ...doc, resources: [...resources, { path, text }] };
}

export function clearOutputs(doc) {
  return { ...doc, runs: {} };
}

// ---------------------------------------------------------------- validation of imported documents

// Validates a parsed document strictly. Returns the list of problems (empty when valid). It never repairs.
export function validateNotebook(doc) {
  const problems = [];
  if (!isPlainObject(doc)) return ["the document is not an object"];
  const allowed = ["format", "version", "id", "title", "created", "updated", "cells", "resources", "runs"];
  for (const key of Object.keys(doc)) if (!allowed.includes(key)) problems.push(`unknown field ${key}`);
  if (doc.format !== NOTEBOOK_FORMAT) problems.push(`format must be ${NOTEBOOK_FORMAT}`);
  if (doc.version !== NOTEBOOK_VERSION) problems.push(`unsupported version ${JSON.stringify(doc.version)}; this reader understands version ${NOTEBOOK_VERSION}`);
  if (typeof doc.id !== "string" || !CELL_ID.test(doc.id)) problems.push("the notebook id is not a valid identifier");
  if (typeof doc.title !== "string" || doc.title.length > LIMITS.maxTitleLength) problems.push("the title must be a string of at most 200 characters");
  for (const field of ["created", "updated"]) {
    if (typeof doc[field] !== "string" || !ISO.test(doc[field])) problems.push(`${field} must be an ISO 8601 UTC time`);
  }
  if (!Array.isArray(doc.cells) || doc.cells.length > LIMITS.maxCells) {
    problems.push(`cells must be a list of at most ${LIMITS.maxCells} cells`);
    return problems;
  }
  const codeIds = new Set();
  const seen = new Set();
  for (const [index, cell] of doc.cells.entries()) {
    const where = `cell ${index}`;
    if (!isPlainObject(cell)) {
      problems.push(`${where} is not an object`);
      continue;
    }
    for (const key of Object.keys(cell)) if (!["id", "kind", "source", "deps"].includes(key)) problems.push(`${where} has unknown field ${key}`);
    if (typeof cell.id !== "string" || !CELL_ID.test(cell.id)) problems.push(`${where} has an invalid id`);
    else if (seen.has(cell.id)) problems.push(`cell id ${cell.id} appears twice`);
    else seen.add(cell.id);
    if (cell.kind !== "code" && cell.kind !== "text") problems.push(`${where} has an unknown kind`);
    if (typeof cell.source !== "string" || bytes(cell.source) > LIMITS.maxSourceBytes) problems.push(`${where} source must be text of at most ${LIMITS.maxSourceBytes} bytes`);
    if (!Array.isArray(cell.deps) || cell.deps.length > LIMITS.maxDepsPerCell || cell.deps.some((d) => typeof d !== "string")) {
      problems.push(`${where} deps must be a list of cell ids`);
    }
    if (cell.kind === "code" && typeof cell.id === "string") codeIds.add(cell.id);
  }
  // Dependencies must name code cells that exist, and must not form a cycle.
  for (const cell of doc.cells) {
    if (!isPlainObject(cell) || !Array.isArray(cell.deps)) continue;
    if (cell.kind === "text" && cell.deps.length > 0) problems.push(`text cell ${cell.id} cannot have dependencies`);
    for (const dep of cell.deps) {
      if (dep === cell.id) problems.push(`cell ${cell.id} depends on itself`);
      else if (!codeIds.has(dep)) problems.push(`cell ${cell.id} depends on ${dep}, which is not a code cell of this notebook`);
    }
  }
  if (cycleOf(doc.cells)) problems.push("the dependencies between cells form a cycle");
  if (!Array.isArray(doc.resources) || doc.resources.length > LIMITS.maxResources) {
    problems.push(`resources must be a list of at most ${LIMITS.maxResources} files`);
  } else {
    const paths = new Set();
    let total = 0;
    for (const resource of doc.resources) {
      if (!isPlainObject(resource) || typeof resource.path !== "string" || !DATA_PATH.test(resource.path)) {
        problems.push("a resource path must be data/<name>.csv, .tsv, .txt or .json");
        continue;
      }
      if (paths.has(resource.path)) problems.push(`resource ${resource.path} appears twice`);
      paths.add(resource.path);
      if (typeof resource.text !== "string") problems.push(`resource ${resource.path} must hold text`);
      else total += bytes(resource.text);
    }
    if (total > LIMITS.maxResourceBytes) problems.push(`resources total more than ${LIMITS.maxResourceBytes} bytes`);
  }
  if (!isPlainObject(doc.runs)) {
    problems.push("runs must be an object keyed by cell id");
  } else {
    for (const [cellId, run] of Object.entries(doc.runs)) {
      problems.push(...validateRun(cellId, run, codeIds));
    }
  }
  return problems;
}

function cycleOf(cells) {
  const deps = new Map(cells.filter((c) => isPlainObject(c) && typeof c.id === "string" && Array.isArray(c.deps)).map((c) => [c.id, c.deps]));
  const state = new Map();
  const visit = (id) => {
    if (state.get(id) === "done") return false;
    if (state.get(id) === "open") return true;
    state.set(id, "open");
    for (const dep of deps.get(id) ?? []) if (visit(dep)) return true;
    state.set(id, "done");
    return false;
  };
  return [...deps.keys()].some(visit);
}

function validateRun(cellId, run, codeIds) {
  const problems = [];
  const where = `run of ${cellId}`;
  if (!codeIds.has(cellId)) return [`${where} refers to a cell that is not a code cell`];
  if (!isPlainObject(run)) return [`${where} is not an object`];
  const allowed = ["hash", "state", "startedAt", "durationMs", "exitCode", "stdout", "stderr", "tables", "figures", "results", "resultsRejected", "error"];
  for (const key of Object.keys(run)) if (!allowed.includes(key)) problems.push(`${where} has unknown field ${key}`);
  if (typeof run.hash !== "string" || !HASH.test(run.hash)) problems.push(`${where} hash must be SHA-256 hex`);
  if (!RUN_STATES.includes(run.state)) problems.push(`${where} has an unknown state`);
  if (typeof run.startedAt !== "string" || !ISO.test(run.startedAt)) problems.push(`${where} startedAt must be an ISO time`);
  if (typeof run.durationMs !== "number" || !Number.isFinite(run.durationMs) || run.durationMs < 0) problems.push(`${where} durationMs must be a finite number`);
  if (run.exitCode !== null && !Number.isInteger(run.exitCode)) problems.push(`${where} exitCode must be an integer or null`);
  for (const field of ["stdout", "stderr"]) {
    if (typeof run[field] !== "string" || bytes(run[field]) > LIMITS.maxOutputBytes) problems.push(`${where} ${field} must be text within the output limit`);
  }
  if (!Array.isArray(run.tables) || run.tables.length > LIMITS.maxTablesPerRun) problems.push(`${where} tables must be a list within the limit`);
  else {
    for (const table of run.tables) {
      if (!isPlainObject(table) || typeof table.title !== "string" || typeof table.csv !== "string" || bytes(table.csv) > LIMITS.maxOutputBytes) {
        problems.push(`${where} has a malformed table`);
      }
    }
  }
  if (!Array.isArray(run.figures) || run.figures.length > LIMITS.maxFiguresPerRun) problems.push(`${where} figures must be a list within the limit`);
  else {
    for (const svg of run.figures) {
      if (typeof svg !== "string" || bytes(svg) > LIMITS.maxFigureBytes || !svg.trim().startsWith("<svg") || !svg.trim().endsWith("</svg>")) {
        problems.push(`${where} has a malformed figure`);
      }
    }
  }
  if (run.results !== null && run.results !== undefined) {
    const shape = isPlainObject(run.results) && run.results.schema === RESULTS_SCHEMA ? validateResults(run.results) : ["not a structured result"];
    if (shape.length > 0) problems.push(`${where} results are invalid: ${shape.join("; ")}`);
  }
  if (run.resultsRejected !== undefined && run.resultsRejected !== null && typeof run.resultsRejected !== "string") problems.push(`${where} resultsRejected must be text`);
  if (run.error !== undefined && run.error !== null && typeof run.error !== "string") problems.push(`${where} error must be text`);
  return problems;
}

// Parses and validates a document. Throws NotebookError with every problem; never returns a partial document.
export function parseNotebook(text) {
  if (typeof text !== "string") throw new NotebookError(["the document is not text"]);
  if (bytes(text) > LIMITS.maxDocumentBytes) throw new NotebookError([`the document is larger than ${LIMITS.maxDocumentBytes} bytes`]);
  let doc;
  try {
    doc = JSON.parse(text);
  } catch (error) {
    throw new NotebookError([`the file is not valid JSON: ${error.message}`]);
  }
  const problems = validateNotebook(doc);
  if (problems.length > 0) throw new NotebookError(problems);
  return doc;
}

export function serializeNotebook(doc) {
  const problems = validateNotebook(doc);
  if (problems.length > 0) throw new NotebookError(problems);
  return JSON.stringify(doc, null, 2) + "\n";
}

// ---------------------------------------------------------------- hashes and freshness

// The hash of every code cell: its kind, source, the hashes of its deps, and the attached data. Equal hashes mean
// that a run recorded with that hash ran exactly this code and these data.
export async function cellHashes(doc, digestOf) {
  const resourcesDigest = await digestOf(JSON.stringify([...doc.resources].sort((a, b) => (a.path < b.path ? -1 : 1))));
  const byId = new Map(doc.cells.map((c) => [c.id, c]));
  const memo = new Map();
  const hashOf = async (id) => {
    if (memo.has(id)) return memo.get(id);
    const cell = byId.get(id);
    const depHashes = [];
    for (const dep of cell.deps) depHashes.push(await hashOf(dep));
    const hash = await digestOf(JSON.stringify({ kind: cell.kind, source: cell.source, deps: depHashes, resources: resourcesDigest }));
    memo.set(id, hash);
    return hash;
  };
  const out = {};
  for (const cell of doc.cells) if (cell.kind === "code") out[cell.id] = await hashOf(cell.id);
  return out;
}

// "current" when the recorded run used the current hash, "stale" when it used an older one, "not-run" otherwise.
export function freshness(doc, hashes, cellId) {
  const run = doc.runs[cellId];
  if (!run) return "not-run";
  return run.hash === hashes[cellId] ? "current" : "stale";
}

// ---------------------------------------------------------------- planning a run

// Transitive dependencies of a cell, in a stable order (a dependency always comes before the cell that uses it).
export function dependencyOrder(doc, cellId) {
  const byId = new Map(doc.cells.map((c) => [c.id, c]));
  const order = [];
  const seen = new Set();
  const visit = (id) => {
    if (seen.has(id)) return;
    seen.add(id);
    for (const dep of byId.get(id).deps) visit(dep);
    order.push(id);
  };
  visit(cellId);
  return order;
}

// The request for one run of a code cell: the files, the entry, and the command. Returns { ok, problems, request }.
export function planRun(doc, cellId) {
  const cell = doc.cells.find((c) => c.id === cellId);
  if (!cell) return { ok: false, problems: [`no cell ${cellId}`] };
  if (cell.kind !== "code") return { ok: false, problems: [`cell ${cellId} is text, not code`] };
  const order = dependencyOrder(doc, cellId);
  const problems = [];
  const files = {};
  for (const id of order) {
    const member = doc.cells.find((c) => c.id === id);
    if (member.kind !== "code") problems.push(`dependency ${id} is not a code cell`);
    if (id !== cellId && MAIN_DEFINITION.test(member.source)) {
      problems.push(`${id} defines fn main, so it cannot be a dependency: a dependency only defines functions`);
    }
    files[`${id}.ostrin`] = member.source;
  }
  if (!MAIN_DEFINITION.test(cell.source)) problems.push(`${cellId} has no fn main, so there is nothing to run`);
  for (const resource of doc.resources) files[resource.path] = resource.text;
  if (problems.length > 0) return { ok: false, problems, request: null };
  const entry = `${cellId}.ostrin`;
  return {
    ok: true,
    problems: [],
    request: { entry, files, command: ["--run", "--json", entry], order },
  };
}

// ---------------------------------------------------------------- outputs from a run

// The outputs of one run from its ordered lines: stdout, stderr, the tables, the figure, and the structured results.
// Tables are printed by the program between `OSTRIN-TABLE <title>` and `OSTRIN-TABLE-END` lines, as CSV.
// A compiler diagnostic printed as JSON (the --json form) is shown as a readable error line. Anything else is text.
function readableDiagnostic(line) {
  if (!line.startsWith("{")) return null;
  try {
    const d = JSON.parse(line);
    if (typeof d.severity !== "string" || typeof d.message !== "string") return null;
    const where = typeof d.line === "number" ? ` (line ${d.line})` : "";
    return `${d.severity}: ${d.message}${where}`;
  } catch (_) {
    return null;
  }
}

export function outputsFromLines(lines) {
  const plainOut = lines.filter(([kind]) => kind === "out").map(([, text]) => text);
  const err = lines.filter(([kind]) => kind === "err").map(([, text]) => text);
  const out = [];
  for (const line of plainOut) {
    const diagnostic = readableDiagnostic(line);
    if (diagnostic !== null) err.push(diagnostic);
    else out.push(line);
  }
  const tables = [];
  const plain = [];
  let open = null;
  for (const line of out) {
    if (open === null && line.startsWith("OSTRIN-TABLE ")) {
      open = { title: line.slice("OSTRIN-TABLE ".length), rows: [] };
    } else if (open !== null && line === "OSTRIN-TABLE-END") {
      tables.push({ title: open.title, csv: open.rows.join("\n") + "\n" });
      open = null;
    } else if (open !== null) {
      open.rows.push(line);
    } else {
      plain.push(line);
    }
  }
  if (open !== null) plain.push("OSTRIN-TABLE " + open.title + " (not closed)");
  const results = parseResults(lines.filter(([kind, text]) => !(kind === "out" && text.startsWith("OSTRIN-TABLE"))));
  let text = plain.join("\n");
  const figure = extractSvg(text);
  if (figure) {
    // The figure's own line is removed with the line break that follows it, so no blank line is left behind.
    const at = text.indexOf(figure);
    text = text.slice(0, at) + text.slice(at + figure.length).replace(/^\n/, "");
  }
  // A valid RESULTS line is shown as a table, not as text; a rejected one stays visible, with its reason.
  if (results.ok) text = text.split("\n").filter((line) => !line.startsWith("RESULTS ")).join("\n");
  const stdout = text.replace(/\n{3,}/g, "\n\n").trim();
  return {
    stdout: stdout === "" ? "" : stdout + "\n",
    stderr: err.join("\n") + (err.length ? "\n" : ""),
    tables,
    figures: figure ? [figure] : [],
    results: results.ok ? results.document : null,
    resultsRejected: results.ok || results.reason === "no structured results in this run" ? null : results.reason,
  };
}

// The single SVG document in the text, or null. Several SVG documents are not one figure and are not shown.
function extractSvg(text) {
  if (text.split("<svg").length !== 2 || text.split("</svg>").length !== 2) return null;
  const start = text.indexOf("<svg");
  const end = text.indexOf("</svg>") + "</svg>".length;
  const svg = text.slice(start, end);
  return bytes(svg) > LIMITS.maxFigureBytes ? null : svg;
}

// Splits a CSV table into rows for display. This is presentation only: no value is computed here.
export function csvRows(csv) {
  const rows = [];
  let row = [];
  let field = "";
  let quoted = false;
  for (let i = 0; i < csv.length; i += 1) {
    const c = csv[i];
    if (quoted) {
      if (c === '"' && csv[i + 1] === '"') { field += '"'; i += 1; }
      else if (c === '"') quoted = false;
      else field += c;
    } else if (c === '"') quoted = true;
    else if (c === ",") { row.push(field); field = ""; }
    else if (c === "\n") {
      // A completely blank line is not a record: it can only come from a stray line break around a table.
      if (field !== "" || row.length > 0) { row.push(field); rows.push(row); }
      row = [];
      field = "";
    }
    else field += c;
  }
  if (field !== "" || row.length > 0) { row.push(field); rows.push(row); }
  return rows;
}
