#!/usr/bin/env node
// Independent oracle for std.mpolynomial.
//
// The oracle does not share code with the Ostrin module. It uses its own exact rational type on BigInt, its own
// polynomial representation (a list of exponent vectors with rational coefficients, canonicalised by a Map), its own
// text formatter and its own refusal rules written from the interchange contract in compiler/std/mpolynomial.ostrin.
//
// Modes:
//   node scripts/mpolynomial-oracle.mjs            property checks with fixed seeds, and the edge-case assertions
//   node scripts/mpolynomial-oracle.mjs --write    regenerate tests/oracle/* and examples/mpolynomial_oracle.ostrin
//   node scripts/mpolynomial-oracle.mjs --check    fail if the checked-in generated files are not current

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");
const CASES_PATH = join(root, "tests", "oracle", "mpolynomial-cases.json");
const EXPECTED_PATH = join(root, "tests", "oracle", "mpolynomial-expected.txt");
const PROGRAM_PATH = join(root, "examples", "mpolynomial_oracle.ostrin");
const DIVISION_CASES_PATH = join(root, "tests", "oracle", "mpolynomial-division-cases.json");
const DIVISION_EXPECTED_PATH = join(root, "tests", "oracle", "mpolynomial-division-expected.txt");
const DIVISION_PROGRAM_PATH = join(root, "examples", "mpolynomial_division_oracle.ostrin");

// The limits of the interchange contract (compiler/std/mpolynomial.ostrin).
export const LIMITS = { variables: 16, exponent: 1000, terms: 10000, lineLength: 4096 };

// ---------------------------------------------------------------- exact rationals on BigInt

const absBig = (x) => (x < 0n ? -x : x);

function gcdBig(a, b) {
  a = absBig(a);
  b = absBig(b);
  while (b !== 0n) {
    [a, b] = [b, a % b];
  }
  return a;
}

/** A rational n/d in lowest terms with d > 0. Throws on a zero denominator. */
export function rat(n, d = 1n) {
  if (d === 0n) throw new RangeError("zero denominator");
  if (d < 0n) {
    n = -n;
    d = -d;
  }
  const g = gcdBig(n, d);
  return { n: n / g, d: d / g };
}

const radd = (x, y) => rat(x.n * y.d + y.n * x.d, x.d * y.d);
const rmul = (x, y) => rat(x.n * y.n, x.d * y.d);
const rneg = (x) => ({ n: -x.n, d: x.d });
const isZero = (x) => x.n === 0n;

function rpow(x, k) {
  let r = rat(1n);
  for (let i = 0; i < k; i++) r = rmul(r, x);
  return r;
}

/** The canonical text: an integer without a denominator, otherwise "n/d" with d > 1. */
export function rtext(x) {
  return x.d === 1n ? x.n.toString() : `${x.n}/${x.d}`;
}

const RATIONAL_TEXT = /^-?\d+(\/\d+)?$/;

/** Parses "-3/2", "5", "2/4". Throws on anything else, and on a zero denominator. */
export function rparse(s) {
  if (!RATIONAL_TEXT.test(s)) throw new SyntaxError(`not a rational: ${s}`);
  const [a, b] = s.split("/");
  return rat(BigInt(a), b === undefined ? 1n : BigInt(b));
}

// ---------------------------------------------------------------- polynomials

// A polynomial is { vars, terms }, where terms is canonical: sorted by exponent vector in decreasing lexicographic
// order, one entry per exponent vector, no zero coefficient.
function lexDesc(a, b) {
  for (let i = 0; i < a.length; i++) {
    if (a[i] !== b[i]) return b[i] - a[i];
  }
  return 0;
}

function canonical(entries) {
  const byKey = new Map();
  for (const { e, c } of entries) {
    const key = e.join(",");
    const previous = byKey.get(key);
    byKey.set(key, { e, c: previous ? radd(previous.c, c) : c });
  }
  return [...byKey.values()].filter((t) => !isZero(t.c)).sort((a, b) => lexDesc(a.e, b.e));
}

const sameVars = (a, b) => a.length === b.length && a.every((v, i) => v === b[i]);

function sameTerms(p, q) {
  return lines(p).join(";") === lines(q).join(";");
}

function padd(p, q) {
  if (!sameVars(p.vars, q.vars)) throw new Refusal("the polynomials are over different variables");
  return { vars: p.vars, terms: canonical([...p.terms, ...q.terms]) };
}

function pneg(p) {
  return { vars: p.vars, terms: p.terms.map((t) => ({ e: t.e, c: rneg(t.c) })) };
}

function pmul(p, q) {
  if (!sameVars(p.vars, q.vars)) throw new Refusal("the polynomials are over different variables");
  const entries = [];
  for (const s of p.terms) {
    for (const t of q.terms) {
      entries.push({ e: s.e.map((x, i) => x + t.e[i]), c: rmul(s.c, t.c) });
    }
  }
  return { vars: p.vars, terms: canonical(entries) };
}

