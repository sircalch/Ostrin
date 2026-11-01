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

The period is constructed once per observation and reused as both operands of
`measurements.mul_quantity`. Both operands retain the same independent and shared
source IDs; squaring a reading does not create a second independent observation.

## Scope

This fixture demonstrates this one bounded example. It does not establish general
native support for all dimensional measurements, cross-platform numeric
reproducibility, R3 maturity, or production readiness. The CI replay result is
specific to the archived compiler and input bundle recorded for the run.

## HIR ownership regression

Compiler source `434f3a258315a0ab8a4d2e37d4b3a9b7329befd2` corrects borrowing
through `Option.unwrap()` and `Result.unwrap()` in HIR. Iterating an unwrapped
borrowed list must not release the wrapper's payload reference. Binding or
returning that borrowed payload retains it; concatenating an unwrapped borrowed
string must also preserve its owner. Owned temporary iterables now participate
in return cleanup while remaining live across `continue` and until `break`
leaves the loop.

This was exposed by a macOS `SIGTRAP` on heads `0b011dd` and `13e53ce`.
The archived crash report identifies an invalid free in `ostrin_release` from
`load_analysis`; code-signature verification passed. A later LLDB execution of
the same binary succeeded, so that rerun does not invalidate the original
failure. A smaller repeated-unwrapping program independently reproduced a
heap-use-after-free under Linux ASan before the correction.

The regression test exercises borrowed Option/Result list and string payloads,
repeated access, and temporary-loop completion, return, error, break and continue.
Focused Windows, Linux ASan/leak-check and UBSan checks pass after the correction,
as do ten actual compiled-WASI workflow cases. The source-bound receipts are
`validation/m1-pendulum-hir-ownership-wasi-linux.json` and
`validation/m1-pendulum-hir-ownership-phases-linux.json`. The phase capture ran
alongside the broader compiler suite; its timings are raw observations, not a
controlled speed comparison with previous receipts. The authoritative checkpoint
records fresh-HEAD CI and remaining integration/review requirements. This fix
does not establish general HIR ownership safety for untested methods or contexts.

The input-stage work-count probe in
`validation/m1-input-string-scan-counts.json` embeds its reproduction script.
With identical scientific output, 1668 input bytes required 6,171,620 byte visits
in `String.length` and 1,662,188 code-point steps in indexed access. Adding only
legal leading JSON whitespace to reach 5004 bytes raised those counts to
51,901,508 and 14,310,632 respectively. The distinct units must not be added.
This supports investigating repeated UTF-8 scans in the input stage; it measures
work rather than timing and does not implement or establish a runtime speedup.

## Process-memory evidence

The report associates the measured native binary with compiler source revision `b2143dac1ee5573d2da662d1b607f21cc5c9f98f` and tested tree revision `a9e1f3099fe1219cf7f4cac33b9e79548433969d`. It records the compiler version/hash, workflow source hash, fixture and runtime-input hashes, executable hash, exact stdout hash, and seven per-run captures in `validation/m1-pendulum-process-memory.json`.

Across seven runs, the median sampled peak was 5,181,440 bytes of working set and 737,280 bytes of private usage. Every run produced the expected output SHA-256 `ca8f0ab2480660e2153bbd9eafa9aef785411ba41c2eac23a09bff554dc33579`; the smallest run had 110 live-process memory samples. The profiler also checks that the runtime `data.json` bytes match the checked-in fixture and validates the compiler hash against the recorded expected hash.

After building the same native workflow binary, repeat the profile on Windows x64 with:

```powershell
pwsh -File scripts/profile-process-memory.ps1 -BinaryPath "path\to\pendulum.exe" -CompilerPath "path\to\ostrinc.exe" -SourcePath "examples/viz_pendulum_calibration.ostrin" -FixturePath "experiments/pendulum-calibration.inputs.json" -ExpectedCompilerSha256 "ae1b5effd967b04cbe8b4f7dfd91b5e3246915c8d0e9339ab7079a8d19f97d66" -ExpectedOutputSha256 "ca8f0ab2480660e2153bbd9eafa9aef785411ba41c2eac23a09bff554dc33579" -SourceRevision "b2143dac1ee5573d2da662d1b607f21cc5c9f98f" -TestedBinaryRevision "a9e1f3099fe1219cf7f4cac33b9e79548433969d" -OutputPath "validation/m1-pendulum-process-memory.json"
```

