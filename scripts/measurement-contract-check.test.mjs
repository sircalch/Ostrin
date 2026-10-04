import test from "node:test";
import assert from "node:assert/strict";
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { repositoryRoot, validateMeasurementContract } from "./measurement-contract-check.mjs";

const trackedFiles = [
  "compiler/std/measurements.ostrin",
  "examples/measurement_scalar.ostrin",
  "compiler/tests/examples.rs",
  "docs/design/25-mediciones-e-incertidumbre.md",
  "scripts/lab-data.mjs",
];

function fixtureRoot() {
  const root = mkdtempSync(path.join(os.tmpdir(), "ostrin-measurement-check-"));
  for (const relativePath of trackedFiles) {
    const target = path.join(root, relativePath);
    mkdirSync(path.dirname(target), { recursive: true });
    copyFileSync(path.join(repositoryRoot, relativePath), target);
  }
  return root;
}

test("the scalar Measurement contract is aligned with source and parity evidence", () => {
  assert.deepEqual(validateMeasurementContract(repositoryRoot).errors, []);
});

test("the gate rejects an example that silently drops unknown uncertainty", () => {
  const root = fixtureRoot();
  try {
    const file = path.join(root, "examples/measurement_scalar.ostrin");
    const source = readFileSync(file, "utf8").replace("measurements.uncertainties", "measurements.values");
    writeFileSync(file, source);
    const result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("uncertainties")));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("the gate rejects documentation that enables arrays prematurely", () => {
  const root = fixtureRoot();
  try {
    const file = path.join(root, "docs/design/25-mediciones-e-incertidumbre.md");
    const source = readFileSync(file, "utf8")
      .replaceAll("no habilita todavía", "habilita")
      .replaceAll("siguen pendientes", "ya están habilitados");
    writeFileSync(file, source);
    const result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("array Measurement boundary")));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});


