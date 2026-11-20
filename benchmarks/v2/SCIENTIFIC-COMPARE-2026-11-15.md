# Scientific comparison (2026-11-15): interpreter builds, native, CPython and Julia

Generated tables: `results/compare-scientific-2026-11-15T06-41-52-032Z.md` (simpson, bisect, matmul, regression, sieve,
collatz) and `results/compare-scientific-2026-11-15T17-07-10-793Z.md` (horner, rerun because the first run's B1 horner cell
was killed by the environment and reported INCOMPLETE; the rerun completed with correct results). Every number below is
copied from those files. Nothing is recomputed from other sources.

## Method and its limits

- Ostrin interpreter builds: 7a38ee8b (B0), ffe90033 (B1), d2035687 (HEAD). External slope, K = 1 and K = 2, three alternating
  repetitions. Two builds are different only when their paired-slope ranges do not overlap.
- Ostrin native (HEAD): compiled three times outside the timed runs; external slope, K = 1 and K = 10, seven repetitions.
- CPython 3.12 and Julia 1.13 (one thread): internal timers, five processes each with five repetitions. Cold is the first
  repetition of each process (Julia includes first-call compilation); warm is the rest.
- Every RESULT was checked against an independent JavaScript reference before its time was used.
- Load: processor load at the start and end of the first run was 28 % and 68 %; for the horner rerun it was 12 % and 32 %.
  Background applications (a game client, a browser, a chat client, Word) were running; they are not controlled. Only
  non-overlapping ranges are treated as conclusive.
- The external and internal methods measure different things. Ostrin and CPython/Julia figures are not directly equivalent;
  the ratios below are indicative.

## Per-run medians (ms)

| Kernel | Interp B0 | Interp B1 | Interp HEAD | Native HEAD | CPython warm | Julia warm | Julia cold |
|---|---|---|---|---|---|---|---|
| simpson | 5 505.5 | 5 087.8 | 4 987.6 | 147.8 | 439.4 | 17.8 | 23.0 |
| bisect | 23 051.8 | 16 566.6 | 15 583.7 | 30.4 | 1 772.9 | 15.1 | 38.7 |
| matmul | 29 664.5 | 23 774.2 | 24 314.7 | 31.6 | 3 912.6 | 18.1 | 59.9 |
| regression | 5 174.2 | 4 527.2 | 5 333.2 | 5.2 | 477.0 | 6.3 | 31.9 |
| horner (rerun) | 51 256.4 | 40 590.8 | 36 701.4 | 100.8 | 4 211.7 | 71.8 | 126.9 |
| sieve | 364.0 | 341.3 | 343.5 | 1.14 | 25.3 | 1.03 | 15.9 |
| collatz | 963.0 | 979.9 | 999.1 | 1.92 | 128.0 | 1.29 | 10.1 |

Verdicts against B0 (conclusive means non-overlapping ranges):

| Kernel | B1 vs B0 | HEAD vs B0 |
|---|---|---|
| simpson | same within noise | 0.906, faster (conclusive) |
| bisect | 0.719, faster (conclusive) | 0.676, same within noise (HEAD range 13 275–25 823 ms) |
| matmul | same within noise | 0.820, faster (conclusive) |
| regression | 0.875, faster (conclusive) | 1.031, same within noise |
| horner (rerun) | 0.792, faster (conclusive) | 0.716, faster (conclusive) |
| sieve | same within noise | same within noise |
| collatz | same within noise | same within noise |

## Answers to the five questions

1. **How much the interpreter improved.** Between B0 and HEAD the kernel gains are real but modest: 0.72× to 0.91× on
   simpson, matmul and horner, and no conclusive change on bisect, sieve or collatz. Regression did not improve: HEAD is 1.03×
   B0 (within noise), and B1 was faster than B0 there. The large gains in the interpreter A/B (for example 0.17× on an empty
   loop) do not transfer to these kernels, whose time is dominated by float arithmetic and calls.
2. **Gap to native.** HEAD interpreter against HEAD native, medians: simpson about 34×, bisect about 510×, matmul about 770×,
   regression about 1 030×, horner about 360×, sieve about 300×, collatz about 520×.
3. **Competitive kernels.** Native is competitive with Julia on regression (native 5.2 ms against Julia 6.3 ms, warm) and
   sieve (1.14 ms against 1.03 ms). Native is faster than CPython on every kernel: about 3× on simpson, 42× on horner, 58× on
   bisect, 92× on regression and 124× on matmul. Julia warm is faster than native on simpson (about 8×), bisect (2×), matmul
   (1.7×), horner (1.4×) and collatz (1.5×).
4. **Main bottlenecks.** The interpreter's per-operation cost (every kernel is 34× to 1 030× slower than native). Native
   floating-point loops on simpson, bisect and matmul, where Julia is 2× to 8× faster; the native time is a candidate for the
   next work, not a verified cause. Julia's cold cost is 10 to 127 ms per process for these sizes.
5. **Optimisations worth developing next (not implemented, not measured).** Native code generation for float loops (bounds
   and boxing checks around array access), a faster `sin` path in simpson, and interpreter call and environment costs, which
   the regression result suggests are not yet addressed. Each needs its own A/B run before it is claimed.

## Methodological differences to keep in mind

- Julia warm and CPython warm are internal per-run figures; Ostrin figures are external slopes with process start removed.
- The start-up estimates from the external method are valid for native (about 6 to 9 ms) but are not reported for the
  interpreter, where the linear model is less reliable. For Python and Julia the start-up cost is shown as the K = 1 process
  wall time, which is not subtracted.
- Interpreter repetitions are few (K = 1 and K = 2, three repetitions) because one horner run takes about 40 s. Ranges are
  reported, not confidence intervals.

## What is not claimed

- No general claim that Ostrin is faster or slower than Python or Julia.
- No MATLAB or Mathematica results. None were run and none are included.
- The sieve and collatz workloads are two additional kernels of this benchmark only; they are not representative of other
  workloads.
