# Capability matrix: scientific and symbolic capabilities of Ostrin

Branch `horizon/scientific-platform-core`, verified at `ca5c93ea` (compiler sources unchanged since `547e01f6`); the suite on that state: 40 unit, 6 differential, 470 example tests, none ignored. The browser suites run on WASM rebuilt from those sources: 30 tests (A to F).

Status terms, used exactly:

- **Implemented**: the code exists in the language, the std library or the notebook.
- **Verified**: an example or test checks it against an independent reference, on the interpreter and the native build
  where the capability is supported there. The test is named.
- **Partial**: implemented for a stated family only; the limit is stated.
- **Not present**: not implemented.
- **Not tested**: implemented or claimed, but a check that would cover it could not run here (the reason is given).
- **Blocked**: a check cannot run because of an environment limit (the limit is given).

"Verified" does not mean general: each row names the family it covers. Nothing here is a claim of parity with Wolfram
or MATLAB. No results from those systems are included.

## 1. Symbolic algebra (`std.symbolic`)

| Capability | Status | Scope and limit | Verified by |
|---|---|---|---|
| Expression store (hash-consed DAG; sums, products, powers, named functions) | Implemented | Nodes are shared; a node is identical to any structurally equal node in the same store. | `symbolic_core`, `symbolic_operators` examples |
| Exact rational arithmetic, including big integers | Implemented | Integers beyond 64 bits go through `std.bigint`. | `std_symbolic_validation_…` test |
| Expansion, substitution, symbolic differentiation (+, ×, powers, sin, cos, exp, ln, abs) | Implemented | Derivative of abs is `sign(x)`; undefined at 0. Other functions: no rule (reported as an error). | `symbolic_validation`, `symbolic_abs_derivative`, `symbolic_numeric` |
| Canonical forms (same expression, same node) | Partial | Equal products and sums combine; no general normal form beyond the rules implemented. | `symbolic_operators` |
| Radical normalisation: integer radicals (`sqrt(8)` = `2*2^(1/2)`), sums and products of radicals | Verified | Positive integer to a half-integer power only. Other exponents and radicals of symbols: not present. | `symbolic_radicals` (exact text and numeric references) |
| Reduced radical form of irrational quadratic roots | Verified | Discriminant within bounds (numerator × denominator ≤ 10¹²); larger keep the old form. | `symbolic_radicals`, `symbolic_solve` |
| Polynomial roots over the rationals (degree ≤ 2 closed form; cubic and higher when rational roots exist) | Partial | Degree-2 closed form; cubic and higher only by rational factorisation. Complex roots: refused, not represented. | `symbolic_solve` |
| Rational factorisation of polynomials | Partial | Rational roots only; a factor of degree ≥ 2 without rational roots is reported as not available. | `symbolic_solve` |
| Multivariate polynomials: canonical form over exact rationals; add, subtract, multiply, evaluate, derivative, degrees, text | Verified | Lexicographic canonical order; oracle with BigInt and fixed seeds (600 property trials, 58 interchange cases). See `docs/mpolynomial.md`. | `mpolynomial_core`, `mpolynomial_oracle` (interpreter and native) |
| Conversion between `std.symbolic` expressions and multivariate polynomials (text interchange lines, version 1) | Verified | Refuses non-polynomial terms, negative exponents, repeated names; limits in `docs/mpolynomial.md`. | `mpolynomial_core`, `mpolynomial_oracle` |
| Multivariate gcd | Not present | Not claimed. | — |
| Multivariate division with remainder by an ordered list of divisors (lexicographic order) | Verified | Identity and remainder property checked; a zero remainder with several divisors is not a membership test (no Gröbner basis). | `mpolynomial_division`, `mpolynomial_division_oracle` (interpreter and native) |
| Irreducible polynomial solving over algebraic or complex numbers | Not present | — | — |
| Rational expressions: cancellation of a factor (linear denominator with a root the numerator vanishes at) | Verified | Linear denominators only; the removed point is kept as a condition. | `symbolic_conditional`, `symbolic_chain` |
| Univariate rational functions in lowest terms (exact gcd of std.polynomial), with monic denominators | Verified | Operations: add, subtract, multiply, divide, negate, derivative, composition. See `docs/rational.md`. | `rational_core`, `rational_oracle` (interpreter and native) |
| Domain requirements kept through operations, substitution, derivatives, composition and evaluation (a removed factor stays excluded) | Verified | Requirements are polynomials that must not vanish; evaluation names the vanishing one. Oracle checks 92 excluded points out of 528. | `rational_core`, `rational_oracle` |
| Multivariate rational functions | Not present | Needs a multivariate gcd, which is not implemented. | — |
| Rational equations R1(x) = R2(x): exact rational roots, quadratic irrational roots verified in Q(sqrt d), excluded points, rejected candidates, identities | Verified | Univariate. Structured result (`RationalSolution`). Oracle: 71 equations, nine of them with hand-derived references (cases 12 to 20). See `docs/rational.md`. | `rational_equations`, `rational_equations_oracle` (interpreter and native) |
| Rational equations with a residue of degree three or more and no rational root | Partial | Reported as `partial` with the residue pending; no numerical root approximation yet. | `rational_equations` |
| Partial fractions | Not present | — | — |

