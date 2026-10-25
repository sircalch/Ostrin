import test from "node:test";
import assert from "node:assert/strict";
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { repositoryRoot, validateMeasurementContract } from "./measurement-contract-check.mjs";

const trackedFiles = [
  "compiler/std/measurements.ostrin",
  "examples/measurement_scalar.ostrin",
  "examples/measurement_quantity_direct_shape.ostrin",
  "examples/measurement_quantity_native_smoke.ostrin",
  "examples/measurement_array_shape.ostrin",
  "examples/measurement_covariance.ostrin",
  "examples/measurement_lab.ostrin",
  "compiler/tests/examples.rs",
  "docs/design/25-mediciones-e-incertidumbre.md",
  "scripts/lab-data.mjs",
];

function fixtureRoot() {
  const root = mkdtempSync(path.join(os.tmpdir(), "ostrin-measurement-check-"));
  for (const relativePath of trackedFiles) {
    const target = path.join(root, relativePath);
    mkdirSync(path.dirname(target), { recursive: true });
    copyFileSync(path.join(repositoryRoot, relativePath), target);
  }
  return root;
}

test("the scalar Measurement contract is aligned with source and parity evidence", () => {
  assert.deepEqual(validateMeasurementContract(repositoryRoot).errors, []);
});

test("the gate requires accurate dimensional native-smoke scope and experimental boundaries", () => {
  const root = fixtureRoot();
  try {
    const file = path.join(root, "scripts/lab-data.mjs");
    const original = readFileSync(file, "utf8");
    writeFileSync(file, original.replace("matching interpreter/native output", "native support"));
    let result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("native-smoke and interpreter/WASM scope")));

    writeFileSync(file, original.replace("IR-generated main functions, zero AST fallback and zero generic type divergences", "native evidence"));
    result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("typed dimensional Measurement smoke evidence")));

    writeFileSync(file, original.replace(
      "One-dimensional Array<Measurement<Float>> has an experimental structural profile",
      "Array<Measurement<Float>> has complete numeric support",
    ));
    result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("arrays and Monte Carlo boundary")));

    writeFileSync(file, original.replace("Equal dimensions in div_quantity reach the explicit experimental boundary error; use div_quantity_same", "native parity"));
    result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("remaining dimensional Measurement native boundary")));

    writeFileSync(file, original.replace("direct full-surface fixture now has native IR/interpreter parity", "native support"));
    result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("direct Measurement<Quantity<D>> native evidence is stale or missing")));

    writeFileSync(file, original.replace("direct full-surface fixture now has native IR/interpreter parity", "native support")
      .replace("Equal dimensions in div_quantity reach the explicit experimental boundary error; use div_quantity_same", "equal dimension support"));
    result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("direct Measurement<Quantity<D>> native evidence is stale or missing")));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("the gate requires direct Measurement<Quantity> native/WASI parity evidence", () => {
  const root = fixtureRoot();
  try {
    const file = path.join(root, "compiler/tests/examples.rs");
    const original = readFileSync(file, "utf8");
    writeFileSync(file, original.replace("dimensional_measurement_full_surface_matches_native_and_wasi", "direct_shape_test"));
    let result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("direct Measurement<Quantity<D>> native/WASI parity test")));

    writeFileSync(file, original.replace("direct Measurement<Quantity> must lower main through IR", "direct shape is supported"));
    result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("direct Measurement<Quantity<D>> native/WASI parity test")));

    writeFileSync(file, original.replace("direct Measurement<Quantity> leaked", "no leaks"));
    result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("direct Measurement<Quantity<D>> native/WASI parity test")));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("the gate rejects an example that silently drops unknown uncertainty", () => {
  const root = fixtureRoot();
  try {
    const file = path.join(root, "examples/measurement_scalar.ostrin");
    const source = readFileSync(file, "utf8").replace("measurements.uncertainties", "measurements.values");
    writeFileSync(file, source);
    const result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("uncertainties")));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("the gate requires non-finite input guards and analytical propagation examples", () => {
  const root = fixtureRoot();
  try {
    const stdFile = path.join(root, "compiler/std/measurements.ostrin");
    const std = readFileSync(stdFile, "utf8").replaceAll("is_finite(value)", "finite_value(value)");
    writeFileSync(stdFile, std);
    const exampleFile = path.join(root, "examples/measurement_scalar.ostrin");
    const example = readFileSync(exampleFile, "utf8").replace("x * x =", "x squared =");
    writeFileSync(exampleFile, example);
    const designFile = path.join(root, "docs/design/25-mediciones-e-incertidumbre.md");
    const design = readFileSync(designFile, "utf8").replace("b·cₐ,s + a·c_b,s", "unsupported model");
    writeFileSync(designFile, design);
    const result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("typed is_finite guards")));
    assert.ok(result.errors.some((error) => error.includes("x * x")));
    assert.ok(result.errors.some((error) => error.includes("analytical propagation model")));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("the gate requires dimensional sources to normalize sigma and retain the static dimension", () => {
  const root = fixtureRoot();
  try {
    const file = path.join(root, "compiler/std/measurements.ostrin");
    const source = readFileSync(file, "utf8").replace("sigma.unit_scale()", "1.0");
    writeFileSync(file, source);
    const result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("canonicalized and keyed by the static dimension")));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("the gate rejects dimensional constructors without sigma validation", () => {
  const root = fixtureRoot();
  try {
    const file = path.join(root, "compiler/std/measurements.ostrin");
    const source = readFileSync(file, "utf8").replace("if declared_sigma < 0.0 {", "if false {");
    writeFileSync(file, source);
    const result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("dimensional constructor input and canonicalization guards")));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("the gate requires correlated dimensional sums and differences to merge by source identity", () => {
  const root = fixtureRoot();
  try {
    const file = path.join(root, "compiler/std/measurements.ostrin");
    const source = readFileSync(file, "utf8").replace("current.source_id == item.source_id", "false");
    writeFileSync(file, source);
    const result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("dimensional add/sub must merge and validate shared source contributions")));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("the gate requires dimensional uncertainty to sum canonical contribution squares", () => {
  const root = fixtureRoot();
  try {
    const file = path.join(root, "compiler/std/measurements.ostrin");
    const source = readFileSync(file, "utf8").replace(
      "contribution_variance = canonical_contribution * canonical_contribution",
      "contribution_variance = 0.0",
    );
    writeFileSync(file, source);
    const result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("quantity uncertainty must use coherent dimensioned contributions")));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("the gate rejects dimensional product propagation without both derivative factors", () => {
  const root = fixtureRoot();
  try {
    const file = path.join(root, "compiler/std/measurements.ostrin");
    const source = readFileSync(file, "utf8").replace(
      "contribution: item.contribution * factor",
      "contribution: item.contribution",
    );
    writeFileSync(file, source);
    const result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("dimensional products and same- or cross-dimension ratios")));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("the gate rejects a dimensional exact constructor without coherent finiteness validation", () => {
  const root = fixtureRoot();
  try {
    const file = path.join(root, "compiler/std/measurements.ostrin");
    const original = readFileSync(file, "utf8");
    const start = original.indexOf("pub fn exact_quantity<D: Dimension>");
    const end = original.indexOf("\npub fn unknown_quantity<D: Dimension>", start);
    const body = original.slice(start, end);
    assert.ok(body.includes("is_finite(value.value() * value.unit_scale())"));
    writeFileSync(file, original.slice(0, start) + body.replace("is_finite(value.value() * value.unit_scale())", "true") + original.slice(end));
    const result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("exact_quantity must reject non-finite nominal and coherent values")));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("the gate rejects a dimensional unknown constructor without coherent finiteness validation", () => {
  const root = fixtureRoot();
  try {
    const file = path.join(root, "compiler/std/measurements.ostrin");
    const original = readFileSync(file, "utf8");
    const start = original.indexOf("pub fn unknown_quantity<D: Dimension>");
    const end = original.indexOf("\nfn merge_quantity_sources<D: Dimension>", start);
    const body = original.slice(start, end);
    assert.ok(body.includes("is_finite(value.value() * value.unit_scale())"));
    writeFileSync(file, original.slice(0, start) + body.replace("is_finite(value.value() * value.unit_scale())", "true") + original.slice(end));
    const result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("unknown_quantity must reject non-finite nominal and coherent values")));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("the gate rejects same-dimension quotient propagation without the numerator derivative", () => {
  const root = fixtureRoot();
  try {
    const file = path.join(root, "compiler/std/measurements.ostrin");
    const source = readFileSync(file, "utf8").replace("a_factor = 1.0 / b_value", "a_factor = 0.0");
    writeFileSync(file, source);
    const result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("dimensional products and same- or cross-dimension ratios")));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("the gate rejects cross-dimension quotient propagation without the numerator derivative", () => {
  const root = fixtureRoot();
  try {
    const file = path.join(root, "compiler/std/measurements.ostrin");
    const source = readFileSync(file, "utf8").replace(
      "contribution: item.contribution / divisor",
      "contribution: item.contribution",
    );
    writeFileSync(file, source);
    const result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("dimensional products and same- or cross-dimension ratios")));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("the gate requires an explicit dimensionless quotient boundary", () => {
  const root = fixtureRoot();
  try {
    const file = path.join(root, "compiler/std/measurements.ostrin");
    const source = readFileSync(file, "utf8").replace(
      "dimensionless quotient requires div_quantity_same",
      "unsupported quotient",
    );
    writeFileSync(file, source);
    const result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("dimensional products and same- or cross-dimension ratios")));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("the gate rejects equating a source dimension with a propagated product dimension", () => {
  const root = fixtureRoot();
  try {
    const file = path.join(root, "compiler/std/measurements.ostrin");
    const source = readFileSync(file, "utf8").replace(
      'if source.source_dimension.length() == 0 {',
      'if source.source_dimension != source.contribution.dimension_key() {',
    );
    writeFileSync(file, source);
    const result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("source dimension metadata must remain independent")));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("the gate rejects claims that exceed the structural Measurement array profile", () => {
  const root = fixtureRoot();
  try {
    const file = path.join(root, "docs/design/25-mediciones-e-incertidumbre.md");
    const source = readFileSync(file, "utf8").replace(
      /pero no\s+operaciones numéricas sobre esos arrays ni covarianza/,
      "con operaciones numéricas sobre esos arrays y covarianza habilitadas",
    );
    writeFileSync(file, source);
    const result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("bounded structural array Measurement boundary")));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("the gate requires bounded scalar covariance evidence and explicit scope", () => {
  const root = fixtureRoot();
  try {
    const stdFile = path.join(root, "compiler/std/measurements.ostrin");
    const std = readFileSync(stdFile, "utf8");
    writeFileSync(stdFile, std.replace("if n > 64 {", "if n > 128 {"));
    let result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("bounded covariance validation")));

    writeFileSync(stdFile, std.replace(
      "and sqrt(diagonal) != source_sigma",
      "and sqrt(diagonal) == source_sigma",
    ));
    result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("bounded covariance validation")));

    writeFileSync(stdFile, std);
    writeFileSync(stdFile, std.replace("relative_tolerance > 1e-6", "relative_tolerance > 1e-3"));
    result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("bounded covariance validation")));

    writeFileSync(stdFile, std.replace("factor.get(row, column)", "factor.get(row, row)"));
    result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("bounded covariance validation")));

    writeFileSync(stdFile, std.replace(
      "if source.source_sigma != 0.0 and expected_variance == 0.0 {",
      "if source.source_sigma != 0.0 {",
    ));
    result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("bounded covariance validation")));

    writeFileSync(stdFile, std);
    writeFileSync(stdFile, std.replace(
      "matrix.get(i, i) / scale",
      "matrix.get(i, i)",
    ));
    result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("bounded covariance validation")));

    writeFileSync(stdFile, std.replace(
      "covariance_correlation_matrix(model.matrix, model.relative_tolerance)",
      "covariance_correlation_matrix(covariance_scale * model.matrix, model.relative_tolerance)",
    ));
    result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("bounded covariance validation")));

    writeFileSync(stdFile, std.replace(
      "adjustment = sqrt(diagonal) / source_sigma",
      "adjustment = 1.0",
    ));
    result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("bounded covariance validation")));

    writeFileSync(stdFile, std.replace(
      "max_exact_integer_root = 94906265.0",
      "max_exact_integer_root = 0.0",
    ));
    result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("bounded covariance validation")));

    writeFileSync(stdFile, std.replace(
      "if abs(scaled_projection) <= projection_limit {",
      "if false {",
    ));
    result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("bounded covariance validation")));

    writeFileSync(stdFile, std.replace(
      "sum = lower + upper",
      "sum = lower - upper",
    ));
    result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("normalized PSD analysis and scale-safe midpoint")));

    writeFileSync(stdFile, std);
    const labFile = path.join(root, "scripts/lab-data.mjs");
    const lab = readFileSync(labFile, "utf8");
    writeFileSync(labFile, lab.replace("with J·Σ·Jᵀ", "with independent variances"));
    result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("website Measurement covariance example")));

    writeFileSync(labFile, lab.replace("Source contributions use exact power-of-two normalization", "Source contributions use rounded normalization"));
    result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("website Measurement covariance example")));

    writeFileSync(labFile, lab.replace(
      "Negative and numerically unresolved positive pivots are rejected rather than changing the covariance result",
      "negative pivots are accepted",
    ));
    result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("website Measurement covariance example")));

    writeFileSync(labFile, lab);
    const testFile = path.join(root, "compiler/tests/examples.rs");
    const tests = readFileSync(testFile, "utf8");
    writeFileSync(testFile, tests.replace("rank-deficient cancellation uncertainty = 0", "rank case removed"));
    result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("analytical oracle, runtime parity")));

    writeFileSync(testFile, tests.replace("subnormal covariance cancellation variance = 0", "subnormal case removed"));
    result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("analytical oracle, runtime parity")));

    writeFileSync(testFile, tests.replace("wide-scale covariance cancellation variance = 0", "wide-scale case removed"));
    result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("analytical oracle, runtime parity")));

    writeFileSync(testFile, tests.replace("rank-three compensated variance = 1", "rank-three case removed"));
    result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("analytical oracle, runtime parity")));

    writeFileSync(testFile, tests.replace("rank-three reordered compensated variance = 1", "rank-three reorder case removed"));
    result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("analytical oracle, runtime parity")));

    writeFileSync(testFile, tests.replace("sqrt-three rank-one cancellation variance = 0", "sqrt-three case removed"));
    result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("analytical oracle, runtime parity")));

    writeFileSync(testFile, tests.replace("direct contribution cancellation variance = 16", "direct contribution case removed"));
    result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("analytical oracle, runtime parity")));

    writeFileSync(testFile, tests.replace(
      "rounded perfect correlation = correlation coefficient is numerically unresolved",
      "rounded perfect correlation = accepted",
    ));
    result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("Measurement<Quantity<D>> correlation")));

    writeFileSync(stdFile, std.replace("correlation coefficient is numerically unresolved", "correlation coefficient accepted"));
    result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("Measurement<Quantity<D>> correlation")));

    writeFileSync(testFile, tests.replace(
      "unit-conversion cancellation = correlated measurement uncertainty is numerically unresolved after unit conversion",
      "unit-conversion cancellation = 0",
    ));
    result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("Measurement<Quantity<D>> correlation")));

    writeFileSync(stdFile, std.replace(
      "variance == 0.0 and has_nonzero_original_contribution and has_lossy_coherent_conversion",
      "variance == 0.0 and has_nonzero_original_contribution and false",
    ));
    result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("Measurement<Quantity<D>> correlation")));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});


