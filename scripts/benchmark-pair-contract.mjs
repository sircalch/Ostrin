import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const floatArrayWorkload = "examples/benchmark_float_array.ostrin";
export const quantityArrayWorkload = "examples/benchmark_quantity_array.ostrin";
export const pairAbsoluteTolerance = 1e-13;
export const pairRelativeTolerance = 1e-12;

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

function transformedExpression(source, label) {
  const match = source.match(/^\s*transformed\s*=\s*(.+)$/m);
  if (!match) throw new Error(`${label}: missing transformed array expression`);
  return match[1].trim().replace(/\s+/g, " ");
}

function normalizeQuantityExpression(expression) {
  return expression.replace(/\(\s*1(?:\.0)?\s+m\s*\)/g, "1.0");
}

export function validateQuantityFloatPairSources(floatSource, quantitySource) {
  const floatExpression = transformedExpression(floatSource, floatArrayWorkload);
  const quantityExpression = transformedExpression(quantitySource, quantityArrayWorkload);
  if (floatExpression !== normalizeQuantityExpression(quantityExpression)) {
    throw new Error(
      `Float/Quantity benchmark operations are not aligned:\nFloat: ${floatExpression}\nQuantity normalized: ${normalizeQuantityExpression(quantityExpression)}`,
    );
  }
  return { floatExpression, quantityExpression };
}

export function validateCheckedInQuantityFloatPair(root = repositoryRoot) {
  const floatSource = readFileSync(path.join(root, floatArrayWorkload), "utf8");
  const quantitySource = readFileSync(path.join(root, quantityArrayWorkload), "utf8");
  return validateQuantityFloatPairSources(floatSource, quantitySource);
}

function numericValues(output, label) {
  const lines = output.replaceAll("\r\n", "\n").split("\n").map((line) => line.trim()).filter(Boolean);
  if (lines.length === 0) throw new Error(`${label}: output contains no numeric values`);
  return lines.map((line, index) => {
    const match = line.match(/^([+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?)(?:\s+.+)?$/);
    if (!match) throw new Error(`${label}: output line ${index + 1} is not a numeric value with an optional unit: ${line}`);
    const value = Number(match[1]);
    if (!Number.isFinite(value)) throw new Error(`${label}: output line ${index + 1} is not finite`);
    return value;
  });
}

export function compareQuantityFloatOutputs(floatOutput, quantityOutput) {
  const floatValues = numericValues(floatOutput, floatArrayWorkload);
  const quantityValues = numericValues(quantityOutput, quantityArrayWorkload);
  if (floatValues.length !== quantityValues.length) {
    throw new Error(
      `Float/Quantity benchmark output lengths differ: ${floatValues.length} versus ${quantityValues.length}`,
    );
  }

  let maximumAbsoluteDifference = 0;
  for (let index = 0; index < floatValues.length; index += 1) {
    const difference = Math.abs(floatValues[index] - quantityValues[index]);
    const scale = Math.max(Math.abs(floatValues[index]), Math.abs(quantityValues[index]));
    const limit = pairAbsoluteTolerance + pairRelativeTolerance * scale;
    if (difference > limit) {
      throw new Error(
        `Float/Quantity benchmark outputs differ at value ${index}: ${floatValues[index]} versus ${quantityValues[index]} (difference ${difference}, limit ${limit})`,
      );
    }
    maximumAbsoluteDifference = Math.max(maximumAbsoluteDifference, difference);
  }

  return {
    comparedValues: floatValues.length,
    maximumAbsoluteDifference,
    absoluteTolerance: pairAbsoluteTolerance,
    relativeTolerance: pairRelativeTolerance,
  };
}
