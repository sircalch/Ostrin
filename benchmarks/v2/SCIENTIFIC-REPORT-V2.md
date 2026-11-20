# Scientific Benchmark V2: five kernels, interpreter before and after, and a classification of every figure

All numbers below are copied from the JSON files in `benchmarks/v2/results/`. Nothing is recomputed from other sources.

## 1. What was measured

| Kernel | Size (identical in all implementations) | Algorithm |
|---|---|---|
| Simpson | ∫₀^π sin, 4 000 000 panels | composite Simpson, same summation order |
| Regression | 2 000 000 points, y = 1 + 2x − 3x² | normal equations, Gaussian elimination |
| Bisection | root of x³ − 2x − 5 on [2, 3], 200 000 bisections per run | bisection, same update rule |
| Matmul | 300 × 300 integer matrices | triple loop, trace and total |
| Horner | degree 50, 2 000 000 points | Horner's rule |

Implementations: Ostrin native (`ostrinc --compile`, run as a program), Ostrin interpreter (`ostrinc --run`), CPython 3.12.10, Julia 1.13.3 with `--threads=1`.

Hardware: AMD Ryzen 7 5700X, 8 physical cores, 16 logical, 32 GiB, Windows 10.0.26200. Thread variables `OMP_NUM_THREADS` and `OPENBLAS_NUM_THREADS` are 1 for every child process.

Every timed run's printed result was checked against a reference computed independently before any timing was used. No cell is incomplete.

## 2. Methods, and how far each one can be trusted

- **External slope (Ostrin native and interpreter; Python and Julia as a cross-check).** Each program takes K as an argument and repeats its kernel K times in one process. The time per kernel run is `(median T(K_big) − median T(1)) / (K_big − 1)`, with K_big = 10 for native, 5 for Python and Julia, 2 for the interpreter. Process start cancels. The reported uncertainty is the sum of the two spreads divided by (K_big − 1), so it is conservative. The startup estimate `median T(1) − slope` is an estimate, not a measurement.
- **Internal (Python and Julia).** Each repetition is timed inside the process with `time.perf_counter` or `time_ns`. Julia's cold figure is its first repetition, which includes first-call compilation; its warm figure is the median of the rest. There is no internal figure for Ostrin, which has no clock inside a program.
- **Interleaved A/B (interpreter before and after).** The two binaries alternate, so both see the same machine conditions. Non-overlapping ranges across repetitions are the evidence that a difference is not noise.

**Load.** The first Benchmark V2 run was recorded while a background application (iTero Coach) was active. It is kept in `results/` as `v2-after-2026-11-13T04-05-35-074Z.json` and is **not used for any conclusion** in this report. The figures below come from runs made after that application was stopped, with the remaining background applications listed in each file's `loadBefore`.

## 3. Ostrin native (external slope, quiet run)

| Kernel | Per run (ms) | Uncertainty (ms) | Startup estimate (ms) | Compile (ms) | Classification |
|---|---|---|---|---|---|
| Simpson | 141.6 | ± 5.5 | 18.7 | 1 592 | observed, conclusive |
| Regression | 5.0 | ± 0.5 | 12.8 | 386 | observed, conclusive |
| Bisection | 29.6 | ± 0.9 | 12.9 | 1 593 | observed, conclusive |
| Matmul | 29.8 | ± 2.9 | 12.8 | 429 | observed, conclusive |
| Horner | 95.9 | ± 3.3 | 14.9 | 416 | observed, conclusive |

The startup estimates agree with the empty-program measurement of about 13 ms, which supports the method for this backend. The compile time is about 1.6 s for programs that import `std.numeric` and about 0.4 s for the others.

## 4. Python (internal warm is the per-run figure; the external slope is a cross-check)

| Kernel | Internal warm median (ms) | Internal warm range (ms) | External slope (ms) | Classification |
|---|---|---|---|---|
| Simpson | 424.8 | 408.7–1 352.9 | 419.6 ± 25.6 | observed; the two methods agree, conclusive |
| Regression | 479.9 | 458.3–555.8 | 649.1 ± 1 393.4 | the external slope is **inconclusive** (uncertainty larger than the value); internal figure used |
| Bisection | 1 350.9 | 1 320.5–1 386.3 | 1 364.7 ± 121.1 | observed; the two methods agree, conclusive |
| Matmul | 2 410.3 | 2 303.9–4 174.7 | 2 382.9 ± 304.9 | observed; the two methods agree; one slow outlier in the range |
| Horner | 4 378.9 | 4 327.5–4 467.6 | 4 382.5 ± 132.9 | observed; the two methods agree, conclusive |

The external Python startup estimates range from −304 to +346 ms. These are not physical values, which confirms that the external subtraction is not valid for Python in this environment.

## 5. Julia 1.13, one thread (internal warm is the per-run figure)

