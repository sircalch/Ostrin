#!/usr/bin/env node

import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const ledger = JSON.parse(readFileSync(path.join(root, "validation/ast-fallback-ledger.json"), "utf8"));
const differential = readFileSync(path.join(root, "compiler/tests/differential.rs"), "utf8");

const expected = {
  ir: Number(differential.match(/verified baseline is\s+[\d,]+\s+AST fallbacks[\s\S]*?[\d,]+\s+IR/)?.[0].match(/[\d,]+(?=\s+IR)/)?.[0].replaceAll(",", "") ?? NaN),
  ast: Number(differential.match(/MAX_AST_FALLBACK_FUNCTIONS: usize = (\d+)/)?.[1]),
};
const errors = [];
if (ledger.schema !== "ostrin.ast-fallback-ledger/v1") errors.push("schema is invalid");
if (ledger.status !== "verified-in-differential-suite") errors.push("status must identify the differential suite");
if (!Array.isArray(ledger.entries) || ledger.entries.length !== ledger.count) errors.push("entry count does not match count");
if (ledger.count !== ledger.verifiedBaseline.ast || ledger.count !== expected.ast) errors.push("AST count drifted from the ratchet");
if (ledger.verifiedBaseline.ir + ledger.verifiedBaseline.hir !== ledger.verifiedBaseline.total) errors.push("baseline HIR/IR total does not add up");
for (const entry of ledger.entries) {
  for (const field of ["source", "function", "reason", "blockedBy", "test", "risk", "plannedResolution"]) {
    if (typeof entry[field] !== "string" || entry[field].length === 0) errors.push(`${field} missing for ${entry.source ?? "unknown"}`);
  }
}
if (errors.length) {
  console.error(errors.map((error) => `fallback-ledger-check: ${error}`).join("\n"));
  process.exitCode = 1;
} else {
  console.log(`fallback-ledger-check: ok (${ledger.count} entries; baseline ${ledger.verifiedBaseline.ir} IR + ${ledger.verifiedBaseline.hir} HIR)`);
}
