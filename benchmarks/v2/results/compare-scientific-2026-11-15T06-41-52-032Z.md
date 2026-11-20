# Scientific comparison: Ostrin interpreter builds, Ostrin native, CPython and Julia

Source: `compare-scientific-2026-11-15T06-41-52-032Z.json`. Started 2026-11-15T06:41:52.032Z, finished 2026-11-15T17:05:55.034Z.

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
- Processor load (instantaneous, Win32_Processor): start 28.0 %, end 68.0 %.
- Processes with the largest cumulative CPU time at the start (seconds since each process started, not a current load): powershell (5086 s), The iTero Coach (2257.2 s), chrome (1879.4 s), claude (1761 s), WINWORD (1696.5 s), claude (1595 s).
- The machine was not otherwise idle. Load samples for each cell are in the JSON (`cell.load`).

## Methods

- Ostrin interpreter builds: external slope with K = 1 and K = 2, alternating builds, rotated order, 3 repetitions. Two builds differ only when their paired-slope ranges do not overlap (“conclusive”).
- Ostrin native (HEAD): compiled 3 times outside the timed runs (compile time reported separately); external slope with K = 1 and K = 10, 7 repetitions. The start-up estimate `median T(1) − slope` is an estimate.
- CPython and Julia: internal timing. Each process repeats the kernel 5 times; cold = first repetition of each process, warm = the other repetitions. Julia's cold repetition includes first-call compilation. The wall time of a K = 1 process is reported too; it is not subtracted.
- Every RESULT line was checked against an independent reference before its time was used. A cell that fails is marked INCOMPLETE and has no figure.
- Internal figures (CPython, Julia) and external figures (Ostrin) measure different things and are not directly interchangeable; the report compares each group only with itself, except for the explicit ratios.

## simpson

| Implementation | Build | Per run (ms) | Range (ms) | Uncertainty / IQR (ms) | Other figures | Verdict |
|---|---|---|---|---|---|---|
| Ostrin interpreter | B0 (`7a38ee8b`) | 5505.51 | 5042.08–5519.82 | 238.87 | — | reference |
| Ostrin interpreter | B1 (`ffe90033`) | 5087.80 | 4679.57–5466.42 | 393.42 | ratio 0.924 vs B0 | same within noise |
| Ostrin interpreter | HEAD (`d2035687`) | 4987.64 | 4521.01–5000.90 | 239.95 | ratio 0.906 vs B0 | faster (conclusive) |
| Ostrin native | HEAD (`d2035687`) | 147.80 | 145.62–161.58 | 5.63 | compile 1731 ms (median of 3); start-up estimate 6.6 ms | observed |
| CPython 3.12 | internal, warm | 439.35 | 421.88–489.57 | 22.09 | cold 435.31 ms (median of 5); K=1 process wall 570 ms | observed (internal timer) |
| Julia 1.13 (1 thread) | internal, warm | 17.81 | 16.11–22.60 | 2.61 | cold 22.99 ms (median of 5); K=1 process wall 545 ms | observed (internal timer) |

## bisect

| Implementation | Build | Per run (ms) | Range (ms) | Uncertainty / IQR (ms) | Other figures | Verdict |
|---|---|---|---|---|---|---|
| Ostrin interpreter | B0 (`7a38ee8b`) | 23051.82 | 21521.62–24650.94 | 1564.66 | — | reference |
| Ostrin interpreter | B1 (`ffe90033`) | 16566.60 | 15663.66–17749.62 | 1042.98 | ratio 0.719 vs B0 | faster (conclusive) |
| Ostrin interpreter | HEAD (`d2035687`) | 15583.71 | 13275.34–25822.73 | 6273.69 | ratio 0.676 vs B0 | same within noise |
| Ostrin native | HEAD (`d2035687`) | 30.35 | 29.57–30.88 | 0.80 | compile 1741 ms (median of 3); start-up estimate 8.1 ms | observed |
| CPython 3.12 | internal, warm | 1772.91 | 1413.64–1936.91 | 265.95 | cold 1465.45 ms (median of 5); K=1 process wall 1711 ms | observed (internal timer) |
| Julia 1.13 (1 thread) | internal, warm | 15.13 | 14.48–16.85 | 0.70 | cold 38.71 ms (median of 5); K=1 process wall 745 ms | observed (internal timer) |

## matmul

| Implementation | Build | Per run (ms) | Range (ms) | Uncertainty / IQR (ms) | Other figures | Verdict |
|---|---|---|---|---|---|---|
| Ostrin interpreter | B0 (`7a38ee8b`) | 29664.45 | 25198.54–30060.17 | 2430.82 | — | reference |
| Ostrin interpreter | B1 (`ffe90033`) | 23774.16 | 20425.26–26466.23 | 3020.49 | ratio 0.801 vs B0 | same within noise |
| Ostrin interpreter | HEAD (`d2035687`) | 24314.69 | 21543.73–24950.07 | 1703.17 | ratio 0.820 vs B0 | faster (conclusive) |
| Ostrin native | HEAD (`d2035687`) | 31.62 | 29.68–34.06 | 1.14 | compile 443 ms (median of 3); start-up estimate 9.2 ms | observed |
| CPython 3.12 | internal, warm | 3912.63 | 3280.30–5453.87 | 877.68 | cold 2563.15 ms (median of 5); K=1 process wall 3539 ms | observed (internal timer) |
| Julia 1.13 (1 thread) | internal, warm | 18.06 | 16.60–35.40 | 3.83 | cold 59.92 ms (median of 5); K=1 process wall 707 ms | observed (internal timer) |

