# Changelog

## Unreleased

### Viz publication manifests

- Figure manifests now preserve the selected explorer state: numeric parameters, an optional 3D
  camera and the share URL, so exported evidence can be reopened with the same view.

### Scalar Measurement contract gate

- Added a source-backed website CI gate for the experimental scalar
  `std.measurements` API, its interpreter/native parity evidence, and its
  explicit boundary before `Array<Measurement<T>>`, Quantity integration,
  covariance, and Monte Carlo support.

### Scalar generic method prepass

- Concrete scalar and record receivers for generic methods now resolve through
  the checker/HIR prepass and reuse the typed IR/C method dispatcher. Quantity
  and dimension-sensitive visualization methods remain on the verified fallback
  until their ownership contract is complete.
- The differential ratchet advances to **6,523 HIR/IR-generated functions
  (6,509 IR + 14 HIR) / 32 AST** after the concrete generic-method slice.

### Quantity visualization ownership status

- Corrected the roadmap and HIR/IR design notes: generic `Figure.unit_line` and
  `Figure.unit_scatter` remain on the verified HIR/AST fallback until scalar
  quantity unit-label ownership is complete.

### Project plan consistency gate

- Added `scripts/project-plan-check.mjs` and its tests to compare the documented
  HIR/IR/AST ratchet with `compiler/tests/differential.rs`, preserve explicit
  `Available`/`Planned` Viz maturity labels, and keep Linguist and R0/R1
  reproducibility evidence visible in the public plan.
- Corrected the differential-test comment that had incorrectly described generic
  Quantity plotting as already lowered through IR/C.

### Scalar Quantity ownership gate

- Added `scripts/quantity-ownership-check.mjs` and its tests to preserve the
  design-27 boundary, the `viz_units.ostrin` native fallback regression, and
  the unimplemented `Ty::Quantity` ownership classification until the complete
  native/WASI/sanitizer matrix exists.

### GitHub Linguist sample provenance

- Added a machine-readable inventory for representative `.ostrin` samples with
  their role, MIT license and canonical source URL.
- Extended the local Linguist gate and tests to validate every sample path,
  origin and license before a future upstream proposal.

### Native IR: fixed-width integer `abs`

- Scalar `abs` now lowers through IR/C for every fixed-width integer type while
  preserving the checked overflow error for the minimum signed value.
  Public website inventory and generated site data now track the 270-test compiler suite.
- `math_functions.ostrin` now reports `ir=4, hir=0, ast=0` and covers
  interpreter/native parity, leak-check, WASI emission, and the minimum-value
  error path. The differential ratchet advances to **6,522 HIR/IR-generated
  functions (6,508 IR + 14 HIR) / 33 AST**.

### Native IR: deterministic `pow` for Float32

- Scalar `pow` now lowers through IR/C for `Float` and `Float32`, using
  `ostrin_dm_pow` and `ostrin_dm_powf` so native code follows the deterministic
  interpreter runtime.
- `detmath.ostrin` now reaches `ir=3, hir=0, ast=0` and verifies both scalar
  precisions with interpreter/native parity, leak-check, and WASI C emission.
  The differential ratchet advances to **6,521 HIR/IR-generated functions
  (6,507 IR + 14 HIR) / 34 AST**.

### Native IR: deterministic `atan2`

- Scalar `atan2` now lowers through IR/C for both `Float` and `Float32`, using
  the existing deterministic runtime helpers `ostrin_dm_atan2` and
  `ostrin_dm_atan2f`.
- `math_functions.ostrin` verifies both precisions with interpreter/native
  parity, leak-check, and WASI C emission. The differential ratchet advances
  to **6,518 HIR/IR-generated functions (6,504 IR + 14 HIR) / 35 AST**.

### Native IR: Float32 collection coercions

- Explicit `Float32` bindings and `List<Float32>`/`Set<Float32>` aggregates now
  insert the checked `Float↔Float32` conversion in IR, preserving
  single-precision output while removing the final AST fallback from
  `float32.ostrin`.
- The differential ratchet advances to **6,513 HIR/IR-generated functions
  (6,499 IR + 14 HIR) / 38 AST** functions.

### Native IR: reflected Complex scalar operators

- `Float + Complex`, `Float - Complex`, `Float * Complex` and `Float / Complex`
  now dispatch to `Complex.radd`/`rsub`/`rmul`/`rdiv` through IR/C while
  preserving receiver and argument order.
- Boolean `to_string()` now uses the same typed IR/C path, allowing
  `examples/numeric_complex.ostrin` to report `ir=3, hir=0, ast=0`.
- Interpreter/native/WASI emission and leak checks remain covered; together
  with the preceding Float32 coercion slice, the differential ratchet advances
  to **6,516 HIR/IR-generated functions (6,502 IR + 14 HIR) / 35 AST**.

### Native IR: checker-resolved generic calls

- Ordinary HIR functions now resolve checker-recorded generic call
  substitutions before the explicit IR is built. Concrete instantiations are
  queued once and their C symbols are visible to the caller, so
  `numeric_units.ostrin` no longer needs an AST entry-point fallback.
- Interpreter/native/WASI parity remains covered, and the differential ratchet
  advances to **6,512 HIR/IR-generated functions (6,498 IR + 14 HIR) / 39 AST**
  functions.

### Native IR: array and quantity string formatting

- `Array<Float32>.to_string()` now uses the generated native show helper from
  IR/C, so `arrays_3d.ostrin` no longer falls back to the AST emitter.
- Scalar `Quantity<D>.to_string()` uses the existing quantity runtime helper
  from IR/C; interpreter/native/WASI output and leak checks remain aligned.
- The differential ratchet now records **6,503 HIR/IR-generated functions
  (6,490 IR + 13 HIR) / 48 AST** functions.

### Native IR: nested optional list ownership

- `List<Option<String>>` now lowers through the typed IR/C path.  Its list
  helpers retain and release only the active `String` payload, preserving the
  existing `None` representation and native ownership contract.
- The regression covers interpreter/native parity, WASI C emission and a
  native leak-check with `live_allocations=0`.

### Native IR: result lists with managed payloads

- `List<Result<T, E>>` now lowers through the typed IR/C path when both result
  payloads are already supported. The generated list helpers retain and
  release only the active `Ok` or `Err` payload.
- The regression covers both managed branches, interpreter/native parity,
  WASI C emission and a native leak-check with `live_allocations=0`.

### Native IR: lists of maps and sets

- `List<Map<K, V>>` and `List<Set<T>>` now lower through the typed IR/C path
  for the scalar key, value and element families already supported by the
  native map and set runtimes.
- The regression covers nested collection ownership, interpreter/native
  parity, WASI C emission and a native leak-check with `live_allocations=0`.

### Native IR: scalar quantity values

- `Quantity<D>.value()` now lowers directly through IR/C, so generic unit-aware
  numeric interpolation no longer falls back to the AST emitter.
- `numeric_units` keeps interpreter/native/WASI parity and lowers its standard
  library source to `ir=81`, `hir=0`, `ast=0`.

### Native IR: Float32 array scalar promotion

- `Array<Float32>` arithmetic with the checker-approved `Float` scalar promotion now uses
  the generated IR/C scalar kernels for both operand orders and for `+`, `-`, `*` and `/`.
- Added interpreter/native/WASI emission parity and native leak-check coverage in
  `native_ir_float32_array_scalar.ostrin`; the measured baseline is now **6,500 HIR/IR
  functions (6,487 IR + 13 HIR) / 51 AST** functions.

### Native IR: scalar record and enum patterns

- Shallow patterns for concrete records with scalar `Int`, sized integer, `Float`, `Bool` and
  `Quantity` fields now lower through IR/C, including generic record fields whose types are
  resolved from HIR metadata.
- Added interpreter/native/WASI parity and native leak-check coverage for nested record patterns;
  fresh record literals passed to ordinary HIR calls are released after the call.
- Plain non-generic enum constructors, unit variants, scalar and fixed-width payload bindings,
  simple `match` tests and scalar literal/range predicates now lower through IR/C. Nested by-value
  enum patterns use the same path; managed payloads and generic enums remain outside this slice.
- The differential ratchet now records **6,500 HIR/IR-generated functions (6,487 IR + 13 HIR) /
  51 AST** functions.

### Native IR: numeric array selection

- `a[mask]`, `not` over `Array<Bool>`, `to`/`until` slices and `where(mask, a, b)`
  now lower through the typed IR/C array kernels for `Int`, `Float`, `Float32` and
  `Bool`; masked selection for `Array<Quantity<D>>` now preserves its unit through
  the same IR/C path, while `where` on `Quantity` remains outside this pass.
- Unary numeric negation for `Array<Int>`, `Array<Float>` and `Array<Float32>` now
  uses the generated array kernels, so composed selection expressions such as
  `where(mask, values * 2, -values)` stay on the IR/C path.
- Scalar `where` operands are materialized as temporary arrays and released inside
  the generated C expression. The new regression covers interpreter/native/WASI
  emission parity and native `live_allocations=0`.

### Native HIR: nested list payload ownership

- `List<Option<String>>` now retains and releases the active `String` payload in
  the generated list helpers.
- HIR tracks managed references nested inside `Option` and `Result` values and
  releases owned temporaries passed to `List.push` and list literals.
- Added interpreter/native/WASI parity and native leak-check coverage; the
  compiler suite now has **260 integration tests**.

### Web: featured scientific workflows

- Added three metadata-driven paths to the Viz page: simulate a system, analyze data and explore
  3D fields. Each reuses recorded Ostrin figures, exposes its `.ostrin` source link and points to
  the existing figure, table, animation or provenance explorer.
- Workflow selection is shareable through `?workflow=...`; static links remain available when
  JavaScript is disabled, and `available`/`experimental` labels preserve the current maturity
  boundary.

### Native IR: trait defaults and quantity-array ownership

- Default method bodies from traits now lower once per concrete implementation,
  removing nine AST fallbacks while preserving checker node identity and
  interpreter/native parity. The verified ratchet before numeric array selection was
  **6,465 IR / 21 HIR / 63 AST fallbacks** across the measured examples. `assert` and `assert_eq` now
  use the same typed IR/C path, including their exact native failure messages.
- Compound unit labels created for `Array<Quantity<D>>` are now owned by the
  array runtime and borrowed by slices. Native leak-check coverage confirms
  `m/s` labels are released exactly once.
- IR-generated programs that use only `Array<Quantity<D>>` now materialize the
  `Array<Float>` backing runtime, including native and WASI C emission.
- `List<Array<Quantity<D>>>` now uses the pointer-backed list runtime over the
  existing `Array<Float>` storage, with interpreter/native/WASI parity and a
  native leak-check proving `live_allocations=0`.
