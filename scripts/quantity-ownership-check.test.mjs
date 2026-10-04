import assert from "node:assert/strict";
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import { repositoryRoot, validateQuantityOwnership } from "./quantity-ownership-check.mjs";

const requiredFiles = [
  "docs/design/27-ownership-de-quantity-escalar.md",
  "compiler/tests/examples.rs",
  "compiler/tests/differential.rs",
  "compiler/src/ownership.rs",
  "ESTADO_Y_PLAN.md",
  "ROADMAP.md",
];

function fixtureRoot() {
  const root = mkdtempSync(path.join(os.tmpdir(), "ostrin-quantity-ownership-"));
  for (const relativePath of requiredFiles) {
    const destination = path.join(root, relativePath);
    mkdirSync(path.dirname(destination), { recursive: true });
    copyFileSync(path.join(repositoryRoot, relativePath), destination);
  }
  return root;
}

function update(root, relativePath, change) {
  const file = path.join(root, relativePath);
  writeFileSync(file, change(readFileSync(file, "utf8")), "utf8");
}

test("current Quantity ownership boundary passes", () => {
  assert.deepEqual(validateQuantityOwnership(repositoryRoot).errors, []);
});

test("fails when the viz_units fallback baseline is removed", () => {
  const root = fixtureRoot();
  try {
    update(root, "compiler/tests/examples.rs", (source) => source.replace("ir=0 hir=0 ast=1", "ir=1 hir=0 ast=0"));
    const { errors } = validateQuantityOwnership(root);
    assert.ok(errors.some((error) => error.includes("fallback regression")));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("fails when design 27 is marked as enabled before its ownership evidence", () => {
  const root = fixtureRoot();
  try {
    update(root, "docs/design/27-ownership-de-quantity-escalar.md", (source) => source.replace("No habilita todavía", "Habilita"));
    const { errors } = validateQuantityOwnership(root);
    assert.ok(errors.some((error) => error.includes("design 27")));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("fails when Quantity enters managed ownership without the migration matrix", () => {
  const root = fixtureRoot();
  try {
    update(root, "compiler/src/ownership.rs", (source) => source.replace(/Ty::Quantity\(_\)\s*\|\s*Ty::Int/, "Ty::Quantity(_) => true,\n        Ty::Int"));
    const { errors } = validateQuantityOwnership(root);
    assert.ok(errors.some((error) => error.includes("Ty::Quantity")));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
