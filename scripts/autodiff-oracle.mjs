// Independent oracle for examples/autodiff_reverse.ostrin (verifies the Ostrin std.autodiff module).
//
// Usage:
//   node scripts/autodiff-oracle.mjs > compiler/tests/fixtures/autodiff_reverse_oracle.txt
//
// No external packages. This file shares no code with compiler/std/autodiff.ostrin: the gradients
// printed here are closed-form derivatives written out by hand, and the check uses central
// differences with step h = 1e-6 on the plain expression. The tape is not reproduced.
//
// Output lines are the same as the Ostrin example's, character for character:
//   f=<expression> x=<x> y=<y> grad=[<gx>, <gy>] ok|MISMATCH
//   case=<label> error: <message>
// Numbers are printed with exactly 9 decimals: round half up of |v| * 1e9 (in IEEE double arithmetic,
// the same operations the Ostrin fixed9 performs), with a "-" only when the rounded value is nonzero.
// The Ostrin output must equal the fixture byte for byte, so the comparison is at 1e-9 absolute
// resolution of the printed value.
//
// Diagnostics (max relative error between the closed form and the central difference) go to stderr
// and are not part of the fixture.

const H = 1e-6;
const TOL = 1e-6;

const NAMES = [
  "x*y + sin(x)",
  "exp(x)*ln(y) - 2*(y/x)",
  "(x-y)^3 + cos(y)*x",
  "-(x*x + 1)/(y*y + 2)",
];

// Plain evaluation of each fixed expression.
function value(kind, x, y) {
  switch (kind) {
    case 0: return x * y + Math.sin(x);
    case 1: return Math.exp(x) * Math.log(y) - 2 * (y / x);
    case 2: { const d = x - y; return d * d * d + Math.cos(y) * x; }
    case 3: return 0 - (x * x + 1) / (y * y + 2);
    default: throw new Error("unknown kind");
  }
}

// Closed-form partial derivatives, written out by hand.
function closedForm(kind, x, y) {
  switch (kind) {
    case 0: return [y + Math.cos(x), x];
    case 1: return [
      Math.exp(x) * Math.log(y) + 2 * y / (x * x),
      Math.exp(x) / y - 2 / x,
    ];
    case 2: {
      const d = x - y;
      return [3 * d * d + Math.cos(y), -3 * d * d - x * Math.sin(y)];
    }
    case 3: {
      const n = x * x + 1;
      const d = y * y + 2;
      return [-(2 * x) / d, n * 2 * y / (d * d)];
    }
    default: throw new Error("unknown kind");
  }
}

// Central difference of the plain evaluation with step H.
function centralDifference(kind, x, y) {
  const fx = (value(kind, x + H, y) - value(kind, x - H, y)) / (2 * H);
  const fy = (value(kind, x, y + H) - value(kind, x, y - H)) / (2 * H);
  return [fx, fy];
}

// Same printing rule as the Ostrin fixed9: round half up of |v| * 1e9, integer arithmetic for the split.
function fixed9(v) {
  const neg = v < 0;
  const n = BigInt(Math.floor(Math.abs(v) * 1000000000 + 0.5));
  const whole = n / 1000000000n;
  const digits = (n % 1000000000n).toString().padStart(9, "0");
  const sign = neg && n !== 0n ? "-" : "";
  return sign + whole.toString() + "." + digits;
}

function closeEnough(a, b) {
  const scale = Math.abs(b) > 1 ? Math.abs(b) : 1;
  return Math.abs(a - b) <= TOL * scale;
}

let worst = 0;

function check(kind, x, y) {
  const head = "f=" + NAMES[kind] + " x=" + fixed9(x) + " y=" + fixed9(y);
  const [gx, gy] = closedForm(kind, x, y);
  const [fx, fy] = centralDifference(kind, x, y);
  const ok = closeEnough(gx, fx) && closeEnough(gy, fy);
  for (const [a, b] of [[gx, fx], [gy, fy]]) {
    const rel = Math.abs(a - b) / Math.max(1, Math.abs(b));
    if (rel > worst) worst = rel;
  }
  console.log(head + " grad=[" + fixed9(gx) + ", " + fixed9(gy) + "] " + (ok ? "ok" : "MISMATCH"));
}

const lines = [
  "# autodiff_reverse: reverse-mode gradients (std.autodiff) checked against central differences",
  "# step h = 1e-6; tolerance |ad - fd| <= 1e-6 * max(1, |fd|); numbers have 9 decimals",
  "# expressions: 0 x*y + sin(x); 1 exp(x)*ln(y) - 2*(y/x); 2 (x-y)^3 + cos(y)*x; 3 -(x*x + 1)/(y*y + 2)",
];
for (const l of lines) console.log(l);

check(0, 0.7, -1.3);
check(0, 2.0, 0.5);
check(0, -0.4, 1.1);
check(1, 0.9, 1.5);
check(1, 1.3, 0.25);
check(1, -0.6, 2.0);
check(2, 0.4, -0.9);
check(2, 1.5, 1.2);
check(2, -2.0, 0.3);
check(3, 0.8, -0.6);
check(3, -1.5, 2.5);
check(3, 0.0, 1.0);

console.log("# non-finite and invalid inputs return Err");
// Non-finite inputs are rejected before the tape is built.
console.log("case=nan_input error: reverse_grad_2 needs finite inputs");
console.log("case=inf_input error: reverse_grad_2 needs finite inputs");
// y = 0: ln(0) = -Infinity is the tape node at position 3 (x, y, exp(x), ln(y)).
console.log("case=ln_of_zero error: autodiff: non-finite value or partial at node 3");
console.log("case=unknown_kind error: reverse_grad_2: unknown expression kind");

console.error("max relative error |closed form - central difference| / max(1,|fd|): " + worst.toExponential(3));
