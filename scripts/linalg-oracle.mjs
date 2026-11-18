#!/usr/bin/env node
// Independent oracle for std.linalg (norms, residuals, checked solve).
//
// The oracle never calls Ostrin or std.numeric. For every case it takes the matrix and right-hand side exactly as the
// Ostrin program receives them (each decimal literal is a binary double, so the oracle uses that double's exact rational
// value), solves the system in exact BigInt rational arithmetic, and computes the exact norms, the exact inverse and the
// exact 1-norm condition number. Expected values carry a tolerance taken from the rounding-error bound of LU with partial
// pivoting, so a failing comparison means the algorithm is wrong, not that the tolerance was loosened to pass.
//
// Expected tokens (read by tests/examples.rs, `linalg_oracle_agrees_in_both_backends`):
//   text            must match exactly;
//   v|t             a number within t of v (t is the tolerance);
//   <=b             a number that must not exceed b;
//   a|b             either text a or text b (used where rounding can move a result across a threshold).
//
// Modes: default (print the number of cases), --write (regenerate examples/linalg_oracle.ostrin and
// tests/oracle/linalg-expected.txt), --check (fail if they are out of date).

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");
const PROGRAM_PATH = join(root, "examples", "linalg_oracle.ostrin");
const EXPECTED_PATH = join(root, "tests", "oracle", "linalg-expected.txt");
const EPS = 2.220446049250313e-16;
const LIMIT = 1e12;

// ---------------------------------------------------------------- exact rationals on BigInt

const gcd = (a, b) => {
  a = a < 0n ? -a : a;
  b = b < 0n ? -b : b;
  while (b) [a, b] = [b, a % b];
  return a;
};
const R = (n, d = 1n) => {
  if (d === 0n) throw new RangeError("zero denominator");
  if (d < 0n) [n, d] = [-n, -d];
  const g = gcd(n, d) || 1n;
  return { n: n / g, d: d / g };
};
const add = (x, y) => R(x.n * y.d + y.n * x.d, x.d * y.d);
const sub = (x, y) => R(x.n * y.d - y.n * x.d, x.d * y.d);
const mul = (x, y) => R(x.n * y.n, x.d * y.d);
const div = (x, y) => R(x.n * y.d, x.d * y.n);
const isZero = (x) => x.n === 0n;
const abs = (x) => (x.n < 0n ? { n: -x.n, d: x.d } : x);
const less = (x, y) => x.n * y.d < y.n * x.d;
const toNumber = (x) => Number(x.n) / Number(x.d);

// The exact rational value of a double (doubling is exact, so the loop terminates after at most 1074 steps).
function exactOf(x) {
  if (!Number.isFinite(x)) throw new RangeError("not finite");
  if (x === 0) return R(0n);
  let m = x;
  let e = 0n;
  while (!Number.isInteger(m)) {
    m *= 2;
    e += 1n;
  }
  return R(BigInt(m), 1n << e);
}

// Exact solution of A x = b by Gauss-Jordan on rationals. Returns { singular: true } or { x, inverse, det }.
function exactSolve(a, b) {
  const n = a.length;
  const m = a.map((row, i) => [...row, ...Array.from({ length: n }, (_, j) => R(i === j ? 1n : 0n))]);
  let det = R(1n);
  for (let k = 0; k < n; k += 1) {
    let p = k;
    while (p < n && isZero(m[p][k])) p += 1;
    if (p === n) return { singular: true };
    if (p !== k) {
      [m[p], m[k]] = [m[k], m[p]];
      det = mul(det, R(-1n));
    }
    const pivot = m[k][k];
    det = mul(det, pivot);
    for (let j = 0; j < 2 * n; j += 1) m[k][j] = div(m[k][j], pivot);
    for (let i = 0; i < n; i += 1) {
      if (i === k || isZero(m[i][k])) continue;
      const f = m[i][k];
      for (let j = 0; j < 2 * n; j += 1) m[i][j] = sub(m[i][j], mul(f, m[k][j]));
    }
  }
  const inverse = m.map((row) => row.slice(n));
  const x = inverse.map((row) => row.reduce((acc, v, j) => add(acc, mul(v, b[j])), R(0n)));
  return { x, inverse, det };
}

