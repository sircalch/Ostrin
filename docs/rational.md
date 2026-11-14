# std.rational: univariate rational functions and their domains

This page documents `std.rational`: exact univariate rational functions, the operations on them, and the way their
domains are kept. The main rule is the one the domain has to satisfy: a simplification must not enlarge the domain of the
expression it simplifies.

## The representation

`RationalFunction { numerator, denominator, requirements }`, with coefficient lists lowest degree first, in canonical
rational text (see `std.bigint`).

- `numerator` and `denominator` are in lowest terms: the gcd is removed with the exact univariate gcd of `std.polynomial`,
  and the denominator is monic. The numerator of the zero function is the empty list.
- `requirements` is a list of polynomials. A point is in the domain only if every requirement is nonzero there.

The requirements are the domain. They are not derived from the reduced form, so reduction cannot remove them.

## Why the domain is kept

`(x^2 - 1)/(x - 1)` reduces to `x + 1`. The reduced function is defined at `x = 1`, but the original expression is not.
The function therefore keeps `x - 1` as a requirement. Evaluation at `x = 1` is refused, with the requirement named:

```
undefined at 1: the requirement [-1, 1] != 0 fails there
```

Each operation adds the requirements of its operands, and reduction never removes one:

| Operation | Value | Requirements added |
| --- | --- | --- |
| `rational_new(n, d)` | n / d | `d` |
| `rational_negate(a)` | -a | none |
| `rational_add(a, b)`, `rational_subtract(a, b)` | a ± b | those of `a` and `b` |
| `rational_multiply(a, b)` | a b | those of `a` and `b` |
| `rational_divide(a, b)` | a / b | those of `a` and `b`, and the numerator of `b` (the quotient is undefined where `b` is zero); `b` must not be the zero function |
| `rational_derivative(a)` | (N' D - N D') / D^2 | those of `a` (the domain does not change) |
| `rational_compose(f, g)` | f(g(x)) | those of `g`, the composed requirements of `f`, and the composed denominator |

Composition: with `K` the largest degree bound of `f`'s numerator, denominator and requirements, `f(g) = P / Q` where
`P = sum a_i Ng^i Dg^(K-i)` and `Q` is the same sum for `f`'s denominator. Each requirement `r` of `f` is composed the same
way. The requirements of `g` ensure that `Dg` is nonzero, so `g(x)` is defined. The composed requirements ensure that
`f` is defined at `g(x)`.

## Evaluation

`rational_evaluate(a, x)` checks every requirement at the rational point `x` first. If one vanishes, it returns an error
that names the requirement. Otherwise it returns the exact value `N(x) / D(x)` as canonical text.

`rational_defined_at(a, x)` returns whether all requirements are nonzero at `x`.

Numerical evaluation at a `Float` point is not offered. Evaluation is exact, at rational points written as text.

## Interchange with std.mpolynomial and std.symbolic

`rational_numerator_lines(a)` and `rational_denominator_lines(a)` return the coefficients as interchange lines
`coefficient|e` (one variable), the format of `std.mpolynomial` and `std.symbolic.expression_of_term_lines`. A program can
turn them into a symbolic expression with `symbolic.expression_of_term_lines([x], lines)`.

The requirements are returned as text by `rational_requirement_texts(a)`. They are not converted into
`std.symbolic.Condition` values: those can be built only by the conditional rules of `std.symbolic`, and this module does
not feed them into that machinery. The exclusion is enforced by `rational_evaluate` and `rational_defined_at`.

## Scope and limits

- One variable only. A multivariate rational function needs a multivariate gcd (or a verified cancellation algorithm),
  which is not implemented. `std.mpolynomial` has exact division with remainder, but a zero remainder is not a
  cancellation test for several divisors, and no multivariate rational type is provided.
- Requirements are not simplified: a requirement list can grow with the number of operations. Duplicates are removed
  by coefficient equality.
- The reduced denominator is not itself a requirement; it is implied by the requirements, because each requirement is a
  multiple of the reduced denominator.
- The composition is exact for rational functions of one variable; the degree bound is the largest one of the operands,
  so degrees grow with the number of compositions.

## Verification

- `examples/rational_core.ostrin`: hand-computed values, the exclusion of `x = 1` through a sum, a product, a quotient, a
  derivative and two compositions, the zero function, and refusals with the requirement named.
- `scripts/rational-oracle.mjs`: an independent oracle with its own rational type. It computes each value from the
  un-reduced formula, with the original denominators as exclusions. It generates 66 cases, 528 points, of which 92 are
  undefined. The Ostrin program must reproduce every line exactly.
