# Scientific platform core: capability matrix and macrodelivery A

This note records what Ostrin can do on `horizon/scientific-platform-core`, based on `main` at `71f70545`. Each
state is backed by a test name in `compiler/tests/examples.rs` or by a run performed for this note. Documentation
alone never sets a state.

States:

- **Verified**: implemented, and a test runs it on the interpreter and native backends (WASI and browser where the test says so).
- **Partial**: implemented for a documented subset.
- **Interpreter only**: the test exercises the interpreter; no native agreement is tested.
- **Native only**: the backend compiles it; no interpreter agreement is tested.
- **Not implemented**: no code.

## Matrix

| Family | State | Evidence | Limits |
| --- | --- | --- | --- |
| Symbolic expressions: constants, symbols, sums, products, powers, functions | Verified | `std_symbolic_core_matches_interpreter_and_native_without_leaks`, `std_symbolic_validation_agrees_with_independent_oracles_in_both_backends` | Exact rationals of any size; functions sin, cos, exp, ln |
| Simplification, substitution, expansion | Verified | `symbolic_core.ostrin`, `symbolic_validation.ostrin` | Canonical form only; no general simplifier |
| Symbolic differentiation | Verified | `symbolic_core`, 28 derivative checks against central differences | No derivative rule for tan and other functions outside sin, cos, exp, ln |
| Numeric evaluation of expressions | Verified | `std_symbolic_workflow_builds_derivatives_and_a_real_figure_in_both_backends` | Float only |
| Polynomial solving and rational factorization (macrodelivery A) | Verified (this branch) | `std_symbolic_solve_matches_interpreter_and_native_without_leaks`; `examples/symbolic_solve.ostrin` | Univariate, rational coefficients; degree 3+ only when the polynomial splits over Q; complex roots and irrational higher-degree factors refused with a reason |
| Exact polynomial arithmetic and rational roots | Verified | `std_polynomial_core_matches_node_oracle_in_both_backends` | Rational root search bounded at 10^12 |
| Dense linear algebra: Cholesky, eigen, SVD | Verified | `std_numeric_cholesky_matches_interpreter_and_native`, `std_numeric_eigen_matches_interpreter_and_native`, `std_numeric_svd_matches_interpreter_and_native` | Float64 only; no arbitrary precision |
| Polynomial fitting | Verified | `std_numeric_polyfit_matches_interpreter_and_native` | Least squares on Float64 |
| Root finding, quadrature, interpolation | Verified | `numeric_adaptive_brent.ostrin` with its test; `std.numeric` bisect, brent, secant, newton, simpson, adaptive_simpson | Tolerances per call; no interval arithmetic |
| ODE integration: RK4, RK45 | Partial | `lab_ode.ostrin`, `viz_ode.ostrin` run through the examples suite | Fixed-step and adaptive Float64; convergence order checked by examples, not by a general proof |
| Physical units and measurements | Verified | `dimensional_measurement_lab_matches_interpreter_and_native`, `dimensional_measurement_full_surface_matches_native_and_wasi`, `symbolic_units.ostrin` | Dimension checks on expressions; coherent units only |
| Statistics on tables and samples | Partial | `lab_statistics.ostrin`, `statistics.ostrin`, `temperature_point_statistics_use_affine_means_with_backend_parity` | Plain lists; no missing-value API |
| Tabular data API with typed columns and missing values | Partial | `examples/data_project/tables/table.ostrin`, used by the `data_project` example | A single example package; no missing-value API and no typed-column validation yet; required before macrodelivery D |
| Visualization: curves, scatter, figures with labels and SVG | Verified | `std_viz_animates_computed_motion_with_smil`, `std_viz_linear_system_matches_both_backends`, `std_viz_marks_linked_plot_points_and_table_rows` | Histograms, surfaces and 3D are not in the matrix as verified |
| Notebook with executable cells, outputs and persistence | Not implemented | No notebook module or example in the repository | The Playground and Workbench cover single programs only |
| Language server and debugger (LSP, DAP) | Verified | `compiler_lsp_negotiates_and_publishes_diagnostics`, `compiler_dap_hits_breakpoints_and_reports_locals` | Editor features as tested |
| Modules and packages | Verified | `native_backend_compiles_project_manifest_and_path_dependency`, `ostrin.lock` checks | Local path dependencies |
| Benchmarks against Python, Julia, MATLAB, Wolfram | Not implemented | `benchmarks/` contains Ostrin-only harnesses | No external comparison has been made; none is claimed |