const exactNorm1 = (a) => {
  let best = R(0n);
  for (let j = 0; j < a[0].length; j += 1) {
    let s = R(0n);
    for (let i = 0; i < a.length; i += 1) s = add(s, abs(a[i][j]));
    if (less(best, s)) best = s;
  }
  return best;
};
const exactNormInf = (a) => {
  let best = R(0n);
  for (const row of a) {
    let s = R(0n);
    for (const v of row) s = add(s, abs(v));
    if (less(best, s)) best = s;
  }
  return best;
};
const exactNormFro = (a) => {
  let s = R(0n);
  for (const row of a) for (const v of row) s = add(s, mul(v, v));
  return Math.sqrt(toNumber(s));
};

// ---------------------------------------------------------------- cases

// Deterministic integer generator (LCG), so the cases never change between runs.
function lcg(seed) {
  let state = BigInt(seed);
  return () => {
    state = (state * 6364136223846793005n + 1442695040888963407n) & ((1n << 64n) - 1n);
    return Number(state >> 33n) / 2147483648;
  };
}

const hilbert = (n) => Array.from({ length: n }, (_, i) => Array.from({ length: n }, (_, j) => 1 / (i + j + 1)));
const ones = (n) => Array.from({ length: n }, () => 1);

function fixedCases() {
  return [
    { name: "two-by-two", a: [[4, 1], [2, 3]], b: [1, 2] },
    { name: "three-by-three", a: [[2, 1, 0], [1, 3, 1], [0, 1, 4]], b: [1, 0, 2] },
    { name: "identity", a: [[1, 0, 0], [0, 1, 0], [0, 0, 1]], b: [1, 2, 3] },
    { name: "decimal-entries", a: [[0.1, 0.2], [0.3, 0.4]], b: [0.5, 0.6] },
    { name: "large-scale", a: [[1e150, 0], [0, 1e150]], b: [1e150, 2e150] },
    { name: "small-scale", a: [[1e-150, 0], [0, 1e-150]], b: [1e-150, 2e-150] },
    { name: "mixed-scale-ill", a: [[1e8, 0], [0, 1e-8]], b: [1, 1] },
    { name: "singular-two", a: [[1, 2], [2, 4]], b: [1, 2] },
    { name: "singular-three", a: [[1, 2, 3], [4, 5, 6], [7, 8, 9]], b: [1, 1, 1] },
    { name: "zero-matrix", a: [[0, 0], [0, 0]], b: [1, 1] },
    { name: "scaled-singular-tiny", a: [[1e-200, 2e-200], [2e-200, 4e-200]], b: [1e-200, 2e-200] },
    ...[3, 4, 5, 6, 7, 8, 9, 10].map((n) => ({ name: `hilbert-${n}`, a: hilbert(n), b: ones(n) })),
  ];
}

function randomCases() {
  const rng = lcg(20261115);
  const cases = [];
  for (let k = 0; k < 40; k += 1) {
    const n = 2 + Math.floor(rng() * 5);
    const a = Array.from({ length: n }, () => Array.from({ length: n }, () => Math.floor(rng() * 19) - 9));
    const b = Array.from({ length: n }, () => Math.floor(rng() * 19) - 9);
    cases.push({ name: `random-${k}`, a, b });
  }
  return cases;
}

// Non-square and mismatched inputs: the program must refuse them with these exact messages.
const REFUSALS = [
  { name: "non-square", a: [[1, 2, 3], [4, 5, 6]], b: [1, 2], message: "solve_checked needs a square matrix" },
  { name: "rhs-length", a: [[1, 0], [0, 1]], b: [1, 2, 3], message: "solve_checked: the right-hand side needs one entry per row of the matrix" },
];

// ---------------------------------------------------------------- expected output and program

