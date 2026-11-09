# Workbench Beta: cubic root demonstration

The Playground's **Demo: cubic root** button loads `website/workbench-demos/cubic-root.ostrin.project.json`.
Nothing runs until you press Run. The project is generated from the four `.ostrin` sources in
`website/workbench-demos/cubic-root/` by `node scripts/workbench-demo.mjs --write`; the check mode
fails when the committed document drifts from its sources.

## What the project computes

- `model.ostrin`: f(x) = x³ − 2x − 5 and its analytic derivative 3x² − 2.
- `analysis.ostrin`: the symbolic form (`std.symbolic`), the symbolic derivative, the same derivative by
  reverse-mode autodiff on a hand-built tape (`std.autodiff`), a central finite difference
  (`std.numeric.derivative`), and the root by Brent's method (`std.numeric.brent`).
- `graphics.ostrin`: a `std.viz` figure of the curve, the tangent at the root and the root marker.
- `main.ostrin`: coordinates the checks and returns exit status 0 only when every check passes.

## Checks and tolerances

| Check | Tolerance | Reference |
| --- | --- | --- |
| Root of f in [1, 3] | 1e-12 | 2.0945514815423265 |
| Residual \|f(root)\| | 1e-12 | 0 |
| Symbolic derivative vs 3x² − 2 | 1e-12 | analytic |
| Autodiff derivative vs 3x² − 2 | 1e-12 | analytic |
| Finite difference vs 3x² − 2 | 1e-5 | analytic (an approximation, not an exact value) |

A root bracket without a sign change, [0, 1], is reported as the method's error, not hidden.

## Limits

- The autodiff tape is built by hand: Ostrin has no closures, so reverse mode cannot differentiate an
  arbitrary user function.
- `std.solve` is not used for this cubic: it solves quadratics, rational roots and linear systems.
- The compiler runs on the page's main thread; a long program can block the page.
- The manifest proves integrity of the recorded bytes only. It makes no reproducibility (R3) claim.

## Tests

- `scripts/workbench-demo.test.mjs` runs the project through the real compiler binary (set
  `OSTRINC_BIN` if it is not at `compiler/target/release/`) and checks every value above.
- `tests/browser/workbench-demo.spec.mjs` loads the demo in Chromium, runs it, checks the figure and the
  manifest hashes against the committed sources, and checks that declining a discard keeps the edit.
