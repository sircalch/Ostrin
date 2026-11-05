// Independent exact oracle for equation solving (verifies the Ostrin std.solve module).
//
// Usage:
//   node scripts/solve-oracle.mjs > tests/fixtures/solve_core_oracle.txt
//
// No external packages. All arithmetic is BigInt. This file shares no code with the Ostrin module.
//
// ---------------------------------------------------------------- OUTPUT FORMAT
// Lines starting with "#" are header comments. Every other line is one case:
//
//     id | op | inputs | result
//
//   id      C001, C002, ... sequential over the case list (hand-picked first, then generated).
//   op      linear | quadratic | rational_roots
//   inputs  linear:          a=<int>,b=<int>
//           quadratic:       a=<int>,b=<int>,c=<int>
//           rational_roots:  c=[c0,c1,...,cn]   coefficients ascending by degree, no spaces, [] if empty
//
// ---------------------------------------------------------------- RESULT RULES
// Rational value: "n/d", reduced, d >= 1 always. Zero is "0/1", integers are "k/1".
// Several roots:  items joined by ", ", sorted ascending by numeric value, distinct.
//
// linear(a, b): a*x + b = 0.
//   a != 0                  -> rational value of x = -b/a
//   a == 0, b == 0          -> "all reals"
//   a == 0, b != 0          -> "no solution"
//
// quadratic(a, b, c): a*x^2 + b*x + c = 0, discriminant D = b*b - 4*a*c.
//   a == 0                  -> same result as linear(b, c)
//   D < 0                   -> "no real roots"
//   D a perfect square      -> rational roots (-b - sqrt(D))/(2a) and (-b + sqrt(D))/(2a),
//                              each reduced to "n/d", sorted, distinct (one item when D == 0).
//   otherwise (irrational)  -> normalized surd form, two items, ascending:
//     1. Write D = k^2 * q with q squarefree, q > 1, k >= 1 (k is the largest such that k^2 divides D).
//     2. P = -b and d = 2a. If d < 0, set P = -P and d = -d.
//        (This leaves the set of two roots unchanged.)
//     3. g = gcd(|P|, k, d). Divide P, k and d by g. Now d > 0 and gcd(|P|, k, d) = 1.
//     4. Items are "(P - S)/d" and then "(P + S)/d", where S = "sqrt(q)" if k = 1,
//        and S = "k*sqrt(q)" if k > 1. P is always printed (0 included) and d is always printed.
//        Example: a=1,b=-2,c=-2 gives D=12=2^2*3, P=2, d=2, k=2, g=2, so P=1, k=1, d=1,
//        result "(1 - sqrt(3))/1, (1 + sqrt(3))/1".
//        Example: a=4,b=0,c=-45 gives D=720=12^2*5, P=0, d=8, k=12, g=4, so P=0, k=3, d=2,
//        result "(0 - 3*sqrt(5))/2, (0 + 3*sqrt(5))/2".
//
// rational_roots(c0..cn): all rational roots of the polynomial c0 + c1*x + ... + cn*x^n.
//   all coefficients zero, or empty list -> "error: zero polynomial"
//   no rational roots (including nonzero constants) -> "none"
//   otherwise               -> rational roots "n/d", sorted ascending, distinct.
//
// ---------------------------------------------------------------- GENERATED CASES
// Park-Miller minimal standard generator: x <- 16807 * x mod 2147483647, seed 7.
// The first draw is the state after the first step. A coefficient is (x mod 61) - 30, in -30..30.
// Generated case i (0..39) has op chosen by i mod 3:
//   0 -> linear, draws a then b
//   1 -> quadratic, draws a then b then c
//   2 -> rational_roots, draws c0 then c1 then c2 then c3

// ---------------------------------------------------------------- BigInt helpers

function babs(x) {
  return x < 0n ? -x : x;
}

function gcd(a, b) {
  a = babs(a);
  b = babs(b);
  while (b !== 0n) {
    [a, b] = [b, a % b];
  }
  return a;
}