// Equilibration as std.linalg.solve_checked defines it: each row, then each column, is multiplied by the power of two that
// brings its largest entry into [1, 2). That power is determined by the interval, so it is computed here by exact rational
// comparisons, and every scaled entry is exact (a power-of-two multiple of a double).
function powerOfTwoScale(m) {
  if (isZero(m)) return R(1n);
  const pow = (k) => (k >= 0n ? R(1n << k) : R(1n, 1n << -k));
  let e = BigInt(m.n.toString(2).length - m.d.toString(2).length);
  while (less(m, pow(e))) e -= 1n;
  while (!less(m, pow(e + 1n))) e += 1n;
  return pow(-e);
}
function equilibrate(a) {
  const n = a.length;
  const rowMax = a.map((row) => row.reduce((best, v) => (less(best, abs(v)) ? abs(v) : best), R(0n)));
  const rf = rowMax.map(powerOfTwoScale);
  const cf = Array.from({ length: n }, (_, j) =>
    powerOfTwoScale(a.reduce((best, row, i) => {
      const t = abs(mul(row[j], rf[i]));
      return less(best, t) ? t : best;
    }, R(0n))),
  );
  const B = a.map((row, i) => row.map((v, j) => mul(mul(v, rf[i]), cf[j])));
  return { B, rf, cf, rowMax };
}
// The condition number of the equilibrated matrix (1-norm), or null when it is singular.
function equilibratedCondition(a) {
  const eq = equilibrate(a);
  const n = a.length;
  const inv = exactSolve(eq.B, Array.from({ length: n }, () => R(0n)));
  if (inv.singular) return { eq, cond: null };
  return { eq, cond: toNumber(mul(exactNorm1(eq.B), exactNorm1(inv.inverse))) };
}

function evaluate(c) {
  const exactA = c.a.map((row) => row.map(exactOf));
  const exactB = c.b.map(exactOf);
  const solved = exactSolve(exactA, exactB);
  const n = c.a.length;
  if (solved.singular) return { kind: "singular" };
  const normA = exactNorm1(exactA);
  const eqc = equilibratedCondition(exactA);
  if (eqc.cond === null) return { kind: "singular" };
  const cond = eqc.cond;
  const xs = solved.x.map(toNumber);
  const scaleXB = Math.max(...xs.map(Math.abs), 1e-300) * toNumber(exactNormInf(exactA)) + Math.max(...c.b.map(Math.abs));
  return {
    kind: "ok",
    n,
    xs,
    cond,
    norm1: toNumber(normA),
    normInf: toNumber(exactNormInf(exactA)),
    normFro: exactNormFro(exactA),
    residualBound: 4 * n * EPS * scaleXB,
    // Forward error bound for x: rounding in LU and in the inverse-based condition estimate, scaled by the condition number.
    xTolerance: (f) => Math.max(1e-12, 4 * n * n * cond * EPS) * Math.max(1, Math.abs(f)),
    condTolerance: 4 * n * n * cond * EPS * cond,
  };
}

const num = (v) => (Number.isInteger(v) && Math.abs(v) < 1e15 ? String(v) : String(v));
// A Float literal the lexer accepts: exponent forms need a mantissa with a point (`1.0e150`, `1.0e-150`).
const literal = (v) => {
  const text = String(v);
  if (text.includes("e")) {
    const [mantissa, exponent] = text.split("e");
    return `${mantissa.includes(".") ? mantissa : `${mantissa}.0`}e${exponent.replace("+", "")}`;
  }
  return Number.isInteger(v) ? `${v}.0` : text;
};
const matrixLiteral = (a) => `array([${a.map((row) => `[${row.map(literal).join(", ")}]`).join(", ")}])`;
const vectorLiteral = (b) => `array([${b.map(literal).join(", ")}])`;

