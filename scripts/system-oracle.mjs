// Independent exact oracle for linear systems A x = b (BigInt rationals, no npm packages).
//
// Usage:
//   node scripts/system-oracle.mjs > compiler/tests/fixtures/solve_systems_oracle.txt
//
// Output: one line per case, and nothing else on stdout:
//   <id> | <size> | <matrix> | <rhs> | <result>
//   <size>   is n, the number of unknowns (1..4).
//   <matrix> rows separated by ";", entries within a row separated by ",", decimal integers.
//   <rhs>    entries separated by ",".
//   <result> one of:
//     "n/d,n/d,..."  one fraction per unknown, in order, separated by ",". Reduced, denominator > 0,
//                    zero is "0/1", integers are written "k/1".
//     "infinitely many"  the system is singular and consistent.
//     "no solution"      the system is singular and inconsistent.
//     "singular system"  reserved label for a unique-solution failure. The rules below never
//                        produce it: every singular matrix is classified by rank, so it cannot occur.
//
// Outcome rules. A is the n x n coefficient matrix, Ab is A with b appended as column n+1.
//   1. det(A) != 0 -> unique solution. Computed by Gauss-Jordan elimination on Ab. For n = 2 the
//      solution is computed by Cramer's rule and cross-checked against Gauss-Jordan. Every unique
//      solution is verified by exact substitution A x = b.
//   2. det(A) = 0 -> compare ranks.
//        rank(A) < rank(Ab)            -> "no solution"
//        rank(A) = rank(Ab) < n        -> "infinitely many"
//        rank(A) = rank(Ab) = n        -> impossible when det(A) = 0; the program aborts if reached.
//
// Cases:
//   H01..H22  hand-picked: unique 1x1, 2x2, 3x3, 4x4 (rational solutions), dependent 2x2 and 3x3,
//             inconsistent 2x2 and 3x3, singular 4x4 (consistent and inconsistent), zero matrix with
//             zero rhs, zero matrix with nonzero rhs.
//   G01..G30  generated with Park-Miller minimal standard generator, seed 3:
//             x <- 16807 * x mod 2147483647, each draw is one step, first draw is 50421.
//             Each coefficient is (x mod 19) - 9, in -9..9, drawn row by row, then the rhs.
//             Sizes cycle 2, 3, 4. Every fifth generated case (G05, G10, ..., G30) is made singular on
//             purpose: its last row is replaced by a copy of its first row, and its last rhs entry is
//             set to the first rhs entry (consistent) or that value plus 1 (inconsistent), chosen by
//             the parity of one more draw.
//
// Diagnostics (case counts per result) go to stderr and are not part of the fixture.

const MOD = 2147483647n;
const MUL = 16807n;

// ---- exact fractions: {n, d} with BigInt n, d; d > 0; gcd(|n|, d) = 1 ----

function gcd(a, b) {
  if (a < 0n) a = -a;
  if (b < 0n) b = -b;
  while (b !== 0n) {
    const t = a % b;
    a = b;
    b = t;
  }
  return a;
}

function fr(n, d = 1n) {
  if (d === 0n) throw new Error("zero denominator");
  if (d < 0n) {
    n = -n;
    d = -d;
  }
  const g = gcd(n, d) || 1n;
  return { n: n / g, d: d / g };
}

const fAdd = (a, b) => fr(a.n * b.d + b.n * a.d, a.d * b.d);
const fSub = (a, b) => fr(a.n * b.d - b.n * a.d, a.d * b.d);
const fMul = (a, b) => fr(a.n * b.n, a.d * b.d);
const fDiv = (a, b) => {
  if (b.n === 0n) throw new Error("division by zero");
  return fr(a.n * b.d, a.d * b.n);
};
const fIsZero = (a) => a.n === 0n;
const fEq = (a, b) => a.n === b.n && a.d === b.d;
const fStr = (a) => `${a.n}/${a.d}`;

// ---- elimination ----