// Rational with positive denominator, reduced. Zero is 0/1.
function rat(n, d = 1n) {
  if (d === 0n) throw new Error("zero denominator");
  if (d < 0n) {
    n = -n;
    d = -d;
  }
  const g = gcd(n, d);
  return { n: n / g, d: d / g };
}

const ratText = (r) => `${r.n}/${r.d}`;

// Exact comparison of two rationals with positive denominators.
function ratCmp(x, y) {
  const l = x.n * y.d;
  const r = y.n * x.d;
  return l < r ? -1 : l > r ? 1 : 0;
}

function sortDistinct(list) {
  const sorted = [...list].sort(ratCmp);
  const out = [];
  for (const r of sorted) {
    if (out.length === 0 || ratCmp(out[out.length - 1], r) !== 0) out.push(r);
  }
  return out;
}

// Floor of the square root, Newton iteration, n >= 0.
function isqrt(n) {
  if (n < 2n) return n;
  let x = n;
  let y = (x + 1n) / 2n;
  while (y < x) {
    x = y;
    y = (x + n / x) / 2n;
  }
  return x;
}

// Positive divisors of n >= 1.
function divisors(n) {
  const out = [];
  for (let i = 1n; i * i <= n; i += 1n) {
    if (n % i === 0n) {
      out.push(i);
      if (i * i !== n) out.push(n / i);
    }
  }
  return out;
}

// n = k^2 * q with q squarefree. Returns { k, q }, n >= 1.
function squarefreeSplit(n) {
  let k = 1n;
  let q = 1n;
  let m = n;
  for (let p = 2n; p * p <= m; p += 1n) {
    let e = 0n;
    while (m % p === 0n) {
      m /= p;
      e += 1n;
    }
    k *= p ** (e / 2n);
    if (e % 2n === 1n) q *= p;
  }
  if (m > 1n) q *= m;
  return { k, q };
}

// ---------------------------------------------------------------- solvers

function solveLinear(a, b) {
  if (a === 0n) return b === 0n ? "all reals" : "no solution";
  return ratText(rat(-b, a));
}

function solveQuadratic(a, b, c) {
  if (a === 0n) return solveLinear(b, c);
  const D = b * b - 4n * a * c;
  if (D < 0n) return "no real roots";
  const s = isqrt(D);
  const den = 2n * a;
  if (s * s === D) {
    const roots = sortDistinct([rat(-b - s, den), rat(-b + s, den)]);
    return roots.map(ratText).join(", ");
  }
  const { k, q } = squarefreeSplit(D);
  let P = -b;
  let d = den;
  if (d < 0n) {
    P = -P;
    d = -d;
  }
  const g = gcd(gcd(P, k), d);
  P /= g;
  const kk = k / g;
  d /= g;
  const S = kk === 1n ? `sqrt(${q})` : `${kk}*sqrt(${q})`;
  return `(${P} - ${S})/${d}, (${P} + ${S})/${d}`;
}

// sum_i co[i] * n^i * d^(N-i), where N = co.length - 1. Zero exactly when n/d is a root (d > 0).
function evalHomogeneous(co, n, d) {
  const N = co.length - 1;
  let sum = 0n;
  for (let i = 0; i <= N; i++) {
    const idx = BigInt(i);
    sum += co[i] * n ** idx * d ** (BigInt(N) - idx);
  }
  return sum;
}

function solveRationalRoots(coeffs) {
  const c = [...coeffs];
  while (c.length > 0 && c[c.length - 1] === 0n) c.pop();
  if (c.length === 0) return "error: zero polynomial";

  const roots = [];
  let m = 0;
  while (c[m] === 0n) m++;
  if (m > 0) roots.push(rat(0n, 1n));

  // body has nonzero constant term and nonzero leading coefficient.
  const body = c.slice(m);
  const N = body.length - 1;
  if (N >= 1) {
    // Rational root theorem: a root p/q (reduced) has p | body[0] and q | body[N].
    const candidates = [];
    for (const p of divisors(babs(body[0]))) {
      for (const q of divisors(babs(body[N]))) {
        candidates.push(rat(p, q), rat(-p, q));
      }
    }
    for (const r of candidates) {
      if (evalHomogeneous(body, r.n, r.d) === 0n) roots.push(r);
    }
  }
  const all = sortDistinct(roots);
  return all.length === 0 ? "none" : all.map(ratText).join(", ");
}