## What this branch adds

- `std.symbolic`: `coefficients`, `solve_polynomial`, `factor_rational`, `roots_text`, `is_root`.
- `examples/symbolic_solve.ostrin`: expected values fixed analytically before the run; identities checked by the
  store's canonical forms; irrational roots checked numerically against sqrt(2).
- `compiler/tests/examples.rs`: the new test, plus an IR count guard moved from 94 to 110 with a comment. The guarded
  property (no fallback to HIR or AST lowering) is unchanged.

## Irrational quadratic roots

Irrational quadratic roots are written in reduced radical form: the square part of the radicand is taken out, so the
roots of `x^2 - 8` are `2*2^(1/2)` and `-2*2^(1/2)`, and `2x^2 - 3x - 1` gives `3/4 + 1/4*17^(1/2)`. Discriminants
whose numerator and denominator product exceeds 10^12 keep the earlier unreduced form `(-b + D^(1/2)) * 1/(2a)`.
A positive integer to a half-integer power is reduced too: `sqrt(8)` is `2*2^(1/2)`, `sqrt(4)` is `2`, `8^(3/2)` is
`16*2^(1/2)`, and sums and products of such radicals combine (`sqrt(8) + sqrt(2)` is `3*2^(1/2)`). Complex roots are
still refused. Not yet implemented: radicals of other exponents or of symbols.

## Numeric bridge

`std.symbolic` can be checked against numbers that do not come from its own code:

- `sample(a, v, xs)` evaluates `a` at each point of `xs`, with `v` as the variable. A point that cannot be evaluated,
  such as `1/x` at 0, fails the whole sample and names the point; a free symbol with no value fails it too.
- `disagreements(a, b, v, xs, tol)` lists the points where two expressions differ by more than `tol`, relative to the
  larger magnitude. A non-empty list is a counterexample to the identity `a = b`; an empty one is evidence for it, not
  a proof.
- `derivative_mismatches(a, v, xs, delta, tol)` compares the symbolic derivative with central differences of `a`.
  It fails when `a` has no derivative rule, so a missing rule is reported rather than skipped.

`examples/symbolic_numeric.ostrin` checks derivatives of a cubic, `sin(x) exp(x)` and `cos(x^2)`, a deliberately wrong
derivative (which must disagree at every point), a true and a false identity, the error for a point that cannot be
evaluated, and a `std.viz` figure built from sampled values. Not yet implemented: adaptive sampling, root isolation on
an interval, and sampling of more than one variable.

## Assumptions and positivity

A symbol is real unless it is declared `positive`, `nonzero` or `integer`. A simplification fires only when the
assumptions prove it. With a positive `x`, `(x^2)^(1/2)` is `x`, `(4x^2)^(1/2)` is `2*x`, and with positive `x` and `y`,
`(x*y)^(1/2)` is `x^(1/2)*y^(1/2)`. With real symbols the same expressions stay as written, because `(x^2)^(1/2)` is
`|x|` and the split is not valid for negative factors. `symbolic.is_positive` and `symbolic.is_nonzero` expose the
same proofs, so a caller can check an assumption before relying on a rewrite. `examples/symbolic_assumptions.ostrin`
checks each case by exact text, the real-symbol negatives and numeric values at chosen points. Not yet implemented: a
nonnegative assumption, and sign reasoning for sums.

## Verification run for this note

- Full example suite, `cargo test --release --test examples`: 429 passed, 0 failed.
- Symbolic example tests, four of them, including the new one: passed.

## Not verified in this note

- WASI and browser backends for the new solver functions: not run.
- Firefox and WebKit: not applicable to the compiler.
- Benchmarks: not run.

## State after blocks 1 to 3 of this stage

Verified in this branch, each with a test that runs the Ostrin program (interpreter and native where stated):