The supplied revision IDs are provenance labels, not a signed build attestation. Memory values are sampled lower bounds from one Windows host and one workload. They include process/runtime overhead, are not allocation counts or leak checks, do not establish memory-safety guarantees, and exclude compiler-build memory. The busy-spin profiler changes timing, so it omits elapsed-time results; use the separately recorded benchmark for timing.

## End-to-end timing evidence

[`validation/m1-pendulum-performance-windows.json`](../validation/m1-pendulum-performance-windows.json)
records a clean-checkout benchmark at commit
`61ea27c43269245b7e9fdc75a2aba465eb1b57af`. It binds the workload source,
checked-in input, Ostrin compiler binary, and generated native executable by
SHA-256. The fixture was staged as `data.json` in an ignored, isolated runtime
directory; the benchmark verifies that staged bytes match the checked-in input
before timing. Seven interpreter and native runs produced matching output, and
five native compilation runs provide a repeated compile-time estimate.

On Windows 11 x64 with an AMD Ryzen 7 5700X, median `ostrinc --compile` process
time was 7,022.824 ms (five samples; standard deviation 45.313 ms). The
end-to-end workflow medians were 209.013 ms interpreted and 14.175 ms native
(seven samples each). These process durations include startup, JSON input,
analysis, uncertainty propagation and SVG generation. They do not isolate the
numerical kernel or predict performance on other machines. The measurement is
one host-specific baseline, not a general speed claim. The separate memory
profile above measures sampled process memory; neither report measures managed
allocation counts or isolates the abstraction overhead of this complete workflow.

Reproduce the timing run with:

```text
cargo build --release --manifest-path compiler/Cargo.toml
node scripts/benchmark.mjs --workloads examples/viz_pendulum_calibration.ostrin --iterations 7 --warmups 1 --compile-iterations 5
node scripts/benchmark-contract-check.mjs
```

For managed allocation counts, append `--allocation-profile` to the benchmark
command. It builds a separate leak-check executable and runs it against fresh
copies of the same input. Each sample must match the timed workload's stdout
identity and report zero live allocations. `nativeAllocations.samples` records
managed live/peak/total counts; they measure neither allocated bytes nor process
RSS. Profile-run times are excluded from timing statistics. This closes the
capture mechanism for managed counts; kernel timing, startup isolation and
scientific abstraction overhead still require separate measurements.

## Managed allocation baseline

`validation/m1-pendulum-allocations-windows.json` records a clean benchmark at
`114e9c6cad7b4c72ca5867910a62b08e43e14291` (SHA-256
`d027768a2b91bd48e128280a6171c90d489b997b6eee9a021878b1adbbec5ee0`).
Seven separately profiled native runs each reported `live=0`, `peak=347` and
`total=12405` managed allocation records. Every profiled run retained normalized
stdout SHA-256 `16853e67b49f29988cc86deb517f417af8c897eb0b4c524e101f74cf3583f9e3`.
The report binds compiler, timed executable, leak-check executable, source and
staged fixture bytes by hash. The compiler source tree is unchanged from the
parent `fcbe5284efffbb30d4acd10309aa04b31d8bdc45`; this package changes profiling
scripts rather than compiler behavior.

For this Windows host, the separate timing samples had a native median of
35.673 ms, interpreter median of 321.889 ms (seven runs each), and native compile
median of 11114.674 ms (five samples). These remain end-to-end durations. The
compiler/source/input identities differ from the earlier timing snapshot, so the
numbers are not a before/after regression or improvement claim. The allocation
runs contribute no timing values. Counts are not bytes/RSS, omit unmanaged
library allocation, and do not attribute costs to numerical analysis versus SVG
rendering. Kernel/startup/abstraction isolation remains open; 12405 total records
provides a measured baseline for a later optimization.

## Tested allocation reduction

