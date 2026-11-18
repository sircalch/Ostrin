# Scientific report: five numerical kernels in Ostrin, Python and Julia

Source of every number: `benchmarks/results/benchmark-2026-11-12T14-15-13-065Z.json` (run 3, `allComplete: true`).
The Markdown table generated from it is `benchmarks/results/REPORT-2026-11-12T14-15-13-065Z.md`.
Commit that holds the suite and this result: `ae70ecf2`.

## 1. What was measured

| Kernel | Problem size | Algorithm (identical in all languages) | Reference used for correctness |
|---|---|---|---|
| `simpson` | ∫₀^π sin, 4 000 000 panels | composite Simpson, same summation order | exact value 2 (tolerance 10⁻¹²) |
| `bisect` | root of x³ − 2x − 5 on [2, 3], 200 000 repeats | bisection, same update rule | Newton iteration in JavaScript (tolerance 10⁻¹²) |
| `matmul` | 300 × 300 integer matrices | triple loop, trace and total | exact integers, computed in JavaScript |
| `regression` | 2 000 000 points, y = 1 + 2x − 3x² | normal equations, Gaussian elimination | exact coefficients 1, 2, −3 (tolerance 10⁻⁹) |
| `horner` | degree 50, 2 000 000 evaluation points | Horner's rule | JavaScript sum with the same operation order (tolerance 10⁻¹²) |

Languages and modes: Ostrin native (compiled with `ostrinc --compile`, executed as a program), Ostrin interpreter
(`ostrinc --run`), Python 3.12.10 (CPython), Julia 1.13.3.

Hardware: AMD Ryzen 7 5700X, 8 physical cores, 16 logical, 32 GiB, Windows 10.0.26200.

Every cell passed its correctness check before timing, and every timed run was checked as well. No cell is incomplete.

## 2. How time was measured, and what it does and does not mean

- **Compile** (Ostrin native only): median wall time of `ostrinc --compile`, 7 repetitions. Not part of the run figures.
- **Init**: median wall time of an empty program. It is the process start of each runtime: 12.8 ms (Ostrin native),
  14.7 ms (Ostrin interpreter, 3 repetitions), 98.9 ms (CPython), 226.0 ms (Julia, `-e nothing`).
- **Total**: median wall time of one run of the kernel program, including process start. 7 repetitions for native,
  Python and Julia; 3 for the interpreter, which is too few for a dispersion estimate, so interpreter dispersion is
  reported but weak.
- **Run (estimate)** = total − init. This is **an estimate, not a measurement of computation time**. For CPython it agrees
  with the in-process measurement within 2.5 ms on Simpson (408 ms against 405 ms), which supports its use for Python.
  For Julia it does not: total − init is 240 ms on Simpson while the in-process cold call is 31 ms, so about 208 ms of the
  Julia process is not the kernel (script loading and top-level compilation). For Julia the external estimate is
  therefore **not used**.
- **In-process cold / warm** (Python and Julia only): the program times one call of the kernel in the same process, then a
  second call. Cold includes the first-call compilation of the kernel for Julia. Warm is the steady state. Neither includes
  process start. Ostrin has no clock inside a program, so Ostrin has no in-process measurement.
- **Dispersion**: minimum, maximum and median absolute deviation (MAD) over the repetitions.

## 3. Results

### 3.1 Totals and dispersion (median, ms; repetitions in brackets)

| Kernel | Ostrin native [7] | Ostrin interp. [3] | CPython [7] | Julia [7] |
|---|---|---|---|---|
| simpson | 152.7 (MAD 0.6) | 7 379 (MAD 33) | 506.8 (MAD 6.3) | 465.1 (MAD 6.7) |
| bisect | 42.4 (MAD 0.4) | 22 675 (MAD 181) | 1 409.1 (MAD 5.4) | 469.3 (MAD 6.5) |
| matmul | 41.9 (MAD 0.7) | 38 552 (MAD 86) | 2 316.4 (MAD 25.5) | 482.6 (MAD 3.3) |
| regression | 17.3 (MAD 0.6; max 34.9) | 8 057 (MAD 16) | 551.8 (MAD 2.0) | 458.3 (MAD 3.5) |
| horner | 108.4 (MAD 0.6) | 85 268 (MAD 416) | 3 861.4 (MAD 24.0) | 547.3 (MAD 6.0) |

### 3.2 Run estimate (total − init, ms) and in-process timings

| Kernel | Ostrin native run est. | Ostrin interp. run est. | CPython run est. | CPython in-process cold | Julia cold | Julia warm |
|---|---|---|---|---|---|---|
| simpson | 139.9 | 7 365 | 407.9 | 405.4 | 31.4 | 20.4 |
| bisect | 29.5 | 22 660 | 1 310.2 | 1 328.0 | 36.1 | 14.6 |
| matmul | 29.1 | 38 537 | 2 217.5 | 2 250.4 | 57.9 | 16.5 |
| regression | 4.5 (inconclusive, see §4) | 8 042 | 452.9 | 452.7 | 31.7 | 5.9 |
| horner | 95.6 | 85 253 | 3 762.5 | 3 735.4 | 119.9 | 68.8 |

### 3.3 Ratios where the comparison is methodologically valid

Only the following ratios are reported.

- **Interpreter ÷ Ostrin native** (both external, same method): 53× (Simpson), 767× (bisect), 1 323× (matmul),
  892× (horner); regression is not reported because its native estimate is inconclusive.