- Native record destructors now release references nested inside `Option` and
  `Result` fields, with leak-check coverage for `Option<String>` payloads.

### Native IR: nested numeric array lists

- `List<Array<Int|Float|Float32|Bool>>` now uses the generated pointer-backed
  list runtime from IR/C, retaining array elements on insertion and releasing
  them from the list destructor.
- `viz_orbits.ostrin` now lowers `orbit` and `main` through IR/C with
  interpreter/native/WASI parity and `live_allocations=0` under the native
  leak check. The prior ratchet was **6,450 IR / 21 HIR / 78 AST fallbacks**;
  the current trait-default pass is tracked above.

### Native IR: borrowed method ownership

- Ownership lowering now treats method receivers and arguments with the same
  borrowed-call contract as ordinary function calls. Native method emitters
  retain managed values they store or return, so the last local reference can
  be released after any method call without a method-name allow-list.
- Visualization builder chains (`Figure`, `Table` and `Scene3D`) therefore
  stay on the ownership-aware IR/C path. Interpreter/native output remains
  identical; the verified ratchet moves to **6,448 IR / 21 HIR / 80 AST
  fallbacks** across the measured examples.
- `viz_hexbin.ostrin` now asserts that its entry point is IR-generated, and
  the visualization suite keeps interpreter/native parity coverage.

### Native IR: normal distribution numerics

- `norm_pdf` and `norm_cdf` now lower through IR/C for scalar `Float` and
  `Array<Float>` inputs, reusing the deterministic runtime helpers and the
  array map implementation with checked `sigma > 0` validation.
- `viz_histogram.ostrin` now has no AST fallback at its entry point. The
  scalar/array PDF and CDF paths have interpreter/native/WASI parity and
  leak-check coverage.

### Native IR: DataFrame error paths and correlation

- `panic(String)` and array `cov`/`corr` now lower through the verified IR/C backend for
  `Float` and `Float32` arrays.
- `examples/dataframe.ostrin` keeps its nested `List<List<String>>` column traversal on IR/C
  with interpreter/native/WASI parity, a checked native panic message and zero leaks.

### Native IR: nested CSV lists

- `parse_csv` now lowers through IR/C for `List<List<String>>`, preserving quoted fields and
  empty records while releasing temporary row buffers, field strings and inner lists.
- `examples/csv_parse.ostrin` verifies interpreter/native parity with `--leak-check`; the WASI
  matrix also compiles and runs the same data program.

### Experimental scalar measurements

- Added `std.measurements` with `Measurement<Float>`, explicit `Exact`/`Standard`/`Unknown`
  states, named sensitivity sources and uncertainty propagation for scale, sum, subtraction,
  product and division. Reusing a source preserves correlated cancellation; unknown uncertainty
  remains explicit.
- Added `examples/measurement_scalar.ostrin` and a Scientific Lab card executed by the same
  `ostrinc.wasm` artifact used in the browser. The API is intentionally function-based and does
  not yet claim `Quantity<T>`, `Array<Measurement<T>>`, Monte Carlo or coverage intervals.
- Added series projections `measurements.values`, `measurements.uncertainties`, `measurements.sum`
  and `measurements.mean`, preserving explicit unknown states and correlated sources. The
  `viz_bars.ostrin` gallery figure feeds those projections into `std.viz.errorbars` and is checked
  for interpreter/native parity.

### Measurement and uncertainty design

- Added a prior-art-reviewed design proposal for `Measurement<T>`, standard uncertainty, correlated
  sources, Quantity integration, reproducible propagation and future uncertainty-aware figures.
  The document is explicitly a proposal; no unimplemented semantics are advertised as available.

### Scientific effects and provenance design

- Added a prior-art-reviewed proposal for effect rows, explicit capabilities, seeded randomness,
  reproducibility levels R0–R3 and W3C PROV-inspired artifacts across native, WASI and web targets.
-  Added the experimental `ostrinc --effect-report` inventory, with text and JSON output for known
  randomness, clock, host I/O, network, concurrency, measurement and provenance sites. It is
  conservative evidence for future checking, not an effect or reproducibility guarantee.
- Added `ostrinc --provenance-report`, a versioned JSON artifact and text summary containing the
  normalized source hash, compiler target, effect sites and explicit replay limitations. The generated
  artifact is checked in as `website/provenance-data.js` and rendered at `website/provenance.html`;
  it remains experimental and does not claim static purity or R2/R3 reproducibility.

### Cross-browser web smoke coverage

- The website verification action now installs Chromium, Firefox and WebKit. The full compiler and
  visualization suite remains on Chromium, while a Firefox/WebKit smoke matrix checks all public
  pages, responsive overflow and keyboard navigation semantics.

### Web accessibility controls
- The shared site navigation now exposes `aria-controls`, a stateful accessible name, keyboard
  focus restoration and Escape-to-close behavior on compact layouts.
- The examples catalogue now exposes filter state with `aria-pressed` and gives its preview tabs
  complete `aria-selected`/`aria-controls` state, roving tab focus and Arrow/Home/End navigation.
  Chromium coverage exercises those interactions alongside the existing live compiler and Viz
  checks.

### Native IR: quantity plotting and managed field stores
- `Array<Quantity<D>>.unit()` and `.values()` now lower through the ownership-aware IR/C
  emitter. Generic `Figure.unit_line` and `Figure.unit_scatter` therefore compile natively
  with the same unit labels, numeric samples and SVG bytes as the interpreter.
- Field stores now count as ownership transfer points for fresh managed values, allowing
  visualization methods that assign generated unit strings to record fields to use IR/C safely.
  The verified ratchet moves to **6,346 IR / 29 HIR / 129 AST fallbacks** across 205 measured
  source modules.

### Native IR: standard-library formatting
- The `format(template, values)` builtin now lowers through IR/C for `String` templates and
  `List<String>` replacements. `std.strings::format_text` therefore keeps native/interpreter
  parity without its AST fallback. The differential baseline is now **6,348 IR / 29 HIR /
  127 AST fallbacks** across 205 measured source modules.

### Viz: capability-detected MP4 export
- Animated figures now expose an MP4 export action when the browser provides an H.264
  `MediaRecorder` codec. Browsers that only support WebM keep the button disabled with an
  explicit explanation; Ostrin never uploads frames or claims an unavailable codec.
- WebM and MP4 share the same Ostrin-produced SVG frame pipeline, loop count and 1×/2× canvas
  quality. The design document and gallery status now describe the capability boundary.

### Web Lab and Viz: shareable scientific states
- The Scientific Lab and Viz explorer now write the selected program, numeric source parameters
  and (for 3D) camera angles to the URL. `Copy link` produces a reproducible browser address
  without a server-side session; reload, direct navigation and browser Back/Forward restore the
  same figure, source and controls.
- The homepage and gallery explain the share contract; browser coverage exercises an initial 3D
  state, navigation history and the clipboard fallback message.

### GitHub Linguist readiness
- Added a machine-readable Linguist proposal at `docs/linguist-language.yml` and a local
  `scripts/linguist-check.mjs` gate that validates the `.ostrin` extension, `ostrinc` interpreter,
  TextMate scope, MIT license and representative samples. The website CI runs both the positive
  repository check and a negative fixture test; upstream submission remains gated on independent
  public usage evidence.

### Native IR: deterministic visualization ids
- Scalar `hash(String)` calls now lower through IR/C and reuse the stable
  `ostrin_hash_string` runtime helper. `std.viz::uid` therefore generates the same
  deterministic SVG ids in the interpreter, native backend and WASM without its shared AST
  fallback.
- The differential ratchet is now **6,131 IR / 124 HIR / 249 AST fallbacks** across 205 measured
  source modules; aggregate-heavy SVG rendering remains the next migration boundary.

### Native IR: visualization renderer grid scan
- `std.viz::render` no longer uses a closure over `Figure.series` to detect grid-backed marks;
  an explicit record-list scan now lowers the complete renderer through IR/C while preserving
  the exact SVG bytes in interpreter, native and WASM execution.
- Together with scalar string hashing, this removes 64 measured visualization fallbacks from the
  previous 6,099 IR / 124 HIR / 281 AST baseline. The verified ratchet is now **6,163 IR /
  124 HIR / 217 AST fallbacks** across 205 measured source modules.

### Native IR: concrete `Self` in trait methods
- HIR now repeats the checker's `Self` substitution for implementation parameter and return types.
  `Complex` trait operators (`add`, `sub`, `mul`, `div` and `equals`) therefore use the existing
  ownership-aware IR/C emitter with the same interpreter/native/WASM behavior.
- The verified ratchet is now **6,248 IR / 125 HIR / 131 AST fallbacks** across 205 measured
  source modules; the numeric standard library reports `ir=77`, `hir=0`, `ast=0` per consumer.

### Native IR: checked numeric casts
- Explicit casts from `Float`/`Float32` and integer families to `Int` or fixed-width integers now
  lower through IR/C with the same truncation, NaN and range checks as the established native
  backend. This migrates color, axis-label and scale helpers used by `std.viz` without changing
  interpreter/native output; out-of-range values still fail with a runtime diagnostic.
- The differential ratchet is now **6,034 IR / 124 HIR / 346 AST fallbacks** across 205 measured
  source modules. Positive and negative casts have native leak-check coverage.

### Native IR: histogram and violin preparation
- `std.viz::Figure.histogram` now lowers its `histogram` and `linspace` calls through IR/C, while
  `Figure.violin` lowers its two-argument `pow` call. Interpreter/native SVG output remains byte-
  equivalent, and both gallery examples assert the migrated standard-library source report.
- The differential ratchet is now **6,099 IR / 124 HIR / 281 AST fallbacks** across 205 measured
  source modules; the remaining fallback is concentrated in aggregate-heavy SVG rendering.

### Native IR: string indexing and codepoints
- `String.char_at`, `String.slice` and `String.codepoint` now lower through the ownership-aware
  IR/C emitter. Their existing runtime bounds and single-codepoint diagnostics remain intact;
  the native example exercises chaining through `codepoint().unwrap()` with zero live allocations.
- The ownership pass recognizes these methods as safe receiver transfer points. The differential
  ratchet first moved to **5,037 IR / 286 HIR / 1,181 AST fallbacks** across 205 measured source
  modules.

### Native IR: joined string lists
- `List<String>.join(separator)` now lowers through IR/C and calls the existing runtime join helper,
  preserving the fresh result's ownership while the list remains borrowed. CSV and table-oriented
  examples therefore use the same native path as scalar string methods.
- The updated ratchet is **5,744 IR / 124 HIR / 636 AST fallbacks** across 205 measured source
  modules.

