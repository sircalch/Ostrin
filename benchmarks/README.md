# Ostrin benchmark contract

This directory documents the reproducible interpreter-versus-native benchmark
that is run by `scripts/benchmark.mjs` and by `.github/workflows/benchmarks.yml`.
The benchmark measures wall-clock process time for the same Ostrin source in two
execution modes:

1. `ostrinc --run`, the reference interpreter;
2. `ostrinc --compile` followed by the generated native executable.

Every workload must print deterministic output. The runner checks that the
interpreter and native outputs match before recording timing samples. Schema 3
also records the exact source hash for every workload, the full Git revision
and checkout state, OS release, CPU model and logical core count, the C
compiler, compiler-managed native defaults and environment flag inputs,
warmups, per-sample timings and summary statistics (minimum, maximum, mean,
median, p95 and standard deviation). The report labels those defaults as
source-defined metadata; it does not pretend to intercept the compiler's
internal subprocess argv.
Results are machine-specific; a ratio is not a claim about another language or
hardware. Peak child memory and allocation counters are still explicitly
unmeasured and remain a future benchmark metric.

## Current workloads

| Workload | Coverage |
| --- | --- |
| `benchmark_numeric.ostrin` | scalar loop and arithmetic |
| `arrays.ostrin` | one to three dimensional arrays and reductions |
| `quantity_arrays.ostrin` | unit-aware array operations |
| `numeric_methods.ostrin` | roots, quadrature, ODEs and FFT |
| `numeric_lu.ostrin` | dense LU factorization and solve |
| `numeric_svd.ostrin` | SVD, rank, condition number and least squares |
| `numeric_complex_linear_algebra.ostrin` | dense split-complex solve and adjoint product |
| `viz_scatter_fit.ostrin` | deterministic fit, band and SVG rendering |

Run a local report after building the release compiler:

```text
cargo build --release --manifest-path compiler/Cargo.toml
node scripts/benchmark.mjs --iterations 7 --warmups 1
node scripts/benchmark-contract-check.mjs
node scripts/benchmark-page.mjs --write
```

The workflow keeps the raw JSON as an artifact. The website only publishes a
report when it is deliberately regenerated, so a reader can inspect the exact
commit and environment behind the displayed numbers.