// Reduced row echelon form over the first ncols columns of M (rows are arrays of fractions).
function rref(M, ncols) {
  const R = M.map((r) => r.slice());
  const pivots = [];
  let row = 0;
  for (let col = 0; col < ncols && row < R.length; col++) {
    let p = row;
    while (p < R.length && fIsZero(R[p][col])) p++;
    if (p === R.length) continue;
    [R[row], R[p]] = [R[p], R[row]];
    const piv = R[row][col];
    R[row] = R[row].map((v) => fDiv(v, piv));
    for (let r = 0; r < R.length; r++) {
      if (r !== row && !fIsZero(R[r][col])) {
        const factor = R[r][col];
        R[r] = R[r].map((v, j) => fSub(v, fMul(factor, R[row][j])));
      }
    }
    pivots.push(col);
    row++;
  }
  return { R, pivots };
}

// Determinant of a square matrix of fractions by forward elimination with row swaps.
function determinant(A) {
  const n = A.length;
  const R = A.map((r) => r.slice());
  let det = fr(1n);
  for (let k = 0; k < n; k++) {
    let p = k;
    while (p < n && fIsZero(R[p][k])) p++;
    if (p === n) return fr(0n);
    if (p !== k) {
      [R[k], R[p]] = [R[p], R[k]];
      det = fr(-det.n, det.d);
    }
    det = fMul(det, R[k][k]);
    for (let r = k + 1; r < n; r++) {
      if (!fIsZero(R[r][k])) {
        const factor = fDiv(R[r][k], R[k][k]);
        for (let j = k; j < n; j++) R[r][j] = fSub(R[r][j], fMul(factor, R[k][j]));
      }
    }
  }
  return det;
}

// ---- solving ----

const toFrac = (M) => M.map((r) => r.map((v) => fr(BigInt(v))));

// For det != 0: Gauss-Jordan on Ab gives the unique solution in column n.
function gaussSolution(Ab, n) {
  const { R, pivots } = rref(Ab, n);
  if (pivots.length !== n || pivots[n - 1] !== n - 1) {
    throw new Error("internal: nonsingular system without a full pivot set");
  }
  return R.map((r) => r[n]);
}

function classify(A, b) {
  const n = A.length;
  const Af = toFrac(A);
  const Ab = Af.map((r, i) => [...r, fr(BigInt(b[i]))]);
  const det = determinant(Af);

  if (!fIsZero(det)) {
    let x;
    if (n === 2) {
      const [a, bq] = Af[0];
      const [c, d] = Af[1];
      const e = Ab[0][2];
      const f = Ab[1][2];
      const dt = fSub(fMul(a, d), fMul(bq, c));
      x = [fDiv(fSub(fMul(e, d), fMul(bq, f)), dt), fDiv(fSub(fMul(a, f), fMul(c, e)), dt)];
      const g = gaussSolution(Ab, n);
      if (!x.every((v, i) => fEq(v, g[i]))) throw new Error("internal: Cramer and Gauss disagree");
    } else {
      x = gaussSolution(Ab, n);
    }
    for (let i = 0; i < n; i++) {
      let s = fr(0n);
      for (let j = 0; j < n; j++) s = fAdd(s, fMul(Af[i][j], x[j]));
      if (!fEq(s, Ab[i][n])) throw new Error("internal: solution fails A x = b");
    }
    return x.map(fStr).join(",");
  }

  const rA = rref(Af, n).pivots.length;
  const rAb = rref(Ab, n + 1).pivots.length;
  if (rA < rAb) return "no solution";
  if (rA === rAb && rA < n) return "infinitely many";
  throw new Error("internal: det = 0 but rank(A) = rank(Ab) = n");
}

// ---- hand-picked cases ----

