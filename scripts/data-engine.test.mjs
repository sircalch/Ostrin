// Checks examples/data_engine.ostrin against the independent reference written by data-engine-fixture.mjs.
// The numbers printed by the program are compared with the exact rational reference within a relative tolerance
// of 1e-12 (the program works in double precision; the reference rounds only at the end).
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
const output = existsSync(binary)
  ? execFileSync(binary, ["--run", "data_engine.ostrin"], { cwd: examples, encoding: "utf8" })
  : null;

function value(prefix) {
  const line = output.split("\n").find((l) => l.startsWith(prefix));
  assert.ok(line, `missing line starting with ${prefix}`);
  return line;
}

function number(line, key) {
  const match = line.match(new RegExp(`${key}=(-?[0-9.eE+-]+)`));
  assert.ok(match, `missing ${key} in: ${line}`);
  return Number(match[1]);
}

function close(actual, expected, label) {
  const scale = Math.max(1, Math.abs(expected));
  assert.ok(Math.abs(actual - expected) <= 1e-12 * scale, `${label}: ${actual} vs ${expected}`);
}

test("the program is present and runs", { skip: output === null ? "compiler binary not built" : false }, () => {
  assert.match(output, /rows read: 36/);
});

test("descriptive statistics match the exact reference", { skip: output === null }, () => {
  const response = value("response:");
  close(number(response, "mean"), reference.response.mean, "response mean");
  close(number(response, "sd"), reference.response.sd, "response sd");
  close(number(response, "min"), reference.response.min, "response min");
  close(number(response, "max"), reference.response.max, "response max");
  close(number(response, "median"), reference.response.median, "response median");
  const temperature = value("temperature:");
  close(number(temperature, "mean"), reference.temperature.mean, "temperature mean");
  close(number(temperature, "sd"), reference.temperature.sd, "temperature sd");
  close(Number(value("correlation").split(": ")[1]), reference.correlation_temperature_response, "correlation");
});

test("linear and quadratic fits in physical units match the exact regression", { skip: output === null }, () => {
  const linear = value("linear:");
  close(number(linear, "intercept"), reference.linear.intercept, "linear intercept");
  close(number(linear, "slope"), reference.linear.slope, "linear slope");
  close(number(linear, "sse"), reference.linear.sse, "linear sse");
  close(number(linear, "rmse"), reference.linear.rmse, "linear rmse");
  close(number(linear, "r2"), reference.linear.r_squared, "linear r2");
  const quadratic = value("quadratic:");
  close(number(quadratic, "intercept"), reference.quadratic.intercept, "quadratic intercept");
  close(number(quadratic, "linear"), reference.quadratic.linear, "quadratic linear");
  close(number(quadratic, "quadratic"), reference.quadratic.quadratic, "quadratic quadratic");
  close(number(quadratic, "sse"), reference.quadratic.sse, "quadratic sse");
  close(number(quadratic, "r2"), reference.quadratic.r_squared, "quadratic r2");
});

test("group means match the exact reference, in order of first appearance", { skip: output === null }, () => {
  const lines = output.split("\n");
  const start = lines.indexOf("group summary by operator:") + 2;
  reference.group_means.forEach((group, i) => {
    // The operator may contain commas (it is quoted in the output), so the three numbers are read from the end.
    const cells = lines[start + i].split(",").slice(-3);
    close(Number(cells[1]), group.mean, `group ${group.operator} mean`);
    assert.equal(Number(cells[0]), group.n);
  });
});

test("the counts and the missing-value policy are exact", { skip: output === null }, () => {
  assert.ok(output.includes(`rows read: ${reference.rows}
`), output);
  assert.ok(output.includes(`rows with a pressure value: ${reference.rows - reference.rows_with_missing_pressure}
`), output);
});
