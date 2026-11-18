# Interpreter A/B: Benchmark V2 extension (interpreter builds, 2026-11-15)

Data: `results/interp-ab-2026-11-15T02-17-51-673Z.json`; the table is in `results/interp-ab-2026-11-15T02-17-51-673Z.md`.
Every number below is copied from those files. Nothing is recomputed from other sources.

## 1. What was measured, and with which builds

| Label | Commit | What changed relative to the previous build |
|---|---|---|
| B0 | `7a38ee8b` | Frames are a hash map. Reference before any frame optimisation. |
| B1 | `ffe90033` | Frames are an ordered list with a linear search. |
| B2 | `54ebc579` (HEAD of the measurement) | Hybrid frames (name index above 16 names, stale-slot reuse) and reuse of the frame of range `for` loops whose body has no closure or task. |

B2's interpreter code equals `a22f94e6`. Between `a22f94e6` and `54ebc579` only unit tests changed in `compiler/src/interpreter/`.
Each build was compiled in release mode from `git archive <commit> compiler` in an isolated directory. The SHA-256 of each executable is in the JSON.

Method: external slope. Each workload runs with K = 1 and K = 5 in alternation, 7 repetitions, with the order of the builds rotated per repetition. Per-run cost is the median of the paired slopes `(T(5) − T(1)) / 4`. Two builds differ only when the ranges of their paired slopes do not overlap. Each run's `RESULT` line was checked against an independent reference (BigInt or IEEE double in JavaScript) before its time was used: 16 workloads × 3 builds, all correct.

## 2. Cumulative gain of the interpreter optimisations (B0 → B2)

Conclusive improvements (non-overlapping ranges):

| Workload | B0 ms/run | B2 ms/run | B2 / B0 |
|---|---|---|---|
| micro_empty_loop | 61.92 | 10.22 | 0.165 |
| micro_int_add | 151.26 | 89.97 | 0.595 |
| micro_function_call | 269.41 | 201.25 | 0.747 |
| micro_nested_block | 177.24 | 115.25 | 0.650 |
| micro_float_mul_add | 150.36 | 95.51 | 0.635 |
| micro_array_read | 189.62 | 134.93 | 0.712 |
| frames_small | 427.55 | 266.38 | 0.623 |
| frames_large | 471.26 | 412.42 | 0.875 |
| nested_loops | 150.97 | 99.42 | 0.659 |
| records | 61.56 | 51.78 | 0.841 |

The largest gain is the empty loop: the per-iteration frame is no longer allocated when the body has no closure or task. Loops whose body contains nothing but ordinary statements (`int_add`, `nested_block`, `float_mul_add`, `nested_loops`) gain 30–40 %. Calls and closures gain less, because the per-call environment is still built.

## 3. Small and large frames

- Small frames (`frames_small`, 3 names): B2 is 0.623 × B0 (conclusive).
- Large frames (`frames_large`, 120 names): B1 was 1.83 × B0 (conclusive **regression**, the list frame with linear search). B2 is 0.875 × B0 (conclusive improvement). The hybrid frame removed the regression introduced by the list frame and kept the small-frame gain.
- `frames_mid` (40 names): B1 1.11 × B0 and B2 0.885 × B0, both **inconclusive** (the ranges overlap; the B0 range is 841–1132 ms, the widest in the run).
- `scope_lookup` (names three frames up): B1 0.88 × B0 (conclusive), B2 0.92 × B0 (overlapping). B2 is not conclusively different from B1 (ranges 354–380 vs 366–421). Small frames are searched linearly in B2 as in B1, so a change here is not expected; the overlap is the measured noise.

## 4. Regressions and noise

- The only conclusive regression in the run is B1 on `frames_large`, already corrected by B2 (section 3).
- `micro_while_loop` is unchanged (B2 / B0 = 1.005, overlapping). No optimisation targets `while`, so this is expected.
- `closures` and `recursion_fib` are unchanged within noise. A closure creation is not optimised, and the call path was not changed.
- The start-up estimates (`median T(1) − slope`) are negative for `frames_mid`, `frames_large` and some other rows. A physical start-up cost is positive, so these estimates are not valid there: the linear model fails at that size. The start-up column should not be used.
- Variability: the coefficient of variation of the K = 1 times is 1–33 % (largest for `recursion_fib`, B0, 32 %, a 26 ms workload). Short workloads are noisier in relative terms.

## 5. Conditions of the measurement (what is and is not controlled)

- Single-threaded interpreter; `OMP_NUM_THREADS` and `OPENBLAS_NUM_THREADS` set to 1 for every child process.
- The machine ran background applications during the measurement: League of Legends client processes, Chrome and The iTero Coach are listed with their cumulative CPU time in the JSON. Their current load was **not** captured: the processor-utilisation probe in the runner returned no value, and the JSON records it as unavailable. The conclusions therefore rest on the paired, rotated repetitions and the non-overlap criterion, not on a quiet machine.
- No compilation ran during the measurement. The three binaries were built before the measurement started.
- The interpreter has no clock inside a program, so the times are external (process-level, including start-up, removed by the slope). No in-program time is reported.

## 6. Remaining bottlenecks (from the data)

- Per-operation overhead: `micro_while_loop` (≈180 ms per 500 000 iterations of compare, add and assign) is the slowest micro path and was not changed. Expression evaluation and assignment dominate.
- Function calls: `micro_function_call` (≈200 ms per 500 000 calls) and the recursion of `recursion_fib`. The call path builds a new environment and a `CallFrame` for every call.
- Frames with many names: `frames_mid` (≈780 ms, 40 defines per iteration) remains the largest cost in the run. Each define still searches or allocates a name slot.
- Closures: `closures` creates an environment for each closure in each iteration (≈160 ms per 100 000 iterations).

## 7. Which comparisons are conclusive

Conclusive: every row marked "conclusive" in the Markdown table (for example `micro_empty_loop`, `micro_int_add`, `frames_small`, `frames_large` B1 vs B0 and B2 vs B0, `nested_loops`).
Not conclusive: `frames_mid`, `closures`, `recursion_fib`, `micro_while_loop` (B2 vs B0), `scope_lookup` (B2 vs B0 and B2 vs B1), and the start-up estimates.

## 8. Not covered by this extension

- Native and Python/Julia comparisons for these workloads. The five scientific kernels with Python and Julia are in `SCIENTIFIC-REPORT-V2.md` (earlier session, unchanged here).
- Symbolic and rational workloads (`std.symbolic`, `std.rational`) were not added to the interpreter set in this run.
- Compilation costs of these workloads (the interpreter has no compile step).