- **CPython ÷ Ostrin native** (both external, and CPython's estimate is confirmed in-process): 2.9× (Simpson), 44× (bisect),
  76× (matmul), 39× (horner). These say that the compiled Ostrin program is faster than CPython on these loops on this
  machine. They do not say that Ostrin is faster than Python in general: the kernels are scalar loops that CPython runs
  without NumPy, and the Ostrin program is compiled to C.
- **Julia against Ostrin native:** the Ostrin figure is external (total − init) and the Julia figures are in-process, so
  these are **mixed-method** comparisons. They are observations, not a ranking. The results by kernel, with the shorter
  time first in each pair:
  - Simpson: Julia cold 31.4 ms, Ostrin native 139.9 ms, Julia warm 20.4 ms. Julia is faster in both modes.
  - Bisection: Ostrin native 29.5 ms, Julia cold 36.1 ms, Julia warm 14.6 ms. Ostrin is faster than Julia's first call; Julia's warm call is faster.
  - Matmul: Ostrin native 29.1 ms, Julia cold 57.9 ms, Julia warm 16.5 ms. Ostrin is faster than Julia's first call; Julia's warm call is faster.
  - Regression: Ostrin native is inconclusive (§4); Julia cold 31.7 ms, warm 5.9 ms.
  - Horner: Ostrin native 95.6 ms, Julia cold 119.9 ms, Julia warm 68.8 ms. Ostrin is faster than Julia's first call; Julia's warm call is faster.

  So Ostrin native is faster than Julia's first call on bisection, matmul and horner, and slower on Simpson. Julia's warm
  call is faster than Ostrin native on every kernel where Ostrin's figure is conclusive. Julia's warm figure is the steady
  state of a long-running process; Ostrin's figure includes the process start, which is short for Ostrin (12.8 ms).

### 3.4 Per-step costs derived from the totals (interpreter and native)

- Ostrin interpreter, Horner: 85 253 ms − 14.7 ms over 2 000 000 × 51 = 102 000 000 inner steps → **836 ns per step**.
- Ostrin native, Horner: 95.6 ms over the same steps → **0.94 ns per step**.
- Ostrin interpreter, matmul: about **1.43 µs per inner multiply-add** (27 000 000 steps).

These are measured totals divided by counted steps. The interpreter figure varies by kernel, which is itself a finding:
Horner's steps are a multiply and an add on an array element, while matmul's add an array indexing.

## 4. Inconclusive results

- **Ostrin native regression** (total 17.3 ms, init 12.8 ms, run estimate 4.5 ms): the total has a maximum of 34.9 ms
  against a median of 17.3 ms, and the estimate is smaller than the spread of the two measurements. It is reported as
  inconclusive. No ratio involving it is used.
- **Ostrin interpreter dispersion**: three repetitions only. The MAD values are small (under 0.5 % of the median on all
  five kernels), but three samples do not establish a distribution.
- **Between sessions**: run 1 was incomplete (its Python matmul timed out and is kept as a partial record); run 3 is the reported run. Run 1 gave
  Julia Simpson 560 ms and Ostrin native Simpson 154 ms; run 2 gave Ostrin native Simpson 281 ms, 80 % above both. Run 2's
  Python and Julia totals are invalid (the warm call was inside the timed process). Its native figures were not explained
  and are not used to set any claim.

## 5. Three bottlenecks supported by these measurements

1. **Interpreter per-operation cost.** 836 ns per Horner step and about 1.4 µs per matmul step, against 0.94 ns for the
   native build of the same Horner loop: about 890× on Horner, 1 320× on matmul. This dominates every run of the
   interpreter. It is the largest gap in the data.
2. **Compile time of programs that import `std.numeric`.** The native compile took 1 548–1 552 ms for the two programs
   that import `std.numeric` (Simpson and bisection) and 345–400 ms for the three that do not (regression, matmul,
   Horner). The compile step is the largest fixed cost of the edit-compile-run loop for those programs. The cause is not
   identified by these measurements; the imported module's size is the likely factor and is not measured here.
3. **Process start of the external runtimes for short programs.** Julia's init is 226 ms, and about 208 ms of the Julia
   process on Simpson is outside the kernel. For a kernel that takes 31 ms in cold call, the process overhead is most of
   the wall time. Ostrin's native init is 12.8 ms, so this bottleneck does not apply to Ostrin native.

## 6. Optimisation

No optimisation was implemented in this round. The bottlenecks above need design work rather than a local change that
could be verified against these measurements in this session: an interpreter's dispatch cost is not a single function,
and the compile cost of `std.numeric` depends on how modules are compiled and cached. Applying one without measurement
would repeat the error this report avoids. The next step that the data supports is a profile of the interpreter on the
Horner kernel, followed by a before-and-after run of the same suite.

## 7. Limits

- One machine, one operating system, one session per run; run-to-run variation is visible (§4) and is not characterised.
- The kernels are equivalent hand-written algorithms, not tuned library calls. NumPy, BLAS, LAPACK and compiled
  scientific libraries were not used, so no statement about optimised library performance is made.
- External timing includes process start and the operating system's scheduling. Ostrin has no in-process clock, so its
  kernel times rest on the subtraction of an empty program, which is an estimate.
- Ostrin native timings are not independently checked in-process; CPython's subtraction was checked and agreed within
  2.5 ms, which supports the method but does not prove it for Ostrin.
- Julia's cold figure includes compilation of the kernel, and its warm figure excludes it; neither is the cost of a fresh
  process.
- The compiler binary used for the run is the one built before the symbolic-module changes of later commits. Those changes
  do not touch the kernels, which use `std.numeric` only, and the kernels were not re-measured after them.
- No Wolfram or MATLAB results are included, and none are implied.