function pderivative(p, index) {
  if (index < 0 || index >= p.vars.length) throw new Refusal("variable index outside the variables");
  const entries = [];
  for (const t of p.terms) {
    if (t.e[index] > 0) {
      const e = t.e.slice();
      e[index] -= 1;
      entries.push({ e, c: rmul(t.c, rat(BigInt(t.e[index]))) });
    }
  }
  return { vars: p.vars, terms: canonical(entries) };
}

function pevaluate(p, point) {
  let total = rat(0n);
  for (const t of p.terms) {
    let value = t.c;
    for (let i = 0; i < p.vars.length; i++) {
      value = rmul(value, rpow(point[i], t.e[i]));
    }
    total = radd(total, value);
  }
  return total;
}

/** The interchange lines "coefficient|e1,e2,...", in canonical order. */
export function lines(p) {
  return p.terms.map((t) => `${rtext(t.c)}|${t.e.join(",")}`);
}

/** The readable text, with the same rules as the module: "2*x^2*y - 3*y + 1/2", "0" for the zero polynomial. */
export function text(p) {
  if (p.terms.length === 0) return "0";
  let out = "";
  p.terms.forEach((t, k) => {
    const negative = t.c.n < 0n;
    const magnitude = { n: absBig(t.c.n), d: t.c.d };
    const factors = [];
    t.e.forEach((power, i) => {
      if (power === 1) factors.push(p.vars[i]);
      else if (power > 1) factors.push(`${p.vars[i]}^${power}`);
    });
    let body;
    if (factors.length === 0) body = rtext(magnitude);
    else if (magnitude.n === 1n && magnitude.d === 1n) body = factors.join("*");
    else body = `${rtext(magnitude)}*${factors.join("*")}`;
    if (k === 0) out = (negative ? "-" : "") + body;
    else out += (negative ? " - " : " + ") + body;
  });
  return out;
}

// ---------------------------------------------------------------- the interchange contract, from its written rules

export class Refusal extends Error {}

/** The reason an interchange input is refused, or null when it is accepted. Written from the contract, not from the module. */
export function refusalReason(vars, inputLines) {
  if (vars.length > LIMITS.variables) return "too many variables";
  if (vars.some((v) => v === "")) return "empty variable name";
  if (new Set(vars).size !== vars.length) return "repeated variable";
  if (inputLines.length > LIMITS.terms) return "too many terms";
  for (const line of inputLines) {
    if (line.length > LIMITS.lineLength) return "line too long";
    const parts = line.split("|");
    if (parts.length !== 2) return "bad line shape";
    try {
      rparse(parts[0]);
    } catch {
      return "bad coefficient";
    }
    const fields = parts[1].split(",");
    if (fields.length !== vars.length) return "wrong number of exponents";
    for (const f of fields) {
      if (!/^-?\d+$/.test(f)) return "bad exponent";
      const e = Number(f);
      if (e < 0) return "negative exponent";
      if (e > LIMITS.exponent) return "exponent above limit";
    }
  }
  return null;
}

/** Parses accepted interchange lines; throws Refusal otherwise. */
export function parseLines(vars, inputLines) {
  const reason = refusalReason(vars, inputLines);
  if (reason !== null) throw new Refusal(reason);
  const entries = inputLines.map((line) => {
    const [coefficient, exponents] = line.split("|");
    return { e: exponents.split(",").map(Number), c: rparse(coefficient) };
  });
  return { vars, terms: canonical(entries) };
}

// ---------------------------------------------------------------- deterministic random inputs