| Block | Deliverable | Where | Verification |
| --- | --- | --- | --- |
| 1. Scientific data engine | `std.data`: explicit schema (int, float, text), explicit missing values (empty or NA), validating CSV/TSV reader with line-numbered errors, row operations (complete_rows, select_columns, filter_between, sort_by, group_summary), statistics (describe, covariance, correlation), polynomial regression through std.numeric.polyfit (QR), predictions, residuals, SSE, RMSE, R² (defined only when the response varies), coefficients in physical units, CSV/TSV and JSON export | `compiler/std/data.ostrin`, `examples/data_engine.ostrin`, `examples/data/calibration.csv` | `std_data_engine_reads_validates_fits_and_exports_identically_in_both_backends` (interpreter = native); `scripts/data-engine.test.mjs` compares every printed statistic and fit with an exact BigInt-rational reference, tolerance 1e-12 |
| 2. Figures from computed data | Figures with std.viz from the computed fits: measurements with linear and quadratic curves, residuals, and the table behind them | `examples/data_figures.ostrin`, `examples/data/figures/` | `std_data_figures_agree_in_both_backends`; `scripts/data-figures.test.mjs` checks the table row by row against the reference, and counts one marker per measurement |
| 3. Dimensional model | First-order decay (k in 1/s, m0 in kg) and a damped oscillator (w in 1/s, x0 in m); RK4 and RK45 from std.numeric; analytic validation; a starved step budget reported as a convergence failure | `examples/scientific_model.ostrin`, `examples/data/model/` | `std_scientific_model_agrees_in_both_backends_and_writes_its_tables`; `scripts/scientific-model.test.mjs` recomputes every row analytically in JavaScript; `a_dimension_mismatch_is_refused_at_compile_time` |

What these blocks do not establish:

- Visual inspection: the four figures were rendered to PNG with the Playwright Chromium and read back: titles, subtitles, axis labels with units, legends, curves and residual values agree with the tables. Pixel snapshots are not stored as tests yet; the structural and numeric checks are the automated guard.
- The log scales, histograms, heat maps and error bars of std.viz are not used by these figures and are not verified here.
- Regression is polynomial only. Multiple regression and weighted fits are not implemented.
- Missing-value handling is explicit but minimal: no imputation, and a numeric computation refuses a column with missing cells.
- The CSV reader supports RFC 4180 quoted fields that span lines (fixed in fd29f169). Malformed quotes are refused with a line number.

### Native backend defect (fixed in 2673a8c8)

A `match` statement on a `Result<Void, String>` whose `Ok` binding is unused fails in the C code generator, while the same match with `Ok(_)` compiles.

Minimal reproducer (fails to compile natively with `undeclared __ir_v…`; the interpreter runs it):

```ostrin
fn main() -> Void {
    match write_file("out.txt", "hello") {
        Ok(done) => print("wrote"),
        Err(message) => print("cannot write: " + message),
    }
}
```

Both the named binding and the constructor `Ok(announce())` with a Void payload are fixed in the C generator (`compiler/src/ir_c.rs`). The regression is `examples/native_void_binding.ostrin`, checked for interpreter and native agreement and for leaks. The `Ok(_)` workaround used in earlier examples is no longer needed.

### Not yet started

- **Notebook (block 5)**: no notebook module, format, or interface exists in the repository.
- **CAS expansion (block 4)**: this branch added polynomial solving and rational factorization over Q. Exact radical simplification, complex results and simplification under explicit assumptions are not done.
- **Benchmarks (block 7)**: Python 3.12 and Julia 1.13 are installed on this machine; Wolfram and MATLAB are not. No benchmark has been run, and no comparison is claimed.
- **Modelling engine breadth (block 5 of the directive)**: the ODE example uses fixed grids. Adaptive error control, stiff solvers and parameter estimation are not implemented.

## Notebook D: symbolic to numeric

`website/notebooks/numeric.ostrin.notebook.json` (example D, "Symbolic to numeric") runs the bridge in the page with the
browser's compiler. Its cells: the parameters (c1, four `Int` coefficients of a cubic); the transformation and the
exact derivative (c2); a table of f, f' and a central difference at nine points (c3); the checks, with a refused tiny
step and a counterexample search over a true identity (c4); and a figure of f and f' (c5). Cells c2 to c5 import c1, so
a changed coefficient makes them stale, and the next run shows the new values. The end-to-end Playwright test runs the
whole flow, changes a coefficient, checks the new outputs, and checks that the saved outputs are restored after a
reload and marked as not run in this page. The cell sources are generated by `scripts/notebook-examples.mjs`, and
`scripts/notebook-examples.test.mjs` checks each cell's outputs natively against hand-computed references.