| Kernel | Cold, first call (ms) | Warm median (ms) | Warm range (ms) | Classification |
|---|---|---|---|---|
| Simpson | 23.9 | 18.1 | 17.0–23.4 | warm: conclusive; cold: observed |
| Regression | 22.8 | 5.9 | 5.8–6.2 | warm: conclusive; cold: observed |
| Bisection | 29.5 | 14.6 | 14.3–14.9 | warm: conclusive; cold: observed |
| Matmul | 48.5 | 16.7 | 16.3–27.0 | warm: conclusive; cold: observed |
| Horner | 111.9 | 69.2 | 68.3–71.3 | warm: conclusive; cold: observed |

The external slopes for Julia are **not valid**: they imply a startup of about 545–580 ms, while the empty-program measurement in the earlier run was about 226 ms, and their uncertainties are larger than the values (for example matmul: 21.0 ± 385.1 ms). The internal warm figures are used instead. The gap between the external estimate and the internal warm figure is the process cost that the external method cannot separate from the kernel.

## 6. Interpreter: before and after the performance work

Baseline binary: commit `0fd8274b`, built in a separate directory. Current binary: `08a96c62` and later, which includes lazy range loops, fewer allocations, cheaper calls and the name hasher.

### 6.1 Interleaved A/B (the conclusive comparison; K = 1, five repetitions each)

| Kernel | Before median [range] (ms) | After median [range] (ms) | After/before |
|---|---|---|---|
| Simpson | 7 500 [7 425–7 648] | 4 610 [4 573–4 686] | 0.615 |
| Regression | 9 212 [9 121–9 284] | 4 823 [4 794–4 848] | 0.523 |
| Bisection | 21 877 [21 728–22 214] | 16 095 [15 991–16 164] | 0.736 |
| Matmul | 38 681 [38 590–38 821] | 21 628 [21 445–22 103] | 0.559 |

For these four kernels the ranges do not overlap, so the speed-up is conclusive for this session. The file is `results/ab-interp-2026-11-13-before-after.txt`; the script is `ab-interp.mjs`.

### 6.2 Back-to-back external slopes (not interleaved; supporting only)

| Kernel | Before (ms per run) | After (ms per run) | Before/after |
|---|---|---|---|
| Simpson | 8 928 ± 524 | 4 620 ± 167 | 1.93 |
| Regression | 8 985 ± 565 | 4 973 ± 414 | 1.81 |
| Bisection | 22 241 ± 1 855 | 15 882 ± 788 | 1.40 |
| Matmul | 39 912 ± 2 192 | 21 698 ± 2 102 | 1.84 |
| Horner | 89 680 ± 10 197 | 48 828 ± 2 082 | 1.84 |

These are consistent with the interleaved result for the four shared kernels. Horner appears only here: a single run takes about 50 s per binary, so it was not interleaved. **Its speed-up is observed, not conclusive.**

## 7. Comparisons that are methodologically valid

Only comparisons where both sides measure the steady cost of one kernel run, without process start, are reported as ratios.

- **Ostrin native against Python (Python internal warm; Ostrin external slope):** Python is slower by 3.0× (Simpson), 45.6× (bisection), 80.9× (matmul) and 45.7× (Horner). Regression is inconclusive on the Python side and is not reported.
- **Ostrin native against Julia (warm, internal):** Julia's warm call is faster on Simpson (≈ 7.8×), bisection (≈ 2.0×), matmul (≈ 1.8×) and Horner (≈ 1.4×); Ostrin native is about 1.2× faster on regression (5.0 ms against 5.9 ms, with spreads that are close to each other). Julia's cold call is slower than its warm call by 1.3× (Simpson) to 3.9× (regression).
- **Interpreter against native (Ostrin only, both external slopes):** after the performance work the interpreter is about 33× slower than native on Simpson (4 620 ms against 141.6 ms), 537× on bisection, 728× on matmul, 995× on regression and 509× on Horner (48 828 ms against 95.9 ms). These ratios are between two Ostrin backends measured the same way, so they are valid, but they are very large; the interpreter is not intended for the heavy kernels.

Ratios that compare external slopes of different languages are not reported, because the external method is not valid for Julia in this environment.

## 8. Bottlenecks supported by these measurements

1. **Interpreter cost per operation.** Even after the performance work, the interpreter runs the same kernels about 33 to 995 times slower than native code, depending on the kernel. The after-to-before reduction (0.52 to 0.74) addresses per-iteration allocation and call overhead but does not change the order of magnitude.
2. **Compile time of programs that import `std.numeric`.** About 1.6 s for Simpson and bisection, against about 0.4 s for the programs without that import. The cause has not been measured.
3. **Julia's first call and process start.** Julia's cold call costs 1.3× to 3.9× its warm call, and its process start is about 0.2–0.6 s, which dominates short programs.

## 9. Limits

- One machine, one operating system, background applications that could not all be stopped; the load is recorded in each file.
- The kernels are equivalent hand-written algorithms, not optimised libraries; no NumPy, BLAS or LAPACK is used.
- The native backend was not changed by the interpreter work; its figures are unaffected by it.
- The interpreter A/B covers four kernels in one session; Horner is supported only by back-to-back runs.
- The uncertainties are conservative sums of spreads; they can overstate the real uncertainty.
- No Wolfram or MATLAB results are included, and none are implied.
