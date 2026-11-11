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

## Known limitation of the solver

Irrational quadratic roots appear in sqrt form, for example `1/2*8^(1/2)`. That expression is exact, but it is not
simplified to `sqrt(2)`; the simplifier does not factor perfect squares out of radicals.

## Verification run for this note

- Full example suite, `cargo test --release --test examples`: 429 passed, 0 failed.
- Symbolic example tests, four of them, including the new one: passed.

## Not verified in this note

- WASI and browser backends for the new solver functions: not run.
- Firefox and WebKit: not applicable to the compiler.
- Benchmarks: not run.