const HAND = [
  { A: [[2]], b: [6] },
  { A: [[4]], b: [-2] },
  { A: [[0]], b: [5] },
  { A: [[1, 1], [1, -1]], b: [3, 1] },
  { A: [[2, 3], [4, 1]], b: [1, 5] },
  { A: [[3, 0], [0, -2]], b: [6, 4] },
  { A: [[-2, 1], [3, -1]], b: [1, 1] },
  { A: [[1, 2, 3], [2, -1, 3], [1, 2, -1]], b: [6, 9, 2] },
  { A: [[2, 0, 1], [0, 3, 1], [1, 1, 1]], b: [1, 2, 3] },
  { A: [[2, 1, 0], [1, 3, 1], [0, 1, 4]], b: [1, 2, 3] },
  { A: [[2, 1, 0, 0], [1, 2, 1, 0], [0, 1, 2, 1], [0, 0, 1, 2]], b: [1, 0, 0, 1] },
  { A: [[1, 2, 3, 4], [0, 1, 2, 3], [0, 0, 1, 2], [0, 0, 0, 2]], b: [1, 1, 1, 1] },
  { A: [[1, -1, 2, 0], [0, 3, -1, 4], [2, 0, 1, -2], [1, 1, 0, 3]], b: [3, -2, 0, 2] },
  { A: [[1, 2], [2, 4]], b: [3, 6] },
  { A: [[1, 2, 3], [4, 5, 6], [5, 7, 9]], b: [1, 2, 3] },
  { A: [[1, 2], [2, 4]], b: [3, 7] },
  { A: [[1, 2, 3], [4, 5, 6], [5, 7, 9]], b: [1, 2, 4] },
  { A: [[0, 0], [0, 0]], b: [0, 0] },
  { A: [[0, 0, 0], [0, 0, 0], [0, 0, 0]], b: [0, 0, 1] },
  { A: [[1, 0, 2, -1], [3, 1, 0, 2], [0, 2, -1, 3], [4, 1, 2, 1]], b: [1, 2, 5, 3] },
  { A: [[1, 0, 2, -1], [3, 1, 0, 2], [0, 2, -1, 3], [4, 1, 2, 1]], b: [1, 2, 5, 4] },
  { A: [[-3]], b: [4] },
];

// ---- generated cases (Park-Miller, seed 3) ----

let state = 3n;
function draw() {
  state = (state * MUL) % MOD;
  return state;
}
const coef = () => Number(draw() % 19n) - 9;

const GENERATED = [];
for (let g = 1; g <= 30; g++) {
  const n = [2, 3, 4][(g - 1) % 3];
  const A = [];
  for (let i = 0; i < n; i++) {
    const row = [];
    for (let j = 0; j < n; j++) row.push(coef());
    A.push(row);
  }
  const b = [];
  for (let i = 0; i < n; i++) b.push(coef());
  if (g % 5 === 0) {
    A[n - 1] = A[0].slice();
    const consistent = draw() % 2n === 0n;
    b[n - 1] = consistent ? b[0] : b[0] + 1;
  }
  GENERATED.push({ A, b });
}

// ---- main ----

const cases = [
  ...HAND.map((c, i) => ({ id: `H${String(i + 1).padStart(2, "0")}`, ...c })),
  ...GENERATED.map((c, i) => ({ id: `G${String(i + 1).padStart(2, "0")}`, ...c })),
];

const lines = [];
const counts = new Map();
for (const c of cases) {
  const n = c.A.length;
  if (n < 1 || n > 4) throw new Error(`${c.id}: size ${n} out of range`);
  if (!c.A.every((r) => r.length === n) || c.b.length !== n) {
    throw new Error(`${c.id}: not square or rhs length mismatch`);
  }
  const result = classify(c.A, c.b);
  const kind = result.includes("/") ? "unique" : result;
  counts.set(kind, (counts.get(kind) ?? 0) + 1);
  const matrix = c.A.map((r) => r.join(",")).join(";");
  lines.push(`${c.id} | ${n} | ${matrix} | ${c.b.join(",")} | ${result}`);
}

process.stdout.write(lines.join("\n") + "\n");
const summary = [...counts].map(([k, v]) => `${k} ${v}`).join(", ");
process.stderr.write(`system-oracle: ${cases.length} cases (${HAND.length} hand-picked, ${GENERATED.length} generated): ${summary}\n`);
