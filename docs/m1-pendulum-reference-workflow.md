# M1 reference workflow: pendulum calibration

`examples/viz_pendulum_calibration.ostrin` is a small end-to-end scientific
workflow intended to exercise the current experimental measurement and
visualization path. It reads the checked-in JSON fixture, assigns dimensions to
the length and period readings, squares the period with `Measurement` arithmetic,
fits `T² = slope · L + intercept`, estimates `g = 4π² / slope`, propagates
first-order source sensitivities, and renders the fit, uncertainty bars and input
table as one SVG. The chart and table are stacked to keep their labels readable
when the gallery scales the figure down on a narrow mobile screen.

The six rows in `experiments/pendulum-calibration.inputs.json` are **synthetic**.
The shared ruler-zero/clock-zero correlation is also a declared synthetic input;
it is not inferred from experimental measurements. The example is a workflow
fixture, not a physical determination of local gravity.

## Uncertainty model

Each length and period has an independent repeatability source. A shared source
ID represents the ruler zero offset across all lengths and another shared ID
represents the clock zero offset across all periods. The dimensional measurement
operations preserve those IDs when periods are squared. The regression then uses
the OLS derivatives

```text
dm/dyᵢ = (xᵢ - x̄) / Sxx
dm/dxᵢ = ((yᵢ - ȳ) - 2m(xᵢ - x̄)) / Sxx
dg/dm  = -g / m
```

and constructs a scalar `Measurement<Float>` whose contributions are already
scaled to one standard deviation. `SourceCovariance` supplies the declared
correlation between the two shared offsets; the other standardized sources are
independent. This is first-order uncertainty propagation. The band in the plot
shows ±2 residual standard deviations; it is not labeled as a confidence or
prediction interval.

The table's marginal `u(T)` combines repeatability and clock-zero uncertainty as
`sqrt(sigma_repeat² + sigma_clock_zero²)`. The uncertainty in `T²` is propagated
through the squared measurement while retaining the shared clock source, so its
contribution remains correlated across readings.

The independent implementation in
`scripts/pendulum-calibration-oracle.test.mjs` uses centered OLS and symmetric
finite differences instead of the Ostrin derivative expressions. The Rust
integration test checks exact interpreter/native report and SVG parity, zero
native live allocations, and WASI C emission. The experiment bundle replay gate
binds the input snapshot and confirms that changing the declared clock-offset
uncertainty changes the figure.

## Scope

This fixture demonstrates this one bounded example. It does not establish general
native support for all dimensional measurements, cross-platform numeric
reproducibility, R3 maturity, or production readiness. The CI replay result is
specific to the archived compiler and input bundle recorded for the run.