// ---------------------------------------------------------------- cases

const HAND = [
  // linear
  { op: "linear", args: [0n, 0n] },
  { op: "linear", args: [0n, 5n] },
  { op: "linear", args: [0n, -1n] },
  { op: "linear", args: [3n, -6n] },
  { op: "linear", args: [-4n, 6n] },
  { op: "linear", args: [6n, -4n] },
  { op: "linear", args: [-7n, 21n] },
  { op: "linear", args: [1n, 0n] },
  { op: "linear", args: [10000n, -9999n] },
  { op: "linear", args: [-10000n, 3n] },
  { op: "linear", args: [7n, 10000n] },
  { op: "linear", args: [-3n, 0n] },
  // quadratic
  { op: "quadratic", args: [0n, 0n, 0n] },
  { op: "quadratic", args: [0n, 0n, 4n] },
  { op: "quadratic", args: [0n, 2n, -6n] },
  { op: "quadratic", args: [1n, -5n, 6n] }, // D=1, rational
  { op: "quadratic", args: [1n, 2n, 1n] }, // D=0, repeated root
  { op: "quadratic", args: [2n, 0n, -8n] }, // D=64, negative root
  { op: "quadratic", args: [1n, 0n, 1n] }, // D<0
  { op: "quadratic", args: [3n, 2n, 5n] }, // D<0
  { op: "quadratic", args: [1n, 0n, -2n] }, // D=8, k=2 q=2
  { op: "quadratic", args: [1n, 0n, -3n] }, // D=12, q=3
  { op: "quadratic", args: [1n, -2n, -2n] }, // D=12, P != 0
  { op: "quadratic", args: [2n, -4n, -2n] }, // D=32, g reduction
  { op: "quadratic", args: [1n, 7n, 1n] }, // D=45, k=3 q=5
  { op: "quadratic", args: [-1n, 7n, -1n] }, // D=45, negative a
  { op: "quadratic", args: [1n, 2n, -17n] }, // D=72, k=6 q=2
  { op: "quadratic", args: [1n, 0n, -5n] }, // D=20, k=2 q=5
  { op: "quadratic", args: [-2n, -4n, 6n] }, // D=64, negative leading
  { op: "quadratic", args: [-3n, 1n, 2n] }, // D=25, negative leading
  { op: "quadratic", args: [10000n, 10000n, -10000n] }, // D=5e8, large
  { op: "quadratic", args: [5000n, -10000n, 5000n] }, // D=0, large
  { op: "quadratic", args: [7n, -70n, 175n] }, // D=0
  { op: "quadratic", args: [1n, -1n, -1n] }, // D=5
  { op: "quadratic", args: [4n, 0n, -45n] }, // D=720, k=12 q=5, g=4
  { op: "quadratic", args: [9999n, -10000n, 1n] }, // large, irrational
  { op: "quadratic", args: [1n, 10000n, 1n] }, // large, irrational
  { op: "quadratic", args: [-10000n, 10000n, 10000n] }, // large, negative leading
  { op: "quadratic", args: [1n, -7n, 12n] }, // D=1, rational
  { op: "quadratic", args: [2n, 3n, -2n] }, // D=25, rational, a>0 non-unit
  { op: "quadratic", args: [3n, 0n, 0n] }, // D=0, zero root
  // rational_roots (coefficients ascending)
  { op: "rational_roots", args: [] },
  { op: "rational_roots", args: [0n, 0n, 0n] },
  { op: "rational_roots", args: [5n] },
  { op: "rational_roots", args: [0n, 1n] },
  { op: "rational_roots", args: [-2n, 0n, 1n] }, // irrational only
  { op: "rational_roots", args: [-6n, 1n, 1n] },
  { op: "rational_roots", args: [-1n, 0n, 1n] },
  { op: "rational_roots", args: [0n, 0n, 1n] },
  { op: "rational_roots", args: [0n, -1n, 0n, 1n] },
  { op: "rational_roots", args: [6n, -5n, 1n] },
  { op: "rational_roots", args: [4n, -4n, 1n] }, // repeated root
  { op: "rational_roots", args: [-1n, 3n, -3n, 1n] }, // triple root
  { op: "rational_roots", args: [-1n, 2n] },
  { op: "rational_roots", args: [3n, -2n] },
  { op: "rational_roots", args: [-2n, 1n, 6n] }, // leading coefficient 6
  { op: "rational_roots", args: [0n, 0n, -2n, 1n] },
  { op: "rational_roots", args: [-10000n, 0n, 0n, 0n, 1n] },
  { op: "rational_roots", args: [-10000n, 9999n, 1n] },
  { op: "rational_roots", args: [-6n, 11n, -6n, 1n] },
  { op: "rational_roots", args: [-1n, 0n, 4n] },
  { op: "rational_roots", args: [-9n, 0n, 4n] },
  { op: "rational_roots", args: [-8n, 0n, 0n, 1n] },
  { op: "rational_roots", args: [-10000n, 0n, 1n] },
  { op: "rational_roots", args: [0n, 0n, 0n, 7n] },
  { op: "rational_roots", args: [7n] },
  { op: "rational_roots", args: [1n, 0n, 0n, 0n, 0n, 0n, 1n] }, // x^6 + 1
  { op: "rational_roots", args: [-1n, 0n, 0n, 0n, 0n, 0n, 1n] }, // x^6 - 1
  { op: "rational_roots", args: [12n, -7n, 1n] },
];