## Explicit assumptions and conditional rules (first increment)

The assumption system lives in `std.symbolic`, next to the expression store, and does not change it. A `Context` is a
value: `assume` returns a new context and leaves the old one unchanged, so one expression can be analysed under several
contexts. The rules return an `Outcome` that carries the original and transformed expressions, the required conditions,
the conditions the context establishes, the conditions it leaves unknown, the rule's name, the status and the reason.

Implemented, for symbols in one named variable:

- Kinds: real (the default), complex, positive, negative, nonnegative, nonpositive, nonzero; zero is derived.
- Closure: positive implies nonzero, nonnegative and real; negative implies nonzero, nonpositive and real; nonnegative
  with nonpositive implies zero.
- Contradictions are errors that name the pair (positive with negative, complex with an ordering, and similar).
- Intervals (open or closed ends): used through the sign facts they imply and their emptiness; no interval arithmetic.
- `holds` answers true, false or unknown; unknown is never reported as false. `sign_of` decides a sign from the structure
  of the expression (products, sums, even powers, exp, abs) and answers unknown when it cannot prove one.
- sqrt(u^2) for a symbol u: abs(u) for real u (no condition); u when u >= 0 is shown; -u when u <= 0 is shown; refused
  for complex u and for a non-symbol argument.
- (x^2 - 1)/(x - 1) and other quotients whose denominator is linear with a root r that the numerator vanishes at: the
  factor cancels exactly, and the condition x != r is kept (unknown unless the context proves it).
- x / x: 1, with the condition x != 0 kept (applied when the context proves it, applied-conditional when it does not,
  refused when the context says x = 0).
- ln(a b) = ln a + ln b only when both a and b are shown positive; otherwise refused, with the conditions listed.
- `abs` is a function of the store: it evaluates in the bridge's evaluator. It has no derivative rule.

Verified by: `examples/symbolic_conditional.ostrin` (exact statuses and results, contradictions, intervals, numeric
checks with the bridge, including the 20 counterexamples that the nonnegative identity has on a grid of 41 points), its
interpreter/native test with a leak check, and notebook E (`conditional`), whose cells the native tests run and the
Playwright flow runs in the WASM compiler.

Not claimed: a general theorem prover; branch analysis of complex logarithms and roots; inequalities between two
symbols; intervals beyond the sign facts they imply; rules for symbols in more than one variable; simplification of a
general rational function beyond a linear denominator. Numeric agreement on a finite grid is evidence and is labelled so.

## Composition of conditional steps

A `Chain` is a derivation: the expression, the context it was built under, the steps applied, and the conditions they
carry. `chain_start(a, c)` begins one; `chain_then(ch, c, input, o)` applies an outcome `o` to `input`, a subexpression of
the chain's current value. The subexpression is matched by node identity, and the outcome's `original` must be the
canonical text of that same input (the rules set `original` from their own node), so a step computed on one expression
cannot be applied to another. The value becomes the value with that subexpression replaced, and no condition is
dropped: conditions are kept by their text, a condition the context proves is listed as established, and the others as
unknown. A step computed under another context, a refused step, and conditions that clash on one expression each refuse
the chain, which then keeps its last valid value and states the reason.

`chain_evaluate(ch, names, values)` gives the value of the chain at points only where its conditions hold: a condition
that is not established is checked at the point, and a point outside the context's facts is refused. A removable point
is therefore an error, not a number: `(x^2-1)/(x-1) + sqrt(x^2)` evaluates to 5 at x = 2 and is refused at x = 1, where the
original is undefined.

Limits: the conditional rules are the four of this document; `Condition` and `Outcome` are built only by rules, so a
program cannot write a condition by hand, and the clash check is a defence that the current rules cannot trigger. The
provenance check compares canonical texts, which is exact for distinct nodes of one store and documented as such.
Verified by `examples/symbolic_chain.ostrin` (both steps, numeric agreement with the original at points, the removable
point, the context boundary, wrong-context and absent-input refusals, and a pending removable singularity from x / x)
and its native test with a leak check.

