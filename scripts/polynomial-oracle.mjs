// Independent oracle for examples/polynomial_core.ostrin (std.polynomial).
//
// It regenerates the same 40 integer polynomials from the same 31-bit linear congruential generator,
// computes every operation with its own BigInt rational arithmetic (no shared code with the Ostrin
// module), and prints the rows in the same format as the Ostrin program. Compare the two outputs:
//
//   node scripts/polynomial-oracle.mjs > expected.txt
//   ostrinc --run examples/polynomial_core.ostrin > actual.txt
//   diff expected.txt actual.txt
//
// No external packages: only Node's built-in BigInt.

const CASES = 40;
const SEED = 20260417n;
const MODULUS = 2147483648n;

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
const risZero = (a) => a.n === 0n;
const rtext = (a) => (a.d === 1n ? a.n.toString() : `${a.n}/${a.d}`);

// ---------------------------------------------------------------- polynomials (ascending order)

function trim(p) {
  let end = p.length;
  while (end > 0 && risZero(p[end - 1])) end--;
  return p.slice(0, end);
}

const fromInts = (ints) => trim(ints.map((x) => rat(BigInt(x))));
const ptext = (p) => (p.length === 0 ? "0" : "[" + p.map(rtext).join(", ") + "]");
const coefAt = (p, i) => (i < p.length ? p[i] : rat(0n));

function padd(a, b) {
  const n = Math.max(a.length, b.length);
  const out = [];
  for (let i = 0; i < n; i++) out.push(radd(coefAt(a, i), coefAt(b, i)));
  return trim(out);
}

function psub(a, b) {
  const n = Math.max(a.length, b.length);
  const out = [];
  for (let i = 0; i < n; i++) out.push(rsub(coefAt(a, i), coefAt(b, i)));
  return trim(out);
}

function pmul(a, b) {
  if (a.length === 0 || b.length === 0) return [];
  const out = new Array(a.length + b.length - 1).fill(rat(0n));
  for (let i = 0; i < a.length; i++) {
    for (let j = 0; j < b.length; j++) {
      out[i + j] = radd(out[i + j], rmul(a[i], b[j]));
    }
  }
  return trim(out);
}

// Long division from the top degree down, on a working copy of the dividend.
function pdivmod(a, b) {
  if (b.length === 0) return { error: "polynomial division by the zero polynomial" };
  if (a.length < b.length) return { quotient: [], remainder: a };
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
  return {
    quotient: trim(quotient),
    remainder: trim(work.slice(0, b.length - 1)),
  };
}

function monic(p) {
  if (p.length === 0) return p;
  const lead = p[p.length - 1];
  return trim(p.map((c) => rdiv(c, lead)));
}

function pgcd(a, b) {
  let x = a;
  let y = b;
  while (y.length > 0) {
    const d = pdivmod(x, y);
    if (d.error) return { error: d.error };
    x = y;
    y = d.remainder;
  }
  return { value: monic(x) };
}

function peval(p, x) {
  let total = rat(0n);
  let power = rat(1n);
  for (let i = 0; i < p.length; i++) {
    total = radd(total, rmul(p[i], power));
    power = rmul(power, x);
  }
  return total;
}

function pderiv(p) {
  const out = [];
  for (let i = 1; i < p.length; i++) out.push(rmul(p[i], rat(BigInt(i))));
  return trim(out);
}

// ---------------------------------------------------------------- rational roots

const LIMIT = 1000000000000n;
const TOO_LARGE = "constant or leading coefficient too large for a root search (above 10^12)";
const LCM_TOO_LARGE = "coefficient denominators too large for a root search (lcm above 10^12)";

function positiveDivisors(n) {
  const out = [];
  for (let d = 1n; d * d <= n; d++) {
    if (n % d === 0n) {
      out.push(d);
      if (d * d !== n) out.push(n / d);
    }
  }
  return out;
}

function compareRat(a, b) {
  const x = a.n * b.d;
  const y = b.n * a.d;
  return x < y ? -1 : x > y ? 1 : 0;
}

function proots(p) {
  if (p.length === 0) return { error: "the zero polynomial has every rational root" };
  // Clear denominators with the lcm of all denominators.
  let lcm = 1n;
  for (const c of p) {
    lcm = (lcm / bgcd(lcm, c.d)) * c.d;
    if (lcm > LIMIT) return { error: LCM_TOO_LARGE };
  }
  const ints = p.map((c) => (c.n * (lcm / c.d)));
  let low = 0;
  while (ints[low] === 0n) low++;
  let roots = [];
  if (low > 0) roots.push(rat(0n));
  const reduced = ints.slice(low);
  if (reduced.length < 2) return { value: roots };
  const a0 = babs(reduced[0]);
  const an = babs(reduced[reduced.length - 1]);
  if (a0 > LIMIT || an > LIMIT) return { error: TOO_LARGE };
  const reducedRat = reduced.map((x) => rat(x));
  const candidates = [];
  for (const pnum of positiveDivisors(a0)) {
    for (const qden of positiveDivisors(an)) {
      if (bgcd(pnum, qden) !== 1n) continue;
      candidates.push(rat(pnum, qden));
      candidates.push(rat(-pnum, qden));
    }
  }
  for (const cand of candidates) {
    if (risZero(peval(reducedRat, cand))) roots.push(cand);
  }
  roots.sort(compareRat);
  return { value: roots };
}

