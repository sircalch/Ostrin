#!/usr/bin/env node
// Independent oracle for the rational equation solver (std.rational.rational_solve).
//
// The oracle works from the un-reduced formulas and uses its own exact rationals on BigInt, its own polynomial code, the
// rational root theorem for the rational roots, synthetic division for the multiplicities and the residue, and its own
// arithmetic in Q(sqrt(d)) for the quadratic residue. It does not call the Ostrin module or std.polynomial.
//
// Expected per case: status, the exact solutions (ascending rationals, then the two irrational roots in the order +, -),
// and counts of excluded points, rejected candidates, pending parts, and domain requirements.
//
// Modes: default (checks), --write (regenerate tests/oracle/rational-equations-* and
// examples/rational_equations_oracle.ostrin), --check.

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");
const CASES_PATH = join(root, "tests", "oracle", "rational-equations-cases.json");
const EXPECTED_PATH = join(root, "tests", "oracle", "rational-equations-expected.txt");
const PROGRAM_PATH = join(root, "examples", "rational_equations_oracle.ostrin");

// ---------------------------------------------------------------- independent exact rationals

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
const qcmp = (x, y) => {
  const d = x.n * y.d - y.n * x.d;
  return d < 0n ? -1 : d > 0n ? 1 : 0;
};
const qtext = (x) => (x.d === 1n ? x.n.toString() : `${x.n}/${x.d}`);
function qparse(s) {
  if (!/^-?\d+(\/\d+)?$/.test(s)) throw new SyntaxError(`not a rational: ${s}`);
  const [a, b] = s.split("/");
  return q(BigInt(a), b === undefined ? 1n : BigInt(b));
}

// ---------------------------------------------------------------- polynomials as rational coefficient arrays, lowest first

const trimmed = (p) => {
  const out = p.slice();
  while (out.length > 0 && qzero(out[out.length - 1])) out.pop();
  return out;
};
const evaluate = (p, t) => p.reduceRight((acc, c) => qadd(qmul(acc, t), c), q(0n));
const polyMul = (a, b) => {
  if (a.length === 0 || b.length === 0) return [];
  const out = Array.from({ length: a.length + b.length - 1 }, () => q(0n));
  a.forEach((x, i) => b.forEach((y, j) => (out[i + j] = qadd(out[i + j], qmul(x, y)))));
  return trimmed(out);
};
const polySub = (a, b) => {
  const n = Math.max(a.length, b.length);
  const out = [];
  for (let i = 0; i < n; i++) {
    out.push(qsub(i < a.length ? a[i] : q(0n), i < b.length ? b[i] : q(0n)));
  }
  return trimmed(out);
};
const derivative = (p) => trimmed(p.slice(1).map((c, i) => qmul(c, q(BigInt(i + 1)))));

/** Remainder of a divided by b (both trimmed, b nonzero). */
function polyRemainder(a, b) {
  let r = a.slice();
  const db = b.length - 1;
  const lb = b[db];
  while (r.length > db && r.length > 0) {
    const shift = r.length - 1 - db;
    const factor = qdiv(r[r.length - 1], lb);
    const next = r.slice();
    for (let j = 0; j <= db; j++) next[j + shift] = qsub(next[j + shift], qmul(factor, b[j]));
    r = trimmed(next.slice(0, r.length - 1));
  }
  return trimmed(r);
}

/** Exact quotient of a by b, which must divide a. */
function polyQuotient(a, b) {
  let r = a.slice();
  const db = b.length - 1;
  const lb = b[db];
  const quotient = new Array(Math.max(0, a.length - db)).fill(q(0n));
  while (r.length > db && r.length > 0) {
    const shift = r.length - 1 - db;
    const factor = qdiv(r[r.length - 1], lb);
    quotient[shift] = factor;
    const next = r.slice();
    for (let j = 0; j <= db; j++) next[j + shift] = qsub(next[j + shift], qmul(factor, b[j]));
    r = trimmed(next.slice(0, r.length - 1));
  }
  return trimmed(quotient);
}

