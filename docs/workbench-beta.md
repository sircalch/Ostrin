# Workbench Beta: cubic root demonstration

The Playground's **Demo: cubic root** button loads `website/workbench-demos/cubic-root.ostrin.project.json`.
Nothing runs until you press Run. The project is generated from the four `.ostrin` sources in
`website/workbench-demos/cubic-root/` by `node scripts/workbench-demo.mjs --write`; the check mode
fails when the committed document drifts from its sources.

## What the project computes

- `model.ostrin`: the single definition of f(x) = x³ − 2x − 5, its analytic derivative 3x² − 2, and
  `samples`, which applies f to every point. The other modules use these functions and never repeat the equation.
- `analysis.ostrin`: the symbolic representation (written out on purpose, so that it can be compared with
  the model), the symbolic derivative, the derivative by reverse-mode autodiff on a hand-built tape
  (`std.autodiff`), a central finite difference (`std.numeric.derivative`), and the root by Brent's method
  (`std.numeric.brent`).
- `graphics.ostrin`: a `std.viz` figure built from `model.samples`, with the tangent at the root (from
  `model.df`) and the root marker. The figure never writes the equation itself.
- `main.ostrin`: runs every check and returns exit status 0 only when all of them pass.

## Checks and tolerances

Every comparison counts as a failure unless it is true. `close_to(value, reference, tol)` is
`abs(value - reference) <= tol`, so NaN and infinities always fail.

| Check | Tolerance | Reference |
| --- | --- | --- |
| Symbolic value vs `model.f` at -2, -0.5, 0, 1, 2.5 | 1e-12 | the model |
| Symbolic derivative vs `model.df` at the same points | 1e-12 | the model |
| Root of f in [1, 3] is finite | finite | - |
| Residual \|f(root)\| | 10 · \|f'(root)\| · 1e-12 | Brent stops at an x tolerance of 1e-12 |
| Symbolic derivative at the root | 1e-12 | `model.df` |
| Autodiff derivative at the root | 1e-12 | `model.df` |
| Finite difference at the root | 1e-5 | `model.df` (an approximation) |
| `root_in(0, 1)` returns an error | - | an Ok is a failed check |

The residual bound replaces a fixed 1e-12: a correct root can have a residual slightly above 1e-12
when the slope is large, because Brent's stopping rule bounds the error in x, not in f.

A mismatch between the symbolic representation and the numerical model is reported as
`FAIL symbolic value differs from model.f` and ends the run with exit status 1. Editing `model.ostrin`
therefore changes the figure and the numbers, and any divergence from the symbolic form is visible.

Native and interpreted runs print the same lines; only the line endings differ on Windows.

## Limits

- The autodiff tape is built by hand: Ostrin has no closures, so reverse mode cannot differentiate an
  arbitrary user function.
- `std.solve` is not used for this cubic: it solves quadratics, rational roots and linear systems.
- Ostrin has no `map` over arrays, so `model.samples` uses a while loop that calls `model.f` once per point.
- The compiler runs on the page's main thread; a long program can block the page.
- The manifest proves integrity of the recorded bytes only. It makes no reproducibility (R3) claim.

## Tests

- `scripts/workbench-demo.test.mjs` (7 cases) runs the project through the real compiler binary (set
  `OSTRINC_BIN` if it is not at `compiler/target/release/`). It covers the consistent run, an edited constant
  whose cause is the symbolic mismatch alone, NaN model values, an unexpected root on [0, 1], and the
  figure caption against the printed root.
- `tests/browser/workbench-demo.spec.mjs` loads the demo in Chromium, runs it, checks the figure and the
  manifest hashes against the committed sources, and checks that declining a discard keeps the edit.
