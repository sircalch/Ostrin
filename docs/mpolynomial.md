# std.mpolynomial: multivariate exact polynomials and the interchange contract

This page documents the public API of `std.mpolynomial`, its conversion to and from `std.symbolic`, and the text format
that carries polynomials across module boundaries. The format is an explicit interoperability boundary (version 1), not
an incidental detail.

## Scope

Implemented and verified:

- Canonical representation of polynomials with exact rational coefficients.
- Addition, subtraction, negation, multiplication.
- Evaluation at rational points, exact.
- Partial derivatives, total degree, degree in one variable.
- Readable text, the interchange lines, and their parsing.
- Conversion to and from `std.symbolic` expressions (`expression_term_lines`, `expression_of_term_lines`).

Not implemented, and not claimed:

- Multivariate gcd. The univariate gcd of `std.polynomial` is not extended to several variables.
- Division with remainder by an ordered list of divisors, lexicographic order (see below). A zero remainder with several divisors is not a membership test.
- Multivariate division is not a gcd: gcd and cancellation are not part of this division.
- Factorisation of multivariate polynomials.

## Public API

Types:

- `Term { exponents: List<Int>, coefficient: String }`: one monomial with its exact rational coefficient in canonical text.
- `MPoly { variables: List<String>, terms: List<Term> }`: a polynomial over named variables.

Functions:

| Function | Result | Notes |
| --- | --- | --- |
| `mpoly_from_terms(variables, raw)` | `Result<MPoly, String>` | Canonicalises raw terms; the general entry point. |
| `mpoly_zero(variables)` | `MPoly` | The zero polynomial. |
| `mpoly_constant(variables, value)` | `Result<MPoly, String>` | A constant from a rational text. |
| `mpoly_variable(variables, index)` | `Result<MPoly, String>` | The variable at position `index`. |
| `mpoly_add`, `mpoly_subtract`, `mpoly_multiply` | `Result<MPoly, String>` | Both operands must have the same variables, in the same order. |
| `mpoly_negate` | `Result<MPoly, String>` | |
| `mpoly_evaluate(p, values)` | `Result<String, String>` | Exact rational value at a point, one value per variable. |
| `mpoly_derivative(p, index)` | `Result<MPoly, String>` | Partial derivative with respect to the variable at `index`. |
| `mpoly_total_degree(p)` | `Int` | Largest sum of exponents; `-1` for the zero polynomial. |
| `mpoly_degree_in(p, index)` | `Result<Int, String>` | Degree in one variable; `-1` for the zero polynomial; an index outside the variables is an error. |
| `mpoly_to_text(p)` | `String` | For example `2*x^2*y - 3*y + 1/2`; the zero polynomial is `0`. |
| `mpoly_parse(variables, lines)` | `Result<MPoly, String>` | Parses interchange lines (below). |
| `mpoly_divide(f, divisors)` | `Result<MDivision, String>` | Division with remainder by an ordered list (below). |
| `mpoly_division_holds(f, divisors, d)` | `Result<Bool, String>` | Checks that `d` is a valid division of `f`. |
| `mpoly_lines(p)` | `List<String>` | Interchange lines in canonical order. |

Conversions in `std.symbolic`:

| Function | Result | Notes |
| --- | --- | --- |
| `expression_term_lines(a, names)` | `Result<List<String>, String>` | Expands `a`; every term must be a rational number times powers of the named symbols with non-negative integer exponents. Anything else is refused, naming the term. |
| `expression_of_term_lines(symbols, lines)` | `Result<Expr, String>` | Builds the expression from interchange lines, one symbol per variable position, in order. |

## Interchange contract, version 1

### Line format

One line per term: `coefficient|e1,e2,...,en`.

- `coefficient` is a rational number in the form accepted by `std.bigint`: an optional `-`, digits, and optionally `/` and
  digits. It is canonicalised on input (`2/4` becomes `1/2`).
- `e1,...,en` are the exponents, separated by commas, one for each variable in the order of the `variables` list.
- A coefficient of `0` is accepted and the term is dropped.
- Lines are the only data that crosses the boundary. The types `MPoly` and `Term` cannot be named in another module's
  signature in the current compiler (qualified types in signatures are not parsed), so modules exchange these lines.

### Variable order

The variables are a list of names. Position `i` of every exponent vector refers to `variables[i]`. The caller supplies the
names with the lines, and both sides must agree on the order. The interchange does not record names; a mismatch in the
order silently changes the polynomial, so callers pass the same list to both functions. `expression_term_lines` takes the
names and `expression_of_term_lines` takes the symbols in the same order.

### Canonical monomial order

Terms are sorted by exponent vector in decreasing lexicographic order: the first variable is compared first, larger
exponents come first. There is one term for each exponent vector, and no coefficient is zero. Every value that the
module returns is canonical, so two equal polynomials have equal `mpoly_lines`. This order is the only order of the
interchange; no other monomial order is defined in version 1.

### Coefficients