The example now constructs each period measurement once and passes the same
value as both operands of its square. Source IDs and first-order self-correlation
are preserved. `validation/m1-pendulum-reuse-windows.json` records clean source
`49374d7a47539d77c03b319c720c41bee6947ede` (report SHA-256
`59b5449ecbcb90fa4a0c6b26d7a90cc6795ff5a39b621671b5c21e6bf27b7643`).
The compiler binary, flags, Node version, host metadata and staged input hashes
match the baseline at `114e9c6`; only the workflow source intentionally differs.
All seven runs report `live=0`, `peak=347`, `total=12291`: 114 fewer managed
allocation records per run (about 0.92%), with no peak-count reduction.
The normalized output hash is unchanged, covering the complete report and SVG.
The focused test also protects the workflow's 7 IR / 2 HIR / 0 AST source-body
profile, eight native replays, missing-input cleanup and WASI C emission.
GCC ASan with leak detection and UBSan both passed for this focused test on Linux.

This is an allocation-count improvement, not a timing improvement. The new
Windows native/interpreter/compile medians were 75.219 / 571.970 / 12482.022 ms;
the host had overlapping build and sanitizer activity during collection.
These slower observed timings remain in the raw report. No causal speedup or
regression conclusion is drawn from this uncontrolled timing comparison.
Startup, kernel/rendering cost and scientific abstraction overhead remain open.

