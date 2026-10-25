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
 * Check that scalar, bounded dimensional, scalar covariance, and structural
 * Measurement array contracts have evidence. Dimensional/array covariance and
 * Monte Carlo remain open.
 */
export function validateMeasurementContract(root = repositoryRoot) {
  const errors = [];
  const std = read("compiler/std/measurements.ostrin", root);
  const example = read("examples/measurement_scalar.ostrin", root);
  const directQuantityExample = read("examples/measurement_quantity_direct_shape.ostrin", root);
  const nativeQuantityExample = read("examples/measurement_quantity_native_smoke.ostrin", root);
  const measurementArrayExample = read("examples/measurement_array_shape.ostrin", root);
  const covarianceExample = read("examples/measurement_covariance.ostrin", root);
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
    ["examples/measurement_covariance.ostrin", covarianceExample],
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
  check(errors, /measurement_source\.contains\([\s\S]*ir=31[\s\S]*hir=0[\s\S]*ast=0/.test(tests),
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
  check(errors, /One-dimensional Array<Measurement<Float>> has an experimental structural profile[\s\S]{0,300}Numeric array operations, Monte Carlo and coverage intervals remain planned/i.test(lab),
    "scripts/lab-data.mjs: dimensional Measurement arrays, covariance, and Monte Carlo boundary is missing");
  check(errors, /bounded SourceCovariance API for dimensionless scalar measurements using J·Σ·Jᵀ/i.test(lab)
      && /SourceCovariance is experimental and limited to dense covariance matrices for at most 64 dimensionless scalar sources/i.test(lab)
      && /its relative tolerance is at most 1e-6 \(default 1e-12\)/i.test(lab)
      && /nonzero source sigma whose square underflows Float is rejected/i.test(lab)
      && /propagation derives a scale-safe correlation matrix and applies it directly to the stored source contributions/i.test(lab)
      && /product of the two source standard deviations, avoiding a variance ratio that could underflow to subnormal/i.test(lab)
      && /rejects unequal-variance correlations when the geometric scale itself is subnormal; equal variances use the exact diagonal/i.test(lab)
      && /avoiding lossy division by sigma and reconstruction/i.test(lab)
      && /midpoint calculation preserves subnormal symmetric covariance entries/i.test(lab)
      && /quadratic form uses a semidefinite factor and compensated projections before summing squares/i.test(lab)
      && /Source contributions use exact power-of-two normalization/i.test(lab)
      && /negative and numerically unresolved positive pivots are rejected/i.test(lab)
      && /covariance matrix that cannot be stably factored is rejected explicitly/i.test(lab)
      && /Covariance propagation for Measurement<Quantity<D>>, arrays and array reductions remains unsupported/i.test(lab),
    "scripts/lab-data.mjs: website Measurement covariance example or its experimental boundary is missing");
  const covarianceValidation = std.match(/fn covariance_validation_error\([\s\S]*?\n}\n/)?.[0] ?? "";
  const covarianceCorrelation = std.match(/fn covariance_correlation_matrix\([\s\S]*?\n}\n/)?.[0] ?? "";
  const covariancePropagation = std.match(/pub fn variance_with_covariance\([\s\S]*?\n}\n/)?.[0] ?? "";
  const covarianceFactorization = std.match(/fn covariance_semidefinite_factor\([\s\S]*?\n}\n/)?.[0] ?? "";
  const covarianceMidpoint = std.match(/fn covariance_midpoint\([\s\S]*?\n}\n/)?.[0] ?? "";
  check(errors, /record SourceCovariance/.test(std)
      && /pub fn covariance\(/.test(std)
      && /n > 64/.test(covarianceValidation)
      && /relative_tolerance > 1e-6/.test(covarianceValidation)
      && /matrix\.shape\(\)/.test(covarianceValidation)
      && /is_finite\(value\)/.test(covarianceValidation)
      && /matrix\.get\(i, i\) \/ scale/.test(covarianceValidation)
      && /matrix\.get\(i, j\) \/ scale/.test(covarianceValidation)
      && /abs\(upper - lower\) > relative_tolerance/.test(covarianceValidation)
      && /numeric\.eigen\(symmetric, relative_tolerance\)/.test(covarianceValidation)
      && /factors\.values\[0\] < 0\.0 - relative_tolerance/.test(covarianceValidation)
      && /covariance_validation_error\(model\.source_ids, model\.matrix, model\.relative_tolerance\)/.test(covariancePropagation)
      && !/covariance_scale/.test(covariancePropagation)
      && /expected_variance = source\.source_sigma \* source\.source_sigma/.test(covariancePropagation)
      && /source\.source_sigma != 0\.0 and expected_variance == 0\.0/.test(covariancePropagation)
      && /covariance diagonal must match the measurement source sigma/.test(covariancePropagation)
      && /source_sigmas\.set\(index, source\.source_sigma\)/.test(covariancePropagation)
      && /if diagonal != expected_variance[\s\S]*?adjustment = sqrt\(diagonal\) \/ source_sigma/.test(covariancePropagation)
      && /covariance_correlation_matrix\(model\.matrix, model\.relative_tolerance\)/.test(covariancePropagation)
      && /covariance_semidefinite_factor\(correlation, model\.relative_tolerance\)/.test(covariancePropagation)
      && /standardized_contributions/.test(covariancePropagation)
      && /contribution_power_scale/.test(covariancePropagation)
      && /contribution_power_scale <= contribution_scale \* 0\.5/.test(covariancePropagation)
      && /standardized_contributions\[row\] \/ contribution_power_scale/.test(covariancePropagation)
      && /coefficient = entry \/ covariance_scale/.test(covarianceCorrelation)
      && /covariance_scale = row_sigma \* column_sigma/.test(covarianceCorrelation)
      && /covariance_scale = row_variance/.test(covarianceCorrelation)
      && /covariance_scale < 2\.2250738585072014e-308/.test(covarianceCorrelation)
      && /covariance correlation is numerically unresolved/.test(covarianceCorrelation)
      && /factor\.get\(row, column\)/.test(covariancePropagation)
      && /contribution_adjustment_flags\.set\(i, 1\.0\)/.test(covariancePropagation)
      && /projection_adjustments_uniform/.test(covariancePropagation)
      && /scaled_unadjusted_projection == 0\.0/.test(covariancePropagation)
      && /projection_correction = projection_correction \+ residual/.test(covariancePropagation)
      && /contribution_variance = propagated_sigma \* propagated_sigma/.test(covariancePropagation)
      && /propagated covariance projection underflows Float range/.test(covariancePropagation)
      && /pivot > 0\.0[\s\S]*?pivot <= relative_tolerance \* pivot_scale/.test(covarianceFactorization)
      && /if pivot < 0\.0\s*\{\s*return Err/.test(covarianceFactorization)
      && /covariance matrix could not be stably factorized/.test(covarianceFactorization),
    "compiler/std/measurements.ostrin: bounded covariance validation and factorized J·Σ·Jᵀ propagation contract is incomplete");
  check(errors, /lower \+ \(upper - lower\) \* 0\.5/.test(covarianceMidpoint)
      && /lower < 0\.0 and upper >= 0\.0/.test(covarianceMidpoint)
      && /upper < 0\.0 and lower >= 0\.0/.test(covarianceMidpoint)
      && /if lower > upper/.test(covarianceMidpoint)
      && /vista\s+normalizada por la entrada máxima finita/i.test(design)
      && /La propagación promedia las entradas originales antes de\s+factorizar/i.test(design)
      && /La matriz[\s\S]{0,200}R = L · Lᵀ[\s\S]{0,200}suma\s+compensada/i.test(design)
      && /normalizan usando una potencia de dos exacta/i.test(design)
      && /pivotes positivos numéricamente irresueltos/i.test(design)
      && /factorización numéricamente estable/i.test(design)
      && /media geométrica `sqrt\(Σᵢᵢ\) · sqrt\(Σⱼⱼ\)`/i.test(design)
      && /cociente entre varianzas, que puede redondearse a subnormal/i.test(design)
      && /producto mismo queda\s+subnormal con varianzas distintas/i.test(design)
      && /borrar la varianza residual/i.test(design)
      && /El punto\s+medio evita perder covarianzas subnormales/i.test(design),
    "Measurement covariance: normalized PSD analysis and underflow-safe midpoint are missing");
  check(errors, /SourceCovariance` valida entre 1 y 64 IDs/i.test(design)
      && /tolerancia relativa está acotada a[\s\S]{0,120}\(0, 1e-6\]/i.test(design)
      && /J · Σ · Jᵀ/i.test(design)
      && /los pares simétricos que difieren dentro de esa\s+tolerancia se promedian explícitamente/i.test(design)
      && /La matriz se vuelve a\s+validar al propagar/i.test(design)
      && /subdesborda[\s\S]{0,120}la propagación se rechaza explícitamente/i.test(design)
      && /no acepta[\s\S]{0,140}covarianza entre elementos/i.test(design),
    "docs/design/25-mediciones-e-incertidumbre.md: scalar covariance contract, mutation guard or scope boundary is incomplete");
  check(errors, tests.includes("std_measurements_covariance_matches_analytic_oracle_and_wasi")
      && /correlated sum variance = \{\}/.test(tests)
      && /rank-deficient cancellation uncertainty = 0/.test(tests)
      && /subnormal covariance cancellation variance = 0/.test(tests)
      && /wide-scale covariance cancellation variance = 0/.test(tests)
      && /rank-three compensated variance = 1/.test(tests)
      && /rank-three reordered compensated variance = 1/.test(tests)
      && /power-of-two gradient normalization variance = 4/.test(tests)
      && /sqrt-three rank-one cancellation variance = 0/.test(tests)
      && /direct contribution cancellation variance = 16/.test(tests)
      && /adjusted contribution cancellation = propagated covariance projection is numerically unresolved/.test(tests)
      && tests.includes("adjusted_contribution_residual_oracle")
      && tests.includes("0.048_596_460_630_071_24")
      && /unresolved positive pivot = covariance correlation is numerically unresolved/.test(tests)
      && /near-perfect covariance residual variance = covariance correlation is numerically unresolved/.test(tests)
      && /subnormal variance ratio covariance variance = 3/.test(tests)
      && /subnormal geometric covariance scale = covariance correlation is numerically unresolved/.test(tests)
      && /review subnormal geometric covariance = covariance correlation is numerically unresolved/.test(tests)
      && /rounded geometric covariance = covariance correlation is numerically unresolved/.test(tests)
      && tests.includes("near_perfect_correlation_oracle")
      && tests.includes("21.316_282_072_803_006")
      && tests.includes("subnormal_variance_ratio_geometric_mean")
      && tests.includes("subnormal_variance_ratio_correlation")
      && tests.includes("subnormal_geometric_scale_oracle")
      && tests.includes("review_subnormal_geometric_scale_oracle")
      && tests.includes("rounded_geometric_mean_oracle")
      && /nonuniform rank-one covariance variance = 0\.01/.test(tests)
      && /large finite PSD covariance = accepted/.test(tests)
      && /large indefinite covariance = covariance matrix must be positive semidefinite/.test(tests)
      && covarianceExample.includes("measurements.covariance")
      && covarianceExample.includes("measurements.variance_with_covariance")
      && covarianceExample.includes("indefinite covariance")
      && covarianceExample.includes("subnormal covariance cancellation variance")
      && covarianceExample.includes("wide-scale covariance cancellation variance")
      && covarianceExample.includes("rank-three compensated variance")
      && covarianceExample.includes("rank-three reordered compensated variance")
      && covarianceExample.includes("power-of-two gradient normalization variance")
      && covarianceExample.includes("sqrt-three rank-one cancellation variance")
      && covarianceExample.includes("direct contribution cancellation variance")
      && covarianceExample.includes("adjusted contribution cancellation")
      && covarianceExample.includes("unresolved positive pivot")
      && covarianceExample.includes("near-perfect covariance residual variance")
      && covarianceExample.includes("nonuniform rank-one covariance variance")
      && covarianceExample.includes("large finite PSD covariance")
      && covarianceExample.includes("large indefinite covariance")
      && covarianceExample.includes("excessive covariance tolerance")
      && covarianceExample.includes("underflowed source variance")
      && covarianceExample.includes("subnormal source variance")
      && covarianceExample.includes("source variance mismatch"),
    "Measurement covariance: analytical oracle, runtime parity and validation regressions are missing");
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


