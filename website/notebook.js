// Ostrin Scientific Notebook: the page. It owns the document in memory, runs code cells through the Workbench's
// executor (one real compiler run per cell, in a Worker), and keeps the document in localStorage. Nothing here
// computes a result: every number and figure comes from the program that ran in the compiler.
//
// Rules enforced here (see workbench-notebook.js for the model):
//   - Opening, importing or restoring a document never runs a program.
//   - A run records the hash of the code it ran. Outputs whose hash no longer matches are shown as stale.
//   - Imported or restored documents are validated completely before they replace the open one.
//   - A stored draft that fails validation is not discarded: it is kept under a separate key and reported.
//   - All document text is inserted with textContent. Figures are images with data URLs, never markup.

import { createExecutor } from "./workbench-execution.js";
import { orderedLines } from "./workbench-protocol.js";
import { sha256Hex } from "./workbench-project.js";
import { resultsTable } from "./workbench-results.js";
import { parseBlocks } from "./workbench-markdown.js";
import {
  addCell,
  attachResource,
  cellHashes,
  clearOutputs,
  createNotebook,
  csvRows,
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
} from "./workbench-notebook.js";

const DRAFT_KEY = "ostrin.notebook.v1.draft";
const REJECTED_KEY = "ostrin.notebook.v1.rejected";
const EXAMPLE_FILES = {
  experimental: "notebooks/experimental.ostrin.notebook.json",
  physical: "notebooks/physical.ostrin.notebook.json",
  symbolic: "notebooks/symbolic.ostrin.notebook.json",
  numeric: "notebooks/numeric.ostrin.notebook.json",
  conditional: "notebooks/conditional.ostrin.notebook.json",
};
const RUN_TIMEOUT_MS = 60_000;
const $ = (id) => document.getElementById(id);
const encoder = new TextEncoder();

let doc = createNotebook({ title: "Untitled notebook" });
let hashes = {};
let activeCell = null;
let runAllActive = false;
// The cells that ran in this page session. Any other recorded output came from the document and is labelled so.
const ranThisSession = new Set();
// The display mode of each text cell for this page session: "read" (rendered) or "edit" (the original text).
const textModes = new Map();
function textMode(cell) {
  return textModes.get(cell.id) ?? (cell.source.trim() === "" ? "edit" : "read");
}
let cancelRequested = false;

const executor = createExecutor({
  createWorker: () => new Worker(new URL("./workbench-worker.js", import.meta.url), { type: "module" }),
  onChange: () => renderStatus(),
});

// ---------------------------------------------------------------- notices and status

function notice(text, kind = "info") {
  const box = $("nb-notice");
  box.textContent = text;
  box.dataset.kind = kind;
  box.hidden = text === "";
}

function renderStatus() {
  const cancel = $("nb-cancel");
  cancel.disabled = !(runAllActive || activeCell !== null);
  $("nb-status").textContent = activeCell ? `Running cell ${activeCell}…` : "";
  renderSaveState();
}

// ---------------------------------------------------------------- persistence
//
// The save state is derived from two counters. `revision` counts every change to the document. `savedRevision` is the
// revision of the last write that completed. A change made while a write is in progress is not lost: it keeps the
// document "unsaved" and starts the next write. The states are:
//   saved    the stored text is the current document (savedRevision equals revision, and no error)
//   saving   a write is in progress
//   unsaved  the document has changes that are not stored yet; an autosave is pending or about to run
//   error    the last write failed: the change is NOT stored, and the document stays open in this page
// "saved" is shown only after the storage call returned; a pending write is never reported as an error.

const save = { revision: 0, savedRevision: 0, writing: false, error: null, timer: null, chain: Promise.resolve() };
const SAVE_LABEL = { saved: "Saved in this browser", saving: "Saving…", unsaved: "Unsaved changes", error: "Not saved" };

function isUnsaved() {
  return save.revision !== save.savedRevision;
}

function saveState() {
  if (save.error !== null) return "error";
  if (save.writing) return "saving";
  if (isUnsaved()) return "unsaved";
  return "saved";
}

function renderSaveState() {
  const state = saveState();
  const node = $("nb-save-state");
  node.dataset.state = state;
  node.textContent = SAVE_LABEL[state];
  // Save is useful whenever something is not stored, including after an error.
  $("nb-save").disabled = state === "saved" || state === "saving";
}

// Records a change. The autosave runs shortly after the last change, so typing does not write on every key.
function markChanged() {
  save.revision += 1;
  renderSaveState();
  clearTimeout(save.timer);
  save.timer = setTimeout(saveNow, 400);
}