### Native IR: scalar formatting
- Numeric scalar `to_string()` methods now lower through IR/C for `Int`, `Float`, `Float32` and
  fixed-width integers. `std.numeric.secant` and `std.numeric.newton` therefore keep their
  convergence diagnostics on the typed native path; the per-consumer report is now
  `ir=36`, `hir=8`, `ast=33`.

### Native IR: concrete record and enum methods
- Methods on concrete non-recursive records and enums now attempt the typed IR/C path after
  ownership lowering, including registered operator methods and numeric record APIs. Recursive
  record graphs remain on the verified HIR/AST path until nested ownership and pattern handling
  are complete; this guard keeps `std.json.Value` leak-free and semantically identical.
- The differential baseline is now **5,035 IR / 286 HIR / 1,183 AST fallbacks**. The measured
  numeric consumer report is `ir=72`, `hir=0`, `ast=5` (`Array<Quantity<D>>`: `ir=75`, `hir=0`,
  `ast=6`).

### Native IR: record arithmetic dispatch
- Binary `+`, `-`, `*`, `/`, `==` and `!=` now dispatch closed record operands to their registered
  operator methods in IR/C. `std.numeric.powi` is the first verified consumer, including the
  `Complex * Complex` loop, and keeps interpreter/native output identical.

### Native IR: scalar casts and deterministic math
- The IR/C emitter now lowers explicit `as<Float>`/`as<Float32>` conversions, `abs` over numeric
  arrays, and deterministic numeric builtins including `sin`, `cos`, `ln`, `exp`, `pi`, `eye` and
  their scalar/array forms. This moves numerical integration, RK methods, FFT utilities and polar
  helpers through the typed native path while preserving interpreter/native output.
- At the time of this earlier scalar-cast migration, the differential ratchet recorded 2,840 IR
  functions, 1,136 HIR functions and 2,528 AST fallbacks; the later record-method migration is
  the current baseline documented above.

### Native IR: quantity-array boundary
- `Array<Quantity<D>>` now has a typed IR/C boundary for parameters, ownership, indexing and
  conversion from `List<Quantity<D>>`. Indexed values preserve the array unit tag, so the unit
  integration and interpolation routines in `std.numeric` can run through the same native CFG
  path as scalar arrays while keeping interpreter/native output identical.
- Unit-aware elementwise arithmetic, comparisons, negation, reductions, range slices, `as<unit>`
  conversion and `to_list` now use dedicated IR/C helpers that preserve the unit tag. The
  `quantity_arrays.ostrin` example matches across interpreter, native and WASI. Array rendering
  now consumes intermediate text strings; its native leak-check is down to `live_allocations=14`.
  Scalar unit expressions and array tags that escape into scalar/list values remain tracked as a
  separate ownership follow-up.

### Performance evidence: reproducible benchmark dashboard
- `scripts/benchmark.mjs` now measures eight deterministic workloads spanning scalar loops,
  arrays, quantities, numerical methods, dense real and complex linear algebra, and SVG
  visualization. Interpreter and native output must match before timing is recorded.
- `website/benchmarks.html` publishes the captured commit, compiler, environment, sampling,
  output hashes and medians without presenting a local ratio as a cross-language benchmark.
  `benchmarks/README.md` documents the contract and the scheduled workflow keeps the raw JSON
  available as an artifact.

### Native IR: numeric array builtins and linear algebra
- The native IR emitter now lowers `zeros`/`ones`, `norm`, scalar `abs` and `sqrt` through
  the typed array/runtime helpers. Dense QR and Cholesky therefore compile through IR/C while
  preserving interpreter/native parity; the full example suite moves from 2,856 to 2,508 AST
  fallback functions (2,232 IR, 1,148 HIR).
- `native_ir_numeric_builtins.ostrin` checks generated C markers, execution parity and
  `live_allocations=0`; the Cholesky integration test keeps a per-source native report ratchet.

### Scientific core: LU factorization
- `std.numeric.lu` now provides dense real LU decomposition with partial pivoting, explicit row
  permutation, determinant, reconstruction and triangular solves. Singular and nonsquare inputs
  return `Result<Lu, String>` diagnostics.
- `numeric_lu.ostrin` compares interpreter and native output, while the Scientific Lab's Linear
  Algebra demo now records LU residuals alongside QR and Cholesky. The four aggregate-heavy LU
  methods add 68 measured AST fallbacks across existing numeric consumers; the differential ratchet
  records this allowance explicitly for the next aggregate migration.

### Scientific core: thin SVD
- `std.numeric.svd` now provides a dense real thin singular value decomposition with descending
  singular values, reconstruction, rank, 2-norm condition-number diagnostics and
  least-squares/minimum-norm solving through the
  pseudoinverse. It uses deterministic Jacobi rotations of `AᵀA`, with a documented LAPACK FFI
  path reserved for large or ill-conditioned workloads.
- `numeric_svd.ostrin` and the Scientific Lab verify interpreter/native/WASM parity, including a
  rank-deficient input diagnostic path. The aggregate SVD implementation adds 88 measured AST
  fallbacks, and the condition-number method adds 14 more to the current ratchet until records
  and arrays complete their IR ownership lowering.

### Scientific core: symmetric eigenpairs
- `std.numeric.eigen` now exposes ascending eigenvalues and orthonormal eigenvectors for dense real
  symmetric matrices, with reconstruction and explicit shape/symmetry diagnostics. The new
  `numeric_eigen.ostrin` example and Linear Algebra Lab output verify interpreter/native/WASM parity.
  The aggregate implementation adds 52 measured AST fallbacks until records and arrays complete
  their IR ownership lowering.

### Scientific core: experimental complex numbers
- `std.numeric` now exposes a `Complex` record backed by `Float64`, with rectangular and polar
  constructors, conjugation, magnitude, equality, overloaded arithmetic, reflected scalar
  operations and non-negative integer powers. `numeric_complex.ostrin` verifies interpreter/native
  parity and the Scientific Lab's Complex demo recomputes the same values in WASM.
- Dense complex linear algebra now adds `ComplexVector` and `ComplexMatrix` using explicit real and
  imaginary arrays, with adjoint, matrix/matrix and matrix/vector products and Gaussian solve with
  partial pivoting. `numeric_complex_linear_algebra.ostrin` and the Linear Algebra Lab tab verify
  the solution, zero residual and singular-matrix diagnostic in interpreter/native/WASM.
- Parametric `Array<Complex>`, generic `Complex<T>` promotion, sparse matrices and complex matrix
  decompositions remain future work; the FFT's existing real/imaginary `Spectrum` API is unchanged.

### Scientific core: Cholesky factorization
- `std.numeric.cholesky` now factors dense symmetric positive-definite real matrices into a lower
  triangular `L`, with reconstruction and triangular solves.
- The interpreter/native/WASM parity suite, invalid-matrix diagnostic and Scientific Lab Linear
  Algebra demo cover the new API alongside QR.

### Visualization: publication-quality animation export
- WebM and GIF exports now accept an accessible 1×/2× quality selector. The browser renders the
  selected frame sequence at the requested canvas resolution, names scaled artifacts explicitly
  and reports the output scale in the animation status.

### Visualization: declarative event contracts
- `Figure`, `Table` and `Scene3D` now expose `.bind(channel, event)` for `hover`, `focus` and
  `select`. The contract is embedded in the SVG and the web explorer links indexed marks across
  nested panels while keeping the data and rendering in Ostrin.

### Visualization: standalone interactive HTML export
- The Viz explorer can now download a self-contained HTML document containing the selected
  Ostrin SVG frame, its reproducibility record, accessible zoom/reset controls, pointer panning
  and an embedded crosshair inspector. The export has no site or CDN dependency and keeps the
  rendered data inside the downloaded document.

### Visualization: crosshair data inspection
- The Viz explorer now offers an accessible crosshair mode. Moving over a rendered mark reads its
  existing SVG tooltip into a precise crosshair overlay and live status, keeping data and computation
  in Ostrin while making point inspection easier in dense figures.

### Visualization: interactive legend toggles
- Every labeled `std.viz` series now carries a stable identifier in the SVG. The web explorer exposes
  accessible legend buttons that hide or restore a series in place and report the visible count without
  changing Ostrin's data, source output or export provenance.

### Visualization: Ostrin-driven parameter controls
- The Viz explorer now exposes declarative numeric controls for gallery programs. Each control replaces
  one `name = literal` assignment and reruns that source with `ostrinc.wasm`; the returned SVG, status
  and provenance remain the compiler's output. The 3D surface (`scale`) and Lorenz trajectory (`rho`)
  demonstrate live scientific parameter exploration. Cross-figure event contracts now use
  `.bind(channel, event)` and are handled by the explorer.

### Visualization: animated GIF export
- The Viz explorer can encode Ostrin's static SVG frames as a browser-side GIF with deterministic
  256-color quantization and LZW compression. The selected finite loop count is respected and a GIF
  comment records the figure's reproducibility metadata when available. MP4 remains dependent on a
  browser codec path and is still planned.

### Visualization: PDF publication print
- The Viz explorer now opens a print-ready vector view for the current figure or animation frame.
  The page preserves SVG dimensions, embedded provenance and publication text; the browser's native
  dialog can save it as PDF without sending the figure or data to a server. Pop-up failures are reported
  without affecting SVG/PNG export.

### Visualization: reproducible provenance
- `Figure`, `Scene3D` and `Table` now expose `.provenance(source_hash, data_hash, seed:, compiler:)`.
  The renderer embeds the supplied publication record as `<ostrin-provenance>` metadata in the SVG;
  the gallery reads it back and shows the source/data hashes, seed and compiler for recorded and live
  renders. `viz_provenance.ostrin` covers interpreter/native parity and the browser gallery.

### Visualization: 2D streamlines
- `Figure.streamplot(grid_x, grid_y, u, v, seed_x, seed_y, steps:, step_size:, label:, color:)`
  interpolates a sampled velocity field bilinearly and integrates deterministic streamlines in both
  directions from each seed; paths stop at the domain boundary or at zero velocity and expose SVG
  tooltips. `viz_streamplot.ostrin` is covered by interpreter/native parity and the live WASM gallery.

### Visualization: 2D vector fields
- `Figure.quiver(xs, ys, us, vs, scale:, label:, color:)` draws deterministic sampled vector
  fields with data-unit scaling, triangular arrowheads, zero-vector markers and SVG tooltips;
  `viz_quiver.ostrin` is recorded in the live WASM gallery and checked for interpreter/native parity.

### Visualization: filled contour bands
- `Figure.contourf(z, x0, x1, y0, y1, levels, colormap:, label:)` renders deterministic discrete
  filled contour bands with per-cell value tooltips and a colorbar; `viz_contourf.ostrin` joins
  the live WASM gallery and can layer marching-squares isolines on top.

