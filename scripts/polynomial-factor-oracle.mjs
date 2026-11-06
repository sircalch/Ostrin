// Independent oracle for examples/polynomial_factor.ostrin (std.polynomial: poly_content,
// poly_primitive, poly_squarefree).
//
// Usage (no packages, only Node's built-in BigInt):
//
//   node scripts/polynomial-factor-oracle.mjs > expected.txt
//   ostrinc --run examples/polynomial_factor.ostrin > actual.txt
//   diff expected.txt actual.txt
//
// Output format: one row per line, "case <idx> <label> <text>". idx is the generated case 0..39, or
// -1 for the fixed edge cases. Labels: p (the input), content, primitive, squarefree, and the named
// edge labels. A failed operation prints "error <message>" with the module's exact message.
//
// Normalization rules:
//   - Polynomial text: ascending coefficients "[c0, c1, ...]", the zero polynomial "0". A coefficient
//     is an integer "N" or a rational "N/D" in lowest terms (only the edge cases have rationals).
//   - content: the non-negative gcd of the integer coefficients, "0" for the zero polynomial. A
//     rational coefficient is the error "content needs integer coefficients".
//   - primitive: the content divided out, leading coefficient made positive. The zero polynomial
//     ("the zero polynomial has no primitive part") and rational input are errors.
//   - squarefree: p / gcd(p, p') over Q, normalized to the primitive integer polynomial with a
//     positive leading coefficient. The zero polynomial is an error ("the zero polynomial has no
//     squarefree part"). This oracle takes a different route than the module: rational input is
//     first scaled to integers by the lcm of the denominators, gcd(p, p') comes from the primitive
//     pseudo-remainder sequence over the integers, and the exact quotient is then made primitive.
//
// Generation: Park-Miller minimal standard generator, state = 16807 * state mod 2147483647, seeded
// with 11. Each draw advances the state once, then takes lo + state mod (hi - lo + 1). Each case
// first draws mode in 0..2:
//   - mode 2: draw dr in 1..3, then dr + 1 coefficients in -12..12 (ascending). The leading
//     coefficient is replaced by 1 when drawn as 0. The case is r * r (repeated factors, degree 2..6).
//   - otherwise: draw d in 1..6, then d + 1 coefficients in -12..12 by the same rule.

const CASES = 40;
const MODULUS = 2147483647n;
const MULTIPLIER = 16807n;

let state = 11n;
function next() {
  state = (state * MULTIPLIER) % MODULUS;
  return state;
}
function draw(s, lo, hi) {
  return BigInt(lo) + (s % (BigInt(hi) - BigInt(lo) + 1n));
}

// ---------------------------------------------------------------- BigInt rationals

function babs(x) {
  return x < 0n ? -x : x;
}

function bgcd(a, b) {
  a = babs(a);
  b = babs(b);
  while (b !== 0n) {
    [a, b] = [b, a % b];
  }
  return a;
}

function rat(n, d = 1n) {
  if (d === 0n) throw new Error("zero denominator");
  if (d < 0n) {
    n = -n;
    d = -d;
  }
  const g = bgcd(n, d);
  return { n: n / g, d: d / g };
}

const radd = (a, b) => rat(a.n * b.d + b.n * a.d, a.d * b.d);
const rsub = (a, b) => rat(a.n * b.d - b.n * a.d, a.d * b.d);
const rmul = (a, b) => rat(a.n * b.n, a.d * b.d);
const rdiv = (a, b) => {
  if (b.n === 0n) throw new Error("rational division by zero");
  return rat(a.n * b.d, a.d * b.n);
};
const rzero = (a) => a.n === 0n;
const rtext = (a) => (a.d === 1n ? a.n.toString() : `${a.n}/${a.d}`);
const rparse = (text) => {
  const slash = text.indexOf("/");
  if (slash < 0) return rat(BigInt(text));
  return rat(BigInt(text.slice(0, slash)), BigInt(text.slice(slash + 1)));
};

// ---------------------------------------------------------------- polynomials (ascending order)

function trimR(p) {
  let end = p.length;
  while (end > 0 && rzero(p[end - 1])) end--;
  return p.slice(0, end);
}

const ptext = (p) => (p.length === 0 ? "0" : "[" + p.map(rtext).join(", ") + "]");

// Integer polynomials are arrays of BigInt; trailing zeros are removed.
function trimI(p) {
  let end = p.length;
  while (end > 0 && p[end - 1] === 0n) end--;
  return p.slice(0, end);
}

function intMul(a, b) {
  if (a.length === 0 || b.length === 0) return [];
  const out = new Array(a.length + b.length - 1).fill(0n);
  for (let i = 0; i < a.length; i++) {
    for (let j = 0; j < b.length; j++) {
      out[i + j] += a[i] * b[j];
    }
  }
  return trimI(out);
}

function derivativeI(p) {
  const out = [];
  for (let i = 1; i < p.length; i++) out.push(BigInt(i) * p[i]);
  return trimI(out);
}

// ---------------------------------------------------------------- content and primitive part

const CONTENT_ERROR = "content needs integer coefficients";
const ZERO_PRIMITIVE = "the zero polynomial has no primitive part";
const ZERO_SQUAREFREE = "the zero polynomial has no squarefree part";

// The non-negative gcd of the coefficients, or an error when one is not an integer.
function contentOf(p) {
  let g = 0n;
  for (const c of p) {
    if (c.d !== 1n) return { error: CONTENT_ERROR };
    g = bgcd(g, c.n);
  }
  return { value: g };
}

