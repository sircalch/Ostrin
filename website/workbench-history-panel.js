// Run History panel for the Workbench: the list of recorded runs, the selected run's details, and the comparison of
// two selected runs. It renders from a history state and reports the user's choices through handlers. It never
// changes the open project or the history. All program text is inserted with textContent; figures are images with
// data URLs, never markup.

import { comparability, compareRuns, figureOf, findRun } from "./workbench-history.js";
import { resultsTable } from "./workbench-results.js";

function el(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function field(label, value) {
  const row = el("div", "history-field");
  row.append(el("dt", null, label), el("dd", null, value === null || value === undefined ? "—" : String(value)));
  return row;
}

function table(headers, rows) {
  const element = el("table", "history-results-table");
  const thead = el("thead");
  const headRow = el("tr");
  for (const header of headers) headRow.append(el("th", null, header));
  thead.append(headRow);
  const tbody = el("tbody");
  for (const cells of rows) {
    const tr = el("tr");
    for (const cell of cells) tr.append(el("td", null, cell));
    tbody.append(tr);
  }
  element.append(thead, tbody);
  return element;
}

function figureImage(svg, alt) {
  const figure = el("div", "figure-preview");
  const img = el("img");
  img.alt = alt;
  img.src = "data:image/svg+xml;charset=utf-8," + encodeURIComponent(svg);
  figure.append(img);
  return figure;
}

const STORED_STATE_TEXT = {
  verified: "verified now",
  missing: "digest only: bytes not stored",
  altered: "altered: failed verification, not used",
};

// One line describing what the storage holds for a run: each registered digest and whether its bytes verified now.
function storedContentText(verification) {
  if (!verification) return "not recorded";
  const parts = Object.entries(verification.sources).map(([name, s]) => "source " + name + ": " + STORED_STATE_TEXT[s]);
  if (verification.stdout) parts.push("stdout: " + STORED_STATE_TEXT[verification.stdout]);
  if (verification.stderr) parts.push("stderr: " + STORED_STATE_TEXT[verification.stderr]);
  if (verification.figure) parts.push("figure: " + STORED_STATE_TEXT[verification.figure]);
  return parts.join("; ");
}

function storageBanner(storage) {
  if (!storage) return null;
  if (storage.state === "loading") return el("p", "history-scope", "Loading saved runs from this browser…");
  if (storage.state === "unavailable") {
    return el(
      "p",
      "history-rejected",
      "Saved history is unavailable (" + storage.reason + "). Runs shown here are kept on this page only and are not saved permanently.",
    );
  }
  const notes = [];
  if (storage.writeError) notes.push(storage.writeError + ". It stays visible on this page only.");
  if (storage.quarantined?.length) {
    notes.push(storage.quarantined.length + " saved record(s) could not be read and were not loaded (" + storage.quarantined.map((q) => q.key).join(", ") + "). They stay in storage; nothing was deleted.");
  }
  if (storage.altered?.length) {
    notes.push(storage.altered.length + " stored content(s) failed verification and are not used by any run.");
  }
  return notes.length ? el("p", "history-rejected", notes.join(" ")) : null;
}

const SAVED_TEXT = { saving: "saving…", saved: "saved", "not-saved": "not saved" };

// Where a run came from. An imported run is never presented as one executed in this browser.
function originText(origin) {
  if (!origin || origin.kind === "local") return "run in this browser";
  return "imported from an evidence package (stored " + origin.at + ")";
}
function originShort(origin) {
  return !origin || origin.kind === "local" ? "local" : "imported";
}

// The import control: a button that opens a file picker. The file is handed to the handler; nothing is read here.
function importControl(handlers) {
  const wrap = el("div", "history-import");
  const button = el("button", "button-quiet", "Import evidence package…");
  button.type = "button";
  const input = el("input");
  input.type = "file";
  input.accept = ".json,.ostrin.evidence.json,application/json";
  input.hidden = true;
  input.setAttribute("aria-label", "Evidence package to import");
  input.addEventListener("change", () => {
    const file = input.files?.[0];
    input.value = "";
    if (file) handlers.onImport(file);
  });
  button.addEventListener("click", () => input.click());
  wrap.append(button, input);
  return wrap;
}

// The evidence coverage of a run, as the Run History shows it: what the stored evidence supports.
function coverageText(entry, verification) {
  if (entry.status !== "completed") return "incomplete: run " + entry.status + ", no validated results";
  if (entry.evidence !== "validated") return "incomplete: no validated results";
  const states = [...Object.values(verification?.sources ?? {}), verification?.stdout, verification?.stderr, verification?.figure].filter(Boolean);
  return states.every((s) => s === "verified") ? "complete: validated results and all declared content verified" : "partial: some declared content is not stored or failed verification";
}

// state: { selectedId, selection: [id, ...], comparison: { a, b } | null,
//          persistence: { storage, runs: { id: { state, reason } }, verification: { id: verification } } }
export function renderHistoryPanel(container, history, state, handlers) {
  if (!container) return;
  container.replaceChildren();
  container.append(el("h2", "history-title", "Run history"));
  const persistence = state.persistence ?? {};
  const banner = storageBanner(persistence.storage);
  if (banner) container.append(banner);

  if (history.entries.length === 0) {
    container.append(el("p", "history-empty", "No runs recorded yet. Run a project and its result is kept here."));
    if (state.notice) container.append(el("p", state.notice.kind === "error" ? "history-rejected" : "history-scope", state.notice.text));
    container.append(importControl(handlers));
    return;
  }

  const selection = state.selection ?? [];
  if (state.notice) container.append(el("p", state.notice.kind === "error" ? "history-rejected" : "history-scope", state.notice.text));
  const bar = el("div", "history-toolbar");
  const compare = el("button", "button-quiet", selection.length === 2 ? "Compare the two selected runs" : `Compare (select two runs, ${selection.length} selected)`);
  compare.type = "button";
  compare.disabled = selection.length !== 2;
  compare.addEventListener("click", () => handlers.onCompare());
  const exportButton = el("button", "button-quiet", `Export selected runs (${selection.length})`);
  exportButton.type = "button";
  exportButton.disabled = selection.length === 0;
  exportButton.addEventListener("click", () => handlers.onExport(selection));
  bar.append(compare, exportButton);
  container.append(bar, importControl(handlers));
  container.append(el("p", "history-scope", "Evidence packages contain source code and program output, which may be sensitive. Share them only on purpose."));

  const grid = el("table", "history-table");
  const head = el("thead");
  const headRow = el("tr");
  for (const label of ["Compare", "Run", "Status", "Evidence", "Project", "Entry", "Recorded", "Origin", "Saved"]) headRow.append(el("th", null, label));
  head.append(headRow);
  const body = el("tbody");
  for (const entry of history.entries) {
    const tr = el("tr", entry.id === state.selectedId ? "is-selected" : null);
    tr.tabIndex = 0;
    tr.dataset.runId = entry.id;
    tr.setAttribute("aria-selected", String(entry.id === state.selectedId));
    const box = el("input");
    box.type = "checkbox";
    box.checked = selection.includes(entry.id);
    box.setAttribute("aria-label", "Select run " + entry.id + " for comparison");
    box.addEventListener("click", (event) => event.stopPropagation());
    box.addEventListener("change", () => handlers.onToggle(entry.id));
    const first = el("td");
    first.append(box);
    tr.append(first);
    for (const text of [entry.id, entry.status, entry.evidence, entry.projectName, entry.entry, entry.startedAt]) {
      tr.append(el("td", null, text));
    }
    tr.append(el("td", "history-origin-" + originShort(persistence.origins?.[entry.id]), originShort(persistence.origins?.[entry.id])));
    const saved = persistence.runs?.[entry.id];
    const savedText = saved ? SAVED_TEXT[saved.state] : SAVED_TEXT["not-saved"];
    const savedCell = el("td", "history-saved-" + (saved?.state ?? "not-saved"), savedText);
    if (saved?.reason) savedCell.title = saved.reason;
    tr.append(savedCell);
    tr.addEventListener("click", () => handlers.onSelect(entry.id));
    tr.addEventListener("keydown", (event) => {
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        handlers.onSelect(entry.id);
      }
    });
    body.append(tr);
  }
  grid.append(head, body);
  container.append(grid);

  if (state.comparison) {
    container.append(renderComparison(history, state.comparison, handlers, persistence));
    return;
  }
  const entry = state.selectedId ? findRun(history, state.selectedId) : null;
  if (entry) container.append(renderDetails(history, entry, handlers, persistence));
}

