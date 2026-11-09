import test from "node:test";
import assert from "node:assert/strict";
import {
  addFile,
  commitEdit,
  createWorkbench,
  deleteFile,
  dirtyFiles,
  importProjectText,
  isDirty,
  manifestForLastRun,
  markSaved,
  projectFromSourceFile,
  recordRun,
  renameActive,
  runInput,
  selectFile,
  setEntry,
} from "../website/workbench-controller.js";
import { createProject, serializeProject } from "../website/workbench-project.js";

const HELPERS = "pub fn square(x: Int) -> Int {\n    x * x\n}\n";
const MAIN = "import helpers\n\nfn main() -> Void {\n    print(helpers.square(7).to_string())\n}\n";

function twoFileWorkbench() {
  let state = createWorkbench(createProject("demo"));
  state = commitEdit(state, MAIN);
  state = addFile(state, "helpers.ostrin", "fn square(x: Int) -> Int {\n    x * x\n}\n");
  state = selectFile(state, "main.ostrin", "");
  return state;
}

test("a new workbench starts on the entry file, not dirty", () => {
  const state = createWorkbench(createProject("demo"));
  assert.equal(state.activePath, "main.ostrin");
  assert.equal(isDirty(state), false);
});

test("switching files saves the current editor text before loading the next one", () => {
  let state = twoFileWorkbench();
  state = selectFile(state, "helpers.ostrin", MAIN);
  assert.equal(state.project.files["main.ostrin"], MAIN, "main keeps the text typed before switching");
  assert.equal(state.activePath, "helpers.ostrin");
  state = selectFile(state, "main.ostrin", "edited helpers do not leak here");
  assert.equal(state.project.files["helpers.ostrin"], "edited helpers do not leak here");
});

test("dirty tracking reports exactly which files differ from the saved state", () => {
  let state = twoFileWorkbench();
  state = markSaved(state);
  assert.equal(isDirty(state), false);
  state = commitEdit(state, "changed");
  assert.equal(isDirty(state), true);
  assert.deepEqual(dirtyFiles(state), ["main.ostrin"]);
  state = markSaved(state);
  assert.deepEqual(dirtyFiles(state), []);
});

test("rename, delete and entry changes are tracked as unsaved changes", () => {
  let state = markSaved(twoFileWorkbench());
  state = renameActive(state, "app.ostrin", "edited");
  assert.equal(state.project.entry, "app.ostrin");
  assert.equal(isDirty(state), true);
  assert.deepEqual(dirtyFiles(state), ["app.ostrin", "main.ostrin"]);

  state = deleteFile(state, "helpers.ostrin");
  assert.equal(state.project.files["helpers.ostrin"], undefined);
  assert.equal(dirtyFiles(state).includes("helpers.ostrin"), true);
});

test("deleting the active file moves the selection to the entry", () => {
  let state = twoFileWorkbench();
  state = selectFile(state, "helpers.ostrin", MAIN);
  state = deleteFile(state, "helpers.ostrin");
  assert.equal(state.activePath, "main.ostrin");
});

test("the entry can be changed to another project file only", () => {
  const state = twoFileWorkbench();
  assert.equal(setEntry(state, "helpers.ostrin").project.entry, "helpers.ostrin");
  assert.throws(() => setEntry(state, "missing.ostrin"), /no file/);
});

test("conflicting operations are refused and leave the state unchanged", () => {
  const state = twoFileWorkbench();
  const before = JSON.stringify(state);
  assert.throws(() => addFile(state, "helpers.ostrin", ""), /already exists/);
  assert.throws(() => addFile(state, "../bad.ostrin", ""), /invalid file path/);
  assert.throws(() => renameActive(state, "helpers.ostrin", "x"), /already exists/);
  assert.throws(() => selectFile(state, "ghost.ostrin", ""), /no file/);
  assert.equal(JSON.stringify(state), before, "a refused operation does not mutate the state");
});

test("a run uses an immutable copy: later edits do not change the recorded input", async () => {
  let state = twoFileWorkbench();
  const input = runInput(state);
  state = commitEdit(state, "fn main() -> Void {\n    print(\"changed after the run\")\n}\n");
  state = recordRun(state, input, {
    command: "--run",
    code: 0,
    lines: [["out", "49"]],
    figureSvg: null,
    compilerIdentity: "ostrinc 0.2.0",
  });
  const manifest = await manifestForLastRun(state);
  assert.equal(manifest.project.sources["main.ostrin"].bytes, Buffer.byteLength(MAIN), "hash is over the run input");
  assert.equal(manifest.run.entry, "main.ostrin");
  assert.equal(manifest.outputs.stdout.bytes, Buffer.byteLength("49\n"));
});

test("stdout and stderr are kept separate in the recorded run", () => {
  const state = recordRun(twoFileWorkbench(), runInput(twoFileWorkbench()), {
    command: "--run",
    code: 1,
    lines: [["out", "partial"], ["err", "error: boom"]],
  });
  assert.equal(state.lastRun.stdout, "partial\n");
  assert.equal(state.lastRun.stderr, "error: boom\n");
  assert.equal(state.lastRun.exitCode, 1);
});

test("a manifest without a runtime compiler report says so instead of inventing one", async () => {
  const state = recordRun(twoFileWorkbench(), runInput(twoFileWorkbench()), { command: "--check", code: 0, lines: [] });
  const manifest = await manifestForLastRun(state);
  assert.equal(manifest.run.compiler, "unknown");
  assert.equal(manifest.run.compilerSource, "not reported");
});

test("an invalid import leaves the current project in place", () => {
  const state = twoFileWorkbench();
  const before = serializeProject(state.project);
  assert.throws(() => importProjectText(state, "{ broken"), /not valid JSON/);
  assert.throws(() => importProjectText(state, JSON.stringify({ format: "ostrin.project", version: 1, name: "p", entry: "x.ostrin", files: {} })), /invalid project/);
  assert.equal(serializeProject(state.project), before);
});

test("a saved project reopens with every module, the entry and a clean state", () => {
  const state = twoFileWorkbench();
  const text = serializeProject(state.project);
  const reopened = importProjectText(state, text);
  assert.deepEqual(reopened.project.files, state.project.files);
  assert.equal(reopened.project.entry, "main.ostrin");
  assert.equal(reopened.activePath, "main.ostrin");
  assert.equal(isDirty(reopened), false);
});

test("opening a single .ostrin file creates a one-file project with a predictable entry", () => {
  const project = projectFromSourceFile("C:\\work\\helpers.ostrin", HELPERS);
  assert.equal(project.entry, "helpers.ostrin");
  assert.deepEqual(Object.keys(project.files), ["helpers.ostrin"]);
  const fallback = projectFromSourceFile("my file (1).ostrin", MAIN);
  assert.equal(fallback.entry, "main.ostrin");
});

test("runInput refuses an entry that is not a project file", () => {
  assert.throws(() => runInput(twoFileWorkbench(), { entry: "ghost.ostrin" }), /not a file of the project/);
});