Notebook E (`conditional`) now has four cells: the rule table under five contexts (c1), the numeric checks with the bridge (c2), the figure of sqrt(x^2) under two contexts (c3), and the derivation of (x^2-1)/(x-1) + sqrt(x^2) step by step, compared with the original at points the reader can edit (c4). Its browser test edits a point, reruns, exports the document and checks the saved state after a reload.

## Block 1: multivariate polynomial engine

`std.mpolynomial` adds exact multivariate polynomials: canonical terms in decreasing lexicographic order, rational
coefficients, addition, subtraction, multiplication, evaluation, partial derivatives, and degrees. The univariate
`std.polynomial` is reused unchanged. The conversion to `std.symbolic` expressions goes through text lines
(`coefficient|exponents`), which is an explicit interoperability boundary of version 1; its contract (variable order,
monomial order, validation, refusals, limits, compatibility) is in [`docs/mpolynomial.md`](mpolynomial.md).

Hardening done in this block, found while writing the oracle: the module accepted negative exponents and repeated variable
names, and `mpoly_degree_in` did not check its index. Those are now refused in `mpoly_from_terms` (and so in every
operation), `mpoly_parse`, `mpoly_degree_in` (now a `Result`), and `expression_term_lines`, and `expression_of_term_lines`
refuses negative exponents. Size limits: 16 variables, exponent at most 1000, 10000 terms per call, 4096 characters per
line.

Verification:

- `scripts/mpolynomial-oracle.mjs` is an independent oracle (exact rationals on BigInt, its own polynomial code and
  text formatter, its own refusal rules). It runs 600 property trials with fixed seeds over commutativity,
  distributivity, the product rule, additive and multiplicative evaluation, cancellation and the interchange round trip,
  plus explicit cases with known answers. Its 58 interchange cases are compiled to `examples/mpolynomial_oracle.ostrin`
  and its output is compared exactly on the interpreter and native backend (`mpolynomial_oracle_cases_match_…` test).
- `examples/mpolynomial_core.ostrin` checks ring identities, the product rule, evaluation as a homomorphism,
  cancellation, merging and fractions, and the round trip with `std.symbolic` (derivative compared with
  `symbolic.diff`), and the refusals.

Not implemented, and not claimed: multivariate gcd, multivariate factorisation.
Division with remainder by an ordered list of divisors (lexicographic order) is implemented and verified; the univariate gcd and division of `std.polynomial` remain the only gcd.

## Block 2: division and univariate rational functions

Division with remainder by an ordered list of divisors is in `std.mpolynomial` (`mpoly_divide`): identity and remainder
property checked, lexicographic order, verified against an independent BigInt oracle (see the Block 1 section). A zero
remainder with several divisors is not a membership test, and the function does not claim one.

`std.rational` adds univariate rational functions in lowest terms, reduced with the exact gcd of `std.polynomial`. The
domain is kept as a list of requirements (polynomials that must not vanish), which reduction never removes, so the removed
point `x = 1` of `(x^2 - 1)/(x - 1)` stays excluded through sums, products, quotients, derivatives, compositions and
evaluation. Details and limits: [`docs/rational.md`](rational.md).

Verification: `examples/rational_core.ostrin` (hand-computed values and exclusions, refusals) and
`examples/rational_oracle.ostrin`, which reproduces 528 points of 66 generated cases (92 undefined) computed from the
un-reduced formulas by `scripts/rational-oracle.mjs`. Both run on the interpreter and the native backend.

Not implemented: multivariate rational functions (they need a multivariate gcd), partial fractions, equation solving on
rational functions (Block 3), the advanced notebook (Block 4) and the performance and benchmark blocks.

## Block 3: rational equations

`std.rational.rational_solve(f, g)` solves R1(x) = R2(x) on the admissible domain of both sides. The difference keeps
the merged domain; rational roots are checked against the original sides; quadratic irrational roots are checked exactly
in Q(sqrt(disc)); excluded points, rejected candidates, identities, contradictions and pending residues are reported
distinctly. Details and limits: [`docs/rational.md`](rational.md).

