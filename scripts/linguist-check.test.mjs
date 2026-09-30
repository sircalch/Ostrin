import test from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { repositoryRoot, validateLinguistPreparation } from "./linguist-check.mjs";
import { assertGhReady, buildUsageReport, parseArgs, QUERY, renderHuman } from "./linguist-usage-report.mjs";

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

test("the usage report records only public code-search evidence", () => {
  const report = buildUsageReport({ total_count: 0, incomplete_results: false }, "2026-09-30T00:00:00.000Z");
  assert.equal(report.indexedPublicFiles, 0);
  assert.equal(report.query, QUERY);
  assert.equal(report.internalRepositoryExcluded, true);
  assert.equal(report.readyForUpstreamPullRequest, false);
  assert.match(renderHuman(report), /internal Ostrin repository counted: no/);
  assert.doesNotMatch(renderHuman(report), /examples\//);
});

test("the usage report aborts if GitHub returns the internal repository", () => {
  assert.throws(
    () => buildUsageReport({
      total_count: 1,
      incomplete_results: false,
      items: [{ repository: { full_name: "sircalch/Ostrin" } }],
    }),
    /uso interno como evidencia/,
  );
});

test("the usage report accepts machine-readable output flags", () => {
  assert.deepEqual(parseArgs(["--json", "--output", "snapshot.json"]), {
    help: false,
    json: true,
    output: "snapshot.json",
  });
});

test("the usage report explains missing authentication", () => {
  assert.throws(
    () => assertGhReady({ versionOutput: "gh version 2.0.0", authStatus: 1 }),
    /gh no está autenticado.*gh auth login/,
  );
});