### Visualization: hexbin density plots
- `Figure.hexbin(xs, ys, xbins, ybins, colormap:, label:)` counts bivariate observations in Ostrin,
  renders deterministic hexagonal cells with count tooltips and a colorbar, and is available in the
  live WASM gallery as `viz_hexbin.ostrin`.

### Visualization: kernel-density violins
- `Figure.violin(position, data, bins:, label:, color:)` computes a deterministic Gaussian KDE in
  Ostrin, renders a mirrored distribution shape with a median marker and adds per-figure tooltips.
  `viz_violin.ostrin` is recorded in the browser gallery and checked in interpreter/native parity.

### Visualization: SVG and PNG publication export
- The Viz explorer can download the exact SVG emitted by Ostrin and rasterize the current figure (or
  selected animation frame) to a 2× PNG through the browser canvas. Export status is announced to
  assistive technology, and the original SVG remains the lossless route for animated figures.

### Visualization: orthogonal volume slices
- `std.viz.slice_xy`, `slice_xz` and `slice_yz` extract planes from `[z, y, x]` `Array<Float>` volumes.
  `Scene3D` renders the three planes as depth-sorted, color-mapped SVG cells with scalar colorbars
  and per-cell tooltips. The new `viz_volume_slices` example is recorded in the live gallery and
  reruns through the interpreter, native backend and WASM.

### Visualization: 3D isosurfaces
- `Scene3D.isosurface(xs, ys, zs, volume, level, colormap:, label:)` polygonizes a scalar volume
  with deterministic marching tetrahedra. The depth-sorted triangle mesh uses flat lighting and
  native SVG tooltips; `viz_isosurface` is now a live, WASM-backed gallery figure.

### Visualization: linked table and figure selection
- `std.viz` emits deterministic `data-viz-index` markers for scatter points and table rows. A new
  `viz_linked_data` gallery program composes both views with `viz.grid`; the browser explorer links
  clicks and keyboard activation across the plot and table, highlights the matching pair, and offers
  an accessible clear-selection action.

### Visualization: live 3D camera exploration
- The Viz explorer now exposes azimuth and elevation sliders for every 3D gallery figure. Moving a
  slider edits the example's `.view(...)` call and reruns that program through `ostrinc.wasm`; the
  resulting SVG replaces the frame and the card records the live camera update. A reset button
  restores the camera declared by the Ostrin source, with a status announcement for each render.

### Visualization: animation controls
- The Viz explorer adds a 0.25×–4× speed control for CSS flip-book and SMIL motion animations.
  Motion SVGs now hide SMIL animation nodes under `prefers-reduced-motion: reduce`, and the
  explorer pauses at the initial frame while announcing the reduced-motion state.
- Playback can run once or for three or five loops, and WebM export repeats the selected number of
  loops. The timeline now follows playback and announces when a finite run completes.

### Visualization: 3D vector fields
- `Scene3D.vector_field(xs, ys, zs, us, vs, ws, color:, scale:)` renders sampled vectors as
  depth-sorted SVG arrows with native `<title>` tooltips. The gallery adds a rotational-field
  example computed entirely by Ostrin.

### Visualization: table explorer
- `std.viz.table` marks its rows, columns and footer in the deterministic SVG it produces. The Viz
  explorer now filters rows and sorts numeric or textual columns in the browser while preserving
  Ostrin as the source of the data and rendering. The live table demo exercises the same WASM output.

### Visualization: continuous motion
- Figures can animate the motion Ostrin computes: `fig.animate(seconds)` with `moving_point`
  (an optional trail follows the distance actually travelled), `rod`, `moving_segment` and `morph`
  (a curve that changes shape). The SVG interpolates with SMIL `<animate>`, which plays wherever
  the SVG is shown, even as an `<img>`, with no scripts. `no_axes()` gives a clean stage.
- New animated gallery programs: `viz_double_pendulum` (rk45, energy drift below 1e-6),
  `viz_orbits` (Kepler orbits, AU and years) and `viz_string` (plucked string, 25 modes). The Lab's
  ODE tab is now an animated pendulum beside its self-drawing phase portrait, recomputed live.
- `ostrinc --run prog | head` no longer panics when the pipe closes; it exits quietly, as native
  binaries do.

### Numerical methods with units
- `numeric.unit_trapz`, `unit_cumtrapz`, `unit_gradient` and `unit_interp` take
  `Array<Quantity<D>>` and return values with the implied unit: speeds in km/h over minutes
  integrate to km, and their gradient is in km/h^2 (convert with `as m/s^2`). Example
  `numeric_units`.
- Dimension generics are inferred inside containers: `fn f<X: Dimension>(xs: Array<Quantity<X>>) ->
  Quantity<X>` now returns the argument's dimension, and a conflicting second binding is E1042.

### Visualization: animation
- `viz.animate(frames, fps)` combines rendered figures or 3D scenes into one SVG that loops them with
  CSS keyframes: no scripts, so it also plays as an `<img>`. Hovering pauses it, reduced-motion
  viewers see the first frame, and ids are renamed per frame. New gallery program
  `viz_animation` (a spreading wave packet, 24 frames).

### Numerical methods: std.numeric 0.1
- New standard-library module `std.numeric`, written in Ostrin: `trapz`, `simpson`, `bisect`,
  `secant`, `newton` (roots return `Result<Float, String>`), `derivative`, `golden_min`, linear
  `interp`/`interp_all`, natural cubic `spline` (`.at`, `.sample`), ODE solvers `rk4` and adaptive
  `rk45` (Dormand–Prince) returning a `Solution` (`t`, `y`, `.component(i)`, `.final_state()`), and
  `fft`/`ifft`/`frequencies`/`amplitude` (radix-2 with a DFT fallback). Design document 24.
- `numeric.frequencies(n, dt)` gives bin k as k / (n dt); for odd `n` it no longer stretches the
  bins to the Nyquist frequency (`frequencies(5, 1.0)` is `[0, 0.2, 0.4]`).
- New examples `numeric_methods`, `viz_ode`, `viz_fft` and `viz_spline` (three more gallery figures)
  and an ODE tab in the Scientific Lab (`lab_ode`: a driven pendulum whose damping, drive and start
  angle recompute live in the browser).
- Native fixes found on the way: fresh arguments passed to closures are released after the call
  (2 891 → 38 live allocations in `numeric_methods`), and HIR-emitted field assignments retain the new
  value and release the old one (a method storing a temporary string in a field was a
  use-after-free).

### Visualization: std.viz 0.1
- New standard-library module `std.viz`, written in Ostrin: 2D figures (line, scatter, area,
  band, error bars, bars, histogram, stairs, reference lines, notes, heatmap with colorbar,
  marching-squares contours), 3D scenes (shaded surfaces with painter's algorithm and directional
  light, wireframes, trajectories colored along time, point clouds) and `viz.grid` layouts, all
  rendered to deterministic SVG. 1-2-5 ticks, legends placed in the emptiest corner, light and
  dark themes, viridis/magma/coolwarm/ocean colormaps. Design document 23.
- Unit-aware plots: `quantity_line`/`quantity_scatter` take `List<Quantity<D>>` and label the axes
  with the data's units (`speed [km/h]`).
- Ten gallery programs (`examples/viz_*.ostrin`) plus `lab_plot` and `lab_surface`; every one is
  byte-identical between the interpreter and native C in the differential test.

- Interaction without scripts: figures embed hover styles and `<title>` tooltips with the values
  of points, bars, error bars and 3D points (`(0.5, 3.609)`, `-1.915 to -1.653: 1`), so they work
  wherever the SVG is opened. The Viz gallery's Explore view shows a figure in a sandboxed frame
  (no scripts) with zoom and scroll-to-pan.

### Units
- Canonical unit algebra: `kg*m/s*m/s` prints `kg*m^2/s^2`, same-dimension units cancel
  (`90 km/h * 30 min` is `45 km`). This also fixes wrong factors for strings such as `km/h*h`,
  which the left-to-right parser read as `(km/h)*h`.
- `as` takes compound units (`v as km/h`, `g as m/s^2`) and checks the dimension (new E1026).
- Unit literals accept `m^2` and negative exponents, and only absorb known unit symbols after
  `*`/`/` (`8 m / t` divides by the variable `t`).
- New units: `um ns us day ug mA umol mL Hz kHz N kN J kJ cal kcal W kW Pa kPa bar atm mmHg C V mV
  ohm`; `atm` is 101 325 Pa (it was 1 Pa). Named dimensions (`Velocity`, `Force`, `Energy`,
  `Pressure`, `Power`, …) expand to base dimensions; diagnostics print `Mass*Length^2/Time^2 (Energy)`.
- `q.value()` and `q.unit()` expose a quantity's number and unit.
- Programs declare units and dimensions (document 01 §3.5): `dimension Money`, `unit coin : Money`,
  `unit ft : Length` with `define 1 ft = 0.3048 m`. Declarations are registered before any
  expression is parsed and reach the native runtime through a generated table
  (`user_units.ostrin`, interpreter/native parity).
- A number divided by a quantity whose units cancel keeps the scale: `2 / (3 km/m)` is
  `0.000666…` (it was `0.666…`), also for arrays.
- `within` compares quantities across units: `6 ft within (1.5 m to 2 m)` was false because the
  raw numbers were compared.
- `Array<Quantity<D>>`: an array with one unit (`array([1 m, 250 cm])`, `linspace(...) as s`,
  `speeds as km/h`). Elementwise `+ - * /` and comparisons follow the scalar rules (dimensionless
  results are `Array<Float>`), `a[i]`, slices and masks keep the unit, and `sum`/`min`/`max`/`mean`/
  `median`/`std`/`percentile` return quantities (`var` squares the unit). Interpreter and native
  output are identical (`quantity_arrays.ostrin`); `std.viz` plots them with `unit_line` and
  `unit_scatter`.

### Compiler
- Scientific notation for Float literals (`6.022e23`, `1e-9`, `2.5E+3`).
- Interpreter: a function body ran in a child of the caller's environment, so `h = ...` in a
  callee rebound the caller's `h`. Functions now get a fresh scope.
- Methods accept named and default arguments in the checker, the interpreter (which bound them by
  position) and natively for generic methods; defaults are typed where they are declared.
- Module rewriting no longer replaces a parameter or local that shares its name with a function
  of the module.
- Native: an owned expression statement was emitted twice (`c.add(1).add(2)` ran each call twice);
  fresh receivers and arguments of method calls are released; an assignment inside a loop or branch
  retains and releases like one at function level (it used to alias a freed value).
- Native: a block whose value was a local of an enclosing block (`if c { line } else { .. }`)
  returned it without retaining it while the enclosing block still released it (use-after-free,
  found by AddressSanitizer once std.viz used the pattern). Only locals of the block itself move
  out now. Generated C silences GCC's false `-Wfree-nonheap-object` on released literals.