Verification: `examples/rational_equations.ostrin` and `examples/rational_equations_oracle.ostrin` (60 equations, seeded
and fixed, expected output from `scripts/rational-equations-oracle.mjs`), on the interpreter and the native backend.
Two defects were found and fixed while writing the oracle: identities did not list their excluded points, and the
quadratic root text depended on the scale of the residue (now monic).

Not implemented: cubic and higher residues without rational roots (pending), numerical approximation, and multivariate
equations.

## Scientific Core V4: linear algebra, ownership and benchmark

This section records the V4 phase on `docs/design-34-polished-figures` (commits `0f38af69`, `16249f49`, `d6a69f60`
and the documentation commit that adds this section). The states follow the terms in `docs/CAPABILITY-MATRIX.md`.

### Ownership: the residual leak (VERIFIED)

`solve_checked(...).unwrap().x.to_string()` read a field of a fresh record through a temporary that was not released on
every route. The fix is general: a field or element read from a fresh object is taken as an owned value on the IR, HIR
and AST routes, and the receiver is released after use. The regression set is 17 programs
(`compiler/tests/fixtures/ownership/residual/r1..r17`): temporary fields, field then method, chained fields, `Result`
unwrap, stored values, multiple use, a field outliving its temporary, fresh array receivers, fields into lists, and
`Option` fields. Each agrees on the interpreter and on all three native routes with zero live allocations
(`ownership_fixtures_agree_on_every_codegen_route_and_leave_no_allocation`).

Other fixes in the same phase: `ostrin_fmt_double` now prints the shortest round-tripping decimal on the native route
(the differential gate caught a one-digit difference); C keyword identifiers are escaped; quantity runtime helpers are
linked when quantity arrays are used.

### std.linalg (VERIFIED for the stated family)

`std.linalg` provides `vector_norm1`, `vector_norm_inf`, `matrix_norm1`, `matrix_norm_inf`, `matrix_norm_frobenius`,
`residual_inf`, `solve_checked` (returns the `CheckedSolution` record with the solution, residual, relative residual,
1-norm condition estimate and a status), `unit_dot` and `unit_matvec` for quantity arrays. `solve_checked` equilibrates
rows and columns by powers of two, refuses non-finite input, and reports singularity relative to ||A||_inf.

Evidence:

- Exact-rational oracle: `scripts/linalg-oracle.mjs --check`, 61 lines in `tests/oracle/linalg-expected.txt`
  (`linalg_oracle_agrees_in_both_backends_and_leaves_no_allocation`).
- Property program: `scripts/linalg-properties.mjs --check`, 44 checks with tolerances from rounding bounds
  (`linalg_properties_agree_in_both_backends_and_leave_no_allocation`).
- Regression example: an exact quadratic fit recovers 1, 2, 0.5
  (`linalg_regression_example_recovers_the_exact_quadratic_on_both_backends`).
- WASM: `scripts/linalg-wasm-check.mjs` runs the real compiler module under `node:wasi`; 6 of 6 checks pass
  (`docs/linalg-wasm-evidence.json`). This is WASI execution, not a browser run.

Limits: dense `Float64`, square matrices for solve and residual, no BLAS, no N-dimensional arrays, no strides or views.
The condition estimate is exact for the computed factors and is not a rigorous bound on the error of x.

One gap found by the native type report: a `match` arm that calls `.to_string()` on a quantity array takes the AST
route. The example was rewritten to `print(f as N)`; the construct itself remains open (PARTIAL).

### Benchmark (VERIFIED with stated limits)

`benchmarks/v2/linalg/compare-linalg.mjs` runs matmul, LU solve and norms at n = 100 and 200 on Ostrin native and
interpreter, CPython (pure and NumPy), and Julia (pure and OpenBLAS). `solve_checked` is measured for Ostrin only.
Every sample is checked against `benchmarks/v2/linalg/reference.mjs`; no cell is incomplete. The full method, numbers,
versions, hardware and limitations are in `benchmarks/v2/linalg/REPORT.md`.

Main finding: Ostrin native is faster than CPython pure in every cell, and it is about 10x slower than OpenBLAS on
matmul at n = 200 (4.78 ms against 0.48 ms, single-threaded). Julia pure is comparable to Ostrin native. This is a
measured comparison of single kernels on one machine, not a general claim about speed. The interpreter `lu_solve`
n = 200 cell has an inconsistent start-up estimate and is reported with that caveat.

