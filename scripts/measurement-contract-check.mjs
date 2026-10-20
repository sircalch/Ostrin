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
  const directQuantityExample = read("examples/measurement_quantity_direct_shape.ostrin", root);
  const tests = read("compiler/tests/examples.rs", root);
  const design = read("docs/design/25-mediciones-e-incertidumbre.md", root);
  const lab = read("scripts/lab-data.mjs", root);
  const codegen = read("compiler/src/codegen.rs", root);
  const testsWithQuantityBoundary = tests.includes("measurement_quantity_boundary_is_explicitly_rejected_natively");
  const testsWithDirectQuantityBoundary = tests.includes("direct_measurement_quantity_boundary_is_explicitly_rejected_natively");

  for (const [name, source] of [
    ["compiler/std/measurements.ostrin", std],
    ["examples/measurement_scalar.ostrin", example],
    ["examples/measurement_quantity_direct_shape.ostrin", directQuantityExample],
    ["compiler/tests/examples.rs", tests],
    ["compiler/src/codegen.rs", codegen],
    ["docs/design/25-mediciones-e-incertidumbre.md", design],
    ["scripts/lab-data.mjs", lab],
  ]) check(errors, source.length > 0, `${name}: missing or empty`);

  for (const symbol of ["record Sensitivity", "record Measurement<T>", "exact(", "standard(", "from_source(", "unknown(", "uncertainty(", "values(", "uncertainties(", "sum(", "mean(", "summary("]) {
    check(errors, std.includes(symbol), `compiler/std/measurements.ostrin: missing scalar API ${symbol}`);
  }
  check(errors, !/fn finite\(/.test(std) && /is_finite\(value\)/.test(std) && /is_finite\(sigma\)/.test(std),
    "compiler/std/measurements.ostrin: typed is_finite guards are missing");
  check(errors, /measurement value must be finite/.test(std) && /measurement sigma must be finite/.test(std),
    "compiler/std/measurements.ostrin: finite input validation is missing");
  for (const marker of [
    "measurements.standard",
    "measurements.from_source",
    "same id, different labels",
    "different ids, same label",
    "source sigma mismatch",
    "legacy and explicit id",
    "measurements.exact",
    "measurements.unknown",
    "measurements.uncertainties",
    "x * x =",
    "x * y =",
    "invalid sigma",
    "empty source",
    "non-finite value",
    "infinite sigma",
  ]) check(errors, example.includes(marker), `examples/measurement_scalar.ostrin: missing regression marker ${marker}`);

  check(errors, tests.includes("std_measurements_scalar_matches_interpreter_and_native"),
    "compiler/tests/examples.rs: scalar Measurement parity test is missing");
  check(errors, tests.includes("std_measurements_series_feeds_reproducible_error_bars"),
    "compiler/tests/examples.rs: Measurement-to-error-bars parity test is missing");
  check(errors, /measurement_source\.contains\([\s\S]*ir=22[\s\S]*hir=0[\s\S]*ast=0/.test(tests),
    "compiler/tests/examples.rs: scalar Measurement IR ratchet is missing");

  check(errors, /primera fase\s+ejecutable/i.test(design) && /Measurement<Float>/i.test(design),
    "docs/design/25-mediciones-e-incertidumbre.md: scalar phase evidence is missing");
  check(errors, /(?:no habilita todavía|siguen pendientes)[\s\S]{0,80}Array<Measurement<T>>/i.test(design),
    "docs/design/25-mediciones-e-incertidumbre.md: array Measurement boundary is missing");
  check(errors, /sensibilidad de cada fuente[\s\S]*?b·cₐ,s \+ a·c_b,s[\s\S]*?NIST TN 1297/i.test(design),
    "docs/design/25-mediciones-e-incertidumbre.md: analytical propagation model or reference is missing");
  check(errors, /Quantity integration[\s\S]{0,220}(?:remain|pending|siguen pendientes)/i.test(lab),
    "scripts/lab-data.mjs: experimental Measurement boundary is missing");
  check(errors, testsWithQuantityBoundary,
    "compiler/tests/examples.rs: Measurement<Quantity<D>> native boundary test is missing");
  check(errors, testsWithDirectQuantityBoundary,
    "compiler/tests/examples.rs: direct Measurement<Quantity<D>> native boundary test is missing");
  check(errors, /Sensitivity<Quantity<Length>>/.test(directQuantityExample)
      && /from_quantity_source\(/.test(directQuantityExample)
      && /20 cm[\s\S]*0\.2 m/.test(directQuantityExample),
    "examples/measurement_quantity_direct_shape.ostrin: generic dimensional sensitivity shape is missing");
  check(errors, /pub fn from_quantity_source<D: Dimension>[\s\S]*?sigma\.unit_scale\(\)[\s\S]*?value\.dimension_key\(\)/.test(std),
    "compiler/std/measurements.ostrin: dimensional source sigma must be canonicalized and keyed by the static dimension");
  const dimensionalConstructor = std.match(/pub fn from_quantity_source<D: Dimension>[\s\S]*?\n}\n/)?.[0] ?? "";
  check(errors, /is_finite\(nominal\)/.test(dimensionalConstructor)
      && /is_finite\(declared_sigma\)/.test(dimensionalConstructor)
      && /declared_sigma < 0\.0/.test(dimensionalConstructor)
      && /source_id\.length\(\) == 0/.test(dimensionalConstructor)
      && /is_finite\(canonical_sigma\)/.test(dimensionalConstructor),
    "compiler/std/measurements.ostrin: dimensional constructor input and canonicalization guards are missing");
  check(errors, /type 'Measurement<Quantity<D>>' is not supported by the native backend yet/.test(codegen),
    "compiler/src/codegen.rs: native Measurement<Quantity<D>> guard is missing");

  const quantityMerge = std.match(/fn merge_quantity_sources<D: Dimension>[\s\S]*?\n}\n/)?.[0] ?? "";
  check(errors, /record Sensitivity<T>/.test(std)
      && /source_dimension: String/.test(std)
      && /source_sigma: Float/.test(std)
      && /contribution: T/.test(std),
    "compiler/std/measurements.ostrin: typed dimensional source metadata is missing");
  check(errors, /pub fn add_quantity<D: Dimension>/.test(std)
      && /pub fn sub_quantity<D: Dimension>/.test(std)
      && /current\.source_id == item\.source_id/.test(quantityMerge)
      && /current\.source_sigma != item\.source_sigma/.test(quantityMerge)
      && /factor \* item\.contribution/.test(quantityMerge)
      && /measurement source id was reused with a different dimension or sigma/.test(quantityMerge)
      && /validate_quantity_source/.test(quantityMerge),
    "compiler/std/measurements.ostrin: dimensional add/sub must merge and validate shared source contributions");
  check(errors, /add_quantity\(shared_a, shared_b\)/.test(directQuantityExample)
      && /sub_quantity\(difference_a, difference_b\)/.test(directQuantityExample),
    "examples/measurement_quantity_direct_shape.ostrin: correlated dimensional sum and difference examples are missing");
    check(errors, tests.includes("24 m\\n40 cm\\n1\\nLength\\n0.4\\n2\\n0.28284271247461906"),
      "compiler/tests/examples.rs: shared dimensional source identity evidence is missing");
    const quantityUncertainty = std.match(/pub fn quantity_uncertainty<D: Dimension>[\s\S]*?\n}\n/)?.[0] ?? "";
    check(errors, /pub record CoherentUncertainty<D>/.test(std)
        && /Result<CoherentUncertainty<D>, String>/.test(quantityUncertainty)
        && /source\.contribution\.value\(\) \* source\.contribution\.unit_scale\(\)/.test(quantityUncertainty)
        && /canonical_contribution \* canonical_contribution/.test(quantityUncertainty)
        && /measurement uncertainty overflowed/.test(quantityUncertainty),
      "compiler/std/measurements.ostrin: quantity uncertainty must use coherent dimensioned contributions and detect overflow");
    check(errors, tests.includes("12 m\\nLength\\n0.02\\nLength\\n0\\nunknown dimensional uncertainty = sensor offline")
        && tests.includes("0.28284271247461906\\noverflowing dimensional uncertainty = measurement uncertainty overflowed"),
      "compiler/tests/examples.rs: dimensional uncertainty must handle exact, unknown, correlated, independent, and overflow cases");
    check(errors, /quantity_uncertainty\(summed\)/.test(directQuantityExample)
        && /quantity_uncertainty\(independent_sum\)/.test(directQuantityExample),
      "examples/measurement_quantity_direct_shape.ostrin: dimensional uncertainty projections are missing");
    check(errors, /type 'Measurement<Quantity<D>>' is not supported by the native backend yet/.test(codegen),
      "compiler/src/codegen.rs: native Measurement<Quantity<D>> guard is missing");

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


