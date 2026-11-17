# Linear-algebra benchmark: Ostrin, CPython and Julia

Source: `benchmarks/v2/linalg/compare-linalg.mjs`, run on commit `0f38af69` (Ostrin compiler `ostrinc 0.2.0`, sha256 `3fe2cf15…9de56`).
Raw data: `benchmarks/v2/linalg/results/linalg-2026-11-16T19-20-15-977Z.json`.

## What was measured

| Kernel | Operation | Size |
|---|---|---|
| `matmul` | dense product C = A·B, checksum = sum of all entries | n = 100, 200 |
| `lu_solve` | LU with partial pivoting and solve D x = b, checksum = sum of x (LU is inside this kernel, not timed alone) | n = 100, 200 |
| `norms` | 1-norm + ∞-norm, and Frobenius norm | n = 100, 200 |
| `solve_checked` | `std.linalg.solve_checked` (equilibration, LU, residual, condition estimate); **Ostrin only** | n = 100, 200 |

Not measured: residuals as a separate kernel (inside `solve_checked`), linear and quadratic fits, and any operation beyond these four.

Implementations:
- **Ostrin native**: `ostrinc --compile`, compiled once per kernel outside every timed run.
- **Ostrin interpreter**: `ostrinc --run` (same commit).
- **CPython 3.12 pure**: plain lists and loops, no library.
- **CPython 3.12 + NumPy 2.5.2**: `@`, `np.linalg.solve`, `np.linalg.norm`.
- **Julia 1.13.3 pure**: loops over `Matrix{Float64}`.
- **Julia 1.13.3 + OpenBLAS/LAPACK**: `*`, `\`, `opnorm`, `norm`, with `BLAS.set_num_threads(1)`.

## Methods

- **Ostrin native**: external slope per sample, (T(K=40) − T(K=1)) / 39, each measured in a separate process with `process.hrtime`. This removes the constant process start-up, which is estimated as T(1) − slope. 5 samples per cell.
- **Ostrin interpreter**: the same external slope with K=3, 3 samples per cell (see the limitation on repetitions below).
- **CPython and Julia**: internal timers. Each process runs the kernel 5 times and times each repetition (`time.perf_counter`, `time_ns`). Repetition 1 is the cold figure; repetitions 2–5 are the warm figure. Each cell pools 5 processes × 4 warm repetitions = 20 warm samples; the table gives their median and IQR. The JSON `samples` field holds the exact list.
- **Order**: each repetition runs the implementations in a rotated order.
- **Inputs** are generated inside the process before timing and are never timed.
- **Verification**: every output line was checked against `benchmarks/v2/linalg/reference.mjs` (exact integers for matmul and the 1- and ∞-norms; for the solve and the Frobenius norm, the tolerances documented in that file). No cell was marked INCOMPLETE.
- **Threads**: `OMP_NUM_THREADS`, `OPENBLAS_NUM_THREADS`, `MKL_NUM_THREADS` = 1 in every child process.

Hardware: AMD Ryzen 7 5700X (8 physical / 16 logical cores), 31.9 GiB RAM, Windows 11 Pro 10.0.26200. The machine was not idle during the run; per-cell processor load is recorded in the JSON as a condition, not a controlled quantity.

## Results (medians, milliseconds)

Ostrin: external slope, median of samples. Julia/CPython: warm median (IQR in brackets).

### n = 100

| Kernel | Ostrin native | Ostrin interp | CPython pure | CPython + NumPy | Julia pure | Julia + OpenBLAS |
|---|---|---|---|---|---|---|
| matmul | 0.545 (0.111) | 55.36 (6.30) | 42.38 (3.63) | 0.707 (0.294) | 0.779 (0.232) | 0.102 (0.055) |
| lu_solve | 2.540 (0.196) | 325.3 (7.66) | 24.02 (4.99) | 0.484 (0.159) | 0.254 (0.050) | 0.094 (0.044) |
| norms | 0.067 (0.043) | 16.30 (5.58) | 1.509 (0.380) | 0.365 (0.098) | 0.015 (0.005) | 0.013 (0.001) |
| solve_checked | 12.65 (0.68) | 903.9 (73.4) | — | — | — | — |

### n = 200

| Kernel | Ostrin native | Ostrin interp | CPython pure | CPython + NumPy | Julia pure | Julia + OpenBLAS |
|---|---|---|---|---|---|---|
| matmul | 4.783 (0.072) | 518.4 (112.7) | 329.9 (13.6) | 3.048 (0.128) | 5.168 (0.444) | 0.476 (0.233) |
| lu_solve | 18.09 (0.437) | 2569 (235) | 163.3 (11.0) | 1.675 (0.180) | 1.531 (0.179) | 0.365 (0.110) |
| norms | 0.366 (0.029) | 60.37 (3.90) | 5.436 (0.567) | 1.558 (0.240) | 0.069 (0.004) | 0.060 (0.004) |
| solve_checked | 96.16 (4.68) | 6979 (1192) | — | — | — | — |

### Cold and compile costs

- CPython + NumPy cold repetition is about 185–217 ms in every cell (first BLAS/NumPy call), against a warm median of 0.4–3 ms.
- Julia cold repetition is 30–235 ms (first-call JIT compilation); the library cold figure is the largest because the first LAPACK call is included.
- Ostrin native compile: `matmul` 0.48 s, `lu_solve` 1.79 s, `norms` 1.94 s, `solve_checked` 1.95 s (median of 3, `ostrinc --compile`). These are not part of any kernel time.
- Ostrin native process start-up, estimated as T(1) − slope: about 10 ms in every native cell.

## Interpretation

- Ostrin native is faster than CPython pure in every cell (about 69× on matmul at n=200, 9× on lu_solve at n=200, 15× on norms at n=200), which is the expected result of compiling loops to C.
- Against optimised libraries, Ostrin native is slower: roughly 10× slower than Julia with OpenBLAS on matmul at n=200 (4.78 ms against 0.48 ms) and about 50× slower on lu_solve. NumPy is also faster than Ostrin native on matmul and lu_solve. Ostrin's native code does not use a blocked BLAS kernel; this is the reason for the difference, not a claim about the language in general.
- Julia pure is comparable to Ostrin native on matmul and faster on norms at n=200. These are single kernels; no general superiority claim follows from them.
- The Ostrin interpreter is about 70–165× slower than Ostrin native on these kernels at n=200.
- `solve_checked` costs about 5× the plain `lu_solve` at n=200 in native code (96 ms against 18 ms), reflecting equilibration, the residual, and the condition estimate. That is a measurement, not an estimate; no reference library exists for this operation in the comparison.

## Limitations (stated before the numbers are trusted)

1. **Interpreter repetitions**: the interpreter ran 3 samples per cell (not 5), with K=3 per process, to keep the run time practical. It was skipped in repetitions 4–5 of the rotation, so its rotated position is not balanced across all 5 repetitions.
2. **Anomaly, interpreter lu_solve n=200**: the estimated start-up was −378 ms, i.e. the K=1 process was faster than the slope predicts. A re-run of the same program (5 pairs K=1 / K=3, plus wall times for K=0..4) gave a median slope of about 2.48 s per repetition (range 2.31–2.72 s). The K=1 process takes about 2.1 s in total, and K=0 (set-up only) takes 0.12 s. The start-up estimate stays negative (about −350 ms) in the re-run, so the time of one repetition in a K=1 process is lower than the slope from the K=3 processes. The cause is run-to-run variance of the interpreter's repetitions (2.0–2.7 s), not a systematic effect that the external-slope model can absorb. Treat the figure of 2569 ms in the table as the upper end of that range; the re-run gives about 2.5 s with roughly ±10% uncertainty. The start-up column is not meaningful for this cell.
3. **Warm sample sizes**: Python and Julia cells have 20 warm repetitions (5 processes × 4). Ostrin cells have 3–5 samples. Ostrin IQRs come from few samples and are indicative only.
4. **Noisy machine**: background load was present (recorded per cell). Absolute times may shift by a few percent between runs.
5. **Single-threaded only**: all libraries were pinned to one thread. Multi-threaded BLAS would change the library rows substantially and is not measured.
6. **Process start-up**: Python and Julia internal timings exclude start-up; Ostrin external slopes exclude it by construction. The wall-clock cost of starting a process (interpreter start, `import numpy`, `using LinearAlgebra`) was not measured separately and is not part of any figure here.
7. **Scope**: four operations, two sizes, one machine, one Ostrin commit. No claim about other sizes, other kernels, or other hardware is made.

## Reproduction

```bash
node benchmarks/v2/linalg/compare-linalg.mjs --sizes 100,200 --reps 5 --interp-reps 3
```

Requires `compiler/target/release/ostrinc.exe` (or `--ostrinc <path>`), Python with NumPy, and Julia 1.13 with the standard library.
