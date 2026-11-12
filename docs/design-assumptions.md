# Design: explicit assumptions for std.symbolic

Status: design for the first increment. The implemented subset is listed at the end; everything else is future work and
is not claimed.

## Problem

Today a symbol carries its assumption inside its hash-consed node (`symbol(s, "x", "positive")` is a different node from
`symbol(s, "x")`), and the simplifier reads that bit directly. Three things follow:

1. A rule cannot ask "what if x were nonnegative?" without building a second symbol.
2. A rule cannot say which condition it relied on, so its result cannot carry the condition of its validity.
3. Two facts about the same symbol (for example `x > 0` and `x < 0`) cannot be stated and checked against each other.

## Separation of the five parts

Each simplification is recorded as five separate things:

1. **Expression**: the node in the store (unchanged, hash-consed).
2. **Context**: a set of facts about symbols, supplied by the program. It is a value, not part of the store.
3. **Transformation**: the named rule applied (for example `sqrt-of-square`, `cancel-factor`, `log-of-product`).
4. **Validity conditions**: the conditions under which the rewritten expression equals the original. A condition is a
   fact about an expression (`x != 1`, `a > 0`, `b > 0`). Conditions are returned with the result; they are not dropped.
5. **Result**: the rewritten expression together with its conditions and the transformation's name.

A rule that is valid only under a condition returns that condition. A rule that cannot keep a condition in its result
form is refused (it returns an error naming the condition), instead of silently dropping it.

## The context

A context holds, for each symbol name, a set of facts. Facts are drawn from a closed vocabulary:

| Fact | Meaning |
|---|---|
| `real` | the symbol is real (the default for a symbol without facts) |
| `complex` | the symbol is complex; no ordering is assumed |
| `positive` | x > 0 |
| `negative` | x < 0 |
| `nonnegative` | x >= 0 |
| `nonpositive` | x <= 0 |
| `nonzero` | x != 0 |

Consistency: a context is contradictory when its facts for one symbol have an empty intersection. For example
`positive` with `negative`, `positive` with `nonpositive`, `negative` with `nonnegative`, or `complex` with any ordering
fact (an ordering presupposes a real value). A contradictory context is an error with the conflicting pair named; no
rule runs under it.

Interval facts (`x in [a, b]`) are not in the first increment. They are the next extension, and the representation is
chosen so that an interval is one more fact kind.

## Sign decisions

`sign_of(context, expression)` answers one of: `positive`, `negative`, `zero`, `nonnegative`, `nonpositive`, `nonzero`,
`unknown`. It is computed from the structure of the expression and the context, not from a stored bit:

- a number: its sign;
- a symbol: its facts (`real` alone gives `unknown`);
- a product: the sign rules for products, and `zero` if any factor is zero;
- a sum: `positive` (or `nonnegative`) only when every term is so; otherwise `unknown`;
- an even power of a real expression: `nonnegative`, and `positive` when the base is nonzero;
- a power of a positive base: `positive`.

Sign decisions are sound and incomplete: `unknown` means the engine could not prove the sign, never that the sign is
absent.

## Rules and their conditions

| Case | Rule | Condition in the result |
|---|---|---|
| `sqrt(x^2)` | real `x`, sign unknown: result `abs(x)`; `x` nonnegative: result `x`; `x` nonpositive: result `-x` | none for the `abs` form; the others carry the sign fact used |
| `(x^2 - 1)/(x - 1)` | the factor `x - 1` cancels: result `x + 1` | `x != 1`, because the original is undefined at 1 |
| `log(a*b)` | rewritten to `log(a) + log(b)` only when `a` and `b` are positive | `a > 0`, `b > 0` |
| `x / x` | `1` only when `x` is nonzero | `x != 0` |

A cancellation whose removed factor can vanish is always reported with its condition. When the context already excludes
the removed zero, the condition is recorded as satisfied and dropped from the list.

## Public API (planned, Ostrin programs)

- `context() -> Context`
- `assume(c: Context, name: String, fact: String) -> Result<Context, String>` (a contradiction is an error)
- `sign_of(c: Context, a: Expr) -> String`
- `simplify_sqrt_square(c: Context, v: Expr) -> Simplified`
- `simplify_cancel(c: Context, a: Expr, b: Expr, v: Expr) -> Simplified` (for a quotient `a / b` in one symbol)
- `simplify_log_product(c: Context, a: Expr) -> Simplified`

`Simplified` holds the expression, the list of conditions, the name of the transformation and the status (applied or
refused, with the reason).

## Verification plan

For every rule: exact identities at symbolic level where the store decides them; positive cases; counterexamples for
the case without the assumption (for `sqrt(x^2)` with real `x`, the value at `x = -2` must be `2`, not `-2`); the
contradictory contexts; the singular points (`x = 1` for the cancellation); and numeric references checked with the
bridge (`sample`, `disagreements`). Numeric agreement at finitely many points is evidence, not a proof, and is labelled
that way in the tests.

## Explicitly not claimed

- No general theorem prover. The rules above are the complete list of what this increment can decide.
- No complex-analytic branch reasoning. `log` and `sqrt` of complex arguments are not simplified.
- No intervals and no inequalities between two symbols (`x < y`).

## Structured result (contract)

A conditional transformation returns a record, never only an expression:

- `original`: the expression before the rule;
- `transformed`: the expression after the rule (equal to `original` when the rule was refused);
- `context`: the context used, as a value;
- `required`: the conditions the rewrite needs for validity (for example `x != 1`, `a > 0`);
- `established`: the subset of `required` that the context proves;
- `unknown`: the conditions the context neither proves nor refutes; a result with a non-empty `unknown` list is valid
  only under those conditions, and the program must say so;
- `refuted`: a required condition that the context contradicts, in which case the rule is refused;
- `rule`: the name of the transformation;
- `status`: `applied`, `applied-conditional` (some conditions unknown), or `refused` (with the reason).

Two cases must stay distinct: a condition that is **unknown** (the context does not decide it) and a condition that is
**false** (the context refutes it). The first produces `applied-conditional`; the second produces `refused`.

## Immutability

The context is a value. Assuming a fact returns a new context and leaves the old one unchanged, so one expression can be
analysed under several contexts side by side. The store is not modified by the context: symbols keep their identity, and
no property of a symbol is changed globally.