### Not verified in this phase

- Browser (Playwright) run of the linalg examples: not run here.
- MATLAB and Wolfram: not available; no results are included.
- Benchmarks of residual and fit kernels as separate operations: not measured.

## V4 validation closure

Branch: `horizon/scientific-platform-core`. The earlier sections of this note describe the state before this closure;
the matrix in `docs/CAPABILITY-MATRIX.md` is the authority for each state.

### Defects found and fixed in this closure (ownership)

- The `solve_checked(...).unwrap().x.to_string()` chain: 3 live allocations on HIR and AST before the V4 fixes, 0 after
  (reproduced with the compiler at `7ebad3de^` and the current compiler).
- A fresh `Result` used as a receiver of `is_ok`/`is_err` was not released on HIR; the AST route released it.
- A fresh `Option` used as a receiver of `is_some`/`is_none` was not released on HIR and AST.
- `ok()` on a fresh `Result` did not retain its payload nor release the wrapper (HIR and AST).
- `unwrap_or` on AST retained an owned fallback, which then leaked.
- An early `return` inside a function that tests a fresh `Result` (`r19`) left one allocation on HIR.

Regression programs: `compiler/tests/fixtures/ownership/residual/r1..r21`, including closures that capture a field of a
temporary (`r18`), early returns (`r19`), `break` inside a loop with temporaries (`r20`), and the fresh `Result`/`Option`
method set (`r21`). All run on the interpreter and the three native routes with zero live allocations
(`ownership_fixtures_agree_on_every_codegen_route_and_leave_no_allocation`).

### Sanitizers

Full ownership set (33 programs, three routes, -O0 and -O1): 192 VERIFIED, 0 FAILED, 6 REJECTED_TYPE
(`docs/sanitizer-evidence-v4-ownership.md`). Before the fixes the same set had 2 FAILED
(`docs/sanitizer-evidence-v4-ownership-before-fix.md`). Default configuration: `docs/sanitizer-evidence-v4-default.md`.

### Rational-equation oracle

The 71 cases are in `tests/oracle/rational-equations-cases.json` (71 entries) and `tests/oracle/rational-equations-expected.txt`
(71 lines). The nine hand-derived cases (12 to 20) match the hand results in `docs/rational.md` one by one. The oracle
script uses BigInt arithmetic. The test assertion was 62 cases until commit `0d837243`, when it was updated to 71 after
`d2035687` added the nine cases.

### `linalg_units`

The example keeps its three operations: `unit_dot` (10 J), `unit_matvec` ([2, 2] N) and the length mismatch error. The
interpreter and the native build print the same values; the only difference is the line ending (CRLF in native output
on Windows). The label `force:` was removed to avoid an AST-route construct, so the example's output is less
descriptive than before. The construct that falls back (`to_string()` on a quantity array inside a `match` arm) is
still open and is recorded as PARTIAL in the matrix.

### Experiment bundles

`scripts/experiment-bundle-determinism.mjs`: 30 fixtures, each rendered six times (3 repetitions, two independent
directories, working directory, time zone and locale varied) gives byte-identical output, equal to the committed files.
Enabling the CI host capture changes five JSON paths, which confirms the check can see environment-dependent fields.
The local bundle is byte-reproducible; the CI-captured bundle is not, by design.

### Benchmark audit

`benchmarks/v2/linalg/audit-interp-startup.mjs` (12 repetitions, random order, all runs verified): the direct fixed cost
is 113 ms; the first repetition of a process is about 10 % faster than a steady one, so the start-up intercept is
INCONCLUSIVE. The steady repetition is about 2.24 s. The per-run figures from the earlier campaigns are reported with
their range, not as a single value.

### Not done in this closure

- The browser run of the linalg examples (only WASI execution was checked).
- A fix for `to_string()` on quantity arrays in the AST route.
- MATLAB and Wolfram: not available.

## V4 consistency review

Branch `horizon/scientific-platform-core`. This review changes evidence and tooling only, plus one presentation change in
`examples/linalg_units.ostrin`; no new language features.