- Native memory: fresh `String` operands of `+`, values pushed into lists, receivers of String and
  array methods and array operands are released after use, and releasing an array now frees its
  shape and data. Rendering the Viz gallery natively went from 233 855 to 303 live allocations at
  exit (peak 3 987 instead of 233 886), with byte-identical output; `native_memory_temporaries.ostrin`
  reaches `live_allocations=0`.
- Generic methods infer dimension parameters (`Quantity<X>`), empty `[]` in a record field takes
  the field's type, and `String.slice`/`char_at`/`codepoint` are typed. The HIR and typed-expression
  ratchets drop from 26 and 11 to 19 and 8 unknowns.
- A local binding now shadows a global function or built-in of the same name when called.
  `fn combine(f: fn(Float, Float) -> Float, ...)` next to a global `fn f` failed with E1041 in the
  checker and at runtime in the interpreter, including across packages (the `autodiff` package's
  `gradient2(f: ...)`). Calls through a function value now also check the argument count
  (E1041). Covered by `function_value_shadowing.ostrin` (interpreter/native parity) and
  `function_value_arity_errors.ostrin`.
- Fixed `quantity as unit`: it relabelled the value instead of converting it (`1500 m as km`
  printed `1500 km`), contradicting design document 01 §3.4. The interpreter and the native AST
  emitter now convert through the unit factors (`1.5 km`); `examples/unit_conversion.ostrin` is
  covered by an exact-output test and by the interpreter/native differential test. A pure
  number still receives the unit (`3 as nm`).
- `ostrinc --help` now lists `--test`.

### Website: Ostrin Viz
- New `viz.html` gallery: ten figures recorded by `ostrinc.wasm` (`website/assets/viz/*.svg`,
  checked for drift by `scripts/lab-data.mjs`) with their source and a Run live button that
  recomputes them in the page. The page renders even when the compiler runtime cannot load.
- Scientific Lab: the Plot tab uses `std.viz`, and a new 3D tab rotates and recomputes a shaded
  surface. The homepage gains a Visualization section, and every page links to Viz.
- `lab-data.mjs` runs each program in its own Node process: repeated WebAssembly instances in one
  process crashed Node once the heavier figures were added.

### Website: homepage 3.0 and Scientific Lab
- New homepage: "Scientific-first. General-purpose. Native by design.", with the release status
  derived from `CHANGELOG.md`/`docs/releases/` (`site-facts.mjs` adds `releaseStatus`,
  `releaseDate` and `releaseUrl`) and a hero program recorded from `examples/lab_hero.ostrin`.
- Scientific Lab with eight tabs — Plot, Linear Algebra, Statistics, Monte Carlo, Autodiff,
  Units, Data and Concurrency. Each is an Ostrin program under `examples/` (`lab_*.ostrin`, or
  the `plot_project/lab` and `autodiff_project/lab` projects that use the real `plot` and
  `autodiff` packages). Run and the parameter sliders recompute with `ostrinc.wasm` in the page;
  multi-file projects run through a nested in-memory WASI directory (`website/ostrin-runtime.js`,
  now shared with the playground). JavaScript only substitutes parameters, draws charts from the
  numbers Ostrin prints and shows the SVG the `plot` package emits. Every tab links to its source
  and documentation, explains how the result is produced and states its current limits.
- `scripts/lab-data.mjs` generates `website/lab-data.js` by running every Lab program, the hero
  and the source → HIR → IR → C pipeline example with `website/ostrinc.wasm` under Node WASI.
  Its check mode fails when a source or recorded output drifts, when a static
  `<pre data-output-source>` on any page shows a line its program does not print, when a showcase
  output has no source, or when the Reference lists a flag missing from `ostrinc --help`.
- The showcase autodiff card showed paraphrased output (`f(2) = -1`, `gradient = (-51, 50)`) that
  the program never prints; it now shows the real lines, verified by the check above.
- Documentation surfaces separated: Learn (`docs.html`), Reference (new `reference.html`: design
  documents and CLI), Guides (new `guides.html`: install, projects, testing, native, WASI,
  editor), Examples and Cookbook (new `cookbook.html`, rendered from the Lab data). Shared
  navigation and footer across all twelve public pages.
- `website-check.mjs` now also rejects cited repository paths and GitHub links to missing files,
  release links or install commands that do not match the recorded release, "unpublished"
  wording once a release is recorded, stale Lab sources, and design documents missing from the
  Reference. The shared CI/Pages verification runs `lab-data.mjs` after building the WASM
  compiler, and the browser suite covers the Lab (live parity, parameter recomputation, the SVG
  project) and the Cookbook.

## 0.1.0 — experimental developer release (2026-09-24)

Ostrin 0.1.0 is the first public developer release. It packages the compiler for Linux
x86_64, macOS arm64 and Windows x64, with SHA-256 checksums, shell and PowerShell installers,
the repository examples, and smoke-tested local package dependencies.

This release includes the Rust compiler and interpreter, physical quantities, records, enums,
traits, generics, `Option`/`Result`, pattern matching, collections, deterministic cooperative
concurrency, optional native threads, native C compilation, leak checking, WASI distribution,
the browser playground, LSP/DAP tooling and the initial scientific packages for tables, plots
and forward-mode autodiff.

The release remains experimental. HIR/IR migration, complete ownership lowering, advanced
iterators, GPU execution, reverse autodiff, public package registries, networking and broader
scientific libraries remain in development.

Release notes: [`docs/releases/v0.1.0.md`](docs/releases/v0.1.0.md).

### Release preparation
- Fixed the WASI workflow, which had never succeeded: the pinned SDK URL used the tag
  `wasi-sdk-34.0` instead of `wasi-sdk-34`, and packaging copied `ostrinc` instead of
  `ostrinc.wasm`. The nine-program WASI matrix now passes on GitHub.
- Added `install-check.yml`, which installs a published release with both installers on clean
  Linux, macOS and Windows runners and runs `hello.ostrin` with the installed compiler.
- The documented Windows command now uses `Invoke-WebRequest -UseBasicParsing`; without it,
  Windows PowerShell 5.1 could hang downloading the installer.
- Published release bodies now come from `docs/releases/<tag>.md` instead of generated notes;
  the publish job checks that all three archives and their `.sha256` files are present and
  uses `--verify-tag`. The release is intentionally not a GitHub prerelease, because the
  installers resolve `releases/latest`, which ignores prereleases.
- Fixed the PowerShell installer's repository validation: `-not $Repository -match …` negated
  the string before matching, so malformed `-Repository` values were never rejected.
- `distribution-check.mjs` now also requires the release notes for the compiler version, the
  README install command for that version and a matching CHANGELOG entry.

### Compiler and ownership

- Hardened the native thread `spawn_scope` ownership regression test against
  scheduler-dependent ordering: concurrent `main`/`task` and `scope-body`/`scope-task`
  output is checked as unordered pairs while the join and scope lifecycle markers remain
  ordered. This keeps CI focused on task-handle cleanup and leak freedom.
- Lowered monomorphized generic record iterators through IR/C. `HirProgram` now preserves
  the receiver pattern for `impl<T> Iterator<T> for Cursor<T>`, the IR specializes `T` from
  `Cursor<Int>`, and the C emitter resolves `Cursor__Int` fields and `next()` calls. The new
  regression requires exact interpreter/native parity, two IR-generated functions, no HIR
  fallback or type divergences, and `live_allocations=0`; indirect iterators and managed field
  stores remain conservative fallback cases.
- Lowered compatible local `try ... catch` handlers held in captured closures through `ClosureCall`.
  The handler keeps its typed environment and managed captures instead of being mistaken for the
  mapped error value; `native_ir_try_captured_handler.ostrin` checks interpreter/native parity,
  zero HIR fallback and `live_allocations=0`.
- Extended closure capture discovery through nested lambdas. Free names now propagate through the
  enclosing environment, so nested closures with scalar and `String` transitive captures lower to
  nested IR helpers and typed C environments instead of `opaque lambda`; the regression checks
  interpreter/native parity and `live_allocations=0`.
- Moved compatible captured lambdas into the ownership-lowered IR/C path. `ClosureMake` now
  synthesizes an IR helper and a typed C environment with a destructor; captured managed values are
  retained on construction and released with the closure environment, while managed values
  returned from captured parameters are retained before crossing the callback boundary. The
  closure regression requires exact interpreter/native parity, zero HIR fallback and
  `live_allocations=0`.
- Replaced E1101's block-array-order scan with forward dataflow over reachable CFG paths. Mutually
  exclusive branches no longer create false positives; joins remain conservative, loops iterate to
  a fixed point, and `Phi` operands are checked only on their selecting predecessor edge. The
  `--ownership-check`, interpreter and native entry points share the analysis and retain the
  runtime guard as a safety net.
- Bound dynamic E1101 state to the lifetime of each managed record: the interpreter keeps weak
  identities with periodic stale-entry cleanup, while native C stores the moved bit in the
  allocation registry under its existing mutex. Releasing a record now clears its move state
  naturally, so allocator address reuse cannot poison a new record; regression coverage churns and
  releases records in cooperative and real-thread task execution and requires zero native leaks.
- Corrected channel transfer semantics for mutable records: the sender binding remains statically
  moved, while the `Some(value)` receiver regains access after the dynamic in-flight guard is
  cleared. Pattern bindings now release the transferred native reference on every backend path.
- Moved named function values and indirect calls into the ownership-lowered IR/C path. The native
  emitter now supplies closure-ABI adapters for global functions, so passing a function as a
  parameter and invoking a local function value no longer forces a HIR fallback; complex captured
  shapes still use the established fallback.

### Website and discovery
- Replaced manually duplicated website version and inventory facts with generated `website/site-data.js`.
  `scripts/site-facts.mjs` derives the compiler version, source/design counts and Rust test totals;
  `scripts/website-metadata.mjs` writes or checks the artifact, while every public page consumes it
  before `site.js`. CI and Pages now fail before publication when the generated metadata is stale.
- Added a pinned Playwright browser suite and a shared CI/Pages verification action. It compiles the
  current compiler to WASM, runs the real homepage program and diagnostic in Chromium, verifies
  mobile/tablet navigation and checks all nine public pages for horizontal overflow at 390 and
  768 px. The new viewport checks also exposed and fixed mobile documentation overflow and the
  tablet-width desktop navigation and example-grid overflow.
- Added a dedicated 1200×630 Ostrin social preview with the untouched official mark and current
  positioning. All nine public pages now share the large Twitter/OG card metadata, and
  `website-check.mjs` validates the PNG signature, dimensions, per-page image URL, alt text, type
  and consistent titles/descriptions before CI or Pages can publish.
