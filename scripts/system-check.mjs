// Re-runs scripts/system-oracle.mjs twice and checks it against the committed fixture.
//
// Usage:
//   node scripts/system-check.mjs [fixture-path]
//
// The default fixture is compiler/tests/fixtures/solve_systems_oracle.txt. An explicit path
// replaces it (used to confirm that a corrupted copy fails).
//
// Exits 0 when two oracle runs are identical and both equal the fixture.
// Exits 1 otherwise, after printing the first differing line. CRLF line endings are tolerated
// in the fixture and in the oracle output. No external packages.

import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const oraclePath = join(here, "system-oracle.mjs");
const defaultFixture = join(here, "..", "compiler", "tests", "fixtures", "solve_systems_oracle.txt");
const fixturePath = process.argv[2] ? resolve(process.argv[2]) : defaultFixture;

const normalize = (text) => text.replace(/\r\n/g, "\n");

function runOracle() {
  return normalize(
    execFileSync(process.execPath, [oraclePath], { encoding: "utf8", maxBuffer: 1 << 24, stdio: ["ignore", "pipe", "pipe"] }),
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

let first;
let second;
try {
  first = runOracle();
  second = runOracle();
} catch (error) {
  console.error(`system-check: oracle failed to run: ${error.message}`);
  process.exit(1);
}

let fixture;
try {
  fixture = normalize(readFileSync(fixturePath, "utf8"));
} catch (error) {
  console.error(`system-check: cannot read fixture ${fixturePath}: ${error.message}`);
  process.exit(1);
}

let failed = false;

const runDiff = firstDifference(first, second);
if (runDiff) {
  failed = true;
  console.error(`system-check: two oracle runs differ at line ${runDiff.line}`);
  console.error(`  run 1: ${runDiff.actual}`);
  console.error(`  run 2: ${runDiff.expected}`);
}

const fixtureDiff = firstDifference(first, fixture);
if (fixtureDiff) {
  failed = true;
  console.error(`system-check: oracle output differs from ${fixturePath} at line ${fixtureDiff.line}`);
  console.error(`  oracle:  ${fixtureDiff.actual}`);
  console.error(`  fixture: ${fixtureDiff.expected}`);
}

if (failed) {
  process.exit(1);
}

const caseLines = first.split("\n").filter((line) => line !== "").length;
console.log(`system-check: ok (${caseLines} cases, two runs identical, fixture matches)`);
