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
 * Check that scalar, bounded dimensional, and structural Measurement array
 * contracts have evidence. Numeric arrays, covariance, and Monte Carlo remain open.
 */
export function validateMeasurementContract(root = repositoryRoot) {
  const errors = [];
  const std = read("compiler/std/measurements.ostrin", root);
  const example = read("examples/measurement_scalar.ostrin", root);
  const directQuantityExample = read("examples/measurement_quantity_direct_shape.ostrin", root);
  const nativeQuantityExample = read("examples/measurement_quantity_native_smoke.ostrin", root);
  const measurementArrayExample = read("examples/measurement_array_shape.ostrin", root);
  const labExample = read("examples/measurement_lab.ostrin", root);
  const tests = read("compiler/tests/examples.rs", root);
  const design = read("docs/design/25-mediciones-e-incertidumbre.md", root);
  const lab = read("scripts/lab-data.mjs", root);
  const codegen = read("compiler/src/codegen.rs", root);
  const testsWithQuantityNativeSmoke = tests.includes("dimensional_measurement_native_smoke_matches_interpreter_and_wasi");
  const testsWithQuantityLab = tests.includes("dimensional_measurement_lab_matches_interpreter_and_native");
  const testsWithDirectQuantityParity = tests.includes("dimensional_measurement_full_surface_matches_native_and_wasi")
    && tests.includes("direct Measurement<Quantity> must lower main through IR")
    && tests.includes("direct Measurement<Quantity> leaked");

  for (const [name, source] of [
    ["compiler/std/measurements.ostrin", std],
    ["examples/measurement_scalar.ostrin", example],
    ["examples/measurement_quantity_direct_shape.ostrin", directQuantityExample],
    ["examples/measurement_quantity_native_smoke.ostrin", nativeQuantityExample],
    ["examples/measurement_array_shape.ostrin", measurementArrayExample],
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
  check(errors, /perfil estructural unidimensional[\s\S]{0,180}Array<Measurement<Float>>[\s\S]{0,220}pero no[\s\S]{0,100}(?:operaciones numéricas|aritmética)[\s\S]{0,80}covarianza/i.test(design),
    "docs/design/25-mediciones-e-incertidumbre.md: bounded structural array Measurement boundary is missing");
  check(errors, /sensibilidad de cada fuente[\s\S]*?b·cₐ,s \+ a·c_b,s[\s\S]*?NIST TN 1297/i.test(design),
    "docs/design/25-mediciones-e-incertidumbre.md: analytical propagation model or reference is missing");
  check(errors, /matching interpreter\/native output[\s\S]{0,500}smoke also emits C for WASI[\s\S]{0,160}interpreter\/WASM path/i.test(lab),
    "scripts/lab-data.mjs: dimensional Measurement native-smoke and interpreter/WASM scope is missing");
  check(errors, /IR-generated main functions, zero AST fallback and zero generic type divergences/i.test(lab),
    "scripts/lab-data.mjs: typed dimensional Measurement smoke evidence is missing");
  check(errors, /equal dimensions[\s\S]{0,120}explicit experimental boundary/i.test(lab),
    "scripts/lab-data.mjs: remaining dimensional Measurement native boundary is missing");
  check(errors, /direct full-surface fixture now has native IR\/interpreter parity[\s\S]{0,220}div_quantity_same/i.test(lab)
      && !/direct full-surface fixture still fails native lowering/i.test(lab),
    "scripts/lab-data.mjs: direct Measurement<Quantity<D>> native evidence is stale or missing");
  check(errors, /One-dimensional Array<Measurement<Float>> has an experimental structural profile[\s\S]{0,240}Numeric array operations, reductions, covariance, Monte Carlo and coverage intervals remain planned/i.test(lab),
    "scripts/lab-data.mjs: dimensional Measurement arrays, covariance, and Monte Carlo boundary is missing");
  check(errors, tests.includes("measurement_array_structural_ownership_matches_interpreter_native_and_wasi")
      && tests.includes("measurement_array_numeric_operators_are_rejected_explicitly")
      && measurementArrayExample.includes("values[array([false, true])]")
      && measurementArrayExample.includes("fn last_measurement()")
      && measurementArrayExample.includes("print(values)"),
    "Array<Measurement<Float>>: structural feature, display, escaping-get and numeric boundary evidence is missing");
  check(errors, testsWithQuantityNativeSmoke,
    "compiler/tests/examples.rs: Measurement<Quantity<D>> native smoke parity test is missing");
  check(errors, testsWithQuantityLab,
    "compiler/tests/examples.rs: dimensional Measurement Lab interpreter/native parity test is missing");
  check(errors, testsWithDirectQuantityParity,
    "compiler/tests/examples.rs: direct Measurement<Quantity<D>> native/WASI parity test is missing");
  check(errors, /Sensitivity<Quantity<Length>>/.test(directQuantityExample)
      && /from_quantity_source\(/.test(directQuantityExample)
      && /measurements\.exact_quantity\(/.test(directQuantityExample)
      && /measurements\.unknown_quantity\(/.test(directQuantityExample)
      && /20 cm[\s\S]*0\.2 m/.test(directQuantityExample),
    "examples/measurement_quantity_direct_shape.ostrin: dimensional sensitivity and explicit-state examples are missing");
  for (const constructor of ["exact_quantity", "unknown_quantity"]) {
    const body = std.match(new RegExp(`pub fn ${constructor}<D: Dimension>[\\s\\S]*?\\n}\\n`))?.[0] ?? "";
    check(errors, body.includes("is_finite(value.value())")
        && body.includes("is_finite(value.value() * value.unit_scale())")
        && body.includes("measurement value must be finite"),
      `compiler/std/measurements.ostrin: ${constructor} must reject non-finite nominal and coherent values`);
  }
  check(errors, /dimensional_measurement_state_constructors_are_explicit_and_finite/.test(tests),
    "compiler/tests/examples.rs: dimensional exact/unknown constructor runtime test is missing");
  check(errors, /non-finite exact = measurement value must be finite[\s\S]*non-finite unknown = measurement value must be finite/.test(tests),
    "compiler/tests/examples.rs: exact/unknown constructor overflow rejection evidence is missing");
  check(errors, /exact_zero = measurements\.exact_quantity\(/.test(labExample),
    "examples/measurement_lab.ostrin: exact dimensional Measurement demo is missing");
  check(errors, /pending_reading = measurements\.unknown_quantity\(/.test(labExample),
    "examples/measurement_lab.ostrin: unknown dimensional Measurement demo is missing");
  check(errors, /pub fn from_quantity_source<D: Dimension>[\s\S]*?sigma\.unit_scale\(\)[\s\S]*?value\.dimension_key\(\)/.test(std),
    "compiler/std/measurements.ostrin: dimensional source sigma must be canonicalized and keyed by the static dimension");
  const dimensionalConstructor = std.match(/pub fn from_quantity_source<D: Dimension>[\s\S]*?\n}\n/)?.[0] ?? "";
  check(errors, /is_finite\(nominal\)/.test(dimensionalConstructor)
      && /is_finite\(declared_sigma\)/.test(dimensionalConstructor)
      && /declared_sigma < 0\.0/.test(dimensionalConstructor)
      && /source_id\.length\(\) == 0/.test(dimensionalConstructor)
      && /is_finite\(canonical_sigma\)/.test(dimensionalConstructor),
    "compiler/std/measurements.ostrin: dimensional constructor input and canonicalization guards are missing");
  check(errors, !/type 'Measurement<Quantity<D>>' is not supported by the native backend yet/.test(codegen)
      && /method '\{method_name\}' on \{obj_ty:\?\}/.test(codegen),
    "compiler/src/codegen.rs: bounded dimensional Measurement native lowering and diagnostic evidence is missing");

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
  check(errors, /from_quantity_source\(/.test(nativeQuantityExample)
      && /mul_quantity\(/.test(nativeQuantityExample)
        && /sub_quantity\(/.test(nativeQuantityExample),
      "examples/measurement_quantity_native_smoke.ostrin: native dimensional arithmetic evidence is missing");

  const quantitySourceValidation = std.match(/fn validate_quantity_source<D: Dimension>[\s\S]*?\n}\n/)?.[0] ?? "";
  check(errors, /source\.source_dimension\.length\(\) == 0/.test(quantitySourceValidation)
      && !/source\.source_dimension != source\.contribution\.dimension_key\(\)/.test(quantitySourceValidation),
    "compiler/std/measurements.ostrin: source dimension metadata must remain independent of propagated contribution dimension");
  check(errors, /pub fn mul_quantity<A: Dimension, B: Dimension>/.test(std)
      && /contribution: item\.contribution \* factor/.test(std)
      && /contribution: factor \* item\.contribution/.test(std)
      && /pub fn div_quantity_same<D: Dimension>/.test(std)
      && /a_factor = 1\.0 \/ b_value/.test(std)
      && /scale_quantity_sources_to_float\(a\.sources, a_factor\)/.test(std)
      && /scale_quantity_sources_to_float\(b\.sources, b_factor\)/.test(std)
      && /pub fn div_quantity<A: Dimension, B: Dimension>/.test(std)
      && /if a\.value\.dimension_key\(\) == b\.value\.dimension_key\(\) \{[\s\S]*?dimensionless quotient requires div_quantity_same/.test(std)
      && /contribution: item\.contribution \/ divisor/.test(std)
      && /numerator \* item\.contribution\) \/ \(denominator \* denominator/.test(std),
    "compiler/std/measurements.ostrin: dimensional products and same- or cross-dimension ratios must propagate derivatives");
  check(errors, /measurements\.div_quantity\(distance_reading, elapsed_reading\)/.test(directQuantityExample)
      && /div_quantity\(distance, elapsed\)/.test(labExample)
      && /same- or cross-dimension ratios/.test(lab)
      && tests.includes("Length*Time^-1\\n5\\n0.05\\n-0.5\\nLength*Time^-1\\n0.5024937810560445")
      && tests.includes("dimensionless quantity quotient = dimensionless quotient requires div_quantity_same")
      && /cocientes entre dimensiones distintas/i.test(design),
    "dimensional measurement evidence: cross-dimension quotient type, derivative values, uncertainty, or web demo is missing");
  check(errors, /Length\^2\\nLength\\nLength\^2\\n0\.8/.test(tests),
    "compiler/tests/examples.rs: correlated product evidence is missing");
  check(errors, /1\\n0\\ndimensionless quantity quotient = dimensionless quotient requires div_quantity_same\\n0\.03535533905932738/.test(tests),
    "compiler/tests/examples.rs: same-dimension quotient correlation and dimensionless boundary evidence is missing");
  check(errors, /Length\*Time\\n0\.2\\n0\.6000000000000001\\n0\.632455532033676/.test(tests),
    "compiler/tests/examples.rs: cross-dimension product derivative evidence is missing");
  check(errors, /zero dimensional denominator = measurement division by zero/.test(tests),
    "compiler/tests/examples.rs: quantity quotient must reject a zero denominator");  return { errors };
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


