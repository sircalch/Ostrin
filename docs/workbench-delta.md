# Workbench Delta: parameter sweep, structured results and measured execution

## Parameter sweep

The Playground's **Demo: parameter sweep** button loads `website/workbench-demos/parameter-sweep.ostrin.project.json`,
generated from the sources in `website/workbench-demos/parameter-sweep/` by `node scripts/workbench-demo.mjs --write`.

- `model.ostrin`: f(x; c) = x³ − 2x − c, its analytic derivative 3x² − 2, and named wrappers `g1`, `g3`, `g5`, `g7`.
  Ostrin has no closures, so `std.numeric.brent` and `std.numeric.derivative` receive one wrapper per scenario,
  each fixing c. This is a language limitation; the parameter is a real argument everywhere else.
- `sweep.ostrin`: one scenario per c. The root is found by `std.numeric.brent` on [1, 3]. Checks:
  the residual against 10 · |f'(root)| · 1e-12 (Brent stops at an x tolerance of 1e-12), the analytic derivative
  against a central finite difference at 1e-5, and the convergence status.
- `graphics.ostrin`: the comparative figure, built from `model.samples(xs, c)` and the computed roots.
- `main.ostrin`: runs the scenarios, checks a bracket without a sign change, prints one line per scenario, one
  `RESULTS` line, and the figure. Exit status 1 when a check fails.

The reference roots are checked in the tests: for c = 1 the root is (1 + √5)/2; for c = 5 it is 2.0945514815423265.

## Structured results contract (`ostrin.results/v1`)

A program reports results by printing one stdout line that starts with `RESULTS ` and continues with a JSON
document. The Workbench reads only that line, validates it (schema, row fields, types, status values) and
shows it as a table. Free text is never interpreted as a table. Stderr never provides results. A run with
no structured results, or with more than one such line, shows no table and says why.

The table shows the program's own values, unrounded. The **Results JSON** button exports the document exactly as
the program printed it.

## Evidence

- The manifest hashes the raw captured stdout, which contains the `RESULTS` line, and the source files.
- Integrity: recorded bytes match their hashes. This is not reproducibility.
- Repeatability: the sweep is deterministic in this build; the browser tests compare two runs after an export,
  reload and import (equal results). This is observed repeatability on one browser, not a cross-platform claim.
- R3 (full reproducibility) is not claimed.

## Measured execution

Measurement: `tests/browser/playwright.bench.config.mjs` (not part of the CI suite), file
`docs/benchmarks/workbench-delta-win32.json`. Environment recorded in that file: Windows 10.0.26200 x64,
Chromium 153.0.8010.12, measured at commit `119675ed`.

| Measure (ms) | n | median | min – max |
| --- | --- | --- | --- |
| Page load to Run enabled (cold context) | 5 | 183 | 157 – 196 |
| Worker: download + compile of `ostrinc.wasm` (cold) | 5 | 25.6 | 23.8 – 27.4 |
| Worker: instantiate (cold) | 5 | 0.5 | 0.5 – 0.9 |
| Worker: run the sweep (cold) | 5 | 252.8 | 244.0 – 257.3 |
| Worker creation (cold) | 5 | 0.2 | 0.1 – 0.2 |
| Reply transfer to the page | 5 | 0.3 | 0.2 – 0.4 |
| Total in the Worker path (cold) | 5 | 294.2 | 283.4 – 298.4 |
| Run the sweep (warm, Worker reused) | 10 | 150.0 | 126.8 – 174.2 |
| Total in the Worker path (warm) | 10 | 151.0 | 127.9 – 175.6 |
| Cancel an endless program until idle | 5 | 33 | 31 – 49 |
| Recovery run after cancel (wall) | 5 | 215 | 204 – 299 |
| Synchronous editor update while a program runs | 5 | 0.1 | 0 – 0.2 |

Reading of the measurements:

- The **ping** message does not compile WASM and is not reported as preparation time.
- Infrastructure costs are small: Worker creation and transfer are below 1 ms, and reusing the Worker saves the
  compile (about 26 ms) and the start-up.
- The **dominant cost is the program's own execution** (about 150 ms warm, 250 ms cold for the sweep). This is the
  measured bottleneck of the Workbench path. No compiler change is made in this package, because the measurements
  do not yet show which part of the program's execution is avoidable.
- Cancellation stops the Worker in about 33 ms, and the editor stays responsive during a long run (editor updates
  are below 1 ms).

These are wall-clock numbers from one machine and one browser. They are not compared with other machines and no
percentage improvement is claimed.

## Limits

- Ostrin has no closures: each scenario needs a named wrapper.
- Results are one document per run; the sweep does not stream partial results.
- The measurement covers one machine; the benchmark is not part of CI.
- Memory is not measured in this package.
