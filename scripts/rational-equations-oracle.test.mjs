import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { buildCases, expectedOutputs } from "./rational-equations-oracle.mjs";

const here = dirname(fileURLToPath(import.meta.url));

test("rational equation oracle cases produce one line per case", () => {
  const cases = buildCases();
  const lines = expectedOutputs(cases);
  assert.equal(cases.length, 71);
  assert.equal(lines.length, 71);
  assert.ok(lines.some((l) => l.includes("status=identity")));
  assert.ok(lines.some((l) => l.includes("status=partial")));
});

test("rational equation oracle files are current", () => {
  execFileSync(process.execPath, [join(here, "rational-equations-oracle.mjs"), "--check"], { stdio: "pipe" });
});
