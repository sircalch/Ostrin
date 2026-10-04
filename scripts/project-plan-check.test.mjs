import test from "node:test";
import assert from "node:assert/strict";
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { repositoryRoot, validateProjectPlan } from "./project-plan-check.mjs";

const trackedFiles = [
  "ESTADO_Y_PLAN.md",
  "ROADMAP.md",
  "CHANGELOG.md",
  "compiler/tests/differential.rs",
  "docs/design/20-hir-y-ir.md",
  "docs/linguist.md",
  "docs/linguist-usage.md",
  "docs/website-audit.md",
  "website/viz.html",
];

function fixtureRoot() {
  const root = mkdtempSync(path.join(os.tmpdir(), "ostrin-plan-check-"));
  for (const relativePath of trackedFiles) {
    const target = path.join(root, relativePath);
    mkdirSync(path.dirname(target), { recursive: true });
    copyFileSync(path.join(repositoryRoot, relativePath), target);
  }
  return root;
}

test("the current plan is aligned with compiler ratchets and public maturity labels", () => {
  const result = validateProjectPlan(repositoryRoot);
  assert.deepEqual(result.errors, []);
  assert.deepEqual(result.baseline, {
    minimum: 6523,
    maximumAst: 32,
    total: 6523,
    ast: 32,
    ir: 6509,
    hir: 14,
  });
});

test("the plan gate rejects the stale claim that Quantity plotting is already IR/C", () => {
  const root = fixtureRoot();
  try {
    const file = path.join(root, "compiler/tests/differential.rs");
    const source = readFileSync(file, "utf8");
    writeFileSync(file, source.replace(
      "Generic quantity plotting instantiations",
      "Generic quantity plotting instantiations now lower through IR/C.\n    //",
    ));
    const result = validateProjectPlan(root);
    assert.ok(result.errors.some((error) => error.includes("stale claim")));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("the plan gate rejects a Viz page that hides planned maturity", () => {
  const root = fixtureRoot();
  try {
    const file = path.join(root, "website/viz.html");
    const source = readFileSync(file, "utf8").replace("<h3>Planned</h3>", "<h3>Available</h3>");
    writeFileSync(file, source);
    const result = validateProjectPlan(root);
    assert.ok(result.errors.some((error) => error.includes("maturity labels")));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("the plan gate requires a reproducibility maturity trail", () => {
  const root = fixtureRoot();
  try {
    const file = path.join(root, "docs/website-audit.md");
    const source = readFileSync(file, "utf8").replaceAll("R1", "future replay");
    writeFileSync(file, source);
    const result = validateProjectPlan(root);
    assert.ok(result.errors.some((error) => error.includes("reproducibility maturity")));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