- Refreshed repository-backed public counts to 196 source programs and 198 integration tests,
  aligned the static version fallbacks, and replaced the stale website audit with a current feature
  inventory. `website-check.mjs` now derives version, example/design counts and Rust test totals,
  then verifies the public JS, every HTML fallback, README, roadmap and audit in CI and Pages builds.
- Added the first website 2.0 foundation without replacing the static site: a real compiler-backed
  playground on the homepage, `?code=` share links, centralized public counters, explicit
  scientific/general-purpose positioning, canonical/OG metadata, JSON-LD, `robots.txt`, `sitemap.xml`
  and a versioned website audit. Existing Pages/WASI infrastructure and visual identity are preserved.
- Added reusable live catalogue demos for quantities, standard-library operations, records/enums and
  concurrency. They share the compiled WASM module and expose real Run, Check, Reset and Copy actions.
- Added `scripts/website-check.mjs` to CI and Pages deployment for local-reference/anchor, metadata,
  sitemap, playground-wiring, source-drift and generated-WASM checks.
- Added a source-backed `showcase.html` for quantities, tables, deterministic SVG and autodiff, with
  explicit maturity labels and links to protecting compiler tests.
- Added `community.html`, a good-first-contribution path, GitHub issue/PR templates and a proposed
  label taxonomy without claiming external chat, Discussions, registry users or community projects.
- Added a 14-step guided learning path to `docs.html`, linking each chapter to real examples, language
  reference anchors, the browser playground or the verified showcase; `available` and `early` labels
  make current implementation maturity explicit.
- Improved the browser playground's real diagnostics: `Check` and failed `Run` request JSON Lines and
  render `OSTRIN-Exxxx`, file, line, column, severity and message accessibly, with a plain-text fallback
  for runtime traps and older output paths. The homepage and live examples share the same behavior.
- Added native editor feedback for the first structured diagnostic: the playground selects the offending
  source line, shows its line/column in the editor header and clears the marker after a valid execution.

### Distribution
- Updated the WASI C backend for wasi-sdk 34: the legacy Ostrin target spelling now maps to
  Clang's `wasm32-wasip1` triple and enables SJLJ for cooperative task cancellation. WASI program
  artifacts document their WebAssembly exception-handling host requirement. Fixed the allocation
  table's pointer hash for 32-bit targets and made the smoke matrix reject over-width shifts.
- Installed and verified the pinned WASI toolchain locally; all five end-to-end program modules
  compiled and ran under Node WASI with exact output checks.
- Extended the executable WASI matrix to six modules with nested `Option`/`Result` consumer chains;
  the same regression requires zero HIR fallback and `live_allocations=0` in native execution.
- Hardened `.github/workflows/release.yml`: tagged releases must match the compiler package version,
  and each native archive is checksum-verified and executed after extraction. The smoke contract covers
  `--version`, `examples/hello.ostrin`, and the packaged local-path dependency project on Linux x86_64,
  macOS arm64 and Windows x64. Manual runs now sanitize branch names in archive paths.
- Package lockfiles now record and validate a deterministic SHA-256 of each dependency's
  `ostrin.toml` and `.ostrin` sources. Locked and normal builds reject local content tampering;
  the decentralized Git/path model remains unchanged.
- Package resolution now follows nested `ostrin.toml` manifests, exposes transitive dependency
  aliases to imports, rejects alias collisions and dependency cycles, and records every resolved
  node with portable paths, package versions and content hashes in `ostrin.lock`.
- Added checksum-verifying Unix and Windows installers for the existing tagged-release contract,
  plus `scripts/distribution-check.mjs` in CI. They install only published, versioned archives;
  no release or public download is claimed until a matching tag is actually published.
- Expanded `std.strings` with cross-backend helpers for trimming, splitting, line extraction,
  blank checks and placeholder formatting, covered by the standard-library test program.
- Added the embedded `std.json` module: a pure-Ostrin DOM with strict number/literal parsing,
  duplicate-key rejection, UTF-16 surrogate decoding, deterministic serialization and explicit
  rejection of `\\u0000`. The `json_library.ostrin` regression matches interpreter/native output
  and ends with `live_allocations=0`.
- Closed native ownership gaps exposed by the JSON block: parsed values transferred into recursive
  lists, string concatenation consumed intermediate buffers, and `String.codepoint()` releases a
  fresh receiver without releasing borrowed bindings. The standard-library suite now covers ten
  tests and keeps the native leak report at zero.
- Added the embedded `std.maps` module with generic, non-mutating `Map<K,V>` helpers for counts,
  emptiness, key membership, fallback lookups and key/value collection extraction. Its coverage
  runs through the interpreter and native backend and explicitly drops extracted lists so the
  native `--leak-check` report remains at zero.
- Added configurable float formatting through `std.strings.format_float(value, digits)`, returning
  `Result<String, String>` for precision outside `0..=18`. The interpreter, HIR/C and IR/C share
  the fixed-decimal contract, including the invalid-precision path, and the standard example keeps
  native ownership at zero.
- Expanded the reproducible WASI program matrix: a single Node WASI checker now compiles and runs
  the standalone example, path-dependent package, `args`/`env` contract, file I/O contract and
  managed `Option`/`Result` ownership example with exact stdout/stderr assertions. The artifact
  checksum covers every program module, while a local emission regression verifies that WASI never
  enables native threads.
- Moved structural `==`/`!=` for supported `List`, `Map`, `Set`, `Option` and `Result` values into
  the native IR/C path. `structural_equality.ostrin` now reports `ir-generated: 1`, keeps exact
  interpreter/native output and finishes with `live_allocations=0`; array comparisons retain their
  separate element-wise scientific semantics.
- Added the embedded `std.args` and `std.env` modules. They provide portable wrappers for program
  arguments, environment lookup, current directory, path joining and file existence; the
  `std_args_env.ostrin` regression compares interpreter/native output and finishes leak-free.
- Lowered the portable process/path builtins (`args`, `env`, `cwd`, `path_join`, `file_exists`) and
  the `clone`/`drop` ownership primitives through native IR/C. The standard process example now
  reports eight IR-generated functions with no HIR fallback, including managed `List<String>` and
  `Option<String>` values.
- Lowered `read_file`/`write_file` through native IR/C, including `Result<Void, String>`, owned error
  strings and checks for seek, short-read, `ferror`, `fputs` and `fclose`. Native-thread tasks now
  wait through detached, reference-counted file workers, so cancellation releases the task at a
  timed checkpoint while the worker finishes and cleans its request; cooperative/WASI execution
  remains synchronous and no libc call is forcibly aborted. The `native_ir_file_io.ostrin`
  regression covers both native modes and finishes with `live_allocations=0`.
- Completed ownership-aware native IR consumers for `Option`/`Result`: managed `unwrap`,
  `unwrap_or`, `ok` and `ok_or` retain the selected payload or fallback before the wrapper and
  arguments are released. Added `native_ir_managed_consumers.ostrin`, which compares interpreter
  and C output across success/error branches and finishes with `live_allocations=0`.
- Extended differential generation with independent managed-wrapper programs. The new
  `generated_managed_wrappers_agree_between_interpreter_and_native_backend` test varies `Some`/`None`
  and `Ok`/`Err`, exercises `unwrap`, `unwrap_or`, `ok`, `ok_or` and wrapper parameters, and checks
  exact interpreter/native output plus `live_allocations=0`; `OSTRIN_FUZZ_SEEDS` widens the run.

### Language and libraries
- Supported `spawn {}` blocks now lower through the native IR/C backend, including immutable
  by-value captures in a generated environment with retain/release; `Task.join()` calls the real
  cooperative or native-thread runtime helper. Supported branch/loop CFGs now use the same callback
  path, and `spawn_scope {}` now opens/drains the native structured-task group inline; nested
  tasks with propagated captures use the same callback ABI, while scope escapes still use the
  verified HIR/AST fallback. The new
  `native_ir_spawn_join.ostrin` regression checks both modes and `live_allocations=0`.
- `Task.cancel()` now also lowers through the native IR/C path for supported `Task<T>` handles,
  calling the typed runtime cancellation helper while preserving last-use ownership. The
  `native_ir_task_cancel.ostrin` regression checks IR selection, cooperative cancellation,
  native-thread compilation/execution and `live_allocations=0`.
- `yield()` now lowers through native IR/C with the runtime's cooperative poll or native-thread
  wait selected at C preprocessing time, followed by the normal cancellation checkpoint. The
  `native_ir_yield.ostrin` regression verifies parity, both execution modes and zero leaks.
- Completing that IR path also closed an ownership edge exposed by real cancellation programs:
  captured `Channel<T>` handles retain one reference per task environment.
- `select([channel, ...])` now lowers through native IR/C for supported channel payloads, using
  the generated `List_Channel_<T>` and `Channel_<T>_try_receive` helpers with the same
  cancellation checkpoint and cooperative/native-thread wait policy as HIR. The
  `native_ir_select.ostrin` regression covers a ready channel in both backends and checks zero
  leaks.
- Channel iteration over concrete payloads now lowers through native CFG/IR: `send`, `close`,
  `receive` and `for` use the generated `Channel_*` runtime helpers, with last-use release of the
  channel handle covered by `examples/native_ir_channel_iterator.ostrin`.
- User-defined concrete record iterators with `Iterator<T>` and `next() -> Option<T>` now lower
  through the native CFG/IR backend, resolve their registered C method and pass differential
  output plus `--leak-check` coverage in `examples/fibonacci.ostrin`.
- Added the embedded `std.time` module: a deterministic proleptic-Gregorian `Date` record with leap-year
  and month validation, day-of-year and ISO-weekday calculations, ordinal conversion, ISO formatting and
  `Result<Date, String>` parsing. `examples/std_tests.ostrin` and `time_library.ostrin` exercise it in both
  the interpreter and native backend; its native leak-check example reports zero live allocations; it does
  not read the system clock or timezone.
- Standard library written in Ostrin and embedded in the compiler: `import std.math`, `std.lists`,
  `std.strings` (`min max clamp gcd sorted reversed contains ...`). Scalars now satisfy `Eq`/`Ord`/
  `Add`... generic bounds, `String` supports `< > <= >=` natively, and functions ending in `return`
  (or an `if`/`else` of returns) type-check.
- `and`/`or` now short-circuit on booleans in the interpreter and the native IR path (masks stay
  elementwise). Previously `x != 0 and 10 / x > 1` failed with a division by zero.
- Remainder operator `%` for `Int`, `Float`, `Float32` and fixed-width integers (truncating, like C;
  division by zero is a runtime error in both backends; not defined for quantities or arrays).
- First-class function values and closures (`fn(Int) -> Int` types, lambdas that
  capture, named functions as values, closures returned from functions), in the
  interpreter, the checker (lambda parameter types inferred from context) and the
  native backend.
