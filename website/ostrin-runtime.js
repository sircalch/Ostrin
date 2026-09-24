// Runs the real Ostrin compiler (ostrinc.wasm, wasm32-wasip1) in the page. The WASI layer comes from
// @bjorn3/browser_wasi_shim; the program's files live in an in-memory preopened directory.
import { WASI, File, Directory, OpenFile, ConsoleStdout, PreopenDirectory } from "https://cdn.jsdelivr.net/npm/@bjorn3/browser_wasi_shim@0.3.0/+esm";

let modulePromise;

export function loadCompiler() {
  modulePromise ??= WebAssembly.compileStreaming(fetch("ostrinc.wasm"));
  return modulePromise;
}

// { "lab/main.ostrin": "...", "plot/plot.ostrin": "..." } -> nested WASI directories.
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

// Runs `ostrinc <args>` over `files`; returns the exit code and [stream, line] pairs.
export async function runOstrinc(files, args) {
  const lines = [];
  const fds = [
    new OpenFile(new File([])),
    ConsoleStdout.lineBuffered((line) => lines.push(["out", line])),
    ConsoleStdout.lineBuffered((line) => lines.push(["err", line])),
    new PreopenDirectory(".", directoryContents(files)),
  ];
  const wasi = new WASI(["ostrinc", ...args], [], fds);
  const instance = await WebAssembly.instantiate(await loadCompiler(), { wasi_snapshot_preview1: wasi.wasiImport });
  let code = 0;
  try {
    code = wasi.start(instance);
  } catch (error) {
    lines.push(["err", `runtime trap: ${error.message ?? error}`]);
    code = 1;
  }
  return { code, lines };
}