function primitiveOf(p) {
  if (p.length === 0) return { error: ZERO_PRIMITIVE };
  const c = contentOf(p);
  if (c.error) return { error: c.error };
  const lead = p[p.length - 1].n;
  const scale = lead < 0n ? -c.value : c.value;
  return { value: p.map((x) => rat(x.n / scale)) };
}

// ---------------------------------------------------------------- squarefree part

// Pseudo-remainder over the integers: a remainder that is a nonzero multiple of a mod b.
function pseudoRemainder(a, b) {
  let r = trimI(a);
  const db = b.length - 1;
  const lb = b[db];
  while (r.length > 0 && r.length - 1 >= db) {
    const dr = r.length - 1;
    const lr = r[dr];
    const shift = dr - db;
    const next = r.map((c) => c * lb);
    for (let j = 0; j <= db; j++) next[shift + j] -= lr * b[j];
    r = trimI(next.slice(0, dr));
  }
  return r;
}

// The primitive integer polynomial with the same direction, positive leading coefficient.
function primitiveInt(p) {
  let c = 0n;
  for (const x of p) c = bgcd(c, x);
  const lead = p[p.length - 1];
  const scale = lead < 0n ? -c : c;
  return p.map((x) => x / scale);
}

// Primitive gcd over the integers (primitive pseudo-remainder sequence); b may be the zero polynomial.
function primitiveGcd(a, b) {
  let x = trimI(a);
  let y = trimI(b);
  if (y.length === 0) return primitiveInt(x);
  while (y.length > 0) {
    const r = pseudoRemainder(x, y);
    x = y;
    y = r.length > 0 ? primitiveInt(r) : [];
  }
  return primitiveInt(x);
}

// Exact quotient of rational polynomials a / b (b nonzero, remainder assumed zero).
function exactQuotient(a, b) {
  const work = a.slice();
  const lead = b[b.length - 1];
  const quotient = new Array(a.length - b.length + 1);
  for (let k = a.length - b.length; k >= 0; k--) {
    const c = rdiv(work[k + b.length - 1], lead);
    quotient[k] = c;
    for (let j = 0; j < b.length; j++) {
      work[k + j] = rsub(work[k + j], rmul(c, b[j]));
    }
  }
  return quotient;
}

function squarefreeOf(p) {
  if (p.length === 0) return { error: ZERO_SQUAREFREE };
  // Scale to integers by the lcm of the denominators (a nonzero scalar, so the primitive part is unchanged).
  let scale = 1n;
  for (const c of p) scale = (scale / bgcd(scale, c.d)) * c.d;
  const ints = p.map((c) => c.n * (scale / c.d));
  const g = primitiveGcd(ints, derivativeI(ints));
  const h = exactQuotient(
    ints.map((x) => rat(x)),
    g.map((x) => rat(x)),
  );
  return primitiveOf(trimR(h));
}

// ---------------------------------------------------------------- generation

function randomCoefficients(deg) {
  const out = [];
  for (let i = 0; i <= deg; i++) {
    let c = draw(next(), -12, 12);
    if (i === deg && c === 0n) c = 1n;
    out.push(c);
  }
  return out;
}

function generatedCase() {
  const mode = draw(next(), 0, 2);
  if (mode === 2n) {
    const dr = 1n + draw(next(), 0, 2);
    const r = randomCoefficients(Number(dr));
    return intMul(r, r);
  }
  const d = 1n + draw(next(), 0, 5);
  return randomCoefficients(Number(d));
}

// ---------------------------------------------------------------- output

const out = [];

function row(idx, label, text) {
  out.push(`case ${idx} ${label} ${text}`);
}

function emit(idx, label, result, render) {
  if (result.error !== undefined) {
    row(idx, label, "error " + result.error);
  } else {
    row(idx, label, render(result.value));
  }
}

const showPoly = (p) => ptext(trimR(p));
const showInt = (x) => x.toString();

for (let idx = 0; idx < CASES; idx++) {
  const ints = generatedCase();
  const p = ints.map((x) => rat(x));
  row(idx, "p", showPoly(p));
  emit(idx, "content", contentOf(p), showInt);
  emit(idx, "primitive", primitiveOf(p), showPoly);
  emit(idx, "squarefree", squarefreeOf(p), showPoly);
}

// Fixed edge cases, given as rationals.
const edge = (label, texts) => texts.map(rparse);
const EDGE = -1;
emit(EDGE, "zero-content", contentOf([]), showInt);
emit(EDGE, "zero-primitive", primitiveOf([]), showPoly);
emit(EDGE, "zero-squarefree", squarefreeOf([]), showPoly);
const half = edge("half", ["1/2", "1"]);
emit(EDGE, "rational-content", contentOf(half), showInt);
emit(EDGE, "rational-primitive", primitiveOf(half), showPoly);
emit(EDGE, "rational-squarefree", squarefreeOf(edge("q", ["1/4", "1", "1"])), showPoly);
emit(EDGE, "negative-primitive", primitiveOf([rat(-4n), rat(-6n)]), showPoly);
emit(EDGE, "constant-squarefree", squarefreeOf([rat(-6n)]), showPoly);
const huge = edge("huge", [
  "123456789012345678901234567890",
  "123456789012345678901234567890000",
]);
emit(EDGE, "huge-content", contentOf(huge), showInt);
emit(EDGE, "huge-primitive", primitiveOf(huge), showPoly);
emit(EDGE, "cubic-squarefree", squarefreeOf([rat(2n), rat(-3n), rat(0n), rat(1n)]), showPoly);

process.stdout.write(out.join("\n") + "\n");
