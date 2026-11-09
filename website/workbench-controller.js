// Ostrin Workbench state controller.
//
// Holds the active project, the selected file, the entrypoint, unsaved changes, and the last run,
// figure and manifest. It is pure data code: every operation returns a new state and never touches
// the DOM, so the page can use it and Node can test it. The page keeps the textarea as the editor
// and calls `commitEdit` before any action that depends on the file contents.

import {
  LIMITS,
  byteLength,
  cleanName,
  createProject,
  parseProject,
  removeFile,
  renameFile,
  setFile,
  validPath,
  validateProject,
  filesForRun,
  buildRunManifest,
} from "./workbench-project.js";

function clone(project) {
  return JSON.parse(JSON.stringify(project));
}

function fail(message) {
  throw new Error(message);
}

export function createWorkbench(project = createProject()) {
  const problems = validateProject(project);
  if (problems.length > 0) fail(`invalid project: ${problems.join("; ")}`);
  const current = clone(project);
  return {
    project: current,
    activePath: current.entry,
    saved: clone(current),
    lastRun: null,
  };
}

// Serialized view of what would be written to disk. Two states with the same snapshot are equal.
function snapshotKey(project) {
  return JSON.stringify({ name: project.name, entry: project.entry, files: Object.keys(project.files).sort().map((p) => [p, project.files[p]]) });
}

export function isDirty(state) {
  return snapshotKey(state.project) !== snapshotKey(state.saved);
}

export function dirtyFiles(state) {
  const changed = [];
  for (const path of Object.keys(state.project.files)) {
    if (state.saved.files[path] !== state.project.files[path]) changed.push(path);
  }
  for (const path of Object.keys(state.saved.files)) {
    if (!(path in state.project.files)) changed.push(path);
  }
  return changed.sort();
}

// Stores the editor text of the active file in the model. Empty input is valid (an empty file).
export function commitEdit(state, text) {
  if (typeof text !== "string") fail("editor text must be a string");
  if (byteLength(text) > LIMITS.maxFileBytes) fail(`file is larger than ${LIMITS.maxFileBytes} bytes`);
  return { ...state, project: setFile(state.project, state.activePath, text) };
}

// Saves the editor text first, then switches the active file.
export function selectFile(state, path, editorText) {
  if (!(path in state.project.files)) fail(`no file ${path}`);
  const committed = editorText === undefined ? state : commitEdit(state, editorText);
  return { ...committed, activePath: path };
}

export function addFile(state, path, text = "") {
  if (!validPath(path)) fail(`invalid file path ${JSON.stringify(path)}`);
  if (path in state.project.files) fail(`${path} already exists`);
  if (Object.keys(state.project.files).length >= LIMITS.maxFiles) fail(`a project can have at most ${LIMITS.maxFiles} files`);
  return { ...state, project: setFile(state.project, path, text), activePath: path };
}

export function renameActive(state, to, editorText) {
  const committed = editorText === undefined ? state : commitEdit(state, editorText);
  const project = renameFile(committed.project, committed.activePath, to);
  return { ...committed, project, activePath: to };
}

export function deleteFile(state, path) {
  const project = removeFile(state.project, path);
  const activePath = state.activePath === path ? project.entry : state.activePath;
  return { ...state, project, activePath };
}

export function setEntry(state, path) {
  if (!(path in state.project.files)) fail(`no file ${path}`);
  if (!validPath(path)) fail(`invalid file path ${JSON.stringify(path)}`);
  return { ...state, project: { ...state.project, entry: path } };
}

// Marks the current content as saved (after a successful export or download).
export function markSaved(state) {
  return { ...state, saved: clone(state.project) };
}

// Replaces the whole workbench with a parsed project. The caller must not change the old state when
// this throws, because the function returns a new object and never mutates the argument.
export function importProjectText(state, text) {
  const project = parseProject(text);
  return createWorkbench(project);
}

// A single .ostrin file becomes a one-file project. The file is always stored as main.ostrin unless
// its name is a valid project path, so the project keeps a predictable entry.
export function projectFromSourceFile(fileName, text) {
  const base = String(fileName ?? "main.ostrin").split(/[\\/]/).pop();
  const path = validPath(base) ? base : "main.ostrin";
  const project = { format: "ostrin.project", version: 1, name: cleanName(path.replace(/\.ostrin$/, "")), entry: path, files: { [path]: text } };
  const problems = validateProject(project);
  if (problems.length > 0) fail(`invalid source file: ${problems.join("; ")}`);
  return project;
}

// The immutable input of one run. The run and every hash later derived from it use this copy only,
// so edits after the run started cannot change the recorded evidence.
export function runInput(state, { entry = state.project.entry } = {}) {
  const project = clone(state.project);
  if (!(entry in project.files)) fail(`entry ${entry} is not a file of the project`);
  return { project, entry, files: filesForRun(project) };
}

// Records the output of one run together with its input snapshot.
export function recordRun(state, input, result) {
  const stdout = result.lines.filter(([kind]) => kind === "out").map(([, text]) => text);
  const stderr = result.lines.filter(([kind]) => kind !== "out").map(([, text]) => text);
  return {
    ...state,
    lastRun: {
      input,
      command: result.command,
      exitCode: result.code,
      stdout: stdout.join("\n") + (stdout.length ? "\n" : ""),
      stderr: stderr.join("\n") + (stderr.length ? "\n" : ""),
      // Exact bytes from the runtime when available; the text above is only a rendering.
      stdoutBytes: result.raw?.stdout ?? null,
      stderrBytes: result.raw?.stderr ?? null,
      figureSvg: result.figureSvg ?? null,
      compilerIdentity: result.compilerIdentity ?? null,
    },
  };
}

// Manifest of the last recorded run. It is built from the run's input snapshot, not from the
// current editor content.
export async function manifestForLastRun(state) {
  if (!state.lastRun) fail("no run has been recorded");
  const run = state.lastRun;
  return buildRunManifest({
    project: run.input.project,
    entry: run.input.entry,
    command: run.command,
    exitCode: run.exitCode,
    stdout: run.stdout,
    stderr: run.stderr,
    stdoutBytes: run.stdoutBytes,
    stderrBytes: run.stderrBytes,
    figureSvg: run.figureSvg,
    compilerIdentity: run.compilerIdentity,
  });
}