The preceding benchmark CI at PR head `1352bf4` passed on Linux, macOS and
Windows, including the cross-platform provenance check. It ran synthetic merge
revision `295c4cdef3c1406904c3f738ae7e5bde3f7eb1b5`, rather than either local
source revision above. The three original report texts, artifact IDs and hashes
are archived in `validation/m1-pendulum-allocation-ci-baseline.json` (run
[41252031304](https://github.com/sircalch/Ostrin/actions/runs/41252031304)).
Each host supplied seven zero-live samples. Linux/macOS reported 12356 total
records and Windows 12405, all with peak 347. Counts can differ by runtime/host;
matching output for these captures does not establish general platform
reproducibility. That older successful run does not validate the updated PR head.

## Compiled WASI execution check

With the host compiler built and the pinned WASI SDK configured through
`OSTRIN_WASI_CC` and `OSTRIN_WASI_SYSROOT`, run:

```text
node scripts/pendulum-wasi-check.mjs
```

This compiles the actual file-backed workflow to WASI with leak-check enabled,
then executes three baseline replays, one analytical three-row case and six
expected-input-error cases under Node 24 WASI Preview 1. Every case stages its
recorded input in an isolated directory. Successful cases must match the full
interpreted report/SVG; rejected cases must match their exact diagnostic. Every
case requires zero live managed allocations. The receipt records source,
input, compiler and WASM hashes, the tested revision and runner/toolchain inputs
in `target/pendulum-wasi-check/report.json`. The standalone WASI workflow archives
this receipt with the compiled program. This supports this one workflow; it does
not establish complete WASI coverage or global R3.

The local receipt `validation/m1-pendulum-wasi-linux.json` records a clean run at
`b8016ee49b89f0d13852957ae2334f99ff5625c6` with Node v24.21.0 and pinned SDK 34.0.
All three compiled WASI runs matched their interpreted full report/SVG and each
reported live=0, peak=347, total=12291. Receipt SHA-256:
`fe612b7963a3d236cfa1a3ea528d4f6d964e2bf1fa456ea9bfd9acfaa8076bec`. WASM SHA-256:
`f9cae3fe7657cf05a52d632d05cebca4c8f513dba56c7996d7575903f6661fff`. This receipt keeps the final newline in its output hash;
the benchmark snapshots instead trim stdout before hashing. The revision labels
the checked-out test tree; the host compiler and configured C compiler are
identified by actual file hashes, not a signed build attestation. The Windows
worktree was checked from WSL using its Git metadata directory and the same
`core.autocrlf=true` policy used by Git for Windows; raw source/input hashes bind
the bytes actually compiled. The next pushed head still needs its own CI.

## Analysis/rendering boundary

`analyze` now returns `Result<PendulumAnalysis, String>` without rendering or
printing. Its managed arrays/lists cross that boundary together with acceleration
quantities for gravity and both reported uncertainties. The fit still operates
on coherent SI Float arrays; it is not a general unit-aware OLS API.
`load_analysis` owns JSON loading and validation, `render` owns figure/table
formatting, and `print_report` preserves the textual scientific output. At source
`5960371`, the bounded native profile was 10 IR / 2 HIR / 0 AST bodies. Tests preserve
full output parity and also exercise invalid uncertainty, correlation and a
non-positive reading through the returned error path with leak-check enabled.

On Linux with GCC, run the independent phase experiment after building the host
compiler:

```text
node scripts/benchmark-phases.mjs --iterations 7
```

It records instrumented analysis/rendering times, an observed launch-to-entry
latency and a paired uninstrumented process control. It also verifies the phase
call graph and all output/allocation receipts. See `docs/benchmarks.md` for the
measurement scope: launch/exit/transport residuals are not pure loader time,
component medians need not sum, and no instrumentation overhead is subtracted.
Scientific abstraction overhead still requires a controlled comparison using the
same model/algorithm; the current phase timings do not establish that cost.

## Phase measurements and current inventory

`validation/m1-pendulum-phases-linux.json` records seven clean Linux/WSL samples
at `e612d73baf2307296e562502e342ce074794fb74`. SHA-256 of the UTF-8 LF snapshot (Git blob):
`0cfd0baea4fba0c6f200e6a0a5230414f7dd803eacb8d566e8eb71edb49bdfbf`. Every control/profiled run retained identical interpreted
report/SVG output and zero live allocations. Both wall and thread-CPU intervals
partition their respective root interval exactly for each sample; the profiler
observed no unexpected instrumented calls.

On this one host, independent median exclusive wall / thread-CPU times were:

| Phase | Wall ms | Thread CPU ms |
| --- | ---: | ---: |
| Input loading/decoding/validation | 17.130883 | 7.530924 |
| Scientific analysis | 0.548730 | 0.290347 |
| Figure/table rendering | 2.504237 | 2.025366 |
| Text report | 0.032023 | 0.032480 |

The launch-to-entry observation median was 6.652421 ms. Uninstrumented control
and profiled process medians were 31.425554 and 28.405775 ms. These samples do
not establish negative instrumentation overhead or an optimization speedup.
The workspace/input directories are on a Windows-mounted WSL filesystem;
loading includes file access, JSON construction and argument validation. The
wall/CPU observations do not isolate any one of those operations. Clock windows
and individual component medians differ, so tiny CPU/wall differences are not
interpreted as waiting time. Scientific abstraction overhead, allocated bytes,
compiler memory and pure loader time remain unmeasured.

`validation/m1-pendulum-phases-wasi-linux.json` records three clean compiled-WASI
runs of the separated workflow at `59603712aacce52ddd4c1b0fc558377bb8b67972` (UTF-8 LF snapshot
SHA-256 `5f26868efef2c97d3efb735abf656e68467519b3f2c760186883e66d8eed88bf`). All matched the interpreter's complete report/SVG and
reported zero live allocations. This precedes the CPU-profiler-only change;
workflow/standard-library source is unchanged between those two revisions.

`validation/m1-backend-inventory.json` retains both current Windows/Linux
ratchet logs: 8789 IR + 46 HIR, zero AST, 245 measured source modules. The corpus
sums standard-library bodies per importing example and does not count unique
functions. All these receipts describe their exact source trees; the next
pushed head still requires its own CI and review.

## Run the native application and export its figure

This workflow uses development sources; published v0.2.0 binaries do not supply
this full dimensional API. Check out the tested source revision recorded in the
receipts and build with `cargo build --release --manifest-path compiler/Cargo.toml`.
The commands below keep the copied input and generated files under ignored
`target/pendulum-demo/` and leave the checked-in dataset unchanged.

PowerShell 7 on Windows (tested):

```powershell
$pendulumRoot = (Get-Location).Path
$pendulumOutput = Join-Path $pendulumRoot 'target/pendulum-demo'
New-Item -ItemType Directory -Force -Path $pendulumOutput | Out-Null
Copy-Item -LiteralPath experiments/pendulum-calibration.inputs.json -Destination (Join-Path $pendulumOutput 'data.json')
& ./compiler/target/release/ostrinc.exe --compile --leak-check --out (Join-Path $pendulumOutput 'pendulum.exe') examples/viz_pendulum_calibration.ostrin
Push-Location $pendulumOutput
try { & ./pendulum.exe > stdout.txt } finally { Pop-Location }
```

Bash on Linux/macOS (the Linux path is tested):

```bash
pendulumCompiler="${OSTRIN_COMPILER:-$PWD/compiler/target/release/ostrinc}"
pendulumSource="$PWD/examples/viz_pendulum_calibration.ostrin"
pendulumOutput="$PWD/target/pendulum-demo"
mkdir -p "$pendulumOutput"
cp experiments/pendulum-calibration.inputs.json "$pendulumOutput/data.json"
"$pendulumCompiler" --compile --leak-check --out "$pendulumOutput/pendulum" "$pendulumSource"
(cd "$pendulumOutput" && ./pendulum > stdout.txt)
```

From the repository root, this common Node command extracts `figure.svg` and
`report.txt` from the actual program output. Open the SVG in a browser or vector
editor; it contains the fit, uncertainty bars and six-row input table.

```text
node --input-type=module -e 'import {readFileSync,writeFileSync} from "node:fs"; import {extractSvg} from "./scripts/experiment-replay.mjs"; const p="target/pendulum-demo/", s=readFileSync(p+"stdout.txt","utf8").replaceAll("\r\n","\n"); writeFileSync(p+"figure.svg",extractSvg(s.split("\n"),"pendulum-calibration")); writeFileSync(p+"report.txt",s.slice(0,s.indexOf("<svg")));'
```

The native figure is a direct export. The source-backed R2 bundle in
`website/assets/experiments/pendulum-calibration.ostrin-experiment.json` supplies
its normalized figure/source/input manifest and replay contract. Runtime and
compiler hashes for the measured native and WASI paths are in the separate
receipts above; these are scoped observations, not signed build attestations.

## Input validation

`pendulum_calibration_rejects_invalid_inputs_with_typed_messages_and_backend_parity` runs the workflow on eleven invalid `data.json` variants (one and two readings, mismatched lengths, a zero length, a negative period, identical lengths, a negative uncertainty, a correlation outside [-1, 1], a missing `period_s`, malformed JSON and an empty file). Each prints one fixed `input error:` line, the interpreter and the native binary agree byte for byte, and the native run reports `live_allocations=0`. A fit with identical lengths used to reach `linfit` and stop with a runtime error; it is now rejected up front with `at least two distinct lengths are required`. A zero repeatability uncertainty is accepted as an exact input.

Limit: `main` returns `Void`, so an input error is printed on stdout and the process exits with status 0. Scripts must check the output rather than the exit status until the language offers a way to set it.

## Authoritative array input

The current loader reads `parameters.length_m` and `parameters.period_s` directly
through `std.json` arrays. These are the same arrays used by the independent
oracle. The obsolete six-scalar loader and duplicated `*_1` through `*_6` fields
are removed. The checked-in fixture still has six synthetic observations, while
the application supports 3–31 paired readings. Two independent sources per row
plus the ruler/clock offsets must fit the current 64-source covariance profile;
32 pairs are rejected before model construction.

The regression suite includes a three-row analytical dataset with L=[1,4,9] m
and T=[2,4,6] s: T²=4 L and the derived g=π² m/s². It checks interpreter/native
output, a three-row exported table and leak cleanup, plus malformed arrays,
non-numeric elements, mismatched lengths, invalid sigma/correlation and the
source-profile limit. The compiled-WASI checker now runs three baseline replays,
that analytical case and six expected-input-error cases, retaining the actual
input text/hash and allocation counters for every case.

Removing the obsolete helper removes one IR body. The current workflow profile
is 9 IR / 2 HIR / 0 AST; earlier source-bound inventory/timing receipts above are
preserved and do not describe the new loader or its changed input bytes. No
performance change is inferred without a new scoped measurement.

The array-input implementation was validated at clean source `3b60f21a5f5c4c1d557472d8ba1c2d918805e1a4`.
`validation/m1-pendulum-array-input-wasi-linux.json` contains all ten actual WASI
cases, including each input text/hash, expected-error mode, output hash and
zero-live counters. The seven paired wall/CPU captures are preserved separately
in `validation/m1-pendulum-array-input-phases-linux.json`. Baseline report/SVG
identity is unchanged from the earlier scalar-loader capture. Input/source bytes
changed, so the two timing snapshots are not a controlled speed comparison.
The new WSL medians were 43.238786 ms control and 42.628076 ms profiled; exclusive
thread-CPU medians were 7.464024 ms input, 0.285300 ms analysis and 2.232129 ms
rendering. They remain host-specific instrumented observations.

`validation/m1-array-input-backend-inventory.json` retains the current Windows
and Linux ratchet logs: 8788 IR + 46 HIR / 0 AST across 245 source modules.
The earlier inventory at `e612d73` remains archived in its original file.