## 2. Assumptions and conditional rules

| Capability | Status | Scope and limit | Verified by |
|---|---|---|---|
| Explicit immutable `Context` of facts about named symbols | Implemented | One named variable per fact; facts are kinds (real, complex, positive, negative, nonnegative, nonpositive, nonzero). | `symbolic_conditional`, `symbolic_assumption_audit` |
| Implication closure (positive ⇒ nonzero, nonnegative, real; nonnegative ∧ nonpositive ⇒ zero) | Verified | Closure over one symbol only. | `symbolic_conditional` |
| Contradictions reported with the conflicting pair | Verified | Pairs of kinds only; interval emptiness for intervals. | `symbolic_conditional`, `symbolic_assumption_audit` |
| Built-in assumption of the older API combined with the context; conflicts refused | Verified | Bit-encoded positive and nonzero; the integer bit is not an ordering kind and is not used. | `symbolic_assumption_audit` |
| Three-valued answers (true, false, unknown, plus contradictory) | Verified | `holds` is name-based; `knows` is node-aware. | `symbolic_assumption_audit` |
| Open and closed intervals | Partial | Used through the sign facts they imply and their emptiness; no interval arithmetic. | `symbolic_conditional` |
| Sign of an expression from its structure | Partial | Products, sums, even powers, exp, abs; unknown otherwise. | `symbolic_conditional`, `symbolic_assumptions` |
| Inequalities between two symbols (x < y) | Not present | — | — |
| Complex-valued symbols with ordering | Not present | A complex symbol is refused any ordering fact. | — |
| Conditional sqrt of a square (abs for real, x under x ≥ 0, −x under x ≤ 0; refused for complex) | Verified | Symbol arguments only. The complex refusal is by definition of the principal branch. | `symbolic_conditional` (20 counterexamples on a 41-point grid for the unconditional form) |
| Conditional logarithm of a product (split only when both factors are positive) | Verified | Refused otherwise; for real factors the split is not an identity. | `symbolic_conditional` |
| `x / x` with the condition x ≠ 0 kept | Verified | Symbol argument only. | `symbolic_conditional`, `symbolic_chain` |
| Structured result: original, transformed, required, established, unknown, status, reason | Implemented | `Condition` values carry the expression, relation, bound and text; a program cannot build them, so only rules produce them. | `symbolic_conditional` |
| `is_identity`: a result is an identity only when it requires no condition | Verified | — | `symbolic_assumption_audit` |
| Composition of conditional steps on subexpressions, with provenance and accumulated conditions | Verified | Provenance by canonical text; the clash check is a defence that the current rules cannot trigger from a program. | `symbolic_chain`; notebook E cell c4 |
| Domain-checked evaluation of a derivation (pending conditions and context facts checked at points) | Verified | Points in the variable only; a removable point is refused, not evaluated. | `symbolic_chain`; notebook E (browser) |

## 3. Numerical bridge

| Capability | Status | Scope and limit | Verified by |
|---|---|---|---|
| `sample`: evaluation of an expression at points, failing at the first undefined point | Verified | One variable; other symbols fail. | `symbolic_numeric` |
| `disagreements`: points where two expressions differ (counterexamples to an identity) | Verified | Evidence on the points only; not a proof. | `symbolic_numeric`, `symbolic_conditional` |
| `derivative_mismatches`: symbolic derivative against central differences at two steps | Verified | Refuses when the two steps disagree (rounding or truncation). | `symbolic_numeric`, `symbolic_abs_derivative` |
| Non-finite values never count as agreement | Verified | — | `symbolic_numeric` |
| Empty point list | Partial | Refused by the library; cannot be shown from a program (no empty `Array` can be built with `zeros([0])`). | — |

