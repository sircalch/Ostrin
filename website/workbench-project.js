// Ostrin Workbench project model: a versioned, portable, multi-file Ostrin project.
//
// Pure data code (no DOM, no network) so that it can be tested in Node and reused by the page.
// Format `ostrin.project` version 1 is a JSON document:
//   { "format": "ostrin.project", "version": 1, "name": string, "entry": "main.ostrin",
//     "files": { "<relative path>.ostrin": "<source text>", ... } }
// Paths are relative, use "/", end in ".ostrin", and may not contain "..", a leading "/", or a
// backslash. The entry file must exist in `files`.

export const PROJECT_FORMAT = "ostrin.project";
export const PROJECT_VERSION = 1;
export const LIMITS = Object.freeze({
  maxFiles: 32,
  maxFileBytes: 256 * 1024,
  maxProjectBytes: 2 * 1024 * 1024,
  maxNameLength: 80,
});

const PATH_PATTERN = /^[A-Za-z0-9_][A-Za-z0-9_-]*(\/[A-Za-z0-9_][A-Za-z0-9_-]*)*\.ostrin$/;
const encoder = new TextEncoder();

export function byteLength(text) {
  return encoder.encode(text).length;
}

export function validPath(path) {
  return typeof path === "string" && path.length <= 160 && PATH_PATTERN.test(path) && !path.includes("..");
}

export function createProject(name = "untitled") {
  const files = { "main.ostrin": "fn main() -> Void {\n    print(\"hello\")\n}\n" };
  return { format: PROJECT_FORMAT, version: PROJECT_VERSION, name: cleanName(name), entry: "main.ostrin", files };
}

export function cleanName(name) {
  const trimmed = String(name ?? "").trim().slice(0, LIMITS.maxNameLength);
  return trimmed === "" ? "untitled" : trimmed;
}

// Returns a list of problems; an empty list means the project is valid.
export function validateProject(project) {
  const problems = [];
  if (!project || typeof project !== "object") return ["project is not an object"];
  if (project.format !== PROJECT_FORMAT) problems.push(`format must be "${PROJECT_FORMAT}"`);
  if (project.version !== PROJECT_VERSION) {
    problems.push(`unsupported project version ${JSON.stringify(project.version)} (this build reads version ${PROJECT_VERSION})`);
  }
  if (typeof project.name !== "string" || project.name.trim() === "") problems.push("name is required");
  if (!project.files || typeof project.files !== "object" || Array.isArray(project.files)) {
    problems.push("files must be an object");
    return problems;
  }
  const paths = Object.keys(project.files);
  if (paths.length === 0) problems.push("project has no files");
  if (paths.length > LIMITS.maxFiles) problems.push(`project has ${paths.length} files; the limit is ${LIMITS.maxFiles}`);
  let total = 0;
  for (const path of paths) {
    if (!validPath(path)) {
      problems.push(`invalid file path ${JSON.stringify(path)}`);
      continue;
    }
    const text = project.files[path];
    if (typeof text !== "string") {
      problems.push(`${path} is not text`);
      continue;
    }
    const bytes = byteLength(text);
    total += bytes;
    if (bytes > LIMITS.maxFileBytes) problems.push(`${path} is ${bytes} bytes; the limit is ${LIMITS.maxFileBytes}`);
  }
  if (total > LIMITS.maxProjectBytes) problems.push(`project is ${total} bytes; the limit is ${LIMITS.maxProjectBytes}`);
  if (!paths.includes(project.entry)) problems.push(`entry ${JSON.stringify(project.entry)} is not a file of the project`);
  return problems;
}

// Parses project text. Throws an Error listing the problems when the document is not a valid project.
export function parseProject(text) {
  if (byteLength(text) > LIMITS.maxProjectBytes) {
    throw new Error(`project file is larger than ${LIMITS.maxProjectBytes} bytes`);
  }
  let project;
  try {
    project = JSON.parse(text);
  } catch (error) {
    throw new Error(`project file is not valid JSON: ${error.message}`);
  }
  const problems = validateProject(project);
  if (problems.length > 0) throw new Error(`invalid project: ${problems.join("; ")}`);
  return project;
}

// Stable serialization: files are written in sorted path order so equal projects give equal bytes.
export function serializeProject(project) {
  const problems = validateProject(project);
  if (problems.length > 0) throw new Error(`cannot save an invalid project: ${problems.join("; ")}`);
  const files = {};
  for (const path of Object.keys(project.files).sort()) files[path] = project.files[path];
  return JSON.stringify(
    { format: PROJECT_FORMAT, version: PROJECT_VERSION, name: cleanName(project.name), entry: project.entry, files },
    null,
    2,
  ) + "\n";
}

// The file map the compiler runtime receives. The entry is passed as the program to run.
export function filesForRun(project) {
  return { ...project.files };
}

export function setFile(project, path, text) {
  if (!validPath(path)) throw new Error(`invalid file path ${JSON.stringify(path)}`);
  return { ...project, files: { ...project.files, [path]: text } };
}

export function renameFile(project, from, to) {
  if (!(from in project.files)) throw new Error(`no file ${from}`);
  if (!validPath(to)) throw new Error(`invalid file path ${JSON.stringify(to)}`);
  if (to in project.files && to !== from) throw new Error(`${to} already exists`);
  const files = {};
  for (const [path, text] of Object.entries(project.files)) files[path === from ? to : path] = text;
  const entry = project.entry === from ? to : project.entry;
  return { ...project, files, entry };
}

export function removeFile(project, path) {
  if (path === project.entry) throw new Error("the entry file cannot be removed; change the entry first");
  const files = { ...project.files };
  delete files[path];
  return { ...project, files };
}

// SHA-256 of a UTF-8 string as lowercase hex. Uses Web Crypto, present in browsers and Node 18+.
export async function sha256Hex(text) {
  const digest = await globalThis.crypto.subtle.digest("SHA-256", encoder.encode(text));
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

// Manifest linking a run's inputs, compiler and outputs. Hashes are over the exact bytes saved.
export async function buildManifest({ project, compilerVersion, command, exitCode, figureSvg = null, outputText = "" }) {
  const sources = {};
  for (const path of Object.keys(project.files).sort()) {
    sources[path] = { sha256: await sha256Hex(project.files[path]), bytes: byteLength(project.files[path]) };
  }
  const manifest = {
    format: "ostrin.run-manifest",
    version: 1,
    project: { name: cleanName(project.name), entry: project.entry, sources },
    run: { command, exitCode, compiler: compilerVersion ?? "unknown" },
    output: { sha256: await sha256Hex(outputText), bytes: byteLength(outputText) },
  };
  if (figureSvg !== null) {
    manifest.figure = { sha256: await sha256Hex(figureSvg), bytes: byteLength(figureSvg) };
  }
  return manifest;
}