// Writes the document now, after any write already in progress. Returns when the write has finished.
function saveNow() {
  clearTimeout(save.timer);
  save.chain = save.chain.then(writeDocument);
  return save.chain;
}

async function writeDocument() {
  if (!isUnsaved() && save.error === null) return;
  save.writing = true;
  renderSaveState();
  // Storage writes are asynchronous; yielding here keeps the "saving" state observable and lets edits queue behind it.
  await new Promise((resolve) => setTimeout(resolve, 0));
  try {
    // The document is read after the yield: a change made meanwhile is stored by this write, and a write never stores
    // text older than the revision it records. Writing a captured earlier text would overwrite the newer change.
    const revision = save.revision;
    const text = serializeNotebook(doc);
    localStorage.setItem(DRAFT_KEY, text);
    save.savedRevision = Math.max(save.savedRevision, revision);
    if (save.error !== null) {
      save.error = null;
      notice("");
    }
  } catch (error) {
    failSave(error);
  } finally {
    save.writing = false;
    renderSaveState();
  }
  // A change made during the write is saved by the next write; it is not reported as an error.
  if (isUnsaved() && save.error === null) {
    clearTimeout(save.timer);
    save.timer = setTimeout(saveNow, 400);
  }
}

function failSave(error) {
  save.error = String(error?.message ?? error);
  notice(`Not saved: ${save.error}. The changes are still in this page, but they are not stored in this browser. Export the notebook to keep a copy.`, "error");
  renderSaveState();
}

function restoreDraft() {
  let raw = null;
  try {
    raw = localStorage.getItem(DRAFT_KEY);
  } catch (_) {
    raw = null;
  }
  if (raw === null) return;
  try {
    doc = parseNotebook(raw);
  } catch (error) {
    // Keep the rejected text so nothing is lost silently; the new document starts empty beside it.
    try {
      localStorage.setItem(REJECTED_KEY, raw);
    } catch (_) {
      // Nothing more can be done without storage; the message below still says what happened.
    }
    const reasons = error instanceof NotebookError ? error.problems.slice(0, 3).join("; ") : String(error.message ?? error);
    notice(`The saved draft was not opened (${reasons}). A copy is kept under ${REJECTED_KEY}.`, "error");
  }
}

// ---------------------------------------------------------------- document changes

async function refreshHashes() {
  hashes = await cellHashes(doc, sha256Hex);
}

// Applies a change to the document, recomputes the hashes, and redraws the cells.
async function commit(next, { redraw = true } = {}) {
  doc = next;
  await refreshHashes();
  if (redraw) renderCells();
  else renderChips();
  markChanged();
}

// ---------------------------------------------------------------- rendering

