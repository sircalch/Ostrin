#!/usr/bin/env node
// Property checks for std.linalg against independent references.
//
// The references never call Ostrin or std.numeric. Every input is the double the Ostrin program reads (each decimal literal
// is converted to its exact rational value here), and the exact quantities (products, inverses, solutions, condition
// numbers) are computed in BigInt rational arithmetic. What the program computes is then compared with those references.
//
// Tolerances are rounding bounds, stated per check (see `bounds` below). They come from the standard error analysis of
// floating-point dot products (gamma_n = n*eps/(1 - n*eps)), of LU with partial pivoting (backward error bounded by
// 3 n eps rho with the worst-case growth rho = 2^(n-1)), and of the forward error (kappa times the backward error). No
// tolerance is chosen to make a result pass.
//
// Expected tokens (read by tests/examples.rs, `linalg_properties_agree_in_both_backends`):
//   text            must match exactly;
//   v|t             a number within t of v;
//   <=b             a number that must not exceed b;
//   a|b             either text a or text b.
//
// Modes: default (count), --write (regenerate examples/linalg_properties.ostrin and
// tests/oracle/linalg-properties-expected.txt), --check (fail if out of date).

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");
const PROGRAM_PATH = join(root, "examples", "linalg_properties.ostrin");
const EXPECTED_PATH = join(root, "tests", "oracle", "linalg-properties-expected.txt");
const EPS = 2.220446049250313e-16;
const LIMIT = 1e12;

// ---------------------------------------------------------------- exact rationals (BigInt)

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
const absR = (x) => (x.n < 0n ? { n: -x.n, d: x.d } : x);
const lessR = (x, y) => x.n * y.d < y.n * x.d;
const toNumber = (x) => Number(x.n) / Number(x.d);

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

// Gauss-Jordan on rationals: solution, inverse, determinant (or singular).
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

const exactMatMul = (a, b) =>
  a.map((row) => b[0].map((_, j) => row.reduce((acc, v, k) => add(acc, mul(v, b[k][j])), R(0n))));
const exactTranspose = (a) => a[0].map((_, j) => a.map((row) => row[j]));
const exactMatVec = (a, x) => a.map((row) => row.reduce((acc, v, j) => add(acc, mul(v, x[j])), R(0n)));
const exactNorm1 = (a) => {
  let best = R(0n);
  for (let j = 0; j < a[0].length; j += 1) {
    let s = R(0n);
    for (let i = 0; i < a.length; i += 1) s = add(s, absR(a[i][j]));
    if (lessR(best, s)) best = s;
  }
  return best;
};
const exactNormInf = (a) => {
  let best = R(0n);
  for (const row of a) {
    let s = R(0n);
    for (const v of row) s = add(s, absR(v));
    if (lessR(best, s)) best = s;
  }
  return best;
};
const exactMaxAbs = (vals) => vals.reduce((best, v) => (lessR(best, absR(v)) ? absR(v) : best), R(0n));

// ---------------------------------------------------------------- deterministic inputs

function lcg(seed) {
  let state = BigInt(seed);
  return () => {
    state = (state * 6364136223846793005n + 1442695040888963407n) & ((1n << 64n) - 1n);
    return Number(state >> 33n) / 2147483648;
  };
}
const randomInt = (rng, lo, hi) => lo + Math.floor(rng() * (hi - lo + 1));
const randomMatrix = (rng, n, m, lo = -9, hi = 9) =>
  Array.from({ length: n }, () => Array.from({ length: m }, () => randomInt(rng, lo, hi) / 2));
const randomVector = (rng, n, lo = -9, hi = 9) => Array.from({ length: n }, () => randomInt(rng, lo, hi) / 2);

// ---------------------------------------------------------------- Ostrin literals

const literal = (v) => {
  if (Number.isNaN(v)) return "(0.0 / 0.0)";
  if (v === Infinity) return "(1.0 / 0.0)";
  if (v === -Infinity) return "(0.0 - 1.0 / 0.0)";
  const text = String(v);
  if (text.includes("e")) {
    const [mantissa, exponent] = text.split("e");
    return `${mantissa.includes(".") ? mantissa : `${mantissa}.0`}e${exponent.replace("+", "")}`;
  }
  return Number.isInteger(v) ? `${v}.0` : text;
};
const matrixLiteral = (a) => `array([${a.map((row) => `[${row.map(literal).join(", ")}]`).join(", ")}])`;
const vectorLiteral = (b) => `array([${b.map(literal).join(", ")}])`;