// Park-Miller generated cases (seed 7).
let pmState = 7n;
function pmDraw() {
  pmState = (pmState * 16807n) % 2147483647n;
  return (pmState % 61n) - 30n;
}

const GENERATED = [];
for (let i = 0; i < 40; i++) {
  const kind = i % 3;
  if (kind === 0) {
    GENERATED.push({ op: "linear", args: [pmDraw(), pmDraw()] });
  } else if (kind === 1) {
    GENERATED.push({ op: "quadratic", args: [pmDraw(), pmDraw(), pmDraw()] });
  } else {
    GENERATED.push({
      op: "rational_roots",
      args: [pmDraw(), pmDraw(), pmDraw(), pmDraw()],
    });
  }
}

const CASES = [...HAND, ...GENERATED];

// ---------------------------------------------------------------- run

function inputsText(op, args) {
  if (op === "linear") return `a=${args[0]},b=${args[1]}`;
  if (op === "quadratic") return `a=${args[0]},b=${args[1]},c=${args[2]}`;
  return `c=[${args.join(",")}]`;
}

function solve(op, args) {
  if (op === "linear") return solveLinear(args[0], args[1]);
  if (op === "quadratic") return solveQuadratic(args[0], args[1], args[2]);
  return solveRationalRoots(args);
}

const lines = [
  "# solve-oracle: exact equation solving (independent BigInt oracle for std.solve)",
  "# line format: id | op | inputs | result",
  "# ops: linear, quadratic, rational_roots. See scripts/solve-oracle.mjs header for result rules.",
  `# cases: ${CASES.length} (${HAND.length} hand-picked, then ${GENERATED.length} Park-Miller seed 7)`,
];
CASES.forEach((tc, i) => {
  const id = `C${String(i + 1).padStart(3, "0")}`;
  lines.push(`${id} | ${tc.op} | ${inputsText(tc.op, tc.args)} | ${solve(tc.op, tc.args)}`);
});

process.stdout.write(lines.join("\n") + "\n");
