#!/usr/bin/env node

import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

function read(relativePath, root) {
  const file = path.join(root, relativePath);
  return existsSync(file) ? readFileSync(file, "utf8").replaceAll("\r\n", "\n") : "";
}

function check(errors, condition, message) {
  if (!condition) errors.push(message);
}

/**
 * Check that the experimental scalar Measurement contract is still source-backed.
 * This gate intentionally verifies evidence and boundaries; it does not enable
 * Array<Measurement>, Quantity integration, covariance, or Monte Carlo.
 */
export function validateMeasurementContract(root = repositoryRoot) {
  const errors = [];
  const std = read("compiler/std/measurements.ostrin", root);
  const example = read("examples/measurement_scalar.ostrin", root);
  const tests = read("compiler/tests/examples.rs", root);
  const design = read("docs/design/25-mediciones-e-incertidumbre.md", root);
  const lab = read("scripts/lab-data.mjs", root);

  for (const [name, source] of [
    ["compiler/std/measurements.ostrin", std],
    ["examples/measurement_scalar.ostrin", example],
    ["compiler/tests/examples.rs", tests],
    ["docs/design/25-mediciones-e-incertidumbre.md", design],
    ["scripts/lab-data.mjs", lab],
  ]) check(errors, source.length > 0, `${name}: missing or empty`);

  for (const symbol of ["record Sensitivity", "record Measurement<T>", "exact(", "standard(", "unknown(", "uncertainty(", "values(", "uncertainties(", "sum(", "mean(", "summary("]) {
    check(errors, std.includes(symbol), `compiler/std/measurements.ostrin: missing scalar API ${symbol}`);
  }
  for (const marker of [
    "measurements.standard",
    "measurements.exact",
    "measurements.unknown",
    "measurements.uncertainties",
    "invalid sigma",
    "empty source",
  ]) check(errors, example.includes(marker), `examples/measurement_scalar.ostrin: missing regression marker ${marker}`);

  check(errors, tests.includes("std_measurements_scalar_matches_interpreter_and_native"),
    "compiler/tests/examples.rs: scalar Measurement parity test is missing");
  check(errors, tests.includes("std_measurements_series_feeds_reproducible_error_bars"),
    "compiler/tests/examples.rs: Measurement-to-error-bars parity test is missing");
  check(errors, /measurement_source\.contains\([\s\S]*ir=21[\s\S]*hir=0[\s\S]*ast=0/.test(tests),
    "compiler/tests/examples.rs: scalar Measurement IR ratchet is missing");

  check(errors, /primera fase\s+ejecutable/i.test(design) && /Measurement<Float>/i.test(design),
    "docs/design/25-mediciones-e-incertidumbre.md: scalar phase evidence is missing");
  check(errors, /(?:no habilita todavía|siguen pendientes)[\s\S]{0,80}Array<Measurement<T>>/i.test(design),
    "docs/design/25-mediciones-e-incertidumbre.md: array Measurement boundary is missing");
  check(errors, /Quantity integration[\s\S]{0,220}(?:remain|pending|siguen pendientes)/i.test(lab),
    "scripts/lab-data.mjs: experimental Measurement boundary is missing");

  return { errors };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const result = validateMeasurementContract();
  if (result.errors.length > 0) {
    for (const error of result.errors) console.error(`measurement-contract-check: ${error}`);
    process.exitCode = 1;
  } else {
    console.log("measurement-contract-check: ok (scalar API, parity tests and experimental boundaries aligned)");
  }
}


