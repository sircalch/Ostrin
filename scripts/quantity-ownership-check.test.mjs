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
  "compiler/src/ir_c.rs",
  "compiler/src/qty_runtime.c",
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

test("current Quantity phase 1 and plot phase 2 evidence passes", () => {
  assert.deepEqual(validateQuantityOwnership(repositoryRoot).errors, []);
});

test("fails when the viz_units IR ownership baseline regresses", () => {
  const root = fixtureRoot();
  try {
    update(root, "compiler/tests/examples.rs", (source) => source.replace(
      "native-source: examples/viz_units.ostrin ir=1 hir=0 ast=0",
      "native-source: examples/viz_units.ostrin ir=0 hir=0 ast=1",
    ));
    const { errors } = validateQuantityOwnership(root);
    assert.ok(errors.length > 0, "the gate accepted the AST fallback regression");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("fails when design 27 loses its phase 1 status", () => {
  const root = fixtureRoot();
  try {
    update(root, "docs/design/27-ownership-de-quantity-escalar.md", (source) => source.replace("fase 1 implementada y validada", "diseño técnico para revisión"));
    const { errors } = validateQuantityOwnership(root);
    assert.ok(errors.some((error) => error.includes("design 27")));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("fails when Quantity enters managed ownership without typed helpers", () => {
  const root = fixtureRoot();
  try {
    update(root, "compiler/src/qty_runtime.c", (source) => source.replaceAll("ostrin_qty_retain", "removed_qty_retain"));
    const { errors } = validateQuantityOwnership(root);
    assert.ok(errors.some((error) => error.includes("typed Quantity helper marker")));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