## 4. Numerical methods (`std.numeric`, `std.solve`, `std.polynomial`, `std.data`)

| Capability | Status | Scope and limit | Verified by |
|---|---|---|---|
| Composite Simpson, trapezoid, adaptive Simpson | Implemented | — | `numeric_methods`; benchmark kernel `simpson` |
| Bisection, Brent, secant, Newton root finding | Implemented | — | `numeric_methods`, `numeric_adaptive_brent`; benchmark kernel `bisect` (bisection) |
| Runge–Kutta 4 and adaptive RK45 | Implemented | — | `physical` notebook; `lab_ode` example |
| Least squares, polynomial fit, QR, Cholesky, LU, eigen, SVD | Implemented | Dense `Array<Float>`; no sparse or blocked algorithms. | `numeric_polyfit`, `numeric_lu`, `numeric_cholesky`, `numeric_eigen`, `numeric_svd` |
| Dense matrix product | Implemented (user-level) | Written as a loop in the benchmark; no BLAS. | benchmark kernel `matmul` |
| CSV reading, grouping, regression on tables | Implemented | Quoted fields and multi-line quoted values supported. | `lab_data` and the data_* examples (see `examples/data_*.ostrin`) |

## 5. Backends and execution

| Backend | Status | Evidence |
|---|---|---|
| Interpreter (`ostrinc --run`) | Implemented and verified for every example in the symbolic group | Symbolic tests run the interpreter and compare the output with the native build. |
| Native (C backend, `--compile`) | Implemented and verified for the symbolic examples above, with leak check and no HIR or AST fallback | 13 symbolic tests pass; IR guard 163 for `std.symbolic` (no HIR or AST fallback; it grew from 157 with the multivariate conversions). |
| Native: a closure that captures a `mut` binding shares its storage with the enclosing function (design 02 §4) | Verified | `captured_mutable_bindings_across_scopes_agree_in_both_backends`, `loop_closure_mutation_agrees_on_native_backend`; `--leak-check` reports `live_allocations=0` for the capture fixtures. Limit: a captured function value whose type cannot be inferred is refused with an error (annotate the binding). |
| Native: shadowing (a declaration in an inner scope never changes an outer binding) | Verified | `shadowing_scopes_agree_in_every_backend_path` runs the IR, forced HIR and forced AST routes; `loop_body_shadow_agrees_on_native_backend`. |
| Native, AST fallback: closure environments release captures and closures bound in closure bodies | Verified (AST route) | `captured_lambda_env_leak_on_ast_path` (was ignored; active since the AST closure-ownership change). Not verified under AddressSanitizer or UBSan in this phase. |
| WASM (browser, WASI) | Verified for the notebook examples A to F | 30 browser tests pass against the rebuilt `website/ostrinc.wasm` (`notebook.spec.mjs` 25, `notebook-rational.spec.mjs` 5). |

The interpreter is substantially slower than native code (see the scientific report). A large program can be impractical
on the interpreter; this is a measured limit, not a design claim.

## 6. Notebook

| Capability | Status | Evidence |
|---|---|---|
| Cells, text (safe Markdown), outputs, tables, figures, persistence, export and import | Implemented | Notebook browser suite: 25 tests |
| Five example notebooks: A experimental, B physical, C symbolic, D symbolic-to-numeric, E conditional | Verified | Native tests run every code cell against references; browser suite runs A, B, C, D, E end to end (25 browser tests) |
| Notebook F: exact algebra and rational equations (polynomials, multivariate exact division with the identity checked, rational simplification with the domain kept, the four mandatory equations, verification, an exact table, a figure that never joins values across excluded points) | Verified | `notebook-examples.test.mjs` (28 tests in the file, F checks each printed value by hand); browser `notebook-rational.spec.mjs` (5 tests) against the rebuilt WASM compiler. Limit: the figure marks x = 1 without a value; limits of excluded points are not computed. |
| Opening a file does not run its code | Verified | Browser import test |
| Outputs are restored after a reload and marked as not run in the page | Verified | Browser tests for physical and conditional notebooks |

## 7. Benchmarks