### native-sanitize exit status

Before this review, `scripts/native-sanitize.mjs` returned 1 whenever a row was not `VERIFIED`, and the 6 intentional
`REJECTED_TYPE` rows of `untyped_closure_parameter` were counted in that set. The exit status therefore said nothing about
whether the rejections were the expected ones. Now:

- Each row gets a category (`scripts/native-sanitize-classify.mjs`): VERIFIED, EXPECTED_TYPE_REJECTION,
  UNEXPECTED_TYPE_REJECTION, COMPILER_REFUSAL, BUILD_FAILURE, EXECUTION_FAILURE, MEMORY_FAILURE, BLOCKED or
  MISSING_EXPECTED_REJECTION.
- An expected rejection is a program, a route, an optimisation level and a message, listed in
  `scripts/native-sanitize-expected-rejections.json`. A rejection that does not match the list is unexpected.
- Exit status 0: every row is VERIFIED or EXPECTED_TYPE_REJECTION, and every expected rejection of a program in the run
  was observed. Exit status 1: any other category, or an expected rejection that was not observed (for example, the
  program now compiles).
- Tests: `scripts/native-sanitize-classify.test.mjs`, 18 tests. One of them checks that the expected message is still the
  one the compiler prints.

Default configuration (`node scripts/native-sanitize.mjs --report docs/sanitizer-evidence-v4-default.md`):
72 rows, 66 VERIFIED, 6 EXPECTED_TYPE_REJECTION, 0 of every other category, 0 missing; exit status 0. The default set
is the top-level fixtures only; the `residual/` directory is run by passing its files.

### std.linalg in a real browser

`tests/browser/linalg.spec.mjs` loads the page `notebook.html`, imports `ostrin-runtime.js` in the page, and calls
`runOstrinc` with the real `ostrinc.wasm`. No mock replaces the compiler or the runtime. Eleven tests pass in chromium:
the oracle (61 lines) and the properties (44 lines) against their references; the quadratic fit; `unit_dot` and
`unit_matvec` with their units and their length errors; LU solve within 1e-14; `solve_checked` with status `ok` and with
`ill_conditioned` (condition above 1e12); the refusals of singular, non-finite, non-square and mismatched inputs; a
compile-time unit mismatch (OSTRIN-E1024); and a runtime shape error in a matrix product. In each error case the program
prints no partial result.

Limit: the browser route is the compiler's interpreter (`--run`). Native code is not generated in the browser, so the
native backend is not verified in the browser.

### Reproducibility of bundles: what is guaranteed

- A. Capture off (default, local): every render is byte-identical across 3 repetitions, 2 directories and 3 environments
  (working directory, `TZ`, `LANG`/`LC_ALL`), for 30 fixtures (180 renders), and equal to the committed bundle.
- B. Capture on (`OSTRIN_CAPTURE_HOST=1`, as set by the CI workflow): `figure.svg`, `data.json` and `source.ostrin` stay
  identical. Five JSON paths change, all in provenance: `provenance.runtime.host` is added (runner identity, hardware,
  Node version, GitHub run identifiers), `provenance.runtime.missing.0` is removed, and `provenance.json` with its manifest
  entry (bytes and SHA-256) changes. The host metadata is provenance, so it is kept in the bundle, not excluded.
- The scientific content is reproducible in both modes. Byte-for-byte reproducibility holds only in mode A.

### linalg_units presentation

The label `force: [2, 2] N` on one line needs `to_string()` on a quantity array, which takes the AST route. The example
now prints `force:` and the array on two lines; this takes the IR route and keeps the label. The change is in layout only.
The values, the units and the error case are unchanged. The WASI checker and the browser test check the new layout exactly.
The AST route for `to_string()` on quantity arrays remains PARTIAL.

### Benchmark

The interpreter timing is broken into categories in `benchmarks/v2/linalg/REPORT.md`. The start-up intercept remains
INCONCLUSIVE, because the first repetition of a process is measurably cheaper than steady repetitions. The direct
process-start figure is 113 ms. The steady per-repetition figure is PARTIAL: 2193 and 2282 ms in the audit, and 2569,
2484 and 2165 ms in earlier estimates. None of these is averaged into a stable figure.
