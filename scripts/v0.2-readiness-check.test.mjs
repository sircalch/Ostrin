import test from "node:test";
import assert from "node:assert/strict";
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import {
  readinessPath,
  repositoryRoot,
  requiredCriteria,
  validateV02Readiness,
} from "./v0.2-readiness-check.mjs";

function readyFixture() {
  const root = mkdtempSync(path.join(os.tmpdir(), "ostrin-v02-gate-"));
  const sourcePath = path.join(root, "compiler/tests/differential.rs");
  mkdirSync(path.dirname(sourcePath), { recursive: true });
  const differential = readFileSync(path.join(repositoryRoot, "compiler/tests/differential.rs"), "utf8")
    ;
  writeFileSync(sourcePath, differential);

  const manifest = JSON.parse(readFileSync(path.join(repositoryRoot, readinessPath), "utf8"));
  manifest.status = "READY";
  for (const criterion of manifest.criteria) {
    criterion.status = "READY";
    if (criterion.requires_ci_run_url) {
      criterion.evidence.push({
        url: "https://github.com/sircalch/Ostrin/actions/runs/123456789",
        claim: "fixture candidate workflow run",
      });
    }
    for (const item of criterion.evidence) {
      if (item.url) continue;
      if (item.path === "compiler/tests/differential.rs") continue;
      const evidencePath = path.join(root, item.path);
      mkdirSync(path.dirname(evidencePath), { recursive: true });
      writeFileSync(evidencePath, "fixture evidence\n");
    }
  }
  for (const file of [
    "validation/release-candidate-v0.2.0.json",
    "validation/quality-scorecard.json",
    "website/status-data.js",
  ]) {
    const target = path.join(root, file);
    mkdirSync(path.dirname(target), { recursive: true });
    copyFileSync(path.join(repositoryRoot, file), target);
  }
  const manifestFile = path.join(root, readinessPath);
  mkdirSync(path.dirname(manifestFile), { recursive: true });
  writeFileSync(manifestFile, JSON.stringify(manifest, null, 2));
  return { root, manifest };
}

test("the current v0.2 gate is ready with candidate release artifact evidence", () => {
  const result = validateV02Readiness(repositoryRoot);
  assert.equal(result.ready, true, result.errors.join("\n"));
  assert.equal(result.errors.length, 0);
});

test("published release instructions support macOS checksums and do not claim it is unpublished", () => {
  const publication = readFileSync(path.join(repositoryRoot, "docs/releases/v0.2.0-publication.md"), "utf8");
  const reviewMap = readFileSync(path.join(repositoryRoot, "docs/review-map-v0.2.md"), "utf8");
  assert.match(publication, /command -v sha256sum[\s\S]*?command -v shasum[\s\S]*?shasum -a 256 --check/);
  assert.match(reviewMap, /Release `v0\.2\.0` is now public/);
  assert.doesNotMatch(reviewMap, /unpublished and untagged pending explicit owner authorization/i);
  assert.doesNotMatch(reviewMap, /draft integration PR/i);
});

test("the gate rejects READY claims without evidence files", () => {
  const { root, manifest } = readyFixture();
  try {
    const criterion = manifest.criteria.find((item) => item.id === "ownership");
    criterion.evidence[0].path = "docs/missing-ownership-evidence.md";
    writeFileSync(path.join(root, readinessPath), JSON.stringify(manifest, null, 2));
    const result = validateV02Readiness(root);
    assert.ok(result.errors.some((error) => error.includes("evidence file does not exist")));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("the gate requires real repository Actions run links for CI-backed criteria", () => {
  const { root, manifest } = readyFixture();
  try {
    const criterion = manifest.criteria.find((item) => item.id === "reproducibility-r1");
    criterion.evidence = criterion.evidence.filter((item) => !item.url);
    writeFileSync(path.join(root, readinessPath), JSON.stringify(manifest, null, 2));
    const result = validateV02Readiness(root);
    assert.ok(result.errors.some((error) => error.includes("successful GitHub Actions run URL")));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("the gate rejects contradictory source, evidence and release-tag revisions", () => {
  const { root, manifest } = readyFixture();
  try {
    manifest.candidateProvenance.evidenceRevision = "a".repeat(40);
    writeFileSync(path.join(root, readinessPath), JSON.stringify(manifest, null, 2));
    const result = validateV02Readiness(root);
    assert.ok(result.errors.some((error) => error.includes("evidenceRevision does not match")));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("the gate accepts a complete evidenced manifest with the zero-fallback ratchet", () => {
  const { root } = readyFixture();
  try {
    const result = validateV02Readiness(root);
    assert.equal(result.ready, true, result.errors.join("\n"));
    assert.equal(result.errors.length, 0);
    assert.equal(requiredCriteria.length, 12);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
