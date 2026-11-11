// Checks that the figures and the table written by examples/data_figures.ostrin carry the computed values.
// The table's predicted values are compared with the exact quadratic reference from data-engine-fixture.mjs,
// row by row, and each SVG must draw one marker per measurement.
import test from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync, existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const examples = join(here, "..", "examples");
const binary = join(here, "..", "compiler", "target", "release", process.platform === "win32" ? "ostrinc.exe" : "ostrinc");
const reference = JSON.parse(readFileSync(join(examples, "data", "calibration.reference.json"), "utf8"));
const csv = readFileSync(join(examples, "data", "calibration.csv"), "utf8");
if (existsSync(binary)) {
  execFileSync(binary, ["--run", "data_figures.ostrin"], { cwd: examples, encoding: "utf8" });
}
const table = readFileSync(join(examples, "data", "figures", "calibration_table.csv"), "utf8").trim().split("\n");
const fit = readFileSync(join(examples, "data", "figures", "calibration_fit.svg"), "utf8");
const residuals = readFileSync(join(examples, "data", "figures", "calibration_residuals.svg"), "utf8");

// The measured temperatures and responses, in file order, read from the CSV (independent of the Ostrin reader).
const measured = csv
  .trim()
  .split("\n")
  .slice(1)
  .filter((line) => line.trim() !== "")
  .map((line) => {
    // The operator field may be quoted with commas; temperature and response are the two fields after it.
    const fields = line.match(/("(?:[^"]|"")*"|[^,]*)/g).filter((_, i) => i % 2 === 0);
    return { temperature: Number(fields[2]), response: Number(fields[4]) };
  });

const quadratic = reference.quadratic;
const predict = (t) => quadratic.intercept + quadratic.linear * t + quadratic.quadratic * t * t;

test("the table has one row per measurement, with the measured values", () => {
  assert.equal(table.length, 1 + measured.length);
  assert.equal(table[0], "temperature_c,response,predicted,residual");
});

test("the predicted values in the table match the exact quadratic reference row by row", () => {
  measured.forEach((m, i) => {
    const [t, response, predicted, residual] = table[i + 1].split(",").map(Number);
    assert.ok(Math.abs(t - m.temperature) < 1e-12, `temperature row ${i}`);
    assert.ok(Math.abs(response - m.response) < 1e-12, `response row ${i}`);
    const expected = predict(m.temperature);
    assert.ok(Math.abs(predicted - expected) <= 1e-12 * Math.max(1, Math.abs(expected)), `predicted row ${i}: ${predicted} vs ${expected}`);
    assert.ok(Math.abs(residual - (m.response - expected)) < 1e-10, `residual row ${i}`);
  });
});

test("the residuals of a least-squares fit with an intercept sum to zero, and their squares give the reference SSE", () => {
  const rows = table.slice(1).map((line) => line.split(",").map(Number));
  const sum = rows.reduce((acc, r) => acc + r[3], 0);
  assert.ok(Math.abs(sum) < 1e-12, `residual sum ${sum}`);
  const sse = rows.reduce((acc, r) => acc + r[3] * r[3], 0);
  assert.ok(Math.abs(sse - quadratic.sse) <= 1e-12 * Math.max(1, quadratic.sse), `sse ${sse} vs ${quadratic.sse}`);
});

test("each figure draws one marker per measurement, and the residual figure draws the zero line", () => {
  // Data points carry class "pt"; the legend draws one more circle that is not a measurement.
  const markers = (svg) => (svg.match(/<circle class="pt"/g) ?? []).length;
  assert.equal(markers(fit), measured.length);
  assert.equal(markers(residuals), measured.length);
  assert.match(fit, /quadratic fit/);
  assert.match(fit, /linear fit/);
  assert.match(residuals, /Residuals of the quadratic fit/);
});
