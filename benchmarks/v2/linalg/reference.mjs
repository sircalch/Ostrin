// Independent references for the linear-algebra benchmark. They use integer arithmetic (exact in JavaScript numbers for these
// sizes) on the same generated inputs as the Ostrin, Python and Julia programs, and never call an implementation under test.
//
// Inputs (exact in double precision):
//   matmul / norms: A_ij = ((7 i + 3 j) mod 11 - 5) / 4,  B_ij = ((5 i + 13 j) mod 9 - 4) / 8 (matmul only);
//   solve (lu_solve, solve_checked): D_ii = 2 n, D_ij = ((3 i + 5 j) mod 7 - 3) / 4 for i != j; x*_j = (j mod 9) - 4;
//     b = D x* (exact: every product is a multiple of 1/4 of small size).
//
// Tolerances (justified in the report):
//   matmul and norms 1 and infinity: exact integers, tolerance 0;
//   solve: D is strictly diagonally dominant by rows, so the partial-pivoting growth rho <= 2 and kappa_inf(D) <= 3 (both
//     from the dominance bounds). Backward error of LU: 3 n eps rho; forward error per entry: kappa * that * max|x*|.
//     The reference is the sum of x*; its error is at most n times the per-entry bound;
//   Frobenius norm: sqrt of an exact sum, so the rounding is at most 2 n eps times the value.

export const EPS = 2.220446049250313e-16;

export function matmulInputs(n) {
  const a = Array.from({ length: n }, (_, i) => Array.from({ length: n }, (_, j) => (((7 * i + 3 * j) % 11) - 5) / 4));
  const b = Array.from({ length: n }, (_, i) => Array.from({ length: n }, (_, j) => (((5 * i + 13 * j) % 9) - 4) / 8));
  return { a, b };
}

export function matmulReference(n) {
  // Work in units of 1/32: A is in quarters (x 4), B in eighths (x 8), so the product is an integer multiple of 1/32.
  const ai = Array.from({ length: n }, (_, i) => Array.from({ length: n }, (_, j) => ((7 * i + 3 * j) % 11) - 5));
  const bi = Array.from({ length: n }, (_, i) => Array.from({ length: n }, (_, j) => ((5 * i + 13 * j) % 9) - 4));
  let total = 0;
  for (let i = 0; i < n; i += 1) {
    for (let j = 0; j < n; j += 1) {
      let c = 0;
      for (let k = 0; k < n; k += 1) c += ai[i][k] * bi[k][j];
      total += c;
    }
  }
  return { value: total / 32, tolerance: 0 };
}

export function solveInputs(n) {
  const d = Array.from({ length: n }, (_, i) => Array.from({ length: n }, (_, j) => (i === j ? 2 * n : (((3 * i + 5 * j) % 7) - 3) / 4)));
  const xStar = Array.from({ length: n }, (_, j) => (j % 9) - 4);
  const b = d.map((row) => row.reduce((acc, v, j) => acc + v * xStar[j], 0));
  return { d, b, xStar };
}

export function solveReference(n) {
  const { xStar } = solveInputs(n);
  const sum = xStar.reduce((acc, v) => acc + v, 0);
  const maxX = Math.max(...xStar.map(Math.abs));
  const rho = 2;
  const kappa = 3;
  const perEntry = 3 * n * EPS * rho * kappa * maxX;
  return { value: sum, tolerance: n * perEntry };
}

export function normsReference(n) {
  const a = Array.from({ length: n }, (_, i) => Array.from({ length: n }, (_, j) => ((7 * i + 3 * j) % 11) - 5));
  // quarters: divide by 4 at the end; the 1-norm and infinity norm of the quarter-entries are exact.
  let col = 0;
  for (let j = 0; j < n; j += 1) {
    let s = 0;
    for (let i = 0; i < n; i += 1) s += Math.abs(a[i][j]);
    col = Math.max(col, s);
  }
  let row = 0;
  for (let i = 0; i < n; i += 1) {
    let s = 0;
    for (let j = 0; j < n; j += 1) s += Math.abs(a[i][j]);
    row = Math.max(row, s);
  }
  let sq = 0;
  for (let i = 0; i < n; i += 1) for (let j = 0; j < n; j += 1) sq += a[i][j] * a[i][j];
  return { exact: (col + row) / 4, fro: Math.sqrt(sq) / 4, tolerance: 2 * n * EPS * (Math.sqrt(sq) / 4) };
}

// The value each benchmark prints, and the reference the printed value is compared with.
export function referenceFor(kernel, n) {
  if (kernel === "matmul") return matmulReference(n);
  if (kernel === "lu_solve" || kernel === "solve_checked") return solveReference(n);
  if (kernel === "norms") return normsReference(n);
  throw new Error(`no reference for ${kernel}`);
}

// Parses the printed RESULT line. Returns the numbers in order.
export function parseResult(stdout, kernel) {
  const line = stdout.split(/\r?\n/).find((text) => text.startsWith(`RESULT ${kernel} `));
  if (line === undefined) return null;
  return line.slice(`RESULT ${kernel} `.length).trim().split(/\s+/).map(Number);
}

// Checks a run's printed numbers against the reference. Returns { ok, message }.
export function checkRun(kernel, n, stdout) {
  const got = parseResult(stdout, kernel);
  if (got === null || got.some((v) => !Number.isFinite(v))) return { ok: false, message: "no finite RESULT line" };
  const ref = referenceFor(kernel, n);
  if (kernel === "norms") {
    const exactOk = Math.abs(got[0] - ref.exact) <= 0;
    const froOk = Math.abs(got[1] - ref.fro) <= ref.tolerance;
    return exactOk && froOk ? { ok: true } : { ok: false, message: `norms ${got} vs exact ${ref.exact} and fro ${ref.fro}` };
  }
  const delta = Math.abs(got[0] - ref.value);
  return delta <= ref.tolerance ? { ok: true } : { ok: false, message: `value ${got[0]} differs from ${ref.value} by ${delta} (tolerance ${ref.tolerance})` };
}
