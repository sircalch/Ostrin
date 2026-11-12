# Benchmarks

A reproducible comparison of five numerical kernels in Ostrin (native and interpreter), Python and Julia.

## What is compared

Each kernel is written with the same algorithm and the same order of floating-point operations in every language:

| Kernel | Operation | Size |
|---|---|---|
| `simpson` | composite Simpson rule for ∫ sin over [0, π] | 4,000,000 panels |
| `bisect` | bisection for the root of x³ − 2x − 5 on [2, 3] | 200,000 repetitions |
| `matmul` | dense matrix product, integer entries, trace and total | 300 × 300 |
| `regression` | quadratic least squares through the normal equations | 2,000,000 points |
| `horner` | polynomial evaluation by Horner's rule | degree 50, 2,000,000 points |

Not compared: the Ostrin symbolic engine. Python and Julia have no operation with the same semantics here, so a timing
would not be a fair comparison.

## Correctness comes first

Every program prints a `RESULT <kernel> ...` line. The driver checks it against a reference computed independently:
analytic values (the Simpson integral is 2, the regression coefficients are 1, 2, −3), a Newton iteration for the root,
and a JavaScript computation for the integer matrix product (exact) and the Horner sum. A result that fails the check is
reported and its timings are not used.

## Timing method

- **compile**: the wall time of `ostrinc --compile` (Ostrin native only).
- **init**: the median wall time of an empty program, which measures the process start of each runtime.
- **total**: the median wall time of one kernel run, including process start.
- **run ≈**: total − init. Ostrin has no clock inside a program, so the kernel time is always estimated from outside
  the process. Values below the measurement resolution (the larger MAD) are marked and not interpreted.
- **dispersion**: minimum, maximum and median absolute deviation over the repetitions.

The driver runs one warm-up and then `BENCH_REPS` repetitions (default 7); the interpreter uses `BENCH_INTERP_REPS`
(default 3) because a single interpreter run of the largest kernel takes minutes.

## Reproduce

Requirements: the Ostrin compiler built in release mode (`cargo build --release` in `compiler/`), Python 3.12 on the
`python` command, Julia 1.13 on the `julia` command, and Node 20 or later.

```bash
node benchmarks/run.mjs
node benchmarks/report.mjs benchmarks/results/benchmark-<stamp>.json
```

Environment variables: `BENCH_REPS`, `BENCH_INTERP_REPS`, `BENCH_TIMEOUT_MS`.

Each run writes `benchmarks/results/benchmark-<stamp>.json`; the report is written next to it as `REPORT-<stamp>.md`.

## Files

- `ostrin/*.ostrin`: the Ostrin kernels.
- `python/kernels.py`, `julia/kernels.jl`: the same algorithms.
- `run.mjs`: the driver (correctness, timing, JSON).
- `report.mjs`: the Markdown report from a JSON file.
- `results/`: the recorded runs.

## Limits

- Results depend on the machine, the operating system's scheduling and the runtimes' versions, all recorded in each file.
- The kernels are not tuned library calls: NumPy, BLAS and other compiled libraries are not used.
- The numbers are measurements on the recorded machine, not claims about the languages in general.
- No Wolfram or MATLAB results are included.