## regression

| Implementation | Build | Per run (ms) | Range (ms) | Uncertainty / IQR (ms) | Other figures | Verdict |
|---|---|---|---|---|---|---|
| Ostrin interpreter | B0 (`7a38ee8b`) | 5174.15 | 4898.23–5273.55 | 187.66 | — | reference |
| Ostrin interpreter | B1 (`ffe90033`) | 4527.22 | 4429.02–4740.24 | 155.61 | ratio 0.875 vs B0 | faster (conclusive) |
| Ostrin interpreter | HEAD (`d2035687`) | 5333.19 | 4863.11–5446.45 | 291.67 | ratio 1.031 vs B0 | same within noise |
| Ostrin native | HEAD (`d2035687`) | 5.16 | 5.01–5.30 | 0.14 | compile 454 ms (median of 3); start-up estimate 8.0 ms | observed |
| CPython 3.12 | internal, warm | 477.00 | 464.92–541.17 | 14.18 | cold 478.29 ms (median of 5); K=1 process wall 644 ms | observed (internal timer) |
| Julia 1.13 (1 thread) | internal, warm | 6.30 | 5.83–6.88 | 0.51 | cold 31.94 ms (median of 5); K=1 process wall 767 ms | observed (internal timer) |

## horner

| Implementation | Build | Per run (ms) | Range (ms) | Uncertainty / IQR (ms) | Other figures | Verdict |
|---|---|---|---|---|---|---|
| Ostrin interpreter | B0 (`7a38ee8b`) | 54869.44 | 51498.06–73993.42 | 11247.68 | — | reference |
| Ostrin interpreter | B1 (`ffe90033`) | INCOMPLETE | exit null at K=1 |  | — | incomplete |
| Ostrin interpreter | HEAD (`d2035687`) | 38786.85 | 35182.31–43455.06 | 4136.38 | ratio 0.707 vs B0 | faster (conclusive) |
| Ostrin native | HEAD (`d2035687`) | 96.93 | 95.14–98.25 | 1.62 | compile 400 ms (median of 3); start-up estimate 6.3 ms | observed |
| CPython 3.12 | internal, warm | 4672.46 | 4299.61–5148.61 | 241.69 | cold 3994.94 ms (median of 5); K=1 process wall 3916 ms | observed (internal timer) |
| Julia 1.13 (1 thread) | internal, warm | 69.01 | 68.16–71.83 | 0.96 | cold 114.40 ms (median of 5); K=1 process wall 624 ms | observed (internal timer) |

## sieve

| Implementation | Build | Per run (ms) | Range (ms) | Uncertainty / IQR (ms) | Other figures | Verdict |
|---|---|---|---|---|---|---|
| Ostrin interpreter | B0 (`7a38ee8b`) | 364.03 | 359.44–400.14 | 20.35 | — | reference |
| Ostrin interpreter | B1 (`ffe90033`) | 341.33 | 311.65–369.49 | 28.92 | ratio 0.938 vs B0 | same within noise |
| Ostrin interpreter | HEAD (`d2035687`) | 343.50 | 338.37–363.73 | 12.68 | ratio 0.944 vs B0 | same within noise |
| Ostrin native | HEAD (`d2035687`) | 1.14 | 0.99–1.43 | 0.18 | compile 348 ms (median of 3); start-up estimate 8.1 ms | observed |
| CPython 3.12 | internal, warm | 25.30 | 24.51–35.20 | 2.28 | cold 25.26 ms (median of 5); K=1 process wall 122 ms | observed (internal timer) |
| Julia 1.13 (1 thread) | internal, warm | 1.03 | 1.00–1.10 | 0.02 | cold 15.87 ms (median of 5); K=1 process wall 589 ms | observed (internal timer) |

## collatz

| Implementation | Build | Per run (ms) | Range (ms) | Uncertainty / IQR (ms) | Other figures | Verdict |
|---|---|---|---|---|---|---|
| Ostrin interpreter | B0 (`7a38ee8b`) | 963.03 | 924.52–987.94 | 31.71 | — | reference |
| Ostrin interpreter | B1 (`ffe90033`) | 979.86 | 849.61–1043.93 | 97.16 | ratio 1.017 vs B0 | same within noise |
| Ostrin interpreter | HEAD (`d2035687`) | 999.12 | 978.74–1015.02 | 18.14 | ratio 1.037 vs B0 | same within noise |
| Ostrin native | HEAD (`d2035687`) | 1.92 | 1.89–2.01 | 0.05 | compile 344 ms (median of 3); start-up estimate 7.9 ms | observed |
| CPython 3.12 | internal, warm | 127.95 | 123.12–240.88 | 7.35 | cold 130.94 ms (median of 5); K=1 process wall 247 ms | observed (internal timer) |
| Julia 1.13 (1 thread) | internal, warm | 1.29 | 1.28–1.47 | 0.03 | cold 10.05 ms (median of 5); K=1 process wall 569 ms | observed (internal timer) |

## Limits

- Ostrin and CPython/Julia figures come from different measurement methods (external slope versus internal timers). Their ratios are indicative and are labelled as such in the report text.
- Interpreter repetitions are few (K = 1 and K = 2, three repetitions) because the heavy kernels take tens of seconds each; ranges are reported, not confidence intervals.
- Load was not controlled. Non-overlap of ranges is the only criterion used to call a difference conclusive.
- Python and Julia were not run with the interpreter workloads; the sizes are the same kernel sizes in all languages.

