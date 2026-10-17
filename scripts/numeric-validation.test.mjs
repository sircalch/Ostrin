import test from "node:test";
import assert from "node:assert/strict";
import { checkTolerance, compareReports, directDft, parseEmittedNumbers, validateFft } from "./numeric-validation.mjs";

test("CI provenance is only emitted when GitHub Actions identifies a run", () => {
  const previous = {
    actions: process.env.GITHUB_ACTIONS,
    run: process.env.GITHUB_RUN_ID,
    server: process.env.GITHUB_SERVER_URL,
    repository: process.env.GITHUB_REPOSITORY,
    sha: process.env.GITHUB_SHA,
    workflow: process.env.GITHUB_WORKFLOW,
  };
  process.env.GITHUB_ACTIONS = "true";
  process.env.GITHUB_RUN_ID = "123";
  process.env.GITHUB_SERVER_URL = "https://github.com";
  process.env.GITHUB_REPOSITORY = "sircalch/Ostrin";
  process.env.GITHUB_SHA = "abc";
  process.env.GITHUB_WORKFLOW = "CI";
  try {
    const url = `${process.env.GITHUB_SERVER_URL}/${process.env.GITHUB_REPOSITORY}/actions/runs/${process.env.GITHUB_RUN_ID}`;
    assert.equal(url, "https://github.com/sircalch/Ostrin/actions/runs/123");
  } finally {
    for (const [key, value] of Object.entries({
      GITHUB_ACTIONS: previous.actions,
      GITHUB_RUN_ID: previous.run,
      GITHUB_SERVER_URL: previous.server,
      GITHUB_REPOSITORY: previous.repository,
      GITHUB_SHA: previous.sha,
      GITHUB_WORKFLOW: previous.workflow,
    })) {
      if (value === undefined) delete process.env[key]; else process.env[key] = value;
    }
  }
});

test("the numeric-output reader accepts full-precision finite values", () => {
  assert.deepEqual(parseEmittedNumbers("1\r\n-0.7500000000000001\r\n3.2e-14\r\n"), [
    1,
    -0.7500000000000001,
    3.2e-14,
  ]);
});

test("the numeric-output reader rejects NaN and unrelated text", () => {
  assert.throws(() => parseEmittedNumbers("NaN\n"), /non-finite/);
  assert.throws(() => parseEmittedNumbers("residual=0\n"), /non-finite/);
});

test("absolute and relative tolerances scale with the declared reference magnitude", () => {
  const accepted = checkTolerance("residual", 2.5e-8, 10, 1e-8, 2e-9);
  assert.equal(accepted.value, 2.5e-8);
  assert.ok(Math.abs(accepted.limit - 3e-8) < 1e-23);
  assert.equal(accepted.scale, 10);
  assert.throws(() => checkTolerance("residual", 3.1e-8, 10, 1e-8, 2e-9), /exceeds abs/);
});

test("interpreter and native reports require matching numeric leaves", () => {
  const left = { solution: [1, -2], residual: { value: 1e-15 } };
  const right = { solution: [1 + 1e-14, -2], residual: { value: 1e-15 } };
  const report = compareReports(left, right);
  assert.equal(report.comparedValues, 3);
  assert.ok(report.maximumAbsoluteDifference <= 1e-13);
  assert.throws(
    () => compareReports(left, { solution: [1.01, -2], residual: { value: 1e-15 } }),
    /interpreter\/native parity/
  );
});

test("FFT validation checks a nontrivial power-of-two DFT, inverse, and Parseval energy", () => {
  const signal = [1, 2, 3, 4];
  const spectrum = directDft(signal);
  assert.equal(spectrum.re[0], 10);
  for (const value of spectrum.re.slice(1)) assert.ok(Math.abs(value + 2) < 1e-12);
  assert.ok(Math.abs(spectrum.im[1] - 2) < 1e-12);
  assert.ok(Math.abs(spectrum.im[3] + 2) < 1e-12);
  const values = {
    numbers: [...spectrum.re, ...spectrum.im, ...signal],
    offset: 0,
  };
  const report = validateFft(values, {
    cases: { radix2: { signal, absolute: 1e-12, relative: 1e-12 } },
  });
  assert.ok(report.radix2.spectralResidual < 1e-12);
  assert.ok(report.radix2.maximumReconstructionError < 1e-12);
  assert.equal(report.radix2.inputEnergy, 30);
  assert.ok(Math.abs(report.radix2.spectralEnergy - 120) < 1e-12);
  assert.ok(report.radix2.parsevalResidual < 1e-12);
  assert.equal(values.offset, 12);
});

test("FFT validation rejects a spectrum that violates the independent DFT reference", () => {
  const values = { numbers: [10, -2, -2, -2, 0, 2.5, 0, -2, 1, 2, 3, 4], offset: 0 };
  assert.throws(() => validateFft(values, {
    cases: { radix2: { signal: [1, 2, 3, 4], absolute: 1e-12, relative: 1e-12 } },
  }), /imaginary DFT bin 1/);
});
