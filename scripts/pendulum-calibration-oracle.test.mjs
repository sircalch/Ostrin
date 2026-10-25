import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const input = JSON.parse(readFileSync(path.join(root, "experiments/pendulum-calibration.inputs.json"), "utf8"));

function mean(values) {
  return values.reduce((sum, value) => sum + value, 0) / values.length;
}

// Independent oracle: fit with a separate centered OLS implementation and
// estimate source derivatives by symmetric finite differences, rather than
// copying the analytical derivative expressions used by the Ostrin example.
export function pendulumCalibrationOracle(parameters) {
  const lengths = [...parameters.length_m];
  const periods = [...parameters.period_s];
  const n = lengths.length;
  if (n < 3 || periods.length !== n) throw new RangeError("paired readings are required");

  function fitGravity(xs, ts) {
    const ys = ts.map((period) => period * period);
    const xMean = mean(xs);
    const yMean = mean(ys);
    const sxx = xs.reduce((sum, value) => sum + (value - xMean) ** 2, 0);
    const sxy = xs.reduce((sum, value, index) => sum + (value - xMean) * (ys[index] - yMean), 0);
    const slope = sxy / sxx;
    const intercept = yMean - slope * xMean;
    const gravity = 4 * Math.PI ** 2 / slope;
    const residualSum = ys.reduce((sum, value, index) => sum + (value - (slope * xs[index] + intercept)) ** 2, 0);
    const totalSum = ys.reduce((sum, value) => sum + (value - yMean) ** 2, 0);
    return { slope, intercept, gravity, rSquared: 1 - residualSum / totalSum };
  }

  const fit = fitGravity(lengths, periods);
  const step = 1e-5;
  const lengthEffects = lengths.map((_, index) => {
    const plus = [...lengths];
    const minus = [...lengths];
    plus[index] += step;
    minus[index] -= step;
    return ((fitGravity(plus, periods).gravity - fitGravity(minus, periods).gravity) / (2 * step))
      * parameters.length_repeat_sigma_m;
  });
  const periodEffects = periods.map((_, index) => {
    const plus = [...periods];
    const minus = [...periods];
    plus[index] += step;
    minus[index] -= step;
    return ((fitGravity(lengths, plus).gravity - fitGravity(lengths, minus).gravity) / (2 * step))
      * parameters.period_repeat_sigma_s;
  });

  const plusLengths = lengths.map((value) => value + step);
  const minusLengths = lengths.map((value) => value - step);
  const rulerZeroEffect = ((fitGravity(plusLengths, periods).gravity - fitGravity(minusLengths, periods).gravity) / (2 * step))
    * parameters.ruler_zero_sigma_m;
  const plusPeriods = periods.map((value) => value + step);
  const minusPeriods = periods.map((value) => value - step);
  const clockZeroEffect = ((fitGravity(lengths, plusPeriods).gravity - fitGravity(lengths, minusPeriods).gravity) / (2 * step))
    * parameters.clock_zero_sigma_s;

  const independentVariance = [...lengthEffects, ...periodEffects]
    .reduce((sum, contribution) => sum + contribution * contribution, 0);
  const correlation = parameters.ruler_clock_zero_correlation;
  const totalVariance = independentVariance + rulerZeroEffect ** 2 + clockZeroEffect ** 2
    + 2 * correlation * rulerZeroEffect * clockZeroEffect;
  return {
    ...fit,
    independentStandardUncertainty: Math.sqrt(independentVariance),
    standardUncertainty: Math.sqrt(totalVariance),
    rulerZeroEffect,
    clockZeroEffect,
  };
}

test("pendulum workflow has a finite-difference oracle for its declared synthetic fixture", () => {
  const result = pendulumCalibrationOracle(input.parameters);
  assert.ok(Number.isFinite(result.gravity));
  assert.ok(Number.isFinite(result.standardUncertainty));
  assert.ok(Math.abs(result.gravity - 9.8084) < 0.001);
  assert.ok(Math.abs(result.standardUncertainty - 0.0485) < 0.0001);
  assert.ok(Math.abs(result.slope - 4.025) < 0.0001);
  assert.ok(Math.abs(result.rSquared - 0.999979) < 1e-6);
  assert.ok(Math.abs(result.independentStandardUncertainty - 0.04826689993) < 1e-9);
  assert.ok(Math.abs(result.rulerZeroEffect) < 1e-8, "a shared ruler zero shift should cancel in a fit with an intercept");
  assert.ok(Math.abs(result.clockZeroEffect) > 0, "a shared clock zero shift should affect period squared");
});