- `String` methods (`length trim split lines replace contains starts_with ends_with
  to_upper to_lower to_int to_float`), `List<String>.join`, `parse_csv`, `\r` escape.
- Operator overloading completed: unary `-x` (`neg`) and scalar-on-the-left operators
  (`2.0 * x` via `rmul`/`radd`/`rsub`/`rdiv`), plus mixed right-hand types.
- Fixed-width integers, `Float32`, `Array<T>` with broadcasting, masks and slices,
  `@` matrix product (now also matrix-vector), statistics, regression, `Rng`,
  deterministic elementary functions, `det inv trace eye norm eigvals`.
- `--test` runner (`test_*` functions with `assert` / `assert_eq`).
- Example packages written in Ostrin itself: `tables` (CSV DataFrame with filter and
  group-by), `plot` (SVG scatter/line) and `autodiff` (forward-mode dual numbers).

### Compiler
- Browser playground (`website/playground.html`) running the compiler as WebAssembly; the Pages workflow
  builds `ostrinc.wasm` on each deploy.
- `ostrinc --new DIR` scaffolds a project (manifest, entry module with a test, `.gitignore`).
- Native runtime: live allocations are tracked in a hash table instead of a linked list, so retain/
  release are O(1) (40k live strings: 6 s -> 0.07 s).
- The parser no longer takes exponential time on deeply nested blocks (statements were parsed twice
  per nesting level); mutation and deep-nesting tests guard the front end against panics.
- Fixed a use-after-release in the native IR path: returning a `String`/`List` parameter (or merging
  parameters through `if`) did not retain it. The IR ownership pass now uses CFG liveness (releases on
  dying edges, retains for `Phi` inputs) and `if`/`match`/`break`/`continue` with managed values compile
  from IR without leaks. A differential generator (`OSTRIN_FUZZ_SEEDS`) checks interpreter vs native.
- Fixed a use-after-release in the AST native path (`return words.length()` released `words` before
  evaluating the expression) and leaks for temporary lists in `for` and for reassigned nested locals.
- Closed the remaining temporary-reference leak at the AST/native boundary: fresh managed arguments
  to ordinary and generic calls are released after borrowing, `print` releases temporary managed
  values after rendering, and field reads release temporary records safely. The standard-library
  native regression now runs with `--leak-check` and requires `live_allocations=0`.
- Official formatter: `ostrinc --fmt FILE` prints the formatted source, `--write` rewrites it and
  `--check` fails when it is not formatted. Layout-only and token-verified (idempotent).
- Package resolution now supports explicit `--fetch` for Git dependencies. Normal
  builds remain offline; an explicit fetch clones or updates a deterministic
  project-local cache, checks out the requested tag/revision, and records the
  resolved commit, source, package version, and portable cache path in
  `ostrin.lock`. Existing lockfiles are now read and validated, normal builds
  reuse their exact cached revisions without network access, and `--locked`
  rejects missing or stale cache entries without rewriting the lockfile.
- Added the first native C emitter backed by the explicit HIR→IR lowering. Straight-line
  scalar functions now become C from SSA temporaries in `ir_c.rs`, including integer
  division and scalar printing; unsupported control flow, checked fixed-width arithmetic
  and managed values initially kept the verified HIR/AST fallback. `--native-type-report` now separates `ir-generated` from
  `hir-generated`, and the migration ratchet counts both paths without weakening the
  interpreter/native differential tests.
- Extended that IR C emitter to verified scalar CFGs: branches, nested `if`, recursive calls,
  loop-carried locals and `phi` selection now lower through C labels and predecessor edges.
  The IR builder emits loop phis, records the real predecessor after nested lowering, and the
  verifier rejects missing, duplicated or non-CFG phi inputs; managed values and iterators
  still use the safe fallback.
- Integer `for` ranges now lower directly to the IR C emitter, including inclusive/exclusive
  bounds, positive and negative `step`, zero-step empty ranges, and `break`/`continue` paths.
  `native_ir_ranges.ostrin` compares interpreter/native output for all directions and requires
  zero live allocations under `--leak-check`.
- Moved checked fixed-width integer arithmetic into the IR C emitter. `Int8`/`Int16`/`Int32`
  and `UInt8`/`UInt16`/`UInt32`/`UInt64` preserve overflow checks, division-by-zero checks,
  signed `min / -1` checks, checked negation, comparisons and printing when generated from
  IR; `native_ir_sized.ostrin` locks interpreter/native parity for this family.
- Migrated the first managed family through the IR C emitter: `String` literals, concatenation,
  equality/inequality, calls, branches, `phi`, printing and explicit `retain`/`release`
  markers now generate native C. `native_ir_strings.ostrin` compares interpreter/native output
  and requires `--leak-check` to finish with zero live allocations; aggregates remain on the
  verified HIR/AST fallback.
- Extended the IR C emitter to the scalar-element `List<T>` core: list literals (including
  empty lists), borrowed parameters, calls, indexing, `length`/`count`, `push` and
  `remove_at` now use the generated native list helpers. `native_ir_lists.ostrin` covers both
  `List<Int>` and managed `List<String>` values and finishes with zero live allocations.
- Migrated `String.split()` and `String.lines()` into the IR C emitter. The temporary arrays and
  freshly allocated pieces are released after the native `List<String>` copies them, and the
  differential string-method regression now requires zero live allocations under `--leak-check`.
- Extended the IR C emitter to scalar-key/value `Map<K,V>` and `Set<T>` cores: literals,
  empty collections, borrowed parameters, `set`/`add`/`remove`, membership/count queries,
  and `keys`/`values` now use the generated native hash helpers. `native_ir_maps_sets.ostrin`
  exercises managed string keys/elements and finishes with zero live allocations.
- Extended the IR C emitter with by-value scalar `Option<T>` (`None`, `Some`,
  `is_some`/`is_none`, `unwrap` and `unwrap_or`) and connected `Map.get`/`Map.remove` to
  their `Option_<T>` helpers for scalar payloads. `native_ir_map_options.ostrin` compares
  interpreter/native output and finishes with zero live allocations.
- Extended the IR C emitter to managed `Option<String>` values and `Some`/`None` patterns;
  `Map<String,String>.get/remove` now retain or transfer string payloads correctly. The
  `native_ir_managed_options.ostrin` regression covers dynamic strings, pattern bindings,
  map lookups and `--leak-check`. Concrete generic instances are now specialized before
  lowering and emitted through IR/C when their operations are supported; unsupported shapes
  retain the verified HIR/AST fallback.
- Migrated supported concrete generic functions and methods through the ownership-lowered IR
  and C emitter. The emitter now resolves logical function names to explicit C symbols, so
  ordinary and monomorphized calls (including recursive instances) target the correct body.
  `native_hir_generics.ostrin` now reports 6 IR-generated and 0 HIR-generated functions;
  `native_generic_methods.ostrin` reports 5 IR and 1 HIR function because a generic-record
  return still needs the HIR fallback. Both interpreter/native regressions require
  `live_allocations=0`; generic record/enum shapes remain on the existing fallback.
- Extended the IR C emitter to concrete heap records and `Option<Record>` values, including
  nested field access, `Some`/`None` pattern binds, record destructors and linear
  `retain/release` markers. `native_ir_records.ostrin` compares interpreter/native output
  and finishes with zero live allocations; unsupported generic records and complex nested
  patterns still use the verified fallback.
- Completed the next ownership-IR slice for managed control flow: simple `Phi` joins now
  transfer an incoming owned reference without an unsafe predecessor release, and proven
  loop-carried `Phi` values release their current iteration value after the final safe body
  use. Added `native_ir_managed_loop.ostrin`, which compares interpreter/native output and
  finishes with `live_allocations=0` after repeated dynamic string concatenation.
- Extended the stable hash builtin to structural Option and Result values when
  their payloads are hashable; interpreter and native tags/payload combination
  remain identical.
- Added the first user-defined hash contract: records marked `derive(Hash)` may
  be hashed when every field is recursively hashable, with matching field-order
  hashing in the interpreter and native backend. Records without the derive and
  unsupported fields remain compile-time errors.
- Extended the user-defined hash contract to non-generic enums: every variant
  receives a stable type/variant tag and its fields are combined in declaration
  order, with interpreter/native parity and compile-time rejection otherwise.
- Added structural hashing for `List`, `Map`, and `Set`; list order is significant,
  while map/set insertion order is deliberately ignored and nested payloads are
  checked recursively.
- Connected the same structural hash to `Map`/`Set` bucket lookup in the interpreter
  and native backend, including composite collection keys. User records/enums use
  buckets only with derived `Hash + Eq` and no custom equality method; other cases
  retain a correct linear fallback.
- Enforced `Hash + Eq` statically for every `Map` key and `Set` element, including
  recursive collection payloads and generic bounds; invalid collection types now
  fail in the checker before either backend is selected.
- Extended native ownership cleanup through `while`/`for` iterations and branch
  exits, including `break`/`continue`, in both the AST and HIR emitters.
- HIR block expressions now release managed locals while preserving returned owned
  tails and retaining borrowed tails when necessary.
- The WASI distribution workflow now runs the release compiler under Node WASI
  before packaging it, checking an Ostrin source file through a preopened
  filesystem and validating the module's CLI exit code.
- Native backend: closures, moved-after-send (E1101), strings, arrays, math, `Rng`,
  module-qualified names; interpreter and native output are compared on every example.
- Typed HIR with a verifier (`--hir`), per-node checker/backend type agreement,
  `--typed-report` and `--native-type-report`.
- Native HIR migration expanded through the fourth family: `Option`/`Result`
  constructors, `match`, basic queries, unwrap/coercion helpers and `try`
  propagation are emitted directly from `hir_c.rs`; lambda combinators and
  `catch` deliberately remain on the AST fallback until closures are migrated.
- Native HIR migration now covers the fifth family’s collection core: `List`,
  `Map` and `Set` literals, local collection types, list indexing/iteration,
  basic list/map/set methods and `List<String>.join`; collection combinators
  that receive closures still use the AST fallback.
- Native HIR migration now covers closures and function values: captured
  lambdas, named-function thunks, indirect calls, and `List` combinators
  (`map`, `filter`, `fold`, `any`, `all`, `find`) that invoke closures from HIR.
- Native HIR migration now specializes concrete generic function instances
  from the checker's `CallSubst`: scalar generic functions, `List<T>` indexing,
  `Option<T>` construction and structural methods can emit directly from the
  specialized HIR, while nested/unsupported generic shapes safely retain the
  AST fallback. The differential HIR ratchet is now 90 (98 measured).
- Generic functions emitted from HIR can now call other concrete generic
  instances, including recursive/self calls: the backend queues any newly
  discovered monomorphization, registers its direct C name and prototype, and
  rewrites the specialized HIR call without applying the ordinary source-name
  prefix. Coverage is now 98 measured functions/methods.
