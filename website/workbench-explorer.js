// File explorer for the Workbench project. It renders the file list of a workbench state and reports
// user intent through callbacks; it never edits the project itself. Names are inserted with
// textContent, never as HTML, so a file name cannot inject markup.

import { dirtyFiles } from "./workbench-controller.js";

export function renderExplorer(container, state, handlers) {
  if (!container) return;
  container.replaceChildren();
  const dirty = new Set(dirtyFiles(state));

  const list = document.createElement("ul");
  list.className = "file-list";
  list.setAttribute("role", "listbox");
  list.setAttribute("aria-label", "Project files");
  const paths = Object.keys(state.project.files).sort();
  for (const path of paths) {
    const item = document.createElement("li");
    item.className = "file-item";
    item.setAttribute("role", "option");
    const selected = path === state.activePath;
    item.setAttribute("aria-selected", String(selected));
    item.tabIndex = selected ? 0 : -1;
    item.dataset.path = path;
    if (selected) item.classList.add("is-active");

    const name = document.createElement("span");
    name.className = "file-name";
    name.textContent = path;
    item.append(name);

    if (path === state.project.entry) {
      const badge = document.createElement("span");
      badge.className = "file-badge";
      badge.textContent = "entry";
      badge.title = "The program starts from this file";
      item.append(badge);
    }
    if (dirty.has(path)) {
      const mark = document.createElement("span");
      mark.className = "file-dirty";
      mark.setAttribute("aria-label", "unsaved changes");
      mark.textContent = "●";
      item.append(mark);
    }

    item.addEventListener("click", () => handlers.onSelect(path));
    item.addEventListener("keydown", (event) => {
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        handlers.onSelect(path);
      } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        const index = paths.indexOf(path);
        const next = paths[(index + (event.key === "ArrowDown" ? 1 : paths.length - 1)) % paths.length];
        handlers.onFocus?.(next);
      }
    });
    list.append(item);
  }
  container.append(list);

  const actions = document.createElement("div");
  actions.className = "file-actions";
  const add = (label, handler, title) => {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "button-quiet";
    button.textContent = label;
    if (title) button.title = title;
    button.addEventListener("click", handler);
    actions.append(button);
    return button;
  };
  add("New file", () => handlers.onNew(), "Create a new .ostrin file");
  add("Rename", () => handlers.onRename(), "Rename the selected file");
  add("Delete", () => handlers.onDelete(), "Delete the selected file");
  add("Set as entry", () => handlers.onEntry(), "Make the selected file the program entry");
  container.append(actions);

  const note = document.createElement("p");
  note.className = "file-note";
  note.textContent = `${paths.length} file${paths.length === 1 ? "" : "s"} · entry ${state.project.entry}`;
  container.append(note);
}
