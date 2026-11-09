// Web Worker that runs the real Ostrin compiler (ostrinc.wasm) outside the page's main thread.
//
// The compiled module is kept for the Worker's lifetime, so a Worker that is reused for the next run
// does not compile the compiler again. Every run gets a fresh WASI instance and a fresh in-memory
// file system. stdout and stderr are captured as the exact bytes the compiler wrote; nothing is
// rebuilt from lines.
//
// The page talks to this file only through the messages defined in workbench-protocol.js.

import { WASI, File, Directory, OpenFile, ConsoleStdout, PreopenDirectory } from "https://cdn.jsdelivr.net/npm/@bjorn3/browser_wasi_shim@0.3.0/+esm";
import { validateRunRequest } from "./workbench-protocol.js";

let modulePromise = null;

function compiler() {
  modulePromise ??= WebAssembly.compileStreaming(fetch(new URL("ostrinc.wasm", import.meta.url)));
  return modulePromise;
}

// { "lab/main.ostrin": "..." } -> nested WASI directories, as the main-thread runtime builds them.
function directoryContents(files) {
  const root = new Map();
  for (const [name, text] of Object.entries(files)) {
    const parts = name.split("/");
    let level = root;
    for (const part of parts.slice(0, -1)) {
      if (!level.has(part)) level.set(part, new Directory(new Map()));
      level = level.get(part).contents;
    }
    level.set(parts.at(-1), new File(new TextEncoder().encode(text)));
  }
  return root;
}

function concat(chunks) {
  const total = chunks.reduce((sum, chunk) => sum + chunk.byteLength, 0);
  const out = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) {
    out.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return out;
}

async function execute(id, command, files) {
  const stdout = [];
  const stderr = [];
  // The same chunks in the order the program wrote them, each tagged with its stream.
  const ordered = [];
  const fds = [
    new OpenFile(new File([])),
    new ConsoleStdout((bytes) => {
      const copy = bytes.slice();
      stdout.push(copy);
      ordered.push({ stream: "out", bytes: copy });
    }),
    new ConsoleStdout((bytes) => {
      const copy = bytes.slice();
      stderr.push(copy);
      ordered.push({ stream: "err", bytes: copy });
    }),
    new PreopenDirectory(".", directoryContents(files)),
  ];
  const wasi = new WASI(["ostrinc", ...command], [], fds);
  let code;
  try {
    const instance = await WebAssembly.instantiate(await compiler(), { wasi_snapshot_preview1: wasi.wasiImport });
    self.postMessage({ type: "started", id });
    code = wasi.start(instance);
  } catch (error) {
    self.postMessage({ type: "runtime-error", id, message: String(error?.message ?? error) });
    return;
  }
  const out = concat(stdout);
  const err = concat(stderr);
  const transfer = [out.buffer, err.buffer, ...ordered.map((chunk) => chunk.bytes.buffer)];
  self.postMessage({ type: "result", id, code, stdout: out, stderr: err, ordered }, transfer);
}

self.onmessage = (event) => {
  const message = event.data;
  if (message?.type === "ping") {
    self.postMessage({ type: "ready", id: "init" });
    return;
  }
  const problems = validateRunRequest(message);
  if (problems.length > 0) {
    const id = typeof message?.id === "string" ? message.id : "invalid";
    self.postMessage({ type: "runtime-error", id: /^[A-Za-z0-9_-]{1,64}$/.test(id) ? id : "invalid", message: `invalid request: ${problems.join("; ")}` });
    return;
  }
  execute(message.id, message.command, message.files);
};
