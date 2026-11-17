#!/usr/bin/env node
// Runs the linear-algebra examples through the real Ostrin compiler compiled to WebAssembly (website/ostrinc.wasm, WASI
// preview 1), as the browser does, and checks every output against its reference:
//   - examples/linalg_oracle.ostrin against tests/oracle/linalg-expected.txt (exact-rational references);
//   - examples/linalg_properties.ostrin against tests/oracle/linalg-properties-expected.txt (property references);
//   - examples/linalg_regression.ostrin and examples/linalg_units.ostrin against their documented output;
//   - two diagnostics: a type error (compiler message) and a runtime dimension error in a matrix product.
// Nothing here is simulated: the compiler is the WebAssembly module, executed by Node's WASI implementation.
// Usage: node scripts/linalg-wasm-check.mjs [--out docs/linalg-wasm-evidence.json]

import { WASI } from "node:wasi";
import { mkdtempSync, openSync, closeSync, readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const argv = process.argv.slice(2);
const outIndex = argv.indexOf("--out");
const outPath = outIndex >= 0 ? argv[outIndex + 1] : null;
const normalize = (text) => text.replace(/\r\n/g, "\n");

// ---------------------------------------------------------------- the WASM compiler

const wasmModule = await WebAssembly.compile(readFileSync(join(root, "website", "ostrinc.wasm")));

// Runs `ostrinc <args>` with the repository mounted as the working directory. Returns the exit status and both streams.
async function runWasm(args) {
  const work = mkdtempSync(join(tmpdir(), "linalg-wasm-"));
  const outFd = openSync(join(work, "stdout.txt"), "w");
  const errFd = openSync(join(work, "stderr.txt"), "w");
  const wasi = new WASI({ version: "preview1", args: ["ostrinc", ...args], env: {}, stdin: 0, stdout: outFd, stderr: errFd, preopens: { ".": root } });
  const instance = await WebAssembly.instantiate(wasmModule, { wasi_snapshot_preview1: wasi.wasiImport });
  let status;
  try {
    status = wasi.start(instance);
  } catch (error) {
    status = `trap: ${error.message}`;
  }
  closeSync(outFd);
  closeSync(errFd);
  const result = {
    status,
    stdout: normalize(readFileSync(join(work, "stdout.txt"), "utf8")),
    stderr: normalize(readFileSync(join(work, "stderr.txt"), "utf8")),
  };
  return result;
}

// ---------------------------------------------------------------- token rules (the oracle header)

const oracleTokenMatches = (expected, actual) => {
  if (expected.startsWith("<=")) {
    const bound = Number(expected.slice(2));
    const x = Number(actual);
    return actual !== "" && Number.isFinite(x) && x <= bound;
  }
  if (expected.includes("|")) {
    const [left, right] = expected.split("|");
    const value = Number(left);
    const tolerance = Number(right);
    if (left !== "" && right !== "" && Number.isFinite(value) && Number.isFinite(tolerance)) {
      const x = Number(actual);
      return actual !== "" && Number.isFinite(x) && Math.abs(x - value) <= tolerance;
    }
    return actual === left || actual === right;
  }
  return expected === actual;
};

function compareLines(actualText, expectedText) {
  const actual = actualText.split("\n").filter((line) => line !== "");
  const expected = expectedText.split("\n").filter((line) => line !== "");
  if (actual.length !== expected.length) {
    return { ok: false, lines: expected.length, failure: `line count ${actual.length} vs ${expected.length}` };
  }
  for (let index = 0; index < expected.length; index += 1) {
    const got = actual[index].split(" ");
    const want = expected[index].split(" ");
    if (got.length !== want.length) return { ok: false, lines: expected.length, failure: `line ${index}: token count` };
    for (let t = 0; t < want.length; t += 1) {
      if (!oracleTokenMatches(want[t], got[t])) {
        return { ok: false, lines: expected.length, failure: `line ${index}: token ${JSON.stringify(got[t])} does not match ${JSON.stringify(want[t])}` };
      }
    }
  }
  return { ok: true, lines: expected.length };
}

// ---------------------------------------------------------------- checks

const checks = [];
const record = (name, ok, detail) => checks.push({ name, ok, detail });

for (const example of ["linalg_oracle", "linalg_properties"]) {
  const result = await runWasm(["--run", `examples/${example}.ostrin`]);
  const expectedFile = example === "linalg_oracle" ? "tests/oracle/linalg-expected.txt" : "tests/oracle/linalg-properties-expected.txt";
  const expected = normalize(readFileSync(join(root, expectedFile), "utf8"));
  const comparison = result.status === 0 ? compareLines(result.stdout, expected) : { ok: false, failure: `exit ${result.status}: ${result.stderr.slice(0, 200)}` };
  record(`wasm ${example} against ${expectedFile}`, comparison.ok, comparison.ok ? `${comparison.lines} lines match` : comparison.failure);
}

{
  const expected = "c0 1\nc1 2\nc2 0.5\nstatus ok\nresidual below 1e-9: true\n";
  const result = await runWasm(["--run", "examples/linalg_regression.ostrin"]);
  record("wasm linalg_regression exact output", result.status === 0 && result.stdout === expected, result.status === 0 ? result.stdout.split("\n")[0] : result.stderr.slice(0, 200));
}
{
  const expected = "work: 10 J\n[2, 2] N\nerror: unit_dot needs vectors of the same length\n";
  const result = await runWasm(["--run", "examples/linalg_units.ostrin"]);
  record("wasm linalg_units exact output (quantities and units)", result.status === 0 && result.stdout === expected, result.status === 0 ? "matches" : result.stderr.slice(0, 200));
}

// A type error is reported by the WASM compiler with its diagnostic code and a non-zero status.
{
  const dir = join(root, "target", "linalg-wasm-check");
  mkdirSync(dir, { recursive: true });
  const bad = join(dir, "type_error.ostrin");
  writeFileSync(bad, "import std.linalg\n\nfn main() -> Void {\n    a = array([[1.0, 2.0], [3.0, 4.0]])\n    print(linalg.solve_checked(a, 5))\n}\n");
  const result = await runWasm(["--run", "target/linalg-wasm-check/type_error.ostrin"]);
  const diagnostic = /OSTRIN-E\d+/.test(result.stdout + result.stderr);
  record("wasm diagnostic: type error reported with an OSTRIN code", result.status !== 0 && diagnostic, (result.stdout + result.stderr).split("\n").find((l) => /OSTRIN-E/.test(l)) ?? "no diagnostic");
}

// A dimension error in a matrix product stops the run with the runtime message (the product is not a Result).
{
  const dir = join(root, "target", "linalg-wasm-check");
  const bad = join(dir, "dimension_error.ostrin");
  writeFileSync(bad, "fn main() -> Void {\n    a = array([[1.0, 2.0], [3.0, 4.0]])\n    w = array([1.0, 2.0, 3.0])\n    print(a @ w)\n}\n");
  const result = await runWasm(["--run", "target/linalg-wasm-check/dimension_error.ostrin"]);
  const message = (result.stdout + result.stderr).includes("matmul needs");
  record("wasm runtime dimension error in a matrix product", result.status !== 0 && message, (result.stderr || result.stdout).split("\n")[0]);
}

// ---------------------------------------------------------------- report

const failed = checks.filter((c) => !c.ok);
const report = {
  compiler: "website/ostrinc.wasm (wasm32-wasip1, executed by node:wasi)",
  sourceRevision: execFileSync("git", ["rev-parse", "--short", "HEAD"], { cwd: root, encoding: "utf8" }).trim(),
  node: process.version,
  checks,
  passed: checks.length - failed.length,
  total: checks.length,
};
if (outPath) writeFileSync(join(root, outPath), JSON.stringify(report, null, 2) + "\n");
for (const c of checks) console.log(`${c.ok ? "PASS" : "FAIL"} ${c.name}: ${c.detail}`);
console.log(`${report.passed} of ${report.total} WASM checks passed`);
process.exitCode = failed.length ? 1 : 0;