// mulberry32: a small, fixed generator, so the same seed gives the same cases on every machine.
export function mulberry32(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const randInt = (rng, lo, hi) => lo + Math.floor(rng() * (hi - lo + 1));
const VARIABLE_NAMES = ["x", "y", "z"];

function randomLines(rng, nvars) {
  const count = randInt(rng, 0, 5);
  const out = [];
  for (let k = 0; k < count; k++) {
    const coefficient = rat(BigInt(randInt(rng, -9, 9)), BigInt(randInt(rng, 1, 6)));
    const e = Array.from({ length: nvars }, () => randInt(rng, 0, 3));
    out.push(`${rtext(coefficient)}|${e.join(",")}`);
  }
  return out;
}

// ---------------------------------------------------------------- property checks

/** Runs the properties on `trials` random pairs of polynomials drawn from `seed`. Returns the counts. */
export function runProperties(seed, trials) {
  const rng = mulberry32(seed);
  const counts = { trials: 0, checks: 0, failures: [] };
  const check = (name, ok, detail) => {
    counts.checks += 1;
    if (!ok) counts.failures.push(`${name} (seed ${seed}): ${detail}`);
  };
  for (let trial = 0; trial < trials; trial++) {
    counts.trials += 1;
    const nvars = randInt(rng, 1, 3);
    const vars = VARIABLE_NAMES.slice(0, nvars);
    const p = parseLines(vars, randomLines(rng, nvars));
    const q = parseLines(vars, randomLines(rng, nvars));
    const r = parseLines(vars, randomLines(rng, nvars));
    const point = Array.from({ length: nvars }, () => rat(BigInt(randInt(rng, -4, 4)), BigInt(randInt(rng, 1, 3))));
    const index = randInt(rng, 0, nvars - 1);

    check("(p + q) - q = p", sameTerms(padd(padd(p, q), pneg(q)), p), text(p));
    check("commutativity", sameTerms(pmul(p, q), pmul(q, p)), `${text(p)} ; ${text(q)}`);
    check(
      "distributivity p(q + r) = pq + pr",
      sameTerms(pmul(p, padd(q, r)), padd(pmul(p, q), pmul(p, r))),
      `${text(p)} ; ${text(q)} ; ${text(r)}`,
    );
    check(
      "product rule d(pq) = p dq + q dp",
      sameTerms(
        pderivative(pmul(p, q), index),
        padd(pmul(p, pderivative(q, index)), pmul(q, pderivative(p, index))),
      ),
      `index ${index}`,
    );
    check(
      "evaluation is multiplicative",
      rtext(pevaluate(pmul(p, q), point)) === rtext(rmul(pevaluate(p, point), pevaluate(q, point))),
      `${text(p)} ; ${text(q)}`,
    );
    check(
      "evaluation is additive",
      rtext(pevaluate(padd(p, q), point)) === rtext(radd(pevaluate(p, point), pevaluate(q, point))),
      `${text(p)} ; ${text(q)}`,
    );
    check("cancellation p - p is zero", lines(padd(p, pneg(p))).length === 0, text(p));
    check(
      "interchange round trip",
      sameTerms(parseLines(vars, lines(p)), p),
      lines(p).join(";"),
    );
    check(
      "canonical text is stable under re-parsing",
      text(parseLines(vars, lines(p))) === text(p),
      text(p),
    );
  }
  return counts;
}

/** Explicit cases with known answers: the oracle's own assertions. Returns the failures. */
export function edgeAssertions() {
  const failures = [];
  const expect = (name, got, want) => {
    if (got !== want) failures.push(`${name}: got ${JSON.stringify(got)}, want ${JSON.stringify(want)}`);
  };
  const vars = ["x", "y"];
  const p = parseLines(vars, ["2|2,1", "-3|0,1", "1/2|0,0"]);
  expect("text of 2x^2y - 3y + 1/2", text(p), "2*x^2*y - 3*y + 1/2");
  expect("value at (2,-1) is -9/2", rtext(pevaluate(p, [rat(2n), rat(-1n)])), "-9/2");
  const square = pmul(parseLines(vars, ["1|1,0", "1|0,1"]), parseLines(vars, ["1|1,0", "1|0,1"]));
  expect("(x+y)^2 lines", lines(square).join(";"), "1|2,0;2|1,1;1|0,2");
  expect("zero polynomial text", text(parseLines(vars, [])), "0");
  expect("zero polynomial lines", lines(parseLines(vars, [])).length, 0);
  expect("derivative of a constant", lines(pderivative(parseLines(vars, ["7|0,0"]), 0)).length, 0);
  expect("(-1/2 x)(-2) = x", lines(pmul(parseLines(vars, ["-1/2|1,0"]), parseLines(vars, ["-2|0,0"]))).join(";"), "1|1,0");
  expect("2/4 is 1/2", lines(parseLines(vars, ["2/4|0,0"])).join(";"), "1/2|0,0");
  expect("1/3 + 1/6 = 1/2", lines(padd(parseLines(vars, ["1/3|0,0"]), parseLines(vars, ["1/6|0,0"]))).join(";"), "1/2|0,0");
  expect("repeated terms merge", lines(parseLines(vars, ["2|1,0", "3|1,0"])).join(";"), "5|1,0");
  expect("a zero term is dropped", lines(parseLines(vars, ["0|1,0"])).length, 0);
  expect("negative exponent refused", refusalReason(vars, ["1|-1,0"]), "negative exponent");
  expect("repeated variable refused", refusalReason(["x", "x"], ["1|1,0"]), "repeated variable");
  expect("bad coefficient refused", refusalReason(vars, ["1/0|0,0"]), "bad coefficient");
  expect("exponent above limit refused", refusalReason(vars, ["1|1001,0"]), "exponent above limit");
  expect("exponent at the limit accepted", refusalReason(vars, ["1|1000,0"]), null);
  return failures;
}

// ---------------------------------------------------------------- the case file

/** The fixed edge cases, followed by seeded random cases. Every case is either "valid" or "refuse". */
export function buildCases() {
  const cases = [];
  const valid = (v, p, q, d, point) => cases.push({ kind: "valid", vars: v, p, q, d, point });
  const refuse = (v, p) => cases.push({ kind: "refuse", vars: v, p });

  valid(["x", "y"], ["1|2,0", "2|1,1", "1|0,2"], ["-1|1,0", "1|0,1"], 0, ["2", "-1"]);
  valid(["x", "y"], [], [], 0, ["2", "3"]);
  valid(["x", "y"], [], ["-1/2|0,0"], 1, ["2", "3"]);
  valid(["x", "y"], ["1/2|1,0"], ["-1/2|1,0"], 0, ["1", "1"]);
  valid(["x", "y"], ["2|1,0", "3|1,0", "0|0,1"], ["2/4|0,0", "-1/3|0,0"], 1, ["-1", "4"]);
  valid(["x"], ["1|3", "-1|1"], ["1|1", "1|0"], 0, ["2"]);
  valid(["x", "y", "z"], ["2|2,1,0", "-3|0,1,0", "1/2|0,0,0"], ["1|1,0,1", "-1|0,0,0"], 2, ["2", "-1", "3"]);
  valid(["x", "y", "z"], ["1|1,1,1"], ["1|1,1,1"], 0, ["-2", "1", "1/2"]);
  valid(["x", "y"], ["-3|0,0"], ["-3|0,0"], 1, ["5", "5"]);

  refuse(["x", "y"], ["1|-1,0"]);
  refuse(["x", "x"], ["1|1,0"]);
  refuse(["x", "y"], ["1/0|0,0"]);
  refuse(["x", "y"], ["1|1"]);
  refuse(["x", ""], ["1|1,0"]);
  refuse(["x", "y"], ["1|a,0"]);
  refuse(["x", "y"], ["1|1001,0"]);
  refuse(["x", "y"], [`1|1,0${" ".repeat(LIMITS.lineLength)}`]);
  refuse(Array.from({ length: LIMITS.variables + 1 }, (_, i) => `v${i}`), ["1|0"]);

  const rng = mulberry32(20260731);
  for (let k = 0; k < 40; k++) {
    const nvars = randInt(rng, 1, 3);
    const vars = VARIABLE_NAMES.slice(0, nvars);
    const point = Array.from({ length: nvars }, () => rtext(rat(BigInt(randInt(rng, -4, 4)), BigInt(randInt(rng, 1, 3)))));
    cases.push({
      kind: "valid",
      vars,
      p: randomLines(rng, nvars),
      q: randomLines(rng, nvars),
      d: randInt(rng, 0, nvars - 1),
      point,
    });
  }
  return cases;
}

const joinLines = (ls) => (ls.length === 0 ? "zero" : ls.join(";"));

/** The expected output line of each case, computed by the oracle. Throws if a case's kind disagrees with the rules. */
export function expectedOutputs(cases) {
  return cases.map((c, i) => {
    if (c.kind === "refuse") {
      const reason = refusalReason(c.vars, c.p);
      if (reason === null) throw new Error(`case ${i} is marked refuse but the rules accept it`);
      return `case ${i}: refused`;
    }
    if (refusalReason(c.vars, [...c.p, ...c.q]) !== null) throw new Error(`case ${i} is marked valid but the rules refuse it`);
    const p = parseLines(c.vars, c.p);
    const q = parseLines(c.vars, c.q);
    const sum = padd(p, q);
    const product = pmul(p, q);
    const derivative = pderivative(product, c.d);
    const value = pevaluate(p, c.point.map(rparse));
    return (
      `case ${i}: text=${text(p)}; sum=${joinLines(lines(sum))}; product=${joinLines(lines(product))}; ` +
      `derivative=${joinLines(lines(derivative))}; value=${rtext(value)}`
    );
  });
}

/** The Ostrin program that runs the same cases through std.mpolynomial. Generated; do not edit by hand. */
export function ostrinProgram(cases) {
  const calls = cases.map((c, i) => {
    const list = (xs) => `[${xs.map((x) => JSON.stringify(x)).join(", ")}]`;
    if (c.kind === "refuse") return `    print(refused_case(${i}, ${list(c.vars)}, ${list(c.p)}))`;
    return `    print(valid_case(${i}, ${list(c.vars)}, ${list(c.p)}, ${list(c.q)}, ${c.d}, ${list(c.point)}))`;
  });
  return `// Generated by scripts/mpolynomial-oracle.mjs --write from tests/oracle/mpolynomial-cases.json. Do not edit by hand.
// The expected output is tests/oracle/mpolynomial-expected.txt, computed by an independent BigInt oracle.
import std.mpolynomial

fn join_lines(items: List<String>) -> String {
    if items.length() == 0 {
        return "zero"
    }
    mut text = ""
    for i in 0 until items.length() {
        if i == 0 {
            text = items[i]
        } else {
            text = text + ";" + items[i]
        }
    }
    text
}

fn valid_case(index: Int, vars: List<String>, pl: List<String>, ql: List<String>, d: Int, point: List<String>) -> String {
    p = mpolynomial.mpoly_parse(vars, pl).unwrap()
    q = mpolynomial.mpoly_parse(vars, ql).unwrap()
    total = mpolynomial.mpoly_add(p, q).unwrap()
    product = mpolynomial.mpoly_multiply(p, q).unwrap()
    derivative = mpolynomial.mpoly_derivative(product, d).unwrap()
    value = mpolynomial.mpoly_evaluate(p, point).unwrap()
    return "case " + index.to_string() + ": text=" + mpolynomial.mpoly_to_text(p) + "; sum=" + join_lines(mpolynomial.mpoly_lines(total)) + "; product=" + join_lines(mpolynomial.mpoly_lines(product)) + "; derivative=" + join_lines(mpolynomial.mpoly_lines(derivative)) + "; value=" + value
}

fn refused_case(index: Int, vars: List<String>, pl: List<String>) -> String {
    return match mpolynomial.mpoly_parse(vars, pl) {
        Err(message) => "case " + index.to_string() + ": refused",
        Ok(p) => "case " + index.to_string() + ": accepted",
    }
}

fn main() -> Void {
${calls.join("\n")}
}
`;
}

// ---------------------------------------------------------------- division with remainder (independent)

const rdiv = (a, b) => {
  if (b.n === 0n) throw new Refusal("division by zero");
  return rat(a.n * b.d, a.d * b.n);
};

/**
 * Division with remainder by an ordered list of divisors, in lexicographic order. The algorithm is written here again,
 * independently of the Ostrin code, on the rational type above. Returns { quotients, remainder }.
 */
export function divide(f, divisors) {
  if (divisors.length === 0) throw new Refusal("give at least one divisor");
  for (const g of divisors) {
    if (!sameVars(f.vars, g.vars)) throw new Refusal("the divisors and the polynomial are over different variables");
    if (g.terms.length === 0) throw new Refusal("a divisor is the zero polynomial");
  }
  const zero = () => ({ vars: f.vars, terms: [] });
  const quotients = divisors.map(zero);
  let remainder = zero();
  let rest = f;
  while (rest.terms.length > 0) {
    const lead = rest.terms[0];
    const term = { vars: f.vars, terms: [lead] };
    const i = divisors.findIndex((g) => g.terms[0].e.every((x, k) => x <= lead.e[k]));
    if (i < 0) {
      remainder = padd(remainder, term);
      rest = padd(rest, pneg(term));
    } else {
      const g = divisors[i];
      const e = lead.e.map((x, k) => x - g.terms[0].e[k]);
      const mono = { vars: f.vars, terms: canonical([{ e, c: rdiv(lead.c, g.terms[0].c) }]) };
      quotients[i] = padd(quotients[i], mono);
      rest = padd(rest, pneg(pmul(mono, g)));
    }
  }
  return { quotients, remainder };
}

/** The sum of quotient_i * divisor_i plus the remainder, and whether no remainder term has a divisible leading monomial. */
export function divisionIsValid(f, divisors, result) {
  let total = { vars: f.vars, terms: [] };
  divisors.forEach((g, i) => {
    total = padd(total, pmul(result.quotients[i], g));
  });
  total = padd(total, result.remainder);
  if (lines(total).join(";") !== lines(f).join(";")) return false;
  return result.remainder.terms.every((t) => divisors.every((g) => !g.terms[0].e.every((x, k) => x <= t.e[k])));
}

/** Ordinary long division of univariate polynomials, coefficients lowest degree first. Independent of divide(). */
export function univariateLongDivision(a, b) {
  let rem = a.map((c) => rparse(c));
  const deg = (p) => {
    for (let k = p.length - 1; k >= 0; k--) if (!isZero(p[k])) return k;
    return -1;
  };
  const db = deg(b.map((c) => rparse(c)));
  const lb = rparse(b[db]);
  const quotient = new Array(Math.max(0, rem.length - db)).fill(rat(0n));
  while (deg(rem) >= db) {
    const k = deg(rem) - db;
    const factor = rdiv(rem[deg(rem)], lb);
    quotient[k] = factor;
    for (let j = 0; j <= db; j++) {
      rem[j + k] = radd(rem[j + k], rneg(rmul(factor, rparse(b[j]))));
    }
  }
  return { quotient: quotient.map(rtext), remainder: rem.slice(0, Math.max(db, 0)).map(rtext) };
}

/** Division properties on seeded random inputs, including a univariate check against long division. */
export function runDivisionProperties(seed, trials) {
  const rng = mulberry32(seed);
  const counts = { trials: 0, checks: 0, failures: [] };
  const check = (name, ok, detail) => {
    counts.checks += 1;
    if (!ok) counts.failures.push(`${name} (seed ${seed}): ${detail}`);
  };
  for (let trial = 0; trial < trials; trial++) {
    counts.trials += 1;
    const nvars = randInt(rng, 1, 3);
    const vars = VARIABLE_NAMES.slice(0, nvars);
    const f = parseLines(vars, randomLines(rng, nvars));
    const count = randInt(rng, 1, 3);
    const divisors = [];
    while (divisors.length < count) {
      const candidate = parseLines(vars, randomLines(rng, nvars));
      if (candidate.terms.length > 0) divisors.push(candidate);
    }
    const result = divide(f, divisors);
    const again = divide(f, divisors);
    check("division identity and remainder property", divisionIsValid(f, divisors, result), `${text(f)}`);
    check(
      "division is deterministic",
      JSON.stringify(result.quotients.map(lines)) === JSON.stringify(again.quotients.map(lines)) &&
        lines(result.remainder).join(";") === lines(again.remainder).join(";"),
      text(f),
    );
    if (nvars === 1 && divisors.length === 1) {
      const coefficientsOf = (p) => {
        const out = Array.from({ length: (p.terms[0]?.e[0] ?? 0) + 1 }, () => "0");
        for (const t of p.terms) out[t.e[0]] = rtext(t.c);
        return out;
      };
      const reference = univariateLongDivision(coefficientsOf(f), coefficientsOf(divisors[0]));
      const got = result.quotients[0].terms.length ? coefficientsOf(result.quotients[0]) : [];
      const gotRemainder = result.remainder.terms.length ? coefficientsOf(result.remainder) : [];
      const trim = (xs) => {
        const out = xs.slice();
        while (out.length > 0 && (out[out.length - 1] === "0" || out[out.length - 1] === undefined)) out.pop();
        return out;
      };
      check(
        "univariate quotient agrees with long division",
        JSON.stringify(trim(got)) === JSON.stringify(trim(reference.quotient)),
        `${text(f)} / ${text(divisors[0])}`,
      );
      check(
        "univariate remainder agrees with long division",
        JSON.stringify(trim(gotRemainder)) === JSON.stringify(trim(reference.remainder)),
        `${text(f)} / ${text(divisors[0])}`,
      );
    }
  }
  return counts;
}

/** Explicit division cases with known answers, including the hand computations of examples/mpolynomial_division.ostrin. */
export function divisionEdgeAssertions() {
  const failures = [];
  const expect = (name, got, want) => {
    if (got !== want) failures.push(`${name}: got ${JSON.stringify(got)}, want ${JSON.stringify(want)}`);
  };
  const xy = ["x", "y"];
  const f2 = parseLines(xy, ["1|2,0", "1|0,2"]);
  const r2 = divide(f2, [parseLines(xy, ["1|1,0", "-1|0,0"]), parseLines(xy, ["1|0,1", "-1|0,0"])]);
  expect("x^2 + y^2 quotient 0", lines(r2.quotients[0]).join(";"), "1|1,0;1|0,0");
  expect("x^2 + y^2 quotient 1", lines(r2.quotients[1]).join(";"), "1|0,1;1|0,0");
  expect("x^2 + y^2 remainder", lines(r2.remainder).join(";"), "2|0,0");
  const f1 = parseLines(xy, ["1|2,0", "-1|0,2"]);
  const r1 = divide(f1, [parseLines(xy, ["1|1,0", "1|0,1"])]);
  expect("x^2 - y^2 quotient", lines(r1.quotients[0]).join(";"), "1|1,0;-1|0,1");
  expect("x^2 - y^2 remainder", lines(r1.remainder).length, 0);
  const fu = parseLines(["x"], ["1|3", "-2|1", "5|0"]);
  const ru = divide(fu, [parseLines(["x"], ["1|2", "1|0"])]);
  expect("univariate quotient", lines(ru.quotients[0]).join(";"), "1|1");
  expect("univariate remainder", lines(ru.remainder).join(";"), "-3|1;5|0");
  expect("zero polynomial divided", lines(divide(parseLines(xy, []), [parseLines(xy, ["1|1,0"])]).remainder).length, 0);
  return failures;
}

/** Division cases for the generated Ostrin program. Divisors are lists of lines; an empty divisor list is refused. */
export function buildDivisionCases() {
  const cases = [];
  const valid = (vars, f, divisors) => cases.push({ kind: "valid", vars, f, divisors });
  const refuse = (vars, f, divisors) => cases.push({ kind: "refuse", vars, f, divisors });
  valid(["x", "y"], ["1|2,0", "-1|0,2"], [["1|1,0", "1|0,1"]]);
  valid(["x", "y"], ["1|2,0", "1|0,2"], [["1|1,0", "-1|0,0"], ["1|0,1", "-1|0,0"]]);
  valid(["x", "y"], ["1|2,1", "1|1,0", "1|0,0"], [["1|1,1", "-1|0,0"]]);
  valid(["x", "y"], [], [["1|1,0"]]);
  valid(["x", "y"], ["1/2|1,0", "1/3|0,0"], [["1|1,0"]]);
  valid(["x"], ["1|3", "-2|1", "5|0"], [["1|2", "1|0"]]);
  valid(["x", "y", "z"], ["2|2,1,0", "-3|0,1,0", "1/2|0,0,0"], [["1|1,0,1", "-1|0,0,0"], ["1|0,2,0"]]);
  valid(["x", "y"], ["1|3,0", "1|0,3"], [["1|1,0"], ["1|0,1"]]);
  refuse(["x", "y"], ["1|1,0"], []);
  refuse(["x", "y"], ["1|1,0"], [[]]);
  refuse(["x", "y"], ["1|1,0"], [["1|1"]]);
  refuse(["x"], ["1|1"], [["1|1,0"]]);
  const rng = mulberry32(20260801);
  for (let k = 0; k < 30; k++) {
    const nvars = randInt(rng, 1, 3);
    const vars = VARIABLE_NAMES.slice(0, nvars);
    const f = randomLines(rng, nvars);
    const divisors = [];
    const count = randInt(rng, 1, 3);
    while (divisors.length < count) {
      const candidate = randomLines(rng, nvars);
      if (parseLines(vars, candidate).terms.length > 0) divisors.push(candidate);
    }
    cases.push({ kind: "valid", vars, f, divisors });
  }
  return cases;
}

/** The expected output of each division case; kind must agree with the rules. */
export function expectedDivisionOutputs(cases) {
  return cases.map((c, i) => {
    const refusal = (() => {
      if (c.divisors.length === 0) return "no divisors";
      if (c.divisors.some((d) => d.length === 0)) return "zero divisor";
      if (c.divisors.some((d) => refusalReason(c.vars, d) !== null)) return "bad divisor";
      if (refusalReason(c.vars, c.f) !== null) return "bad input";
      return null;
    })();
    if (c.kind === "refuse") {
      if (refusal === null && refusalReason(c.vars, c.f) === null) {
        throw new Error(`division case ${i} is marked refuse but the rules accept it`);
      }
      return `case ${i}: refused`;
    }
    if (refusal !== null) throw new Error(`division case ${i} is marked valid but the rules refuse it: ${refusal}`);
    const f = parseLines(c.vars, c.f);
    const divisors = c.divisors.map((d) => parseLines(c.vars, d));
    const result = divide(f, divisors);
    if (!divisionIsValid(f, divisors, result)) throw new Error(`division case ${i} violates the identity`);
    const quotients = result.quotients.map((q) => joinLines(lines(q))).join(" | ");
    return `case ${i}: quotients=${quotients}; remainder=${joinLines(lines(result.remainder))}`;
  });
}

/** The Ostrin program for the division cases. Encoding: lines joined by ";", divisors joined by "#". */
export function ostrinDivisionProgram(cases) {
  const quote = (s) => JSON.stringify(s);
  const list = (xs) => `[${xs.map(quote).join(", ")}]`;
  const calls = cases.map((c, i) => {
    const f = c.f.join(";");
    const ds = c.divisors.map((d) => d.join(";")).join("#");
    if (c.kind === "refuse") return `    print(refused_division(${i}, ${list(c.vars)}, ${quote(f)}, ${quote(ds)}))`;
    return `    print(valid_division(${i}, ${list(c.vars)}, ${quote(f)}, ${quote(ds)}))`;
  });
  return `// Generated by scripts/mpolynomial-oracle.mjs --write from tests/oracle/mpolynomial-division-cases.json. Do not edit by hand.
// The expected output is tests/oracle/mpolynomial-division-expected.txt, computed by the independent BigInt oracle.
import std.mpolynomial

fn lines_of(text: String) -> List<String> {
    mut items: List<String> = []
    if text == "" {
        return items
    }
    for piece in text.split(";") {
        items.push(piece)
    }
    items
}

fn join_lines(items: List<String>) -> String {
    if items.length() == 0 {
        return "zero"
    }
    mut text = ""
    for i in 0 until items.length() {
        if i == 0 {
            text = items[i]
        } else {
            text = text + ";" + items[i]
        }
    }
    text
}

fn valid_division(index: Int, vars: List<String>, f_text: String, divisors_text: String) -> String {
    f = mpolynomial.mpoly_parse(vars, lines_of(f_text)).unwrap()
    parts = divisors_text.split("#")
    mut divisors = [mpolynomial.mpoly_parse(vars, lines_of(parts[0])).unwrap()]
    for k in 1 until parts.length() {
        divisors.push(mpolynomial.mpoly_parse(vars, lines_of(parts[k])).unwrap())
    }
    d = mpolynomial.mpoly_divide(f, divisors).unwrap()
    mut quotients = ""
    for i in 0 until d.quotients.length() {
        if i == 0 {
            quotients = join_lines(mpolynomial.mpoly_lines(d.quotients[i]))
        } else {
            quotients = quotients + " | " + join_lines(mpolynomial.mpoly_lines(d.quotients[i]))
        }
    }
    return "case " + index.to_string() + ": quotients=" + quotients + "; remainder=" + join_lines(mpolynomial.mpoly_lines(d.remainder))
}

fn refused_division(index: Int, vars: List<String>, f_text: String, divisors_text: String) -> String {
    refused = "case " + index.to_string() + ": refused"
    if divisors_text == "" {
        return refused
    }
    match mpolynomial.mpoly_parse(vars, lines_of(f_text)) {
        Err(message) => {
            return refused
        },
        Ok(f) => {
            parts = divisors_text.split("#")
            match mpolynomial.mpoly_parse(vars, lines_of(parts[0])) {
                Err(message) => {
                    return refused
                },
                Ok(first) => {
                    mut divisors = [first]
                    mut bad = false
                    for k in 1 until parts.length() {
                        match mpolynomial.mpoly_parse(vars, lines_of(parts[k])) {
                            Err(message) => {
                                bad = true
                            },
                            Ok(g) => {
                                divisors.push(g)
                            },
                        }
                    }
                    if bad {
                        return refused
                    }
                    match mpolynomial.mpoly_divide(f, divisors) {
                        Err(message) => {
                            return refused
                        },
                        Ok(d) => {
                            return "case " + index.to_string() + ": accepted"
                        },
                    }
                },
            }
        },
    }
    return refused
}

fn main() -> Void {
${calls.join("\n")}
}
`;
}

// ---------------------------------------------------------------- command line

function generated() {
  const cases = buildCases();
  const divisionCases = buildDivisionCases();
  return {
    cases: JSON.stringify({ format: "mpolynomial-oracle-cases", version: 1, cases }, null, 2) + "\n",
    expected: expectedOutputs(cases).join("\n") + "\n",
    program: ostrinProgram(cases),
    divisionCases: JSON.stringify({ format: "mpolynomial-division-cases", version: 1, cases: divisionCases }, null, 2) + "\n",
    divisionExpected: expectedDivisionOutputs(divisionCases).join("\n") + "\n",
    divisionProgram: ostrinDivisionProgram(divisionCases),
  };
}

const GENERATED = [
  [CASES_PATH, "cases"],
  [EXPECTED_PATH, "expected"],
  [PROGRAM_PATH, "program"],
  [DIVISION_CASES_PATH, "divisionCases"],
  [DIVISION_EXPECTED_PATH, "divisionExpected"],
  [DIVISION_PROGRAM_PATH, "divisionProgram"],
];

function main() {
  const mode = process.argv[2] ?? "";
  if (mode === "--write" || mode === "--check") {
    const files = generated();
    if (mode === "--write") {
      for (const [path, key] of GENERATED) writeFileSync(path, files[key]);
      console.log("wrote the interchange and division oracle files under tests/oracle/ and examples/");
      return;
    }
    const stale = GENERATED.map(([path, key]) => [path, files[key]]).filter(([path, text]) => {
      try {
        return readFileSync(path, "utf8").replace(/\r\n/g, "\n") !== text;
      } catch {
        return true;
      }
    });
    if (stale.length > 0) {
      console.error(`stale generated files: ${stale.map(([p]) => p).join(", ")}; run with --write`);
      process.exit(1);
    }
    console.log("generated files are current");
    return;
  }

  let failures = edgeAssertions().concat(divisionEdgeAssertions());
  const seeds = [1, 2, 3, 20260731];
  let trials = 0;
  let checks = 0;
  for (const seed of seeds) {
    const counts = runProperties(seed, 150);
    trials += counts.trials;
    checks += counts.checks;
    failures = failures.concat(counts.failures);
    const division = runDivisionProperties(seed, 100);
    trials += division.trials;
    checks += division.checks;
    failures = failures.concat(division.failures);
  }
  const cases = buildCases();
  expectedOutputs(cases);
  const divisionCases = buildDivisionCases();
  expectedDivisionOutputs(divisionCases);
  console.log(`edge assertions: ${failures.length === 0 ? "all pass" : "failures"}`);
  console.log(`division cases: ${divisionCases.length} (${divisionCases.filter((c) => c.kind === "refuse").length} refused)`);
  console.log(`property trials: ${trials}, checks: ${checks}, seeds: ${seeds.join(", ")}`);
  console.log(`interchange cases: ${cases.length} (${cases.filter((c) => c.kind === "refuse").length} refused)`);
  if (failures.length > 0) {
    console.error(failures.join("\n"));
    process.exit(1);
  }
  console.log("all oracle checks pass");
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  main();
}