| Capability | Status | Evidence |
|---|---|---|
| Reproducible suite: five kernels, four runtime modes | Implemented | `benchmarks/run.mjs`, `benchmarks/README.md` |
| Correctness checked before timing and for every timed run | Verified | `allComplete: true` in run 3 |
| Scientific report with valid comparisons and stated limits | Verified | `benchmarks/SCIENTIFIC-REPORT.md` |
| Historical scientific comparison: pinned interpreter builds (7a38ee8b, ffe90033, HEAD) on seven kernels, native, CPython and Julia cold and warm | Partial | `benchmarks/v2/results/compare-scientific-2026-11-15T06-41-52-032Z.json` (horner from `compare-scientific-2026-11-15T17-07-10-793Z.json`). Load was 28 to 68 % in the first run and 12 to 32 % in the horner rerun; interpreter conclusions use non-overlapping ranges only. Python and Julia are internal timers and are not directly comparable with the external figures. |
| Statistical stability across sessions | Partial | Only one complete run is reported; run-to-run variation is visible and not characterised. |
| Interpreter A/B across pinned builds (16 workloads: micro loops and calls, frames of 3, 40 and 120 names, scope lookup, closures, nested loops, recursion, records) | Verified (with stated limits) | `benchmarks/v2/interp/`, `benchmarks/v2/INTERP-AB-REPORT.md`. Conclusive: the empty loop 0.165x, frames_small 0.623x, frames_large 0.875x of the first build; one regression of the list-frame build (frames_large 1.83x) was removed by the hybrid frames. Not conclusive: frames_mid, closures, recursion, while loops. Processor load was not captured. |
| Comparison with optimised libraries (BLAS, NumPy) | Not present | — |
| Wolfram or MATLAB results | Not present | — |

## 8. Not present, and the largest gaps toward a general system

- General simplification of rational functions (gcd, partial fractions) and of arbitrary algebraic expressions.
- Solving polynomial equations of degree ≥ 3 in closed form, and complex roots as values.
- Assumptions on several variables, inequalities between symbols, and interval arithmetic.
- Branch-aware reasoning for complex logarithms and roots.
- Limits, series and integration in closed form.
- Multivariate gcd, and rational functions in several variables (no multivariate gcd is available to reduce them).
- Irrational roots of a requirement of degree three or more: the excluded list is not exhaustive, and the solver says so in `completeness`.
- A general theorem prover, and any membership test in an ideal for divisors that are not a Gröbner basis.
- Exact limits of an original expression at an excluded point (the figure marks the point without a value).
- Clang sanitizer runs (not installed here): NOT_TESTED. GCC 13 under WSL is the only sanitizer toolchain used.
- A native backend for untyped closure parameters that hold function values: Blocked by inference; the compiler refuses with a diagnostic.
- A native compile-time cache for `std` modules (compile cost, measured in the scientific report).
- Interpreter performance (orders of magnitude slower than native on the measured kernels).

These are gaps, not planned claims: none is implemented, and none is claimed.

## 9. Ownership, capture and the C boundary (native)

| Capability | Status | Scope and limit | Verified by |
|---|---|---|---|
| Native ownership: closures, captures, tasks, quantities, lists, arrays, Results and records, checked with AddressSanitizer, LeakSanitizer and UBSan at -O0 and -O1 (GCC 13.3, WSL) | Verified | 25 programs on three codegen routes, 150 runs: 144 VERIFIED (no sanitizer report, zero live allocations, output equal to the interpreter); 6 REJECTED_TYPE (the untyped-parameter refusal, never compiled, so not a memory result). Evidence: `docs/sanitizer-evidence.md`, rules: `docs/native-ownership.md`. | `scripts/native-sanitize.mjs`; `ownership_fixtures_agree_on_every_codegen_route_and_leave_no_allocation`; `existing_closure_fixtures_agree_on_every_codegen_route_and_leave_no_allocation` |
| Use-after-free, double free and leak detection in this toolchain | Verified (control) | A negative control with the same flags reports each defect; the fixtures reported none. | `docs/sanitizer-evidence.md` (controls) |
| Identifiers that are C keywords (`double`, `int`, `struct`) as locals, captures, record fields and functions | Verified | Lexer escape while native output is built; the map is injective. | `own_c_keyword_names` on IR, HIR and AST |
| Capture by declaration: several bindings named `x` in different scopes, each closure capturing the binding its declaration refers to | Verified | Loops, nested blocks, a shadow inside the same lambda, and a lambda's own `mut x`; checked on the interpreter and on all three native routes. | `capture_declarations` fixture; the ownership test above |
| Function values that capture other function values and mutable bindings, with parameter types from annotated signatures | Verified | Function values passed to `compose`, with the captured `mut x` read at call time. | `function_value_capture` fixture |
| Untyped closure parameter holding a function value, on the native backend | Blocked | The interpreter runs it; the native backend cannot infer the type and refuses with an explicit diagnostic. | `untyped_closure_parameter_is_refused_by_the_native_backend_with_a_diagnostic` |
| Clang as a second C compiler | Not tested | Not installed on this machine. | — |
| Native float text: the interpreter's shortest round-tripping decimal, reproduced by the native formatter | Verified | Every example agrees by text in the differential gate; a byte comparison covers 0.1, 1e150, 1e-7, 4.9e-324, 5.684341886080802e-14. | `compiler/tests/differential.rs`; `linalg_oracle_agrees_in_both_backends_and_leaves_no_allocation` |