- Rust tests `rational_hand_cases_keep_excluded_points_in_both_backends` and
  `rational_cases_match_the_independent_oracle_in_both_backends` run both programs on the interpreter and the native
  backend.
- `node scripts/rational-oracle.mjs --check` fails when the generated files are out of date.

## Rational equations R1(x) = R2(x)

`rational_solve(f, g)` returns a `RationalSolution`:

| Field | Meaning |
| --- | --- |
| `status` | `identity`, `solved`, `no_solution`, or `partial` (see below) |
| `domain` | the requirements of the difference `f - g`; the admissible points are where all of them are nonzero |
| `solutions` | exact solutions, verified against the original equation; rational roots ascending, then the irrational pair `+`, `-` |
| `solution_kinds` | `rational`, `quadratic irrational`, or `complex quadratic`, one per solution |
| `multiplicities` | multiplicity of each solution as a root of the difference (1 for the irrational pair) |
| `excluded` | points that are excluded from the domain, with the requirement that vanishes there; includes removable points of the reduced form |
| `rejected` | candidates that fail the check against the original equation |
| `pending` | parts of the difference that this method does not solve |
| `complex_roots` | non-real roots (from a quadratic cofactor with negative discriminant); over the real numbers they are not solutions, and they are listed separately |
| `exactness` | every value is exact; no approximate value is produced |
| `completeness` | whether every root of the difference is classified |

Method: the difference `h = f - g` is formed with the merged domain, so reduction cannot remove a requirement. Its
numerator (reduced) is factored by the rational root finder of `std.polynomial`. Each rational root `r` is then:

- excluded, if a requirement vanishes at `r`;
- otherwise checked against the original sides: `f(r)` and `g(r)` are computed with `rational_evaluate` and compared
  exactly, so the multiplication by a denominator cannot introduce a candidate that the original equation rejects;
- a solution when the two values agree, and rejected otherwise.

A quadratic cofactor `x^2 + b x + c` (made monic) has the irrational roots `-b/2 ± sqrt(b^2 - 4c)/2`. Each root is
checked exactly in `Q(sqrt(disc))`: a requirement is excluded when it vanishes at the root, and the root is a solution when
`N_f(r) D_g(r) = N_g(r) D_f(r)` holds in that field. A zero residue in this field is an exact equality, not a numerical
residual. The rational roots of the requirements are listed as excluded points even when the reduced difference does not
show them, so that `(x + 1)/(x - 2) = 3/(x - 2)` reports `x = 2` as excluded.

Statuses: `identity` when `h` is zero (both sides agree at every admissible point; the identity is not an empty solution
set); `solved` when there is a solution and nothing is pending; `no_solution` when every root is excluded or rejected and
nothing is pending (a complete answer, such as the contradiction `x = x + 1`); `partial` when a residue of degree three or
more remains. A partial result never says there are no solutions.

Real domain: the search is over the real numbers. A quadratic cofactor with negative discriminant gives non-real roots, which are reported in `complex_roots`, not in `solutions`; an equation whose only roots are non-real has status `no_solution` over the reals, and its completeness note says so.

The `excluded` list is not exhaustive. It contains the rational roots of each requirement, and the candidates (rational or irrational) that a requirement excludes among the roots of the difference. Irrational roots of a requirement that are not roots of the difference are not enumerated: for `(x^2-2)/(x^2-2) = 1` the points `x = +-sqrt(2)` are excluded by the domain but do not appear in `excluded`. The domain (`domain`) is the complete description.

Limits: no algorithm for cubic and higher residues without rational roots (they are reported as pending, not solved); no
numerical root approximation (so the exact-verification path is the only one, and a residue that cannot be solved exactly is
pending); radicands are not reduced to square-free form (`sqrt(8)` is printed as is); multivariate equations are not
handled.

Verification: `examples/rational_equations.ostrin` (the examples of the task: `(x^2-1)/(x-1) = 3` keeps `x = 2`,
`= 2` rejects `x = 1`, the irrational pair, a repeated root, an identity, shared denominators, a zero numerator, a
contradiction, and a cubic that is partial), and `scripts/rational-equations-oracle.mjs`, which computes 60 seeded and
fixed equations from the un-reduced formulas using the rational root theorem and its own Q(sqrt d) arithmetic, and the
expected output must match exactly on the interpreter and the native backend.