function renderDetails(history, entry, handlers, persistence) {
  const section = el("section", "history-details");
  section.setAttribute("aria-label", "Selected run");
  section.append(el("h3", null, "Run " + entry.id));

  const meta = el("dl", "history-fields");
  meta.append(
    field("Status", entry.status),
    field("Evidence", entry.evidence === "validated" ? "validated structured results" : "incomplete: no validated results"),
    field("Project", entry.projectName),
    field("Entry and command", entry.entry + " · " + entry.command),
    field("Exit code", entry.exitCode),
    field("Compiler", entry.compilerIdentity ?? "not recorded"),
    field("Recorded", entry.startedAt),
    field("Duration (ms)", entry.durationMs === null || entry.durationMs === undefined ? null : Math.round(entry.durationMs)),
    field("Parameters", entry.parameters ? "c = " + entry.parameters.c.join(", ") : null),
    field("Method", entry.results?.method ?? null),
    field("Tolerances", entry.results?.tolerances ?? null),
    field("Rejected results", entry.resultsRejected),
    field("Origin", originText(persistence?.origins?.[entry.id])),
    field("Storage", SAVED_TEXT[persistence?.runs?.[entry.id]?.state ?? "not-saved"]),
    field("Stored content", storedContentText(persistence?.verification?.[entry.id])),
    field("Evidence coverage", coverageText(entry, persistence?.verification?.[entry.id])),
  );
  section.append(meta);

  if (entry.results) {
    const t = resultsTable(entry.results);
    const wrap = el("div", "history-results");
    wrap.append(table(t.headers, t.rows));
    section.append(wrap);
  }

  const svg = figureOf(history, entry);
  if (svg) section.append(figureImage(svg, "Figure recorded with run " + entry.id));
  else if (entry.digests.figure) {
    section.append(el("p", "history-scope", "Figure not available: its stored bytes are " + (persistence?.verification?.[entry.id]?.figure === "altered" ? "altered" : "missing") + ", so it is not shown."));
  }

  const provenance = el("div", "history-provenance");
  provenance.append(el("h4", null, "Provenance and digests (SHA-256)"));
  const list = el("ul");
  for (const [name, digest] of Object.entries(entry.digests.sources)) list.append(el("li", null, "source " + name + " " + digest));
  if (entry.digests.stdout) list.append(el("li", null, "stdout (raw bytes) " + entry.digests.stdout));
  if (entry.digests.stderr) list.append(el("li", null, "stderr (raw bytes) " + entry.digests.stderr));
  if (entry.digests.figure) list.append(el("li", null, "figure " + entry.digests.figure));
  provenance.append(list);
  provenance.append(el("p", "history-scope", "Integrity: these digests identify the recorded bytes. They are not a reproducibility claim (R3 is not claimed)."));
  section.append(provenance);

  const actions = el("div", "history-actions");
  const back = el("button", "button-quiet", "Back to the open project");
  back.type = "button";
  back.addEventListener("click", () => handlers.onClose());
  const exportOne = el("button", "button-quiet", "Export this run as evidence");
  exportOne.type = "button";
  exportOne.addEventListener("click", () => handlers.onExport([entry.id]));
  actions.append(back, exportOne);
  section.append(actions);
  return section;
}