/** Monic greatest common divisor by the Euclidean algorithm (zero when both are zero). */
function polyGcd(a, b) {
  let x = trimmed(a);
  let y = trimmed(b);
  while (y.length > 0) {
    [x, y] = [y, polyRemainder(x, y)];
  }
  if (x.length === 0) return [];
  const lead = x[x.length - 1];
  return x.map((c) => qdiv(c, lead));
}

/** Division of p by (x - r): the quotient, exact when r is a root. */
function divideByLinear(p, r) {
  const n = p.length;
  if (n === 0) return [];
  const quotient = new Array(n - 1).fill(q(0n));
  let carry = q(0n);
  for (let i = n - 1; i >= 0; i--) {
    carry = qadd(carry, p[i]);
    if (i > 0) {
      quotient[i - 1] = carry;
      carry = qmul(carry, r);
    } else {
      carry = qmul(carry, r);
    }
  }
  return trimmed(quotient);
}

// ---------------------------------------------------------------- rational roots by the rational root theorem

function integerDivisors(n) {
  const m = abs(n);
  const out = [];
  for (let d = 1n; d * d <= m; d++) {
    if (m % d === 0n) {
      out.push(d);
      if (d * d !== m) out.push(m / d);
    }
  }
  return out;
}

/**
 * The distinct rational roots of p (ascending), each with its multiplicity, and the residue p divided by all of them.
 * Coefficients are cleared to integers; roots at zero are divided out first.
 */
function rationalRoots(p) {
  let poly = trimmed(p);
  const roots = [];
  if (poly.length === 0) return { roots, residue: [] };
  // clear denominators
  let lcm = 1n;
  for (const c of poly) lcm = (lcm * c.d) / gcd(lcm, c.d);
  let integers = poly.map((c) => (c.n * (lcm / c.d)));
  // zero root
  let zeroMultiplicity = 0;
  while (integers.length > 0 && integers[0] === 0n) {
    integers = integers.slice(1);
    zeroMultiplicity++;
  }
  const candidates = [];
  if (zeroMultiplicity > 0) candidates.push(q(0n));
  if (integers.length >= 2) {
    const constant = integers[0];
    const lead = integers[integers.length - 1];
    for (const pnum of integerDivisors(constant)) {
      for (const qden of integerDivisors(lead)) {
        candidates.push(q(pnum, qden), q(-pnum, qden));
      }
    }
  }
  const seen = new Set();
  const distinct = [];
  for (const c of candidates) {
    const key = qtext(c);
    if (!seen.has(key)) {
      seen.add(key);
      distinct.push(c);
    }
  }
  distinct.sort(qcmp);
  let residue = poly;
  for (const r of distinct) {
    if (qzero(evaluate(residue, r))) {
      let multiplicity = 0;
      while (residue.length > 1 && qzero(evaluate(residue, r))) {
        residue = divideByLinear(residue, r);
        multiplicity++;
      }
      roots.push({ root: r, multiplicity });
    }
  }
  return { roots, residue };
}

// ---------------------------------------------------------------- Q(sqrt d)

const fieldMul = (x, y, d) => [qadd(qmul(x[0], y[0]), qmul(qmul(x[1], y[1]), d)), qadd(qmul(x[0], y[1]), qmul(x[1], y[0]))];
const fieldAdd = (x, y) => [qadd(x[0], y[0]), qadd(x[1], y[1])];
const fieldEval = (p, alpha, d) => p.reduceRight((acc, c) => fieldAdd(fieldMul(acc, alpha, d), [c, q(0n)]), [q(0n), q(0n)]);
const fieldZero = (x) => qzero(x[0]) && qzero(x[1]);
const fieldEqual = (x, y) => qcmp(x[0], y[0]) === 0 && qcmp(x[1], y[1]) === 0;

// ---------------------------------------------------------------- the expected summary of one equation

