import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { buildCases, expectedOutputs } from "./rational-oracle.mjs";

const here = dirname(fileURLToPath(import.meta.url));

test("rational oracle cases produce the expected number of points", () => {
  const cases = buildCases();
  const lines = expectedOutputs(cases);
  assert.equal(cases.length, 66);
  assert.equal(lines.length, 528);
  assert.equal(lines.filter((line) => line.endsWith("undefined")).length, 92);
});

test("rational oracle files are current", () => {
  execFileSync(process.execPath, [join(here, "rational-oracle.mjs"), "--check"], { stdio: "pipe" });
});
