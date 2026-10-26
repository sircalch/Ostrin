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

## Process-memory evidence

The report associates the measured native binary with compiler source revision `b2143dac1ee5573d2da662d1b607f21cc5c9f98f` and tested tree revision `a9e1f3099fe1219cf7f4cac33b9e79548433969d`. It records the compiler version/hash, workflow source hash, fixture and runtime-input hashes, executable hash, exact stdout hash, and seven per-run captures in `validation/m1-pendulum-process-memory.json`.

Across seven runs, the median sampled peak was 5,181,440 bytes of working set and 737,280 bytes of private usage. Every run produced the expected output SHA-256 `ca8f0ab2480660e2153bbd9eafa9aef785411ba41c2eac23a09bff554dc33579`; the smallest run had 110 live-process memory samples. The profiler also checks that the runtime `data.json` bytes match the checked-in fixture and validates the compiler hash against the recorded expected hash.

After building the same native workflow binary, repeat the profile on Windows x64 with:

```powershell
pwsh -File scripts/profile-process-memory.ps1 -BinaryPath "path\to\pendulum.exe" -CompilerPath "path\to\ostrinc.exe" -SourcePath "examples/viz_pendulum_calibration.ostrin" -FixturePath "experiments/pendulum-calibration.inputs.json" -ExpectedCompilerSha256 "ae1b5effd967b04cbe8b4f7dfd91b5e3246915c8d0e9339ab7079a8d19f97d66" -ExpectedOutputSha256 "ca8f0ab2480660e2153bbd9eafa9aef785411ba41c2eac23a09bff554dc33579" -SourceRevision "b2143dac1ee5573d2da662d1b607f21cc5c9f98f" -TestedBinaryRevision "a9e1f3099fe1219cf7f4cac33b9e79548433969d" -OutputPath "validation/m1-pendulum-process-memory.json"
```

The supplied revision IDs are provenance labels, not a signed build attestation. Memory values are sampled lower bounds from one Windows host and one workload. They include process/runtime overhead, are not allocation counts or leak checks, do not establish memory-safety guarantees, and exclude compiler-build memory. The busy-spin profiler changes timing, so it omits elapsed-time results; use the separately recorded benchmark for timing.