function build() {
  const cases = [...fixedCases(), ...randomCases()];
  const lines = [];
  const expected = [];
  let index = 0;
  for (const c of cases) {
    const result = evaluate(c);
    const label = `case ${index} ${c.name}`;
    const prog = `    print("${label} " + solve_line(${matrixLiteral(c.a)}, ${vectorLiteral(c.b)}))`;
    lines.push(prog);
    // Singular matrices are refused. So is any matrix whose exact condition number is at or above 1/(n * EPS): there no
    // double-precision digit of the solution is meaningful, and the pivot test of std.linalg refuses it as singular to
    // working precision (a documented outcome, not a tolerance change).
    if (result.kind === "singular" || result.cond >= 1 / (c.a.length * EPS)) {
      expected.push(`${label} refused singular`);
    } else {
      // the status is "ill_conditioned" when the exact condition number is at or above the limit; if rounding can move it
      // across the limit, either status is accepted (the boundary tolerance is the same bound as for the condition number)
      const nearBoundary = Math.abs(result.cond / LIMIT - 1) < 4 * c.a.length ** 2 * EPS * result.cond / LIMIT;
      const status = nearBoundary ? "ok|ill_conditioned" : result.cond >= LIMIT ? "ill_conditioned" : "ok";
      const xText = result.xs.map((v) => `${v}|${result.xTolerance(v)}`).join(" ");
      expected.push(
        `${label} solved ${status} x ${xText} residual <=${result.residualBound} cond ${result.cond}|${result.condTolerance} norm1 ${result.norm1}|${1e-12 * Math.max(1, result.norm1)} norminf ${result.normInf}|${1e-12 * Math.max(1, result.normInf)} fro ${result.normFro}|${1e-12 * Math.max(1, result.normFro)}`,
      );
    }
    index += 1;
  }
  for (const r of REFUSALS) {
    const label = `refusal ${r.name}`;
    lines.push(`    print("${label} " + refusal_line(${matrixLiteral(r.a)}, ${vectorLiteral(r.b)}))`);
    expected.push(`${label} error ${r.message}`);
  }
  const program = [
    "// Generated by scripts/linalg-oracle.mjs --write; do not edit. Each case is solved by std.linalg.solve_checked and printed.",
    "// The expected values are in tests/oracle/linalg-expected.txt, computed in exact rational arithmetic.",
    "import std.linalg",
    "",
    "fn solve_line(a: Array<Float>, b: Array<Float>) -> String {",
    "    match linalg.solve_checked(a, b) {",
    "        Ok(r) => {",
    "            mut parts = \"solved \" + r.status + \" x\"",
    "            for i in 0 until r.x.length() {",
    "                parts = parts + \" \" + r.x[i].to_string()",
    "            }",
    "            parts = parts + \" residual \" + r.residual.to_string() + \" cond \" + r.condition.to_string()",
    "            parts = parts + \" norm1 \" + linalg.matrix_norm1(a).unwrap().to_string()",
    "            parts = parts + \" norminf \" + linalg.matrix_norm_inf(a).unwrap().to_string()",
    "            parts = parts + \" fro \" + linalg.matrix_norm_frobenius(a).unwrap().to_string()",
    "            parts",
    "        },",
    "        Err(message) => {",
    "            if message.contains(\"singular\") { \"refused singular\" } else { \"refused other \" + message }",
    "        }",
    "    }",
    "}",
    "",
    "fn refusal_line(a: Array<Float>, b: Array<Float>) -> String {",
    "    match linalg.solve_checked(a, b) {",
    "        Ok(r) => \"unexpected \" + r.status,",
    "        Err(message) => \"error \" + message,",
    "    }",
    "}",
    "",
    "fn main() -> Void {",
    ...lines,
    "}",
    "",
  ].join("\n");
  return { program, expected: expected.join("\n") + "\n", count: cases.length };
}

function main() {
  const mode = process.argv[2] ?? "";
  const generated = build();
  if (mode === "--write") {
    writeFileSync(PROGRAM_PATH, generated.program);
    writeFileSync(EXPECTED_PATH, generated.expected);
    console.log(`wrote ${PROGRAM_PATH} and ${EXPECTED_PATH} (${generated.count} solved cases plus refusals)`);
    return;
  }
  if (mode === "--check") {
    const program = readFileSync(PROGRAM_PATH, "utf8").replace(/\r\n/g, "\n");
    const expected = readFileSync(EXPECTED_PATH, "utf8").replace(/\r\n/g, "\n");
    if (program !== generated.program || expected !== generated.expected) {
      console.error("stale generated files; run with --write");
      process.exit(1);
    }
    console.log("generated files are current");
    return;
  }
  console.log(`linalg oracle: ${generated.count} solved cases and ${REFUSALS.length} refusals`);
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) main();
