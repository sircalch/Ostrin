// Run History panel for the Workbench. It renders the recorded runs and the selected run's details from a history
// state, and reports the user's choices through handlers. It never changes the open project or the history itself.
// All program text is inserted with textContent; the figure is shown as an image data URL, never as markup.

import { figureOf, findRun } from "./workbench-history.js";
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

export function renderHistoryPanel(container, history, { selectedId = null }, handlers) {
  if (!container) return;
  container.replaceChildren();
  container.append(el("h2", "history-title", "Run history"));

  if (history.entries.length === 0) {
    container.append(el("p", "history-empty", "No runs recorded yet. Run a project and its result is kept here."));
    return;
  }

  const table = el("table", "history-table");
  const head = el("thead");
  const headRow = el("tr");
  for (const label of ["Run", "Status", "Evidence", "Project", "Entry", "Recorded"]) headRow.append(el("th", null, label));
  head.append(headRow);
  const body = el("tbody");
  for (const entry of history.entries) {
    const tr = el("tr", entry.id === selectedId ? "is-selected" : null);
    tr.tabIndex = 0;
    tr.dataset.runId = entry.id;
    tr.setAttribute("aria-selected", String(entry.id === selectedId));
    for (const text of [entry.id, entry.status, entry.evidence, entry.projectName, entry.entry, entry.startedAt]) {
      tr.append(el("td", null, text));
    }
    tr.addEventListener("click", () => handlers.onSelect(entry.id));
    tr.addEventListener("keydown", (event) => {
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        handlers.onSelect(entry.id);
      }
    });
    body.append(tr);
  }
  table.append(head, body);
  container.append(table);

  const entry = selectedId ? findRun(history, selectedId) : null;
  if (!entry) return;
  container.append(renderDetails(history, entry, handlers));
}

function renderDetails(history, entry, handlers) {
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
  );
  section.append(meta);

  if (entry.results) {
    const table = resultsTable(entry.results);
    const wrap = el("div", "history-results");
    const element = el("table", "history-results-table");
    const thead = el("thead");
    const headRow = el("tr");
    for (const header of table.headers) headRow.append(el("th", null, header));
    thead.append(headRow);
    const tbody = el("tbody");
    for (const cells of table.rows) {
      const tr = el("tr");
      for (const cell of cells) tr.append(el("td", null, cell));
      tbody.append(tr);
    }
    element.append(thead, tbody);
    wrap.append(element);
    section.append(wrap);
  }

  const svg = figureOf(history, entry);
  if (svg) {
    const figure = el("div", "figure-preview");
    const img = el("img");
    img.alt = "Figure recorded with run " + entry.id;
    img.src = "data:image/svg+xml;charset=utf-8," + encodeURIComponent(svg);
    figure.append(img);
    section.append(figure);
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
  actions.append(back);
  section.append(actions);
  return section;
}