function pfactor(p) {
  const r = proots(p);
  if (r.error) return { error: r.error };
  let rest = p;
  const factors = [];
  for (const root of r.value) {
    const lin = [rat(-root.n, root.d), rat(1n)];
    for (;;) {
      const d = pdivmod(rest, lin);
      if (d.remainder.length !== 0) break;
      factors.push(lin);
      rest = d.quotient;
    }
  }
  return { factors, cofactor: rest, roots: r.value };
}

// ---------------------------------------------------------------- generator (same stream as the Ostrin program)

let state = SEED;
function nextState() {
  state = (state * 1103515245n + 12345n) % MODULUS;
  return state;
}
const draw = (s, lo, hi) => lo + (s % (hi - lo + 1n));
const roll = (lo, hi) => draw(nextState(), BigInt(lo), BigInt(hi));

function intMul(a, b) {
  const out = [];
  for (let k = 0; k < a.length + b.length - 1; k++) {
    let acc = 0n;
    for (let i = 0; i < a.length; i++) {
      const j = k - i;
      if (j >= 0 && j < b.length) acc += a[i] * b[j];
    }
    out.push(acc);
  }
  return out;
}

const rows = [];
function row(idx, label, text) {
  rows.push(`case ${idx} ${label} ${text}`);
}

for (let idx = 0; idx < CASES; idx++) {
  const fq = roll(1, 3);
  const fr = roll(-4, 4);
  const shared = [-fr, fq];

  const mode = roll(0, 2);
  let pList = shared;
  if (mode === 2n) {
    const qa = roll(-3, 3);
    const qb = roll(-3, 3);
    pList = intMul(shared, [qb, qa, 1n]);
  } else {
    let c = roll(-3, 3);
    if (c === 0n) c = 2n;
    pList = intMul(shared, [c]);
    const l1q = roll(1, 3);
    const l1r = roll(-4, 4);
    pList = intMul(pList, [-l1r, l1q]);
    if (mode === 1n) {
      const l2q = roll(1, 3);
      const l2r = roll(-4, 4);
      pList = intMul(pList, [-l2r, l2q]);
    }
  }

  const qmode = roll(0, 2);
  let qList = shared;
  if (qmode === 2n) {
    const a0 = roll(-4, 4);
    const a1 = roll(-4, 4);
    let a2 = roll(-4, 4);
    if (a2 === 0n) a2 = 1n;
    qList = [a0, a1, a2];
  } else {
    const l3q = roll(1, 3);
    const l3r = roll(-4, 4);
    qList = [-l3r, l3q];
    if (qmode === 1n) qList = intMul(shared, qList);
  }

  const en = roll(-3, 3);
  const ed = roll(1, 3);
  const xtext = `${en}/${ed}`;
  const x = rat(en, ed);

  const p = fromInts(pList);
  const q = fromInts(qList);
  row(idx, "p", ptext(p));
  row(idx, "q", ptext(q));
  row(idx, "add", ptext(padd(p, q)));
  row(idx, "sub", ptext(psub(p, q)));
  row(idx, "mul", ptext(pmul(p, q)));
  const d = pdivmod(p, q);
  row(idx, "quotient", ptext(d.quotient));
  row(idx, "remainder", ptext(d.remainder));
  const g = pgcd(p, q);
  row(idx, "gcd", ptext(g.value));
  row(idx, "eval " + xtext, rtext(peval(p, x)));
  row(idx, "derivative", ptext(pderiv(p)));
  const roots = proots(p);
  row(idx, "roots", "[" + roots.value.map(rtext).join(", ") + "]");
  const f = pfactor(p);
  let factorText = "";
  let prod = f.cofactor;
  for (const factor of f.factors) {
    factorText += ptext(factor) + " ";
    prod = pmul(prod, factor);
  }
  row(idx, "factors", factorText + "cofactor " + ptext(f.cofactor));
  row(idx, "check", ptext(prod) === ptext(p) ? "ok" : "FAIL");
  const zeroPoly = fromInts([]);
  const dz = pdivmod(p, zeroPoly);
  row(idx, "divzero", "error " + dz.error);
}

// Fixed edge cases.
{
  const halfThird = [rat(2n, 3n), rat(1n, 3n)];
  const r = proots(halfThird);
  row(-1, "rational-roots", "[" + r.value.map(rtext).join(", ") + "]");
  row(-1, "bad-text", "error rational with a zero denominator");
  const huge = fromInts([10000000000000, 1]);
  const h = proots(huge);
  row(-1, "huge-constant", "error " + h.error);
  const z = proots(fromInts([]));
  row(-1, "zero-roots", "error " + z.error);
  row(-1, "constant-derivative", ptext(pderiv(fromInts([5]))));
  const gcdA = fromInts([1, 2]);
  const gcdB = [rat(1n, 2n), rat(1n)];
  row(-1, "gcd-rational", ptext(pgcd(gcdA, gcdB).value));
}

process.stdout.write(rows.join("\n") + "\n");
