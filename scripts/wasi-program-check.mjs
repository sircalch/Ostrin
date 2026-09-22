import { WASI } from "node:wasi";
import {
  closeSync,
  existsSync,
  mkdirSync,
  openSync,
  readFileSync,
  rmSync,
} from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const hostCompiler = process.env.OSTRIN_HOST_COMPILER
  || path.join(root, "compiler", "target", "release", `ostrinc${process.platform === "win32" ? ".exe" : ""}`);
const dist = path.join(root, "dist");
const scratch = path.join(root, "target", "wasi-program-check");

const programs = [
  {
    name: "hello",
    source: "examples/hello.ostrin",
    args: ["hello.wasm"],
    expected: "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\nhola desde Ostrin\n",
  },
  {
    name: "pkg_project",
    project: "examples/pkg_project/main_app",
    args: ["pkg_project.wasm"],
    expected: "hola, Ostrin\n",
  },
  {
    name: "wasi_io_contract",
    source: "examples/wasi_io_contract.ostrin",
    args: ["wasi_io_contract.wasm", "alpha", "beta"],
    env: { OSTRIN_WASI_TEST: "wasi-value" },
    expected: "2\nalpha\nbeta\nwasi-value\nfalse\ntrue\ntrue\nwasi file\n",
  },
  {
    name: "native_ir_file_io",
    source: "examples/native_ir_file_io.ostrin",
    args: ["native_ir_file_io.wasm"],
    expected: "true\ntrue\nhello from IR\ntrue\n",
  },
  {
    name: "native_ir_managed_consumers",
    source: "examples/native_ir_managed_consumers.ostrin",
    args: ["native_ir_managed_consumers.wasm"],
    expected: "native option\nnative option\noption fallback\nnative option\noption error\nnative result\nnative result\nresult fallback\ntrue\n",
  },
];

function fail(message) {
  throw new Error(`wasi-program-check: ${message}`);
}

function runCompiler(args, label) {
  if (!existsSync(hostCompiler)) {
    fail(`host compiler is missing at ${hostCompiler}; build compiler/target/release/ostrinc first`);
  }
  const result = spawnSync(hostCompiler, args, {
    cwd: root,
    env: process.env,
    encoding: "utf8",
    stdio: "inherit",
  });
  if (result.error) fail(`${label} could not start: ${result.error.message}`);
  if (result.status !== 0) fail(`${label} exited with ${result.status}`);
}

async function runWasi(program) {
  const stdoutPath = path.join(scratch, `${program.name}.stdout`);
  const stderrPath = path.join(scratch, `${program.name}.stderr`);
  rmSync(stdoutPath, { force: true });
  rmSync(stderrPath, { force: true });
  let stdoutFd;
  let stderrFd;
  try {
    stdoutFd = openSync(stdoutPath, "w");
    stderrFd = openSync(stderrPath, "w");
    const wasi = new WASI({
      version: "preview1",
      args: program.args,
      env: program.env ?? {},
      preopens: { ".": root },
      stdout: stdoutFd,
      stderr: stderrFd,
      returnOnExit: true,
    });
    const module = await WebAssembly.compile(readFileSync(path.join(dist, `${program.name}.wasm`)));
    const instance = await WebAssembly.instantiate(module, wasi.getImportObject());
    const exitCode = wasi.start(instance);
    closeSync(stdoutFd);
    stdoutFd = undefined;
    closeSync(stderrFd);
    stderrFd = undefined;
    const output = readFileSync(stdoutPath, "utf8").replaceAll("\r\n", "\n");
    const error = readFileSync(stderrPath, "utf8").replaceAll("\r\n", "\n");
    if (exitCode !== 0) fail(`${program.name} exited with ${exitCode}: ${error}`);
    if (output !== program.expected) {
      fail(`${program.name} output mismatch\nexpected:\n${program.expected}\nactual:\n${output}\nstderr:\n${error}`);
    }
  } finally {
    if (stdoutFd !== undefined) closeSync(stdoutFd);
    if (stderrFd !== undefined) closeSync(stderrFd);
    rmSync(stdoutPath, { force: true });
    rmSync(stderrPath, { force: true });
  }
}

async function main() {
  mkdirSync(dist, { recursive: true });
  mkdirSync(scratch, { recursive: true });
  rmSync(path.join(root, "target", "ostrin-wasi-contract.txt"), { force: true });
  rmSync(path.join(root, "target", "ostrin-ir-file-io.txt"), { force: true });
  for (const program of programs) {
    const output = path.join(dist, `${program.name}.wasm`);
    rmSync(output, { force: true });
    const args = ["--compile", "--target", "wasm32-wasi", "--out", output];
    if (program.project) {
      args.push("--project", path.join(root, program.project));
    } else {
      args.push(path.join(root, program.source));
    }
    runCompiler(args, `compile ${program.name}`);
    await runWasi(program);
  }
  rmSync(path.join(root, "target", "ostrin-wasi-contract.txt"), { force: true });
  rmSync(path.join(root, "target", "ostrin-ir-file-io.txt"), { force: true });
  console.log(`wasi-program-check: ok (${programs.length} programs, compiler + package + args/env + file I/O + ownership)`);
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
});
