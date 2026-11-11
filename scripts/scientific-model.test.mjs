// Checks examples/scientific_model.ostrin against analytic solutions computed here, independently of the
// Ostrin program: the exponential decay m0 exp(-k t) and the underdamped oscillator. Each row of the exported
// tables must agree with the analytic value at its own time, and the stored error must be the difference.
import test from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync, existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const examples = join(here, "..", "examples");
const binary = join(here, "..", "compiler", "target", "release", process.platform === "win32" ? "ostrinc.exe" : "ostrinc");
const output = existsSync(binary) ? execFileSync(binary, ["--run", "scientific_model.ostrin"], { cwd: examples, encoding: "utf8" }) : null;

// Parameters exactly as the program states them.
const m0 = 2.0;
const k = 0.35;
const tEnd = 12.0;
const zeta = 0.1;
const omega = 2.0;
const x0 = 0.5;
const v0 = 0.0;

const readRows = (file) =>
  readFileSync(join(examples, "data", "model", file), "utf8")
    .trim()
    .split("\n")
    .slice(1)
    .map((line) => line.split(",").map(Number));

test("decay: every row matches m0 exp(-k t), and the error column is the difference", { skip: output === null }, () => {
  const rows = readRows("decay.csv");
  assert.equal(rows.length, 1201);
  let worst = 0;
  for (const [t, numeric, analytic, error] of rows) {
    const exact = m0 * Math.exp(-k * t);
    assert.ok(Math.abs(analytic - exact) <= 1e-12 * m0, `analytic column at t=${t}`);
    assert.ok(Math.abs(numeric - exact) <= 1e-9, `numerical mass at t=${t}: ${numeric} vs ${exact}`);
    assert.ok(Math.abs(error - Math.abs(numeric - analytic)) <= 1e-15, `error column at t=${t}`);
    worst = Math.max(worst, Math.abs(numeric - exact));
  }
  assert.ok(worst < 1e-9, `worst decay error ${worst}`);
  assert.ok(Math.abs(rows[rows.length - 1][0] - tEnd) < 1e-12, "the grid ends at t_end");
});

test("oscillator: every row matches the underdamped solution, and the error column is the difference", { skip: output === null }, () => {
  const rows = readRows("oscillator.csv");
  assert.equal(rows.length, 2401);
  const omegaD = omega * Math.sqrt(1 - zeta * zeta);
  let worst = 0;
  for (const [t, numeric, analytic, error] of rows) {
    const exact = Math.exp(-zeta * omega * t) * (x0 * Math.cos(omegaD * t) + ((v0 + zeta * omega * x0) / omegaD) * Math.sin(omegaD * t));
    assert.ok(Math.abs(analytic - exact) <= 1e-12, `analytic position at t=${t}`);
    assert.ok(Math.abs(numeric - exact) <= 1e-7, `numerical position at t=${t}: ${numeric} vs ${exact}`);
    assert.ok(Math.abs(error - Math.abs(numeric - analytic)) <= 1e-15, `error column at t=${t}`);
    worst = Math.max(worst, Math.abs(numeric - exact));
  }
  assert.ok(worst < 1e-7, `worst oscillator error ${worst}`);
});

test("the program reports the solver's accuracy and detects the convergence failure", { skip: output === null }, () => {
  assert.match(output, /decay: 1201 RK4 nodes/);
  assert.match(output, /decay RK45 final mass error/);
  assert.match(output, /convergence failure detected: stopped at t = 0\.35/);
  assert.match(output, /oscillator: 2401 RK4 nodes/);
  assert.ok(!output.includes("unexpected"), output);
});

test("the figures and tables are written", { skip: output === null }, () => {
  for (const file of ["model/decay_figure.svg", "model/oscillator_figure.svg"]) {
    assert.match(readFileSync(join(examples, "data", file), "utf8"), /<svg/);
  }
});