function el(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function button(label, onClick, { disabled = false, title = "" } = {}) {
  const node = el("button", "button-quiet", label);
  node.type = "button";
  node.disabled = disabled;
  if (title) node.title = title;
  node.addEventListener("click", onClick);
  return node;
}

const STATE_TEXT = {
  current: "current",
  stale: "stale: the code or its data changed since this ran",
  "not-run": "not run",
  running: "running…",
};

function chipFor(cellId) {
  if (activeCell === cellId) return { label: STATE_TEXT.running, kind: "running" };
  const state = freshness(doc, hashes, cellId);
  return { label: STATE_TEXT[state], kind: state };
}

function renderChips() {
  for (const cell of doc.cells) {
    const node = document.querySelector(`[data-chip="${cell.id}"]`);
    if (!node) continue;
    const chip = chipFor(cell.id);
    node.textContent = chip.label;
    node.className = `nb-chip nb-chip-${chip.kind}`;
  }
  renderStatus();
}

function renderCells() {
  const list = $("nb-cells");
  list.replaceChildren();
  if (doc.cells.length === 0) {
    list.append(el("p", "notebook-empty", "This notebook is empty. Add a code cell with + Code, or load an example."));
  }
  doc.cells.forEach((cell, index) => list.append(renderCell(cell, index)));
  $("notebook-title").value = doc.title;
  renderResources();
  renderChips();
  renderStatus();
}

function renderCell(cell, index) {
  const article = el("article", `nb-cell nb-cell-${cell.kind}`);
  article.dataset.cellId = cell.id;
  article.setAttribute("aria-label", `${cell.kind === "code" ? "Code" : "Text"} cell ${cell.id}`);

  const head = el("header", "nb-cell-head");
  head.append(el("span", "nb-cell-id", cell.id), el("span", "nb-cell-kind", cell.kind === "code" ? "Ostrin code" : "Text"));
  if (cell.kind === "code") {
    const chip = el("span", "nb-chip");
    chip.dataset.chip = cell.id;
    head.append(chip);
  }
  const actions = el("div", "nb-cell-actions");
  if (cell.kind === "code") {
    actions.append(button("Run", () => runCell(cell.id), { disabled: runAllActive || activeCell !== null, title: "Compile and run this cell" }));
  } else {
    const reading = textMode(cell) === "read";
    const toggle = button(reading ? "Edit" : "Read", () => {
      textModes.set(cell.id, reading ? "edit" : "read");
      renderCells();
    });
    toggle.setAttribute("aria-pressed", String(reading));
    toggle.setAttribute("aria-label", reading ? `Edit text cell ${cell.id}` : `Read text cell ${cell.id} rendered`);
    actions.append(toggle);
  }
  actions.append(
    button("Up", () => commit(moveCell(doc, cell.id, -1)), { disabled: index === 0 }),
    button("Down", () => commit(moveCell(doc, cell.id, 1)), { disabled: index === doc.cells.length - 1 }),
    button("Delete", () => removeCellWithConfirm(cell.id)),
  );
  head.append(actions);
  article.append(head);

  const source = el("textarea", "nb-source");
  source.value = cell.source;
  source.spellcheck = false;
  source.rows = Math.max(3, cell.source.split("\n").length);
  source.setAttribute("aria-label", `${cell.kind === "code" ? "Ostrin code" : "Text"} of cell ${cell.id}`);
  source.addEventListener("input", () => {
    doc = setSource(doc, cell.id, source.value);
    // The text is not redrawn while typing; only the status and the outputs' stale note are updated.
    refreshHashes().then(() => {
      renderChips();
      const outputs = article.querySelector(".nb-outputs");
      if (outputs) outputs.replaceWith(renderOutputs(cell.id));
    });
    markChanged();
  });
  if (cell.kind === "text" && textMode(cell) === "read") {
    article.append(renderMarkdown(cell.source));
  } else {
    article.append(source);
  }

  if (cell.kind === "code") {
    const deps = el("label", "nb-deps");
    deps.append(el("span", null, "Uses cells (definitions only)"));
    const input = el("input");
    input.type = "text";
    input.value = cell.deps.join(", ");
    input.placeholder = "e.g. c1";
    input.setAttribute("aria-label", `Dependencies of cell ${cell.id}`);
    input.addEventListener("change", () => {
      const list = input.value.split(",").map((s) => s.trim()).filter((s) => s !== "");
      commit(setDeps(doc, cell.id, list));
    });
    deps.append(input);
    article.append(deps);
    article.append(renderOutputs(cell.id));
  }
  return article;
}

function renderOutputs(cellId) {
  const box = el("div", "nb-outputs");
  const run = doc.runs[cellId];
  if (!run) {
    box.append(el("p", "nb-muted", "No run recorded for this cell."));
    return box;
  }
  const state = freshness(doc, hashes, cellId);
  const meta = el("p", "nb-meta");
  const origin = ranThisSession.has(cellId) ? "computed in this session" : "restored from the document (not run in this page)";
  meta.textContent = `${origin} · ${run.state}${run.exitCode === null ? "" : ` · exit ${run.exitCode}`} · ${Math.round(run.durationMs)} ms · started ${run.startedAt}`;
  box.append(meta);
  if (state === "stale") {
    box.append(el("p", "nb-stale", "These outputs are from an earlier version of this cell. They are not the result of the code shown above; run the cell again."));
  }
  if (run.error) box.append(el("p", "nb-error", run.error));
  if (run.stdout) box.append(el("pre", "nb-stdout", run.stdout));
  if (run.stderr) box.append(el("pre", "nb-stderr", run.stderr));
  for (const table of run.tables) box.append(renderTable(table));
  for (const svg of run.figures) {
    const figure = el("figure", "nb-figure");
    const img = el("img");
    img.alt = `Figure produced by cell ${cellId}`;
    img.src = "data:image/svg+xml;charset=utf-8," + encodeURIComponent(svg);
    figure.append(img);
    box.append(figure);
  }
  if (run.results) {
    const t = resultsTable(run.results);
    const wrap = el("div", "nb-table-wrap");
    wrap.append(tableElement(t.headers, t.rows, "Structured results"));
    box.append(wrap);
  }
  if (run.resultsRejected) box.append(el("p", "nb-error", `Structured results rejected: ${run.resultsRejected}`));
  return box;
}

// Rendered Markdown of a text cell. Only the subset of workbench-markdown.js is built, with createElement and
// textContent; a link gets an href only when its target passed the policy.
function renderMarkdown(source) {
  const root = el("div", "nb-md");
  const blocks = parseBlocks(source);
  if (blocks.length === 0) root.append(el("p", "nb-muted", "Empty text cell. Use Edit to write it."));
  for (const block of blocks) root.append(markdownBlock(block));
  return root;
}

function markdownBlock(block) {
  switch (block.type) {
    case "heading": {
      const node = el(`h${Math.min(block.level + 2, 6)}`);
      node.append(...markdownInlines(block.inlines));
      return node;
    }
    case "paragraph": {
      const node = el("p");
      node.append(...markdownInlines(block.inlines));
      return node;
    }
    case "list": {
      const list = el(block.ordered ? "ol" : "ul");
      for (const item of block.items) {
        const li = el("li");
        li.append(...markdownInlines(item));
        list.append(li);
      }
      return list;
    }
    case "code": {
      const pre = el("pre", "nb-md-code");
      pre.append(el("code", null, block.text));
      return pre;
    }
    case "quote": {
      const quote = el("blockquote");
      for (const inner of block.blocks) quote.append(markdownBlock(inner));
      return quote;
    }
    default:
      return el("hr");
  }
}

function markdownInlines(nodes) {
  return nodes.map((node) => {
    switch (node.type) {
      case "code":
        return el("code", null, node.text);
      case "strong":
        return el("strong", null, node.text);
      case "em":
        return el("em", null, node.text);
      case "link": {
        if (node.href === null) return document.createTextNode(`${node.text} (link not allowed)`);
        const anchor = el("a", null, node.text);
        anchor.href = node.href;
        anchor.rel = "noopener noreferrer";
        return anchor;
      }
      default:
        return document.createTextNode(node.text);
    }
  });
}

function renderTable(table) {
  const rows = csvRows(table.csv);
  const [headers = [], ...body] = rows;
  return tableElement(headers, body, table.title);
}

function tableElement(headers, rows, caption) {
  const wrap = el("div", "nb-table-wrap");
  const table = el("table", "nb-table");
  table.append(el("caption", null, caption));
  const thead = el("thead");
  const head = el("tr");
  for (const h of headers) head.append(el("th", null, h));
  thead.append(head);
  const tbody = el("tbody");
  for (const row of rows) {
    const tr = el("tr");
    for (const cell of row) tr.append(el("td", null, cell));
    tbody.append(tr);
  }
  table.append(thead, tbody);
  wrap.append(table);
  return wrap;
}

function renderResources() {
  const box = $("nb-resources");
  box.replaceChildren();
  if (doc.resources.length === 0) return;
  box.append(el("h2", "nb-resources-title", "Attached data"));
  for (const resource of doc.resources) {
    const row = el("div", "nb-resource");
    row.append(el("code", null, resource.path), el("span", "nb-muted", `${encoder.encode(resource.text).length} bytes`));
    row.append(button("Detach", () => commit(attachDetach(resource.path))));
    box.append(row);
  }
}

function attachDetach(path) {
  return { ...doc, resources: doc.resources.filter((r) => r.path !== path) };
}

// ---------------------------------------------------------------- running

// Lines from the two raw streams, used only when the worker did not send ordered chunks.
function bytesLines(stdout, stderr) {
  const decoder = new TextDecoder("utf-8", { fatal: false });
  const split = (bytes, kind) => {
    const text = decoder.decode(bytes);
    if (text === "") return [];
    return (text.endsWith("\n") ? text.slice(0, -1) : text).split("\n").map((line) => [kind, line]);
  };
  return [...split(stdout ?? new Uint8Array(), "out"), ...split(stderr ?? new Uint8Array(), "err")];
}

function recordFromOutcome(outcome, hash, startedAt, durationMs) {
  const base = { hash, startedAt, durationMs, exitCode: null, stdout: "", stderr: "", tables: [], figures: [], results: null, resultsRejected: null, error: null };
  if (outcome.state === "cancelled") return { ...base, state: "cancelled", error: "The run was cancelled. Its outputs are incomplete." };
  if (outcome.state === "timed-out") return { ...base, state: "timed-out", error: `The run exceeded ${RUN_TIMEOUT_MS / 1000} seconds. Its outputs are incomplete.` };
  if (outcome.state !== "completed") return { ...base, state: "failed", error: outcome.error?.message ?? "The run failed." };
  const result = outcome.result;
  const lines = Array.isArray(result.ordered) ? orderedLines(result.ordered) : bytesLines(result.stdout, result.stderr);
  const outputs = outputsFromLines(lines);
  const cap = Math.floor(LIMITS.maxOutputBytes / 4);
  const clip = (text) => (text.length > cap ? text.slice(0, cap) + "\n[output truncated]\n" : text);
  const tables = outputs.tables.filter((t) => encoder.encode(t.csv).length <= LIMITS.maxOutputBytes);
  return {
    ...base,
    state: result.code === 0 ? "completed" : "failed",
    exitCode: result.code,
    stdout: clip(outputs.stdout),
    stderr: clip(outputs.stderr),
    tables,
    figures: outputs.figures,
    results: outputs.results,
    resultsRejected: outputs.resultsRejected,
    error: result.code === 0 ? null : `The program exited with status ${result.code}. Its diagnostics are in the error output.`,
  };
}

async function runCell(cellId) {
  if (activeCell !== null) return;
  const plan = planRun(doc, cellId);
  if (!plan.ok) {
    notice(`Cell ${cellId} cannot run: ${plan.problems.join("; ")}`, "error");
    return;
  }
  notice("");
  const hash = hashes[cellId];
  const startedAt = new Date().toISOString();
  const started = performance.now();
  activeCell = cellId;
  renderCells();
  let outcome;
  try {
    outcome = await executor.run({ command: plan.request.command, entry: plan.request.entry, files: plan.request.files, timeoutMs: RUN_TIMEOUT_MS });
  } catch (error) {
    outcome = { state: "failed", error: { message: error.message ?? String(error) } };
  }
  activeCell = null;
  const record = recordFromOutcome(outcome, hash, startedAt, performance.now() - started);
  // The cell may have been removed while it ran; its outputs are then dropped with it.
  if (doc.cells.some((c) => c.id === cellId)) {
    doc = { ...doc, runs: { ...doc.runs, [cellId]: record } };
    ranThisSession.add(cellId);
    markChanged();
  }
  await refreshHashes();
  renderCells();
}

async function runAll() {
  if (runAllActive || activeCell !== null) return;
  runAllActive = true;
  cancelRequested = false;
  renderStatus();
  try {
    for (const cell of doc.cells) {
      if (cancelRequested) break;
      if (cell.kind !== "code") continue;
      await runCell(cell.id);
    }
  } finally {
    runAllActive = false;
    renderCells();
  }
}

function cancel() {
  cancelRequested = true;
  executor.cancel();
}

// ---------------------------------------------------------------- commands

async function removeCellWithConfirm(cellId) {
  if (!window.confirm(`Delete cell ${cellId}? Its outputs are deleted with it.`)) return;
  await commit(removeCell(doc, cellId));
}

async function addCellOf(kind) {
  const focus = document.activeElement?.closest?.("[data-cell-id]")?.dataset.cellId ?? null;
  try {
    await commit(addCell(doc, kind, { afterId: focus }));
    notice("");
  } catch (error) {
    notice(error.message, "error");
  }
}

function download(filename, text) {
  const url = URL.createObjectURL(new Blob([text], { type: "application/vnd.ostrin.notebook+json" }));
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  document.body.append(anchor);
  anchor.click();
  anchor.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

function safeName(text) {
  return String(text).replace(/[^A-Za-z0-9_-]+/g, "-").replace(/^-|-$/g, "").slice(0, 80) || "notebook";
}

function exportNotebook() {
  try {
    const text = serializeNotebook(doc);
    download(`${safeName(doc.title)}.ostrin.notebook.json`, text);
    notice("Exported. The file holds the cells, the attached data and the recorded outputs.", "info");
  } catch (error) {
    notice(`Export refused: ${error.problems ? error.problems.join("; ") : error.message}`, "error");
  }
}

// Replaces the open document only after the new one is validated, and never without asking when there is work to keep.
async function replaceDocument(next, what) {
  ranThisSession.clear();
  if (isUnsaved() && doc.cells.length > 0 && !window.confirm(`Replace the open notebook with ${what}? Unsaved changes will be lost.`)) {
    return false;
  }
  doc = next;
  await refreshHashes();
  renderCells();
  markChanged();
  saveNow();
  notice(`${what} is open. Nothing has been run.`, "info");
  return true;
}

async function importFile(file) {
  if (file.size > LIMITS.maxDocumentBytes) {
    notice(`Import refused: the file is ${file.size} bytes; the limit is ${LIMITS.maxDocumentBytes}.`, "error");
    return;
  }
  try {
    const next = parseNotebook(await file.text());
    await replaceDocument(next, `“${next.title}”`);
  } catch (error) {
    const reasons = error instanceof NotebookError ? error.problems.slice(0, 4).join("; ") : error.message;
    notice(`Import refused: ${reasons}. The open notebook is unchanged.`, "error");
  }
}

async function loadExample(name) {
  if (!EXAMPLE_FILES[name]) return;
  try {
    const response = await fetch(EXAMPLE_FILES[name], { cache: "no-cache" });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const next = parseNotebook(await response.text());
    await replaceDocument(next, `the example “${next.title}”`);
  } catch (error) {
    notice(`The example was not loaded: ${error.message}`, "error");
  }
}

async function attachFile(file) {
  if (file.size > LIMITS.maxResourceBytes) {
    notice(`The file is larger than ${LIMITS.maxResourceBytes} bytes and is not attached.`, "error");
    return;
  }
  const name = safeName(file.name.replace(/\.[^.]+$/, "")) + (file.name.match(/\.(csv|tsv|txt|json)$/i)?.[0].toLowerCase() ?? ".csv");
  const path = `data/${name}`;
  const next = attachResource(doc, path, await file.text());
  const problems = validateNotebook(next);
  if (problems.length > 0) {
    notice(`Not attached: ${problems.join("; ")}`, "error");
    return;
  }
  await commit(next);
  notice(`Attached ${path}. Programs read it with read_file("${path}").`, "info");
}

function newNotebook() {
  if (isUnsaved() && doc.cells.length > 0 && !window.confirm("Start a new notebook? Unsaved changes will be lost.")) return;
  doc = createNotebook({ title: "Untitled notebook" });
  refreshHashes().then(() => {
    renderCells();
    markChanged();
    saveNow();
    notice("A new notebook is open.", "info");
  });
}

function restartContext() {
  if (!window.confirm("Restart the context? All recorded outputs are cleared. The code is kept.")) return;
  cancel();
  doc = clearOutputs(doc);
  refreshHashes().then(() => {
    renderCells();
    markChanged();
    notice("The context was restarted: no output is recorded now.", "info");
  });
}

// ---------------------------------------------------------------- wiring

function wire() {
  $("nb-run-all").addEventListener("click", runAll);
  $("nb-save").addEventListener("click", () => saveNow());
  $("nb-cancel").addEventListener("click", cancel);
  $("nb-restart").addEventListener("click", restartContext);
  $("nb-add-code").addEventListener("click", () => addCellOf("code"));
  $("nb-add-text").addEventListener("click", () => addCellOf("text"));
  $("nb-attach").addEventListener("click", () => $("nb-attach-file").click());
  $("nb-attach-file").addEventListener("change", (event) => {
    const file = event.target.files?.[0];
    event.target.value = "";
    if (file) attachFile(file);
  });
  $("nb-export").addEventListener("click", exportNotebook);
  $("nb-import").addEventListener("click", () => $("nb-import-file").click());
  $("nb-import-file").addEventListener("change", (event) => {
    const file = event.target.files?.[0];
    event.target.value = "";
    if (file) importFile(file);
  });
  $("nb-new").addEventListener("click", newNotebook);
  $("nb-example").addEventListener("change", (event) => {
    const name = event.target.value;
    event.target.value = "";
    if (name) loadExample(name);
  });
  $("notebook-title").addEventListener("input", (event) => {
    doc = { ...doc, title: event.target.value.slice(0, LIMITS.maxTitleLength) };
    markChanged();
  });
  window.addEventListener("beforeunload", (event) => {
    if (isUnsaved()) {
      // Best effort only: a synchronous localStorage write on unload. It is not a recovery guarantee; the page
      // asks before closing while the document is unsaved. The document counts as saved only if this write succeeds.
      try {
        localStorage.setItem(DRAFT_KEY, serializeNotebook(doc));
        save.savedRevision = Math.max(save.savedRevision, save.revision);
      } catch (_) {
        // Still unsaved: the page asks before it closes.
      }
      renderSaveState();
      if (isUnsaved()) event.preventDefault();
    }
  });
}

async function start() {
  wire();
  restoreDraft();
  await refreshHashes();
  renderCells();
  if (!$("nb-notice").textContent) notice("");
  renderStatus();
  document.documentElement.dataset.notebookReady = "true";
}

start();
