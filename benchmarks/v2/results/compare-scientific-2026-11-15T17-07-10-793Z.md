# Scientific comparison: Ostrin interpreter builds, Ostrin native, CPython and Julia

Source: `compare-scientific-2026-11-15T17-07-10-793Z.json`. Started 2026-11-15T17:07:10.793Z, finished 2026-11-15T17:32:01.849Z.

## Builds and versions

| Label | Commit | SHA-256 (exe) | Version |
|---|---|---|---|
| B0 | `7a38ee8b` | `94784d1db7fe7ec0…` | ostrinc 0.2.0 |
| B1 | `ffe90033` | `0239ff781c8f473c…` | ostrinc 0.2.0 |
| HEAD | `d2035687` | `8eb45ea52f4ff542…` | ostrinc 0.2.0 |
| HEAD native | `d2035687` | `8eb45ea52f4ff542…` | ostrinc 0.2.0 |

Python: Python 3.12.10. Julia: julia version 1.13.3. Node: v24.19.0.

## System and conditions

- win32 10.0.26200 x64; AMD Ryzen 7 5700X 8-Core Processor; 8 physical / 16 logical processors; 31.9 GiB.
- Thread environment: {"OMP_NUM_THREADS":1,"OPENBLAS_NUM_THREADS":1,"julia":1,"python":1,"ostrinNative":"single-threaded"}.
- Processor load (instantaneous, Win32_Processor): start 12.0 %, end 32.0 %.
- Processes with the largest cumulative CPU time at the start (seconds since each process started, not a current load): powershell (8313.7 s), claude (2820.1 s), The iTero Coach (2342.4 s), claude (2202.1 s), chrome (2013.7 s), chrome (1748.3 s).
- The machine was not otherwise idle. Load samples for each cell are in the JSON (`cell.load`).

## Methods

- Ostrin interpreter builds: external slope with K = 1 and K = 2, alternating builds, rotated order, 3 repetitions. Two builds differ only when their paired-slope ranges do not overlap (“conclusive”).
- Ostrin native (HEAD): compiled 3 times outside the timed runs (compile time reported separately); external slope with K = 1 and K = 10, 7 repetitions. The start-up estimate `median T(1) − slope` is an estimate.
- CPython and Julia: internal timing. Each process repeats the kernel 5 times; cold = first repetition of each process, warm = the other repetitions. Julia's cold repetition includes first-call compilation. The wall time of a K = 1 process is reported too; it is not subtracted.
- Every RESULT line was checked against an independent reference before its time was used. A cell that fails is marked INCOMPLETE and has no figure.
- Internal figures (CPython, Julia) and external figures (Ostrin) measure different things and are not directly interchangeable; the report compares each group only with itself, except for the explicit ratios.

## horner

| Implementation | Build | Per run (ms) | Range (ms) | Uncertainty / IQR (ms) | Other figures | Verdict |
|---|---|---|---|---|---|---|
| Ostrin interpreter | B0 (`7a38ee8b`) | 51256.38 | 50643.51–54382.92 | 1869.71 | — | reference |
| Ostrin interpreter | B1 (`ffe90033`) | 40590.80 | 39909.03–41354.10 | 722.53 | ratio 0.792 vs B0 | faster (conclusive) |
| Ostrin interpreter | HEAD (`d2035687`) | 36701.36 | 36608.94–40569.07 | 1980.06 | ratio 0.716 vs B0 | faster (conclusive) |
| Ostrin native | HEAD (`d2035687`) | 100.76 | 99.14–111.61 | 4.07 | compile 419 ms (median of 3); start-up estimate 6.7 ms | observed |
| CPython 3.12 | internal, warm | 4211.65 | 4050.20–5293.44 | 822.45 | cold 4399.44 ms (median of 5); K=1 process wall 4254 ms | observed (internal timer) |
| Julia 1.13 (1 thread) | internal, warm | 71.79 | 69.88–75.89 | 2.97 | cold 126.91 ms (median of 5); K=1 process wall 718 ms | observed (internal timer) |

## Limits

- Ostrin and CPython/Julia figures come from different measurement methods (external slope versus internal timers). Their ratios are indicative and are labelled as such in the report text.
- Interpreter repetitions are few (K = 1 and K = 2, three repetitions) because the heavy kernels take tens of seconds each; ranges are reported, not confidence intervals.
- Load was not controlled. Non-overlap of ranges is the only criterion used to call a difference conclusive.
- Python and Julia were not run with the interpreter workloads; the sizes are the same kernel sizes in all languages.