/** The expected summary of f = g, computed from the un-reduced formulas. */
export function expectedSummary(nf, df, ng, dg) {
  // requirements: the original denominators, deduplicated by coefficient equality
  const requirements = [];
  for (const r of [df, dg]) {
    if (!requirements.some((s) => s.length === r.length && s.every((c, i) => qcmp(c, r[i]) === 0))) requirements.push(r);
  }
  // the difference over the common denominator, with the common factors of the denominators removed (the reduction)
  const raw = trimmed(polySub(polyMul(nf, dg), polyMul(ng, df)));
  const commonDenominator = polyMul(df, dg);
  const numerator = raw.length === 0 ? [] : polyQuotient(raw, polyGcd(raw, commonDenominator));
  const excludedRationals = new Set();
  let excludedIrrational = 0;
  let rejected = 0;
  const solutions = [];
  let status;
  let pending = 0;

  // requirement roots are excluded points, whether or not they are candidates
  for (const r of requirements) {
    for (const { root } of rationalRoots(r).roots) excludedRationals.add(qtext(root));
  }

  if (numerator.length === 0) {
    status = "identity";
  } else {
    const { roots, residue } = rationalRoots(numerator);
    for (const { root, multiplicity } of roots) {
      const excluded = requirements.some((r) => qzero(evaluate(r, root)));
      if (excluded) {
        excludedRationals.add(qtext(root));
        continue;
      }
      const left = evaluate(nf, root);
      const right = evaluate(ng, root);
      const leftValue = qdiv(left, evaluate(df, root));
      const rightValue = qdiv(right, evaluate(dg, root));
      if (qcmp(leftValue, rightValue) === 0) {
        solutions.push({ text: qtext(root), kind: "rational", multiplicity });
      } else {
        rejected++;
      }
    }
    const degree = residue.length - 1;
    if (degree === 2) {
      // monic: x^2 + b x + c, so that the discriminant does not depend on the scale of the residue
      const lead = residue[2];
      const b = qdiv(residue[1], lead);
      const c = qdiv(residue[0], lead);
      const disc = qsub(qmul(b, b), qmul(q(4n), c));
      const p = qdiv(qmul(b, q(-1n)), q(2n));
      const qh = qdiv(q(1n), q(2n));
      for (const sign of [1n, -1n]) {
        const qv = qmul(qh, q(sign));
        const alpha = [p, qv];
        const text = qv.n < 0n ? `${qtext(p)} - ${qtext(q(-qv.n, qv.d))}*sqrt(${qtext(disc)})` : `${qtext(p)} + ${qtext(qv)}*sqrt(${qtext(disc)})`;
        const excluded = requirements.some((r) => fieldZero(fieldEval(r, alpha, disc)));
        if (excluded) {
          excludedIrrational++;
          continue;
        }
        const lhs = fieldMul(fieldEval(nf, alpha, disc), fieldEval(dg, alpha, disc), disc);
        const rhs = fieldMul(fieldEval(ng, alpha, disc), fieldEval(df, alpha, disc), disc);
        if (fieldEqual(lhs, rhs)) {
          solutions.push({ text, kind: qcmp(disc, q(0n)) < 0 ? "complex" : "irrational", multiplicity: 1 });
        } else {
          rejected++;
        }
      }
    } else if (degree > 0) {
      pending = 1;
    }
    status = pending > 0 ? "partial" : solutions.length > 0 ? "solved" : "no_solution";
  }
  const excludedCount = excludedRationals.size + excludedIrrational;
  const solutionText = solutions.length === 0 ? "none" : solutions.map((s) => s.text).join(" | ");
  return `status=${status}; solutions=${solutionText}; excluded=${excludedCount}; rejected=${rejected}; pending=${pending}; domain=${requirements.length}`;
}

// ---------------------------------------------------------------- deterministic cases

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

const FACTOR_POOL = [
  [-1, 1],
  [0, 1],
  [1, 1],
  [-2, 1],
  [2, 1],
  [1, 0, 1],
  [-2, 0, 1],
];
const multiplyInt = (a, b) => {
  const out = new Array(a.length + b.length - 1).fill(0);
  a.forEach((x, i) => b.forEach((y, j) => (out[i + j] += x * y)));
  return out;
};
const randomProduct = (rng) => {
  let p = [randInt(rng, 1, 2)];
  const count = randInt(rng, 0, 2);
  for (let k = 0; k < count; k++) p = multiplyInt(p, FACTOR_POOL[randInt(rng, 0, FACTOR_POOL.length - 1)]);
  return p;
};
const randomNumerator = (rng) => {
  let p = randomProduct(rng);
  if (rng() < 0.2) p = [0];
  return p;
};
const toText = (p) => p.map(String);

