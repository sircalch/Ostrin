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
    writeFileSync(file, original.replace("native smoke now has parity", "native support"));
    let result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("native-smoke and interpreter/WASM scope")));

    writeFileSync(file, original.replace("The native type report still shows AST fallback and generic type divergences, so this is output evidence for those cases, not proof of complete typed lowering.", "Native typed lowering is complete."));
    result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("fallback and type-divergence limitation")));

    writeFileSync(file, original.replace("Array<Measurement>, covariance, Monte Carlo", "native support"));
    result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("arrays, covariance, and Monte Carlo boundary")));

    writeFileSync(file, original.replace("div_quantity is instantiated with equal dimensions and should return Err; this remains an explicit experimental boundary", "native parity"));
    result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("remaining dimensional Measurement native boundary")));
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

test("the gate rejects documentation that enables arrays prematurely", () => {
  const root = fixtureRoot();
  try {
    const file = path.join(root, "docs/design/25-mediciones-e-incertidumbre.md");
    const source = readFileSync(file, "utf8")
      .replaceAll("no habilita todavía", "habilita")
      .replaceAll("siguen pendientes", "ya están habilitados");
    writeFileSync(file, source);
    const result = validateMeasurementContract(root);
    assert.ok(result.errors.some((error) => error.includes("array Measurement boundary")));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});


