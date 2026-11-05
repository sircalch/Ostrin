// Re-runs scripts/solve-oracle.mjs and checks it against the committed fixture.
//
// Usage:  node scripts/solve-check.mjs
//
// Exits 0 when two oracle runs are identical and both equal tests/fixtures/solve_core_oracle.txt.
// Exits 1 otherwise, after printing the first differing line. No external packages.

import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const oraclePath = join(here, "solve-oracle.mjs");
const fixturePath = join(here, "..", "tests", "fixtures", "solve_core_oracle.txt");

const normalize = (text) => text.replace(/\r\n/g, "\n");

function runOracle() {
  return normalize(
    execFileSync(process.execPath, [oraclePath], { encoding: "utf8", maxBuffer: 1 << 24 }),
  );
}

function firstDifference(a, b) {
  const x = a.split("\n");
  const y = b.split("\n");
  const limit = Math.max(x.length, y.length);
  for (let i = 0; i < limit; i++) {
    if (x[i] !== y[i]) {
      return { line: i + 1, actual: x[i] ?? "<missing>", expected: y[i] ?? "<missing>" };
    }
  }
  return null;
}

const first = runOracle();
const second = runOracle();
const fixture = normalize(readFileSync(fixturePath, "utf8"));

let failed = false;

const runDiff = firstDifference(first, second);
if (runDiff) {
  failed = true;
  console.error(`solve-check: two oracle runs differ at line ${runDiff.line}`);
  console.error(`  run 1: ${runDiff.actual}`);
  console.error(`  run 2: ${runDiff.expected}`);
}

const fixtureDiff = firstDifference(first, fixture);
if (fixtureDiff) {
  failed = true;
  console.error(`solve-check: oracle output differs from tests/fixtures/solve_core_oracle.txt at line ${fixtureDiff.line}`);
  console.error(`  oracle:  ${fixtureDiff.actual}`);
  console.error(`  fixture: ${fixtureDiff.expected}`);
}

if (failed) {
  process.exit(1);
}

const caseLines = first.split("\n").filter((line) => line !== "" && !line.startsWith("#")).length;
console.log(`solve-check: ok (${caseLines} cases, two runs identical, fixture matches)`);
