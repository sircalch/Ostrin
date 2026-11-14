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