- Generic `record<T>` and `enum<T>` instances are now visible to HIR with their
  concrete C names, fields, variants and tags. Specialized generic bodies can
  emit record literals/field access, enum constructors and `match` patterns;
  nested applied type arguments preserve their source-level HIR spelling. The
  differential HIR ratchet is now 110 (117 measured).
- Generic method instances now use their collision-free impl declaration in
  HIR, including generic methods on applied records and nested calls such as
  `container.map<U>(value)`. The existing monomorphization queue remains the
  single source of concrete C bodies; the differential HIR ratchet is now 115
  (119 measured).
- Native memory now goes through one generated runtime API
  (`ostrin_alloc`/`ostrin_calloc`/`ostrin_realloc`/`ostrin_free`). Every generated
  heap block is registered and reclaimed at process exit, including records,
  closures, collections, strings, arrays and runtime buffers. This is the first
  leak-free baseline for native programs; scope-level ARC, ownership checking and
  type-aware destructors remain the next memory milestone.
- Added opt-in native concurrency with `--native-threads`: `spawn` uses POSIX
  pthreads or Windows threads, `join` uses mutex/condition synchronization, and
  channels use blocking condition variables while the deterministic cooperative
  scheduler remains the default. Captured task environments are retained for the
  thread lifetime and released on completion; the native test covers a blocking
  receive and `live_allocations=0`.
- Synchronized the native task registry with its own mutex. Registered tasks now
  hold a runtime reference while they are schedulable, polling takes a temporary
  reference, `join` unregisters completed tasks, and scope/exit draining retires
  task nodes without stale pointers. More complex ownership escapes still await
  complete lowering.
- Native blocking channel receives now use a short timed condition wait, release
  the channel mutex before the cancellation checkpoint, and retry only when the
  task is still live. This lets `Task.cancel()` wake a task waiting on an empty
  channel without pretending to preempt arbitrary external I/O; the interpreter,
  cooperative C backend and `--native-threads` path remain covered by one example.
- Native Unix builds now link `libm` explicitly, so package programs using
  `sqrt`, `round` or related math builtins link successfully on Linux as well
  as macOS.
- Extended native ownership into nested block expressions. Their reference-like
  locals now have a scoped cleanup frame, preserve block results across releases,
  and are covered by a native-thread `spawn_scope` leak-check test; task
  registration now happens before a native thread starts.
- Added `select([channel1, channel2, ...]) -> Option<T>` for deterministic channel
  selection. The interpreter and cooperative native runtime poll in list order;
  `--native-threads` uses mutex-protected nonblocking receives and yields between
  attempts. Closed empty channels return `None`, and the checker requires a
  homogeneous `List<Channel<T>>`.
- Added `Task.cancel() -> Bool` for safe cancellation of pending tasks. It is
  deterministic in the cooperative scheduler, returns `false` after a task has
  started, and deliberately does not force-stop an already-running native thread.
- Added the `yield() -> Void` standard builtin. It advances one pending task in
  the interpreter and cooperative native scheduler, while `--native-threads`
  yields the current OS thread; the behavior is covered by a parity and leak-check test.
- Extended `Task.cancel()` to request cooperative cancellation from running tasks. The
  interpreter checks at statement boundaries, while generated C checks at `yield()` using
  a scoped jump context; cancellation never preempts arbitrary code. Added a parity and
  leak-check example covering a running task that is cancelled before its next statement.
- Hardened task ownership around cancellation: captured environments now have a dedicated
  destructor invoked on normal completion, cooperative cancellation, or pending-task
  disposal. This prevents retained lists, records, and other managed captures from leaking.
- Added structured task groups for `spawn_scope`: cancellation propagates immediately to
  active nested scopes in the interpreter and both C runtimes, child tasks are drained
  before the scope frame is released, and task handles created inside cancelled callbacks
  are reclaimed. `select` now checks cancellation after releasing its temporary channel
  list, with a deterministic interpreter/cooperative/native-thread leak-check regression.
- Hardened task-handle ownership during cancellation: each handle tracked inside a running
  task records whether its local reference was already released, so explicit `drop(handle)`
  cannot be released again when a `longjmp`-based cancellation cleanup drains the task.
- Made the generated cooperative C runtime portable across threadless toolchains:
  `pthread`/Windows thread headers and implementations are now guarded behind
  `OSTRIN_NATIVE_THREADS`, while the default scheduler uses no-op synchronization
  primitives. The generated source has an explicit test for both modes.
- Added target-aware C compilation for `--target wasm32-wasi`. The pinned WASI workflow
  now compiles and runs both `ostrinc.wasm` and a real `hello.wasm` program under Node WASI,
  packaging both modules with a checksums file.
- Added native and WASI smoke coverage for a manifest-selected project with a relative `path`
  dependency; the package module is now included in the WASI distribution artifact.
- Added `--project DIR` package entry-point selection. The compiler now reads the
  manifest's `entry` field when no source path is supplied, and generated
  `ostrin.lock` files sort dependencies and store project-relative paths where
  possible, avoiding checkout-specific absolute paths.
- Added a reproducible WASI distribution workflow for the compiler:
  `wasm32-wasip1` release builds are packaged with a SHA-256 checksum on manual
  runs and version tags. This distributes `ostrinc` itself; program-to-WASM
  code generation remains a separate runtime milestone.
- Added the `hash(value)` standard builtin for scalar keys. The interpreter and
  native backend share stable splitmix/FNV hashing for integers, fixed-width
  integers, booleans, floats, Float32 and strings; unsupported composite values
  are rejected by the checker.
- Added the first HIR-to-IR lowering pass and `ostrinc --ir`. Functions now expose
  explicit temporaries, basic blocks, branches, loop edges, calls, aggregates, phi
  nodes and named opaque instructions for constructs awaiting semantic lowering.
  The IR verifier rejects missing block terminators and invalid CFG targets; the C
  backend remains unchanged until this representation is mature enough to host RC
  and last-use insertion.
- Added the conservative ownership report `ostrinc --ownership-report`. It classifies
  heap-like IR values, records their uses and identifies straight-line last-use candidates
  while marking cross-block and opaque cases as barriers. It is analysis only: no
  `retain`/`release` is emitted until joins, loops and escape behavior are modeled.
- Added the first ownership lowering tools: `ostrinc --ownership-check` reports static
  E1101 use-after-channel-send facts from the IR, and `ostrinc --ownership-ir` emits a cloned
  IR with `release` markers only at modeled linear transfers. Aggregates, calls, phis, loops
  and opaque escapes remain unresolved until their retain/borrow contracts are explicit.
- Integrated static E1101 into normal checking, interpretation and native compilation. The
  move classifier now distinguishes mutable/nested-mutable records from immutable records;
  immutable records are shareable through channels in both backends, while collections remain
  managed by identity. Added `examples/immutable_record_channel.ostrin` and parity coverage.
- Native ownership now has a verified first automatic scope: both HIR and AST C emitters retain
  borrowed aliases, release replaced direct locals, transfer returned owned references, and
  release direct callable locals on every generated return path. Added
  `examples/ownership_auto.ostrin` and leak-check coverage for aliases, reassignment and a
  returned parameter. HIR record construction now uses typed registered destructors and retains
  reference fields, fixing ownership-safe records returned from package functions.
- Native compile tests now use unique temporary C source names, so parallel test processes cannot
  overwrite one another's generated source.
- Added the native ownership runtime ABI (`ostrin_retain`/`ostrin_release`) and the
  `--leak-check` diagnostic mode, which reports live, peak and total allocations before the
  global cleanup safety net runs. The ABI is ready for IR-driven insertion; automatic retain/
  release at every ownership boundary is still the next memory stage.
- Added typed destruction callbacks for native records, lists, maps, sets and channels. Managed
  children are retained when stored and released when their owner is destroyed; `clone` and
  `drop` expose an explicit ownership exercise path shared by the interpreter and native
  backend. `examples/ownership_primitives.ostrin` proves that a cloned list reaches
  `live_allocations=0` under `--leak-check`. Automatic last-use ARC remains pending.
- Added the cross-backend `args()` standard-library builtin. Interpreted programs read
  arguments after the `--` separator, while native programs receive `argc/argv` directly;
  both expose a `List<String>` with identical behavior.
- Added cross-backend standard-library primitives `env(String) -> Option<String>` and
  `path_join(String, String) -> String`, with matching interpreter/native behavior.
- Added structural equality for `List`, `Map`, `Set`, `Option` and `Result` in both backends;
  maps and sets compare by contents rather than insertion order, including nested values.
- Added cross-backend formatting and filesystem primitives: `format(template, values)`, `cwd()` and
  `file_exists(path)`, with a bounded `{}` placeholder contract and platform-aware current-directory
  lookup in native programs.
- Replaced scalar-key `Map` linear lookup with an insertion-order-preserving hash index in the
  interpreter and native C runtime. Updates, removals and rehashing are covered by a 51-entry
  cross-backend stress test; unsupported composite keys retain a correct linear fallback.
- Extended the same indexed representation to scalar-key `Set` membership, duplicate detection and
  removal while preserving insertion order and the existing set API.
- Lowered `match` and `try` into explicit IR control flow: pattern tests,
  pattern bindings, guarded-arm branches, try success/error blocks and phi convergence.
  The IR now keeps these families semantic instead of representing them as opaque operations;
  closures and concurrency remain the next control-flow families.
- Lowered the concurrency surface into explicit IR operations: task regions for `spawn`,
  `channel`/`send`/`receive`/`close` and `task_join`. This is the
  compiler-side contract for the runtime. Both interpreter and C backend now have the same
  deterministic cooperative semantics: `spawn` is deferred, `join` runs the task,
  `spawn_scope` drains child tasks, and channel waits pump runnable tasks. Native OS threads,
  blocking cross-thread channels, cancellation and `select` remain future work.
- Module loader now rewrites types in signatures, fields, variants and annotations
  (a `record` from another module can be used as a type).
- CI on Linux, macOS and Windows.

### Project
- Added the official Ostrin geometric logo and initial brand guide.
- Added the first VS Code extension with `.ostrin` recognition, syntax
  highlighting and compiler commands.
- Added the public-project README, contribution guidelines, code of conduct,
  license and website foundation.
- Continued strengthening generic trait defaults, applied implementations and
  static concrete method checking.

## 0.1.0 — prototype

- Lexer, parser, AST and static type checker implemented in Rust.
- Interpreter with records, enums, collections, traits, modules and packages.
- Physical quantities with dimensional arithmetic and conversions.
- Simulated task/channel concurrency model with mutation-safety checks.
- 47 passing compiler integration tests.
