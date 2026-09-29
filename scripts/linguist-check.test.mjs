import test from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { repositoryRoot, validateLinguistPreparation } from "./linguist-check.mjs";

test("the repository has a complete Linguist preparation", () => {
  const result = validateLinguistPreparation(repositoryRoot);
  assert.deepEqual(result.errors, []);
  assert.equal(result.grammarScope, "source.ostrin");
  assert.ok(result.examples >= result.sampleCandidates);
});

test("the readiness gate rejects a proposal without the extension", () => {
  const root = mkdtempSync(path.join(os.tmpdir(), "ostrin-linguist-"));
  try {
    mkdirSync(path.join(root, "docs"), { recursive: true });
    mkdirSync(path.join(root, "vscode-ostrin", "syntaxes"), { recursive: true });
    mkdirSync(path.join(root, "examples"), { recursive: true });
    writeFileSync(path.join(root, "docs", "linguist-language.yml"), "Ostrin:\n  type: programming\n");
    writeFileSync(path.join(root, "docs", "linguist.md"), "docs/linguist-language.yml");
    writeFileSync(path.join(root, "LICENSE"), "MIT License\n");
    writeFileSync(
      path.join(root, "vscode-ostrin", "syntaxes", "ostrin.tmLanguage.json"),
      JSON.stringify({ scopeName: "source.ostrin", fileTypes: ["ostrin"] }),
    );
    for (const sample of [
      "physics",
      "quantity_arrays",
      "arrays",
      "numeric_methods",
      "native_concurrency",
      "concurrency",
      "native_records",
    ]) {
      writeFileSync(path.join(root, "examples", `${sample}.ostrin`), "");
    }
    const result = validateLinguistPreparation(root);
    assert.ok(result.errors.some((error) => error.includes("missing .ostrin extension")));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
