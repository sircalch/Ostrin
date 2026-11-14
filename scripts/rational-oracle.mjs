#!/usr/bin/env node
// Independent oracle for std.rational (univariate rational functions and their domains).
//
// The oracle computes the value of each expression from its UN-REDUCED formula, with its own BigInt rational type and its
// own polynomial evaluation (Horner on coefficient lists). A point is undefined when one of the original denominators
// vanishes there (for division also the divisor's numerator; for composition the inner denominator and the outer
// denominator at the inner value). The Ostrin module must agree exactly, including the undefined points, so a reduction
// that loses an exclusion is a failure.
//
// Modes: default (checks), --write (regenerate tests/oracle/rational-* and examples/rational_oracle.ostrin), --check.

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");
const CASES_PATH = join(root, "tests", "oracle", "rational-cases.json");
const EXPECTED_PATH = join(root, "tests", "oracle", "rational-expected.txt");
const PROGRAM_PATH = join(root, "examples", "rational_oracle.ostrin");

// ---------------------------------------------------------------- an independent exact rational type

const abs = (x) => (x < 0n ? -x : x);
function gcd(a, b) {
  a = abs(a);
  b = abs(b);
  while (b !== 0n) [a, b] = [b, a % b];
  return a;
}
function q(n, d = 1n) {
  if (d === 0n) throw new RangeError("zero denominator");
  if (d < 0n) [n, d] = [-n, -d];
  const g = gcd(n, d);
  return { n: n / g, d: d / g };
}
const qadd = (x, y) => q(x.n * y.d + y.n * x.d, x.d * y.d);
const qsub = (x, y) => q(x.n * y.d - y.n * x.d, x.d * y.d);
const qmul = (x, y) => q(x.n * y.n, x.d * y.d);
const qdiv = (x, y) => q(x.n * y.d, x.d * y.n);
const qzero = (x) => x.n === 0n;
const qtext = (x) => (x.d === 1n ? x.n.toString() : `${x.n}/${x.d}`);
function qparse(s) {
  if (!/^-?\d+(\/\d+)?$/.test(s)) throw new SyntaxError(`not a rational: ${s}`);
  const [a, b] = s.split("/");
  return q(BigInt(a), b === undefined ? 1n : BigInt(b));
}

// ---------------------------------------------------------------- coefficient lists (lowest degree first)

/** Horner evaluation of a coefficient list at a rational point; the empty list is the zero polynomial. */
function evaluate(coefficients, t) {
  let acc = q(0n);
  for (let i = coefficients.length - 1; i >= 0; i--) {
    acc = qadd(qmul(acc, t), qparse(coefficients[i]));
  }
  return acc;
}

/** Coefficients of the derivative, as rational texts. */
function derivativeCoefficients(coefficients) {
  return coefficients.slice(1).map((c, i) => qtext(qmul(qparse(c), q(BigInt(i + 1)))));
}

// ---------------------------------------------------------------- deterministic generator

