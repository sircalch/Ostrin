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
The raw `sourceSha256` and runtime-input `sha256` values cover the exact bytes
in that runner's checkout and staged files. Windows checkouts can use CRLF
while Unix checkouts use LF, so the report also records `sourceContentSha256`
and runtime-input `contentSha256`, which normalize UTF-8 line endings to LF.
The cross-platform workflow compares those content identities and exact output
hashes for Linux, macOS and Windows while retaining the host-specific raw
hashes and byte counts. Binary or invalid UTF-8 runtime inputs have no normalized
text hash and are compared by their raw SHA-256.
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
| `viz_pendulum_calibration.ostrin` | file-backed dimensional calibration, correlated uncertainty and scientific SVG output |
| `temperature-float.ostrin` | scalar `Float` addition control |
| `temperature-interval.ostrin` | linear `Quantity<Temperature>` additions in K |
| `temperature-point.ostrin` | absolute Celsius point shifted by Kelvin intervals |
| `measurement-source-identity.ostrin` | repeated scalar measurement addition with one shared source ID; checks correlation-preserving coefficient accumulation |

The measurement workload performs 50,000 additions of the same source to exercise
correlation-aware coefficient merging. Compare it only with the scalar workload as
a coarse same-host control: process startup, `Result` handling, ownership and
measurement bookkeeping are included, so the ratio does not isolate the cost of
the source ID field or predict application performance. Peak memory and allocation
counts remain unmeasured.

The temperature workloads use the same 50,000-iteration loop and let the
report compare `Float`, linear interval and affine point costs on one host. The
reported wall time includes process startup; it does not isolate per-operation
latency or allocation counts, and it must not be generalized beyond the
recorded platform and compiler configuration.
The first source-backed Windows x64 run is stored in
`validation/affine-temperature-cost.json`. Its `commit` field identifies the
clean source revision used for the binaries; the later commit that checks in
the report is the evidence revision, so the report does not need a self-hash.

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
