# Changelog

## Unreleased

### Website and discovery
- Added a pinned Playwright browser suite and a shared CI/Pages verification action. It compiles the
  current compiler to WASM, runs the real homepage program and diagnostic in Chromium, verifies
  mobile/tablet navigation and checks all nine public pages for horizontal overflow at 390 and
  768 px. The new viewport checks also exposed and fixed mobile documentation overflow and the
  tablet-width desktop navigation overflow.
- Added a dedicated 1200×630 Ostrin social preview with the untouched official mark and current
  positioning. All nine public pages now share the large Twitter/OG card metadata, and
  `website-check.mjs` validates the PNG signature, dimensions, per-page image URL, alt text, type
  and consistent titles/descriptions before CI or Pages can publish.
- Refreshed repository-backed public counts to 195 source programs and 192 integration tests,
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
