import { WASI } from "node:wasi";
import { spawnSync, execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { closeSync, mkdirSync, mkdtempSync, openSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { parseAllocationCounters } from "./benchmark-allocation-profile.mjs";
import { assertBenchmarkWorkloadSucceeded } from "./benchmark-runtime-inputs.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const output = path.join(root, "target", "pendulum-wasi-check");
const sourcePath = "examples/viz_pendulum_calibration.ostrin";
const inputPath = "experiments/pendulum-calibration.inputs.json";
const compiler = path.resolve(root, process.env.OSTRIN_HOST_COMPILER
  || `compiler/target/release/ostrinc${process.platform === "win32" ? ".exe" : ""}`);
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");
const normalize = (text) => text.replaceAll("\r\n", "\n");

function run(args, cwd = root) {
  const result = spawnSync(compiler, args, { cwd, encoding: "utf8", timeout: 180000, maxBuffer: 16 * 1024 * 1024 });
  if (result.error || result.status !== 0) {
    throw new Error(`pendulum compiler ${args[0]} failed: ${result.error?.message || result.stderr}`);
  }
  return result.stdout;
}

async function main() {
  mkdirSync(output, { recursive: true });
  // Remove any earlier receipt before validation so a failed run cannot leave
  // an apparently current successful report for the workflow to archive.
  const reportPath = path.join(output, "report.json");
  const wasmPath = path.join(output, "pendulum.wasm");
  rmSync(reportPath, { force: true });
  rmSync(wasmPath, { force: true });
  const cc = process.env.OSTRIN_WASI_CC?.trim();
  const sysroot = process.env.OSTRIN_WASI_SYSROOT?.trim();
  if (!cc || !sysroot) throw new Error("configure OSTRIN_WASI_CC and OSTRIN_WASI_SYSROOT with the pinned WASI SDK");
  const ccVersion = spawnSync(cc, ["--version"], { encoding: "utf8", timeout: 10000 });
  if (ccVersion.error || ccVersion.status !== 0) throw new Error("could not identify the configured WASI C compiler");
  const ccHash = sha256(readFileSync(cc));
  const sourceBytes = readFileSync(path.join(root, sourcePath));
  const inputBytes = readFileSync(path.join(root, inputPath));
  const compilerHash = sha256(readFileSync(compiler));
  const sourceRevision = execFileSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8" }).trim();
  const gitClean = !execFileSync("git", ["status", "--porcelain"], { cwd: root, encoding: "utf8" }).trim();
  const compileArgs = ["--compile", "--target", "wasm32-wasi", "--leak-check", "--out", wasmPath, path.join(root, sourcePath)];
  run(compileArgs);
  const module = await WebAssembly.compile(readFileSync(wasmPath));
  const samples = [];
  for (let index = 0; index < 3; index += 1) {
    const cwd = mkdtempSync(path.join(output, "input-"));
    let stdoutFd;
    let stderrFd;
    try {
      const stagedInput = path.join(cwd, "data.json");
      writeFileSync(stagedInput, inputBytes);
      const expected = normalize(run(["--run", path.join(root, sourcePath)], cwd));
      assertBenchmarkWorkloadSucceeded(expected, "", "pendulum interpreter");
      if (!expected.includes("<svg") || !expected.includes("linearized standard uncertainty")) {
        throw new Error("pendulum interpreter did not produce the full scientific report and SVG");
      }
      const stdoutPath = path.join(cwd, "stdout");
      const stderrPath = path.join(cwd, "stderr");
      stdoutFd = openSync(stdoutPath, "w");
      stderrFd = openSync(stderrPath, "w");
      const wasi = new WASI({
        version: "preview1", args: ["pendulum.wasm"], env: {},
        preopens: { ".": cwd }, stdout: stdoutFd, stderr: stderrFd, returnOnExit: true,
      });
      const instance = await WebAssembly.instantiate(module, wasi.getImportObject());
      const exitCode = wasi.start(instance);
      closeSync(stdoutFd); stdoutFd = undefined;
      closeSync(stderrFd); stderrFd = undefined;
      const actual = normalize(readFileSync(stdoutPath, "utf8"));
      const stderr = normalize(readFileSync(stderrPath, "utf8"));
      if (exitCode !== 0) throw new Error(`pendulum WASI exited with ${exitCode}: ${stderr}`);
      assertBenchmarkWorkloadSucceeded(actual, stderr, "pendulum WASI");
      if (actual !== expected) throw new Error("compiled WASI report/SVG differs from interpreter output");
      if (sha256(readFileSync(stagedInput)) !== sha256(inputBytes)) throw new Error("pendulum input bytes changed during execution");
      if (samples.length && sha256(actual) !== samples[0].outputSha256) throw new Error("pendulum output changed across repeated runs");
      samples.push({ outputSha256: sha256(actual), outputBytes: Buffer.byteLength(actual), allocations: parseAllocationCounters(stderr) });
    } finally {
      if (stdoutFd !== undefined) closeSync(stdoutFd);
      if (stderrFd !== undefined) closeSync(stderrFd);
      rmSync(cwd, { recursive: true, force: true });
    }
  }
  if (sha256(readFileSync(compiler)) !== compilerHash
    || sha256(readFileSync(path.join(root, sourcePath))) !== sha256(sourceBytes)
    || sha256(readFileSync(path.join(root, inputPath))) !== sha256(inputBytes)
    || sha256(readFileSync(cc)) !== ccHash) {
    throw new Error("compiler/source/input identity changed during the check");
  }
  const report = {
    schema: "ostrin.pendulum-wasi/v1", status: "pass", sourceRevision, gitClean,
    compilerSha256: compilerHash, compilerVersion: run(["--version"]).trim(),
    source: { path: sourcePath, sha256: sha256(sourceBytes) },
    input: { path: inputPath, stagedAs: "data.json", sha256: sha256(inputBytes) },
    wasmSha256: sha256(readFileSync(wasmPath)),
    compilerArguments: ["--compile", "--target", "wasm32-wasi", "--leak-check"],
    toolchain: {
      cc, ccSha256: ccHash, ccVersion: ccVersion.stdout.trim(), sysroot,
      sdkArchiveSha256: process.env.WASI_SDK_SHA256 || null,
      cflags: process.env.OSTRIN_CFLAGS || null,
    },
    runner: { node: process.version, wasi: "preview1", host: `${process.platform}/${process.arch}` },
    outputCheck: "exact UTF-8 report and SVG parity with interpreter after CRLF normalization",
    scope: "One synthetic workflow; no general WASI coverage, timing equivalence or global R3 claim.",
    samples,
  };
  writeFileSync(reportPath, JSON.stringify(report, null, 2) + "\n");
  console.log(`pendulum-wasi-check: ok (${samples.length} compiled WASI runs, full report/SVG parity, zero live managed allocations)`);
}

main().catch((error) => { console.error(`pendulum-wasi-check: ${error.message}`); process.exitCode = 1; });