// ---------------------------------------------------------------- checks

const rows = []; // { line: program statement, expected: expected line }
const count = { n: 0 };

function label(name) {
  count.n += 1;
  return `prop ${name}`;
}

// Checks that compare two computed matrices through the program: the program prints the maximum entry difference.
function diffCheck(name, expr, bound) {
  const l = label(name);
  rows.push({
    program: `    print("${l} " + max_diff(${expr.left}, ${expr.right}).to_string())`,
    expected: `${l} <=${bound}`,
  });
}

// Identity: A I = I A = A. The products only multiply by exact 1 and 0 and add exact zeros, so the difference is exactly 0.
for (const n of [1, 2, 3, 5, 8]) {
  const rng = lcg(1000 + n);
  const a = randomMatrix(rng, n, n);
  const id = Array.from({ length: n }, (_, i) => Array.from({ length: n }, (_, j) => (i === j ? 1 : 0)));
  diffCheck(`identity-right-${n}`, { left: `${matrixLiteral(a)} @ ${matrixLiteral(id)}`, right: matrixLiteral(a) }, 0);
  diffCheck(`identity-left-${n}`, { left: `${matrixLiteral(id)} @ ${matrixLiteral(a)}`, right: matrixLiteral(a) }, 0);
}

// Transposition: (A^T)^T = A exactly (a transpose copies entries).
for (const [n, m] of [[2, 3], [4, 4], [6, 2]]) {
  const rng = lcg(2000 + n * 10 + m);
  const a = randomMatrix(rng, n, m);
  diffCheck(`transpose-involution-${n}x${m}`, { left: `${matrixLiteral(a)}.transpose().transpose()`, right: matrixLiteral(a) }, 0);
}

// (A B)^T = B^T A^T. Each side is a sum of n products; each computed entry differs from its exact value by at most
// gamma_n * sum_k |a_ik||b_kj|, so the two sides differ by at most 2 gamma_n times that sum (the max over entries).
const gamma = (k) => (k * EPS) / (1 - k * EPS);
for (const [n, k, m] of [[2, 2, 2], [3, 4, 2], [5, 5, 5], [8, 6, 3], [20, 20, 20]]) {
  const rng = lcg(3000 + n * 100 + k * 10 + m);
  const a = randomMatrix(rng, n, k, -40, 40);
  const b = randomMatrix(rng, k, m, -40, 40);
  const ea = a.map((row) => row.map(exactOf));
  const eb = b.map((row) => row.map(exactOf));
  let worst = 0;
  for (let i = 0; i < n; i += 1) {
    for (let j = 0; j < m; j += 1) {
      let s = R(0n);
      for (let t = 0; t < k; t += 1) s = add(s, absR(mul(ea[i][t], eb[t][j])));
      worst = Math.max(worst, toNumber(s));
    }
  }
  const bound = 2 * gamma(k) * worst;
  diffCheck(`product-transpose-${n}x${k}x${m}`, { left: `(${matrixLiteral(a)} @ ${matrixLiteral(b)}).transpose()`, right: `${matrixLiteral(b)}.transpose() @ ${matrixLiteral(a)}.transpose()` }, bound);
}

// Linearity of the matrix-vector product: A (alpha x + beta y) against alpha A x + beta A y.
// Each computed entry has error at most gamma_(n+2) times sum_j |a_ij|(|alpha x_j| + |beta y_j|); the two sides together
// differ by at most 3 gamma_(n+2) times the largest such sum (the max over rows).
for (const n of [2, 4, 7]) {
  const rng = lcg(4000 + n);
  const a = randomMatrix(rng, n, n);
  const x = randomVector(rng, n);
  const y = randomVector(rng, n);
  const alpha = 1.5;
  const beta = -0.75;
  const combo = x.map((v, j) => alpha * v + beta * y[j]);
  let worst = 0;
  for (let i = 0; i < n; i += 1) {
    let s = 0;
    for (let j = 0; j < n; j += 1) s += Math.abs(a[i][j]) * (Math.abs(alpha * x[j]) + Math.abs(beta * y[j]));
    worst = Math.max(worst, s);
  }
  const bound = 3 * gamma(n + 2) * worst;
  // left: A * combo; right: alpha * (A x) + beta * (A y), built from the program's own matrix-vector products.
  const l = label(`matvec-linearity-${n}`);
  rows.push({
    program: `    print("${l} " + max_diff_vec(${matrixLiteral(a)} @ (${vectorLiteral(x)} * ${alpha} + ${vectorLiteral(y)} * ${beta}), (${matrixLiteral(a)} @ ${vectorLiteral(x)}) * ${alpha} + (${matrixLiteral(a)} @ ${vectorLiteral(y)}) * ${beta}).to_string())`,
    expected: `${l} <=${bound}`,
  });
  void combo;
}

