import test from "node:test";
import assert from "node:assert/strict";
import {
  LIMITS,
  buildRunManifest,
  createProject,
  filesForRun,
  parseProject,
  removeFile,
  renameFile,
  serializeProject,
  setFile,
  sha256Hex,
  validPath,
  validateProject,
} from "../website/workbench-project.js";

test("a new project is valid and round-trips through serialization", () => {
  const project = createProject("  Orbit model  ");
  assert.deepEqual(validateProject(project), []);
  const back = parseProject(serializeProject(project));
  assert.equal(back.name, "Orbit model");
  assert.equal(back.entry, "main.ostrin");
  assert.deepEqual(back.files, project.files);
});

test("serialization is stable: file order does not change the bytes", () => {
  const a = { format: "ostrin.project", version: 1, name: "p", entry: "main.ostrin", files: { "main.ostrin": "x", "b.ostrin": "y" } };
  const b = { format: "ostrin.project", version: 1, name: "p", entry: "main.ostrin", files: { "b.ostrin": "y", "main.ostrin": "x" } };
  assert.equal(serializeProject(a), serializeProject(b));
});

test("paths must be relative .ostrin files without traversal", () => {
  for (const good of ["main.ostrin", "lib/geometry.ostrin", "a-b_c/d.ostrin"]) assert.equal(validPath(good), true, good);
  for (const bad of ["../x.ostrin", "/abs.ostrin", "a\\b.ostrin", "notes.txt", "a/../b.ostrin", "", "a//b.ostrin", "x.ostrin.exe"]) {
    assert.equal(validPath(bad), false, bad);
  }
});

test("validation reports version, missing entry, empty name and limits", () => {
  const problems = validateProject({ format: "other", version: 2, name: "", entry: "missing.ostrin", files: { "main.ostrin": "x" } });
  assert.ok(problems.some((p) => p.includes("format must be")));
  assert.ok(problems.some((p) => p.includes("unsupported project version")));
  assert.ok(problems.some((p) => p.includes("name is required")));
  assert.ok(problems.some((p) => p.includes("entry")));

  const tooBig = createProject();
  tooBig.files["main.ostrin"] = "x".repeat(LIMITS.maxFileBytes + 1);
  assert.ok(validateProject(tooBig).some((p) => p.includes("limit is")));

  const tooMany = createProject();
  for (let i = 0; i < LIMITS.maxFiles; i += 1) tooMany.files[`f${i}.ostrin`] = "";
  assert.ok(validateProject(tooMany).some((p) => p.includes("files; the limit is")));
});

test("parsing rejects malformed JSON, invalid documents and oversized text", () => {
  assert.throws(() => parseProject("{not json"), /not valid JSON/);
  assert.throws(
    () => parseProject(JSON.stringify({ format: "ostrin.project", version: 1, name: "p", entry: "a.ostrin", files: {} })),
    /invalid project/,
  );
  assert.throws(() => parseProject("x".repeat(LIMITS.maxProjectBytes + 1)), /larger than/);
});

test("editing operations keep the project consistent", () => {
  let p = createProject();
  p = setFile(p, "lib/helpers.ostrin", "pub fn one() -> Int {\n    1\n}\n");
  assert.throws(() => setFile(p, "../escape.ostrin", ""), /invalid file path/);

  p = renameFile(p, "main.ostrin", "app.ostrin");
  assert.equal(p.entry, "app.ostrin", "renaming the entry moves the entry");
  assert.throws(() => renameFile(p, "app.ostrin", "lib/helpers.ostrin"), /already exists/);

  assert.throws(() => removeFile(p, "app.ostrin"), /entry file cannot be removed/);
  p = removeFile(p, "lib/helpers.ostrin");
  assert.deepEqual(Object.keys(p.files), ["app.ostrin"]);
  assert.deepEqual(validateProject(p), []);
});

test("the run file map contains every project file, including imports, as a copy", () => {
  let p = createProject();
  p = setFile(p, "helpers.ostrin", "pub fn square(x: Int) -> Int {\n    x * x\n}\n");
  const files = filesForRun(p);
  assert.deepEqual(Object.keys(files).sort(), ["helpers.ostrin", "main.ostrin"]);
  files["main.ostrin"] = "changed";
  assert.notEqual(p.files["main.ostrin"], "changed", "the run map is a copy");
});

test("run manifest hashes exact bytes and marks an unknown compiler identity", async () => {
  const p = setFile(createProject("demo"), "helpers.ostrin", "pub fn one() -> Int {\n    1\n}\n");
  const svg = '<svg xmlns="http://www.w3.org/2000/svg"></svg>';
  const manifest = await buildRunManifest({
    project: p,
    entry: "main.ostrin",
    command: "--run",
    exitCode: 0,
    stdout: "49\n",
    stderr: "",
    figureSvg: svg,
    compilerIdentity: "ostrinc 0.2.0",
  });
  assert.equal(manifest.format, "ostrin.run-manifest");
  assert.equal(manifest.run.compiler, "ostrinc 0.2.0");
  assert.equal(manifest.project.sources["main.ostrin"].sha256, await sha256Hex(p.files["main.ostrin"]));
  assert.equal(manifest.outputs.stdout.sha256, await sha256Hex("49\n"));
  assert.equal(manifest.outputs.stderr.sha256, await sha256Hex(""));
  assert.equal(manifest.outputs.figure.sha256, await sha256Hex(svg));
  assert.equal(manifest.claims.reproducibility, "not assessed (no R3 claim)");

  const unknown = await buildRunManifest({ project: p, entry: "main.ostrin", command: "--run", exitCode: 0 });
  assert.equal(unknown.run.compiler, "unknown");
  assert.equal(unknown.run.compilerSource, "not reported");
  assert.equal("figure" in unknown.outputs, false);
});

test("sha256Hex matches the known digest of a fixed string", async () => {
  assert.equal(await sha256Hex("abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
});

test("the manifest hashes the exact captured bytes, including CRLF and the final newline", async () => {
  const project = createProject("bytes");
  const raw = new Uint8Array([49, 52, 57, 13, 10]); // "49\r\n" exactly as captured
  const manifest = await buildRunManifest({
    project,
    entry: "main.ostrin",
    command: "--run main.ostrin",
    exitCode: 0,
    stdout: "49\n",
    stdoutBytes: raw,
    stderr: "",
    stderrBytes: new Uint8Array(0),
    compilerIdentity: "ostrinc 0.2.0",
  });
  assert.equal(manifest.outputs.stdout.sha256, await sha256Hex(raw), "hash is over the raw bytes");
  assert.notEqual(manifest.outputs.stdout.sha256, await sha256Hex("49\n"), "not over the normalized text");
  assert.equal(manifest.outputs.stdout.bytes, 5);
  assert.equal(manifest.outputs.stdout.source, "raw runtime bytes");
  assert.equal(manifest.outputs.stderr.source, "raw runtime bytes");
});