function mulberry32(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
const randInt = (rng, lo, hi) => lo + Math.floor(rng() * (hi - lo + 1));

/** Integer coefficients, lowest first, with the given degree; the leading coefficient is nonzero. */
function randomPolynomial(rng, degree) {
  const out = [];
  for (let i = 0; i <= degree; i++) out.push(randInt(rng, -3, 3));
  if (out[degree] === 0) out[degree] = 1;
  return out;
}

/** Product of integer coefficient lists (for building common factors on purpose). */
function multiplyInt(a, b) {
  const out = new Array(a.length + b.length - 1).fill(0);
  a.forEach((x, i) => b.forEach((y, j) => (out[i + j] += x * y)));
  return out;
}

const toText = (xs) => xs.map(String);

const POINTS = ["-2", "-1", "0", "1", "2", "3", "1/2", "-3/2"];

// ---------------------------------------------------------------- the expected value of an operation, from the formula

/** The value of the un-reduced expression at t, or null when the original formula is undefined there. */
function formulaValue(op, f, g, t) {
  const Nf = f.numerator;
  const Df = f.denominator;
  // a unary operation has no second operand; its Ng and Dg are not read
  const Ng = g ? g.numerator : [];
  const Dg = g ? g.denominator : ["1"];
  const nf = evaluate(Nf, t);
  const df = evaluate(Df, t);
  switch (op) {
    case "add":
    case "subtract":
    case "multiply": {
      const ng = evaluate(Ng, t);
      const dg = evaluate(Dg, t);
      if (qzero(df) || qzero(dg)) return null;
      if (op === "multiply") return qdiv(qmul(nf, ng), qmul(df, dg));
      const numerator = op === "add" ? qadd(qmul(nf, dg), qmul(ng, df)) : qsub(qmul(nf, dg), qmul(ng, df));
      return qdiv(numerator, qmul(df, dg));
    }
    case "divide": {
      const ng = evaluate(Ng, t);
      const dg = evaluate(Dg, t);
      if (qzero(df) || qzero(dg) || qzero(ng)) return null;
      return qdiv(qmul(nf, dg), qmul(df, ng));
    }
    case "derivative": {
      if (qzero(df)) return null;
      const dnf = evaluate(derivativeCoefficients(Nf), t);
      const ddf = evaluate(derivativeCoefficients(Df), t);
      return qdiv(qsub(qmul(dnf, df), qmul(nf, ddf)), qmul(df, df));
    }
    case "compose": {
      const ng = evaluate(Ng, t);
      const dg = evaluate(Dg, t);
      if (qzero(dg)) return null;
      const u = qdiv(ng, dg);
      const du = evaluate(Df, u);
      if (qzero(du)) return null;
      return qdiv(evaluate(Nf, u), du);
    }
    default:
      throw new Error(`unknown operation ${op}`);
  }
}

// ---------------------------------------------------------------- the case file

export function buildCases() {
  const cases = [];
  const rng = mulberry32(20260802);
  const ops = ["add", "subtract", "multiply", "divide", "derivative", "compose"];
  // fixed cases first: the examples of the design
  const fixed = [
    { op: "multiply", f: { numerator: ["-1", "0", "1"], denominator: ["-1", "1"] }, g: { numerator: ["-1", "1"], denominator: ["1", "1"] } },
    { op: "add", f: { numerator: ["-1", "0", "1"], denominator: ["-1", "1"] }, g: { numerator: ["1"], denominator: ["2", "1"] } },
    { op: "divide", f: { numerator: ["-1", "0", "1"], denominator: ["-1", "1"] }, g: { numerator: ["-1", "1"], denominator: ["1"] } },
    { op: "derivative", f: { numerator: ["-1", "0", "1"], denominator: ["-1", "1"] }, g: null },
    { op: "compose", f: { numerator: ["1"], denominator: ["0", "1"] }, g: { numerator: ["1", "1"], denominator: ["1"] } },
    { op: "compose", f: { numerator: ["0", "1"], denominator: ["1"] }, g: { numerator: ["1"], denominator: ["0", "1"] } },
  ];
  for (const c of fixed) cases.push({ ...c, points: POINTS });
  for (let k = 0; k < 60; k++) {
    const op = ops[randInt(rng, 0, ops.length - 1)];
    // a common factor (x - r) on purpose, so that reductions happen and exclusions matter
    const root = [-1, 0, 1, 2][randInt(rng, 0, 3)];
    const factor = [-root, 1];
    const fa = randomPolynomial(rng, randInt(rng, 0, 2));
    const fb = randomPolynomial(rng, randInt(rng, 0, 2));
    const f = { numerator: toText(multiplyInt(factor, fa)), denominator: toText(multiplyInt(factor, fb)) };
    const ga = randomPolynomial(rng, randInt(rng, 0, 2));
    const gb = randomPolynomial(rng, randInt(rng, 0, 2));
    // g's numerator is nonconstant or at least nonzero, so that composition and division are well defined
    const gn = op === "compose" ? randomPolynomial(rng, randInt(rng, 1, 2)) : ga;
    const g = { numerator: toText(gn), denominator: toText(gb) };
    cases.push({ op, f: { numerator: f.numerator, denominator: f.denominator }, g: op === "derivative" ? null : g, points: POINTS });
  }
  return cases;
}

/** The expected output lines: one per (case, point). */
export function expectedOutputs(cases) {
  const lines = [];
  cases.forEach((c, i) => {
    const f = { numerator: c.f.numerator, denominator: c.f.denominator };
    const g = c.g === null ? null : { numerator: c.g.numerator, denominator: c.g.denominator };
    for (const point of c.points) {
      const t = qparse(point);
      const value = formulaValue(c.op, f, g, t);
      lines.push(`case ${i} at ${point}: ${value === null ? "undefined" : qtext(value)}`);
    }
  });
  return lines;
}

/** The Ostrin program: builds each case with std.rational and evaluates it at each point. Generated. */
export function ostrinProgram(cases) {
  const quote = (s) => JSON.stringify(s);
  const join = (xs) => xs.join(";");
  const calls = cases.map((c, i) => {
    const g = c.g === null ? { numerator: [], denominator: ["1"] } : c.g;
    return `    print(build_case(${i}, ${quote(c.op)}, ${quote(join(c.f.numerator))}, ${quote(join(c.f.denominator))}, ${quote(join(g.numerator))}, ${quote(join(g.denominator))}, ${quote(join(c.points))}))`;
  });
  return `// Generated by scripts/rational-oracle.mjs --write from tests/oracle/rational-cases.json. Do not edit by hand.
// The expected output is tests/oracle/rational-expected.txt, computed from the un-reduced formulas by an independent oracle.
import std.rational

fn coeffs_of(text: String) -> List<String> {
    mut out: List<String> = []
    if text == "" {
        return out
    }
    for piece in text.split(";") {
        out.push(piece)
    }
    out
}

fn text_of(value: Result<String, String>) -> String {
    return match value {
        Err(message) => "undefined",
        Ok(v) => v,
    }
}

fn build_case(index: Int, op: String, nf: String, df: String, ng: String, dg: String, points: String) -> String {
    f = rational.rational_new(coeffs_of(nf), coeffs_of(df)).unwrap()
    g = rational.rational_new(coeffs_of(ng), coeffs_of(dg)).unwrap()
    mut result = f
    if op == "add" {
        result = rational.rational_add(f, g).unwrap()
    } else if op == "subtract" {
        result = rational.rational_subtract(f, g).unwrap()
    } else if op == "multiply" {
        result = rational.rational_multiply(f, g).unwrap()
    } else if op == "divide" {
        result = rational.rational_divide(f, g).unwrap()
    } else if op == "derivative" {
        result = rational.rational_derivative(f).unwrap()
    } else if op == "compose" {
        result = rational.rational_compose(f, g).unwrap()
    }
    mut out = ""
    for point in points.split(";") {
        line = "case " + index.to_string() + " at " + point + ": " + text_of(rational.rational_evaluate(result, point))
        if out == "" {
            out = line
        } else {
            out = out + "\\n" + line
        }
    }
    out
}

fn main() -> Void {
${calls.join("\n")}
}
`;
}

// ---------------------------------------------------------------- command line

function generated() {
  const cases = buildCases();
  return {
    cases: JSON.stringify({ format: "rational-oracle-cases", version: 1, cases }, null, 2) + "\n",
    expected: expectedOutputs(cases).join("\n") + "\n",
    program: ostrinProgram(cases),
  };
}

const GENERATED = [
  [CASES_PATH, "cases"],
  [EXPECTED_PATH, "expected"],
  [PROGRAM_PATH, "program"],
];

function main() {
  const mode = process.argv[2] ?? "";
  if (mode === "--write" || mode === "--check") {
    const files = generated();
    if (mode === "--write") {
      for (const [path, key] of GENERATED) writeFileSync(path, files[key]);
      console.log("wrote tests/oracle/rational-* and examples/rational_oracle.ostrin");
      return;
    }
    const stale = GENERATED.filter(([path, key]) => {
      try {
        return readFileSync(path, "utf8").replace(/\r\n/g, "\n") !== files[key];
      } catch {
        return true;
      }
    });
    if (stale.length > 0) {
      console.error(`stale generated files; run with --write`);
      process.exit(1);
    }
    console.log("generated files are current");
    return;
  }
  const cases = buildCases();
  const lines = expectedOutputs(cases);
  const undefinedCount = lines.filter((l) => l.endsWith("undefined")).length;
  console.log(`rational cases: ${cases.length}, points: ${lines.length}, undefined: ${undefinedCount}`);
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  main();
}