## 10. Numerical core and linear algebra (this phase)

| Capability | Status | Scope and limit | Verified by |
|---|---|---|---|
| Matrix norms by name: 1, infinity and Frobenius; vector norms 1 and infinity (`std.linalg`) | Verified | Shape checked; the 2-norm of a vector is the built-in `norm`. | `linalg_oracle` (exact rational norms, 61 lines on every route) |
| Residual of a computed solution, infinity norm (`residual_inf`) | Verified | Square matrices only; reported as a value, not as a proof. | `linalg_oracle` (bound `<=4 n eps (||A|| ||x|| + ||b||)`) |
| Checked solve (`solve_checked`): LU with partial pivoting, singularity relative to ||A||_inf, residual, relative residual, 1-norm condition estimate, status | Verified | Singular when a pivot is at most n*eps*||A||_inf; status `ill_conditioned` when the 1-norm condition estimate is at least 1e12. Dense, double precision, square only. | `linalg_oracle` (Hilbert 3 to 10, mixed scales, random integer systems, singular and zero matrices, non-square and mismatched inputs) |
| Condition estimate | Partial | Exact for the computed LU factors (the inverse is formed column by column); not a rigorous bound on the rounding error of x. | `linalg_oracle` (cond1 tolerance 4 n^2 kappa eps kappa) |
| Unit-preserving dot product and matrix-vector product for arrays of quantities (`unit_dot`, `unit_matvec`) | Verified | Result unit is the product of the operand units; shapes are checked and reported with Result. Matrix is row-major flat data with an explicit shape. | `examples/linalg_regression.ostrin`; `native_round_regressions` (interpreter and native) |
| `@` (matrix product) on quantity arrays | Not present | The compiler refuses it: `matmul expects Array<Float>`. Use `unit_matvec`. | — |
| Linear regression through the normal equations (transpose, @, solve_checked) | Verified | Exact quadratic fit recovers 1, 2, 0.5; example `examples/linalg_regression.ostrin`. | `linalg_regression_example_recovers_the_exact_quadratic_on_both_backends` |
| N-dimensional arrays (rank above 2), strides, non-copying views | Not present | The runtime is dense and row-major; `reshape`, `transpose`, `row` and `col` copy. | — |
| Broadcasting and axis reductions beyond the existing `std.arrays` | Partial | Existing operations only (see design note 19 section 5.1). | existing examples |
| Dimension errors for `@` and `reshape` | Partial | Reported at run time with the shapes; not checked statically. | runtime message tests in `examples` |
| Benchmarks of the new operations against Python (pure and NumPy/BLAS) and Julia (pure and BLAS) | Not tested | Not run in this phase; see the report. | — |
| BLAS/LAPACK integration inside Ostrin | Not present | No external library is linked or claimed. | — |
| Contextual inference for unannotated function parameters on the native backends | Blocked | Needs inference variables and a unification layer; see `docs/higher-order-inference.md`. The native compiler refuses such programs with an explicit diagnostic. | `untyped_closure_parameter_is_refused_by_the_native_backend_with_a_diagnostic` |
| Known native leak on the HIR and AST routes: a field of a fresh record used as a method receiver (`f().unwrap().x.to_string()`) | Partial | Measured: 3 allocations on HIR and AST; zero on IR (the default). Not in the linalg examples. | probe recorded in this phase; not yet a test |