Exact rationals, held as canonical text (`n` or `n/d` with `d > 1` and `gcd(n, d) = 1`, sign on the numerator). There is no
bound on the size of the integers, apart from memory.

### Exponent validation

Each exponent must be an integer with `0 ≤ e ≤ 1000`. A negative exponent, a non-integer exponent, or one above the limit
is refused.

### Refused input

The following are refused with an `Err`, never accepted silently:

- A line that does not have exactly one `|`.
- A coefficient that is not a rational, or whose denominator is zero.
- An exponent field that is not an integer.
- A number of exponents that differs from the number of variables.
- A negative exponent, or an exponent above 1000.
- In `expression_term_lines`: a term that is not a rational times powers of the named symbols, such as `sin(x)`, a
  symbol that is not named, or a non-integer or negative power. The refusal names the term.
- In `expression_of_term_lines`: an exponent field that is negative or not an integer.

### Duplicate variables

Variable names must be non-empty and distinct. A repeated name is refused with the name in the message, both in
`mpoly_from_terms` (and so in every function that builds a polynomial) and in `expression_term_lines`.

### Size limits

These limits bound the work and memory. They are not mathematical limits, and a larger problem would need a new version of
the contract.

| Limit | Value | Where it is enforced |
| --- | --- | --- |
| Variables | at most 16 | `mpoly_from_terms` |
| Exponent | at most 1000 | `mpoly_from_terms`, `mpoly_parse` |
| Terms in one call | at most 10000 | `mpoly_from_terms` |
| Length of one interchange line | at most 4096 characters | `mpoly_parse` |

The term limit and the line limit are enforced by the module. They are not exercised by the oracle at full size (the
oracle checks the refusal rule on the line-length and variable-count cases).

### Format errors

Errors are returned as the `Err` message of `Result`. The messages are written for a person; a caller should not parse
them. Their wording may change within version 1; their presence and the fact of refusal will not.

### Compatibility

- Version 1 is defined by this document and by the oracle in `tests/oracle/`. It is the only version.
- Adding a function is compatible. Changing the meaning of a line, the monomial order, the limits, or the validation is
  not, and requires a new version with new function names (for example `mpoly_parse_v2`), so that version 1 callers keep
  working.
- The record field layout of `Term` and `MPoly` is part of the public API. The module's own functions return canonical
  values. A program that builds an `MPoly` with a record literal bypasses the canonicalisation, so it must use
  `mpoly_from_terms` instead.

## Division with remainder

`mpoly_divide(f, divisors)` divides `f` by an ordered list of divisors and returns `MDivision { quotients, remainder }`.

Algorithm: while `f` has terms left, take its leading term (the largest monomial in lexicographic order). If the leading
monomial is divisible by the leading monomial of some divisor (the first such divisor in the list), subtract the
corresponding multiple of that divisor and add the monomial quotient to that divisor's quotient. Otherwise move the term to
the remainder. Each step lowers the leading monomial of the part not yet divided. Lexicographic order on monomials in a
fixed number of variables is a well-order, so the leading monomials cannot decrease forever and the division terminates.

Guarantees, checked by the oracle and by `mpoly_division_holds`:

- Identity: `f = q_0 g_0 + ... + q_k g_k + r`, exactly.
- Remainder property: no term of `r` has a leading monomial divisible by the leading monomial of any divisor.
- Determinism: the result depends only on the inputs and the order of the divisors.

Not guaranteed: with several divisors, a zero remainder is not a membership test in the ideal they generate, because
the divisors need not form a Gröbner basis. With one divisor, a zero remainder means `f = q g` exactly. The division is
not a gcd and does not cancel common factors by itself.

Refused: an empty divisor list, a zero divisor, divisors over different variables. A zero polynomial `f` gives zero
quotients and a zero remainder. The univariate case agrees with ordinary long division and with
`std.polynomial.poly_divmod` (checked in the example).

`mpoly_division_holds(f, divisors, d)` returns whether `d` satisfies the identity and the remainder property. It is a
check of a result, not a second algorithm.

## Verification

- Oracle: `scripts/mpolynomial-oracle.mjs` implements exact rationals on BigInt, its own polynomial representation,
  text formatter and refusal rules. It runs 600 property trials (commutativity, distributivity, the product rule,
  additive and multiplicative evaluation, cancellation, interchange round trip, stability of the text) with fixed seeds,
  and explicit cases with known answers. Its 58 interchange cases (49 valid, 9 refusals) are compiled into
  `examples/mpolynomial_oracle.ostrin`; the expected lines are in `tests/oracle/mpolynomial-expected.txt`.
- The Rust test `mpolynomial_oracle_cases_match_the_independent_bigint_oracle_in_both_backends` compares the output with
  that file on the interpreter and on the native backend.
- `examples/mpolynomial_core.ostrin` checks ring identities, evaluation, cancellation, merging, the product rule, and the
  round trip with `std.symbolic`, and the refusals listed above.
- `node scripts/mpolynomial-oracle.mjs --check` fails when the generated files are out of date.