export function buildCases() {
  const cases = [];
  const rng = mulberry32(20260803);
  // fixed cases from the design of the task
  const fixed = [
    { f: [[-1, 0, 1], [-1, 1]], g: [[3], [1]] },
    { f: [[-1, 0, 1], [-1, 1]], g: [[2], [1]] },
    { f: [[-2, 0, 1], [1]], g: [[0], [1]] },
    { f: [[1, -2, 1], [1]], g: [[0], [1]] },
    { f: [[-1, 0, 1], [-1, 1]], g: [[1, 1], [1]] },
    { f: [[1, 1], [-2, 1]], g: [[3], [-2, 1]] },
    { f: [[0], [1, 1]], g: [[0], [1]] },
    { f: [[0, 1], [1]], g: [[1, 1], [1]] },
    { f: [[0, 1], [2]], g: [[1], [3]] },
    { f: [[-2, 0, 0, 1], [1]], g: [[0], [1]] },
  ];
  for (const c of fixed) cases.push(c);
  for (let k = 0; k < 50; k++) {
    const shared = randomProduct(rng);
    const f = { nf: randomNumerator(rng), df: multiplyInt(shared, randomProduct(rng)) };
    const g = { ng: randomNumerator(rng), dg: multiplyInt(shared, randomProduct(rng)) };
    cases.push({ f: [f.nf, f.df], g: [g.ng, g.dg] });
  }
  return cases.map((c) => ({ f: [toText(c.f[0]), toText(c.f[1])], g: [toText(c.g[0]), toText(c.g[1])] }));
}

const parsePoly = (texts) => texts.map(qparse);

export function expectedOutputs(cases) {
  return cases.map((c, i) => {
    const nf = parsePoly(c.f[0]);
    const df = parsePoly(c.f[1]);
    const ng = parsePoly(c.g[0]);
    const dg = parsePoly(c.g[1]);
    if (trimmed(df).length === 0 || trimmed(dg).length === 0) throw new Error(`case ${i}: zero denominator`);
    return `case ${i}: ${expectedSummary(nf, df, ng, dg)}`;
  });
}

// ---------------------------------------------------------------- the Ostrin program (generated)

export function ostrinProgram(cases) {
  const quote = (s) => JSON.stringify(s);
  const join = (xs) => xs.join(";");
  const calls = cases.map(
    (c, i) =>
      `    print(summary(${i}, ${quote(join(c.f[0]))}, ${quote(join(c.f[1]))}, ${quote(join(c.g[0]))}, ${quote(join(c.g[1]))}))`,
  );
  return `// Generated by scripts/rational-equations-oracle.mjs --write from tests/oracle/rational-equations-cases.json.
// Do not edit by hand. The expected output is tests/oracle/rational-equations-expected.txt, from an independent oracle.
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

fn summary(index: Int, nf: String, df: String, ng: String, dg: String) -> String {
    f = rational.rational_new(coeffs_of(nf), coeffs_of(df)).unwrap()
    g = rational.rational_new(coeffs_of(ng), coeffs_of(dg)).unwrap()
    s = rational.rational_solve(f, g).unwrap()
    mut sols = "none"
    if s.solutions.length() > 0 {
        sols = s.solutions.join(" | ")
    }
    return "case " + index.to_string() + ": status=" + s.status + "; solutions=" + sols + "; excluded=" + s.excluded.length().to_string() + "; rejected=" + s.rejected.length().to_string() + "; pending=" + s.pending.length().to_string() + "; domain=" + s.domain.length().to_string()
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
    cases: JSON.stringify({ format: "rational-equations-cases", version: 1, cases }, null, 2) + "\n",
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
      console.log("wrote tests/oracle/rational-equations-* and examples/rational_equations_oracle.ostrin");
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
      console.error("stale generated files; run with --write");
      process.exit(1);
    }
    console.log("generated files are current");
    return;
  }
  const lines = expectedOutputs(buildCases());
  const statuses = lines.map((l) => l.split("status=")[1].split(";")[0]);
  const tally = {};
  for (const s of statuses) tally[s] = (tally[s] ?? 0) + 1;
  console.log(`rational equation cases: ${lines.length}; statuses: ${JSON.stringify(tally)}`);
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  main();
}