// Solutions: residual backward error, forward error with a known exact solution, and status.
// Backward error of LU with partial pivoting: ||b - A x|| <= 3 n eps rho (||A|| ||x|| + ||b||) with the worst-case growth
// rho = 2^(n-1) (Wilkinson). Forward error: ||x - x*|| <= kappa_1 * (backward error relative to ||A||) * ||x*|| to first
// order, with kappa_1 computed exactly from the inverse of the exact matrix. Cases where kappa * backward error >= 1 are not
// bounded by this argument: they are required to report ill_conditioned instead.
// Equilibration as std.linalg.solve_checked defines it: each row, then each column, is multiplied by the power of two that
// brings its largest entry into [1, 2). That power is determined by the interval, so it is computed here by exact rational
// comparisons, and every scaled entry is exact (a power-of-two multiple of a double).
function powerOfTwoScale(m) {
  if (isZero(m)) return R(1n);
  const pow = (k) => (k >= 0n ? R(1n << k) : R(1n, 1n << -k));
  let e = BigInt(m.n.toString(2).length - m.d.toString(2).length);
  while (lessR(m, pow(e))) e -= 1n;
  while (!lessR(m, pow(e + 1n))) e += 1n;
  return pow(-e);
}
function equilibrate(a) {
  const n = a.length;
  const rowMax = a.map((row) => row.reduce((best, v) => (lessR(best, absR(v)) ? absR(v) : best), R(0n)));
  const rf = rowMax.map(powerOfTwoScale);
  const cf = Array.from({ length: n }, (_, j) =>
    powerOfTwoScale(a.reduce((best, row, i) => {
      const t = absR(mul(row[j], rf[i]));
      return lessR(best, t) ? t : best;
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

function solveCase(name, a, b) {
  const n = a.length;
  const ea = a.map((row) => row.map(exactOf));
  const eb = b.map(exactOf);
  const solved = exactSolve(ea, eb);
  const l = label(`solve-${name}`);
  if (solved.singular) {
    rows.push({ program: `    print("${l} " + solve_line(${matrixLiteral(a)}, ${vectorLiteral(b)}))`, expected: `${l} refused singular` });
    return;
  }
  const eqc = equilibratedCondition(ea);
  if (eqc.cond === null || eqc.cond >= 1 / (n * EPS)) {
    rows.push({ program: `    print("${l} " + solve_line(${matrixLiteral(a)}, ${vectorLiteral(b)}))`, expected: `${l} refused singular` });
    return;
  }
  const kappa = eqc.cond;
  const rho = 2 ** (n - 1);
  const xNorm = toNumber(exactMaxAbs(solved.x));
  const aNorm = toNumber(exactNormInf(ea));
  const eq = eqc.eq;
  const scaledRhs = b.map((v, i) => mul(eq.rf[i], exactOf(v)));
  const yExact = exactSolve(eq.B, scaledRhs).x;
  const yNorm = toNumber(exactMaxAbs(yExact));
  const bNormB = toNumber(exactNormInf(eq.B));
  const rowMaxScale = Math.max(...eq.rowMax.map(toNumber));
  const scaledRhsMax = Math.max(...scaledRhs.map(toNumber).map(Math.abs));
  const backward = 3 * n * EPS * rho;
  // Residual of the original system: each row of the scaled residual is multiplied by 1/rf_i, which is at most rowMax_i.
  // The first factor carries the first-order effect of the forward error of y on ||y||.
  const bNorm = Math.max(...b.map(Math.abs), 0);
  const rounding = Math.max(...ea.map((row, i) => toNumber(row.reduce((acc, v, j) => add(acc, absR(mul(v, solved.x[j]))), absR(exactOf(b[i]))))));
  const residualBound = backward * rowMaxScale * (bNormB * yNorm * (1 + kappa * backward) + scaledRhsMax) + (n + 1) * EPS * rounding;
  void bNorm;
  void aNorm;
  const status = kappa >= LIMIT ? "ill_conditioned" : "ok";
  const nearLimit = Math.abs(kappa / LIMIT - 1) < 1e-6;
  const statusToken = nearLimit ? "ok|ill_conditioned" : status;
  const forward = kappa * backward * Math.max(1, toNumber(exactMaxAbs(solved.x)));
  const xText = solved.x.map((v) => `${toNumber(v)}|${Math.max(forward, 1e-300)}`).join(" ");
  rows.push({
    program: `    print("${l} " + solve_line(${matrixLiteral(a)}, ${vectorLiteral(b)}))`,
    expected: `${l} solved ${statusToken} x ${xText} residual <=${residualBound}`,
  });
  void kappa;
}

// Well-conditioned systems of several sizes, with a known exact solution (the exact solution of the double inputs).
for (const n of [1, 2, 3, 5, 8, 16]) {
  const rng = lcg(5000 + n);
  const a = Array.from({ length: n }, (_, i) =>
    Array.from({ length: n }, (_, j) => (i === j ? n + 2 + randomInt(rng, 0, 4) : randomInt(rng, -2, 2))),
  );
  const b = randomVector(rng, n, -20, 20);
  solveCase(`well-${n}`, a, b);
}

// Nearly singular: [[1,1],[1,1+d]] has condition number about 4/d.
for (const d of [1e-8, 1e-11, 1e-13, 3e-16]) {
  solveCase(`nearly-singular-${d}`, [[1, 1], [1, 1 + d]], [2, 2 + d]);
}

// Rows and columns on very different scales: A = diag(s) B diag(t) with B well conditioned.
for (const [name, s, t] of [
  ["rows-scaled", [1e-8, 1, 1e8], [1, 1, 1]],
  ["cols-scaled", [1, 1, 1], [1e-8, 1, 1e8]],
  ["both-scaled", [1e-6, 1, 1e6], [1e6, 1, 1e-6]],
]) {
  const rng = lcg(6000 + name.length);
  const bmat = Array.from({ length: 3 }, (_, i) => Array.from({ length: 3 }, (_, j) => (i === j ? 4 : randomInt(rng, -1, 1))));
  const a = bmat.map((row, i) => row.map((v, j) => v * s[i] * t[j]));
  const b = randomVector(rng, 3, -5, 5);
  solveCase(name, a, b);
}

// Singular and non-finite inputs are refused with the messages below.
const SINGULAR = [
  ["singular-rank-one", [[1, 2], [2, 4]], [1, 2]],
  ["singular-three", [[1, 2, 3], [4, 5, 6], [7, 8, 9]], [1, 1, 1]],
  ["zero-matrix", [[0, 0], [0, 0]], [1, 1]],
  ["scaled-singular", [[1e-200, 2e-200], [2e-200, 4e-200]], [1e-200, 2e-200]],
];
for (const [name, a, b] of SINGULAR) {
  const l = label(name);
  rows.push({ program: `    print("${l} " + solve_line(${matrixLiteral(a)}, ${vectorLiteral(b)}))`, expected: `${l} refused singular` });
}

const NON_FINITE = [
  ["nan-entry", [[Number.NaN, 0], [0, 1]], [1, 2]],
  ["inf-entry", [[Infinity, 0], [0, 1]], [1, 2]],
  ["nan-rhs", [[1, 0], [0, 1]], [Number.NaN, 2]],
];
for (const [name, a, b] of NON_FINITE) {
  const l = label(name);
  rows.push({ program: `    print("${l} " + solve_line(${matrixLiteral(a)}, ${vectorLiteral(b)}))`, expected: `${l} refused non-finite` });
}

// Dimension errors reported with Result.
const REFUSED = [
  ["non-square", [[1, 2, 3], [4, 5, 6]], [1, 2], "solve_checked needs a square matrix"],
  ["rhs-length", [[1, 0], [0, 1]], [1, 2, 3], "solve_checked: the right-hand side needs one entry per row of the matrix"],
];
for (const [name, a, b, message] of REFUSED) {
  const l = `refusal ${name}`;
  rows.push({ program: `    print("${l} " + refusal_line(${matrixLiteral(a)}, ${vectorLiteral(b)}))`, expected: `${l} error ${message}` });
}

// Matrix-vector products of quantities with mismatched shapes are refused with Result.
{
  const l = "refusal unit-matvec-shape";
  rows.push({
    program: `    print("${l} " + unit_matvec_refusal())`,
    expected: `${l} error unit_matvec: the vector needs one entry per column of the matrix`,
  });
}

// ---------------------------------------------------------------- program

function program() {
  const lines = [
    "// Generated by scripts/linalg-properties.mjs --write; do not edit. Property checks for std.linalg against exact references.",
    "// Expected values and their rounding bounds are in tests/oracle/linalg-properties-expected.txt.",
    "import std.linalg",
    "",
    "fn max_diff(a: Array<Float>, b: Array<Float>) -> Float {",
    "    shape = a.shape()",
    "    mut worst = 0.0",
    "    for i in 0 until shape[0] {",
    "        for j in 0 until shape[1] {",
    "            d = abs(a.get(i, j) - b.get(i, j))",
    "            if d > worst {",
    "                worst = d",
    "            }",
    "        }",
    "    }",
    "    worst",
    "}",
    "",
    "fn max_diff_vec(a: Array<Float>, b: Array<Float>) -> Float {",
    "    mut worst = 0.0",
    "    for i in 0 until a.length() {",
    "        d = abs(a[i] - b[i])",
    "        if d > worst {",
    "            worst = d",
    "        }",
    "    }",
    "    worst",
    "}",
    "",
    "fn solve_line(a: Array<Float>, b: Array<Float>) -> String {",
    "    match linalg.solve_checked(a, b) {",
    "        Ok(r) => {",
    '            mut parts = "solved " + r.status + " x"',
    "            for i in 0 until r.x.length() {",
    '                parts = parts + " " + r.x[i].to_string()',
    "            }",
    '            parts = parts + " residual " + r.residual.to_string()',
    "            parts",
    "        },",
    '        Err(message) => {',
    '            if message.contains("singular") { "refused singular" } else if message.contains("non-finite") { "refused non-finite" } else { "refused other " + message }',
    "        }",
    "    }",
    "}",
    "",
    "fn refusal_line(a: Array<Float>, b: Array<Float>) -> String {",
    "    match linalg.solve_checked(a, b) {",
    '        Ok(r) => "unexpected " + r.status,',
    '        Err(message) => "error " + message,',
    "    }",
    "}",
    "",
    "fn unit_matvec_refusal() -> String {",
    "    m = array([2.0, 0.0, 0.0, 4.0]) as N/m",
    "    v = array([1.0, 0.5, 0.25]) as m",
    "    match linalg.unit_matvec(m, 2, 2, v) {",
    '        Ok(f) => "unexpected " + (f as N).length().to_string(),',
    '        Err(message) => "error " + message,',
    "    }",
    "}",
    "",
    "fn main() -> Void {",
    ...rows.map((r) => r.program),
    "}",
    "",
  ];
  return lines.join("\n");
}

function expectedText() {
  return rows.map((r) => r.expected).join("\n") + "\n";
}

function main() {
  const mode = process.argv[2] ?? "";
  const prog = program();
  const exp = expectedText();
  if (mode === "--write") {
    writeFileSync(PROGRAM_PATH, prog);
    writeFileSync(EXPECTED_PATH, exp);
    console.log(`wrote ${PROGRAM_PATH} and ${EXPECTED_PATH} (${rows.length} checks)`);
    return;
  }
  if (mode === "--check") {
    const p = readFileSync(PROGRAM_PATH, "utf8").replace(/\r\n/g, "\n");
    const e = readFileSync(EXPECTED_PATH, "utf8").replace(/\r\n/g, "\n");
    if (p !== prog || e !== exp) {
      console.error("stale generated files; run with --write");
      process.exit(1);
    }
    console.log("generated files are current");
    return;
  }
  console.log(`linalg properties: ${rows.length} checks`);
}

main();