// The comparison of two runs. Incompatible runs show the reasons and no numbers. Compatible runs show the shared
// parameters with their differences, the values exclusive to each run, and the references to the original runs.
function renderComparison(history, { a: aId, b: bId }, handlers, persistence = {}) {
  const a = findRun(history, aId);
  const b = findRun(history, bId);
  const section = el("section", "history-compare");
  section.setAttribute("aria-label", "Comparison of two runs");
  section.append(el("h3", null, "Comparison: " + aId + " and " + bId));
  if (!a || !b) {
    section.append(el("p", "history-scope", "One of the compared runs is no longer in the history."));
  } else {
    const check = comparability(a, b);
    if (!check.comparable) {
      section.append(el("p", "history-rejected", "These runs are not comparable, so no numbers are compared:"));
      const reasons = el("ul", "history-reasons");
      for (const reason of check.reasons) reasons.append(el("li", null, reason));
      section.append(reasons);
    } else {
      const cmp = compareRuns(a, b);
      section.append(el("p", "history-scope", `Comparable: same experiment family, bracket, method, tolerances and compiler (${a.compilerIdentity}). References: run ${a.id} (A, ${originShort(persistence.origins?.[a.id])}) and run ${b.id} (B, ${originShort(persistence.origins?.[b.id])}).`));
      if (cmp.rows.length === 0) {
        section.append(el("p", "history-scope", "No parameter is shared, so no numeric difference is computed."));
      } else {
        const rows = cmp.rows.map((r) => [
          String(r.c),
          String(r.rootA),
          String(r.rootB),
          String(r.rootDifference),
          String(r.residualA),
          String(r.residualB),
          r.passedA && r.passedB ? "yes" : "no",
        ]);
        section.append(table(["c", "root A", "root B", "root B − root A", "residual A", "residual B", "passed in both"], rows));
      }
      if (cmp.onlyInA.length > 0) section.append(el("p", "history-scope", "Only in A: c = " + cmp.onlyInA.join(", ")));
      if (cmp.onlyInB.length > 0) section.append(el("p", "history-scope", "Only in B: c = " + cmp.onlyInB.join(", ")));
    }
    const figures = el("div", "history-figures");
    for (const [label, run] of [["A", a], ["B", b]]) {
      const svg = figureOf(history, run);
      const card = el("div", "history-figure-card");
      card.append(el("p", "history-scope", label + ": run " + run.id));
      if (svg) card.append(figureImage(svg, "Figure of run " + run.id + " (" + label + ")"));
      else card.append(el("p", "history-scope", "No figure recorded for this run."));
      figures.append(card);
    }
    section.append(figures);
  }
  const back = el("button", "button-quiet", "Back to the list");
  back.type = "button";
  back.addEventListener("click", () => handlers.onCloseCompare());
  section.append(back);
  return section;
}
