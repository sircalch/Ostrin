# Changelog

## Unreleased
test

### Language and libraries
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
- Extended the stable hash builtin to structural Option and Result values when
  their payloads are hashable; interpreter and native tags/payload combination
  remain identical.
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
- Extended native ownership into nested block expressions. Their reference-like
  locals now have a scoped cleanup frame, preserve block results across releases,
  and are covered by a native-thread `spawn_scope` leak-check test; task
  registration now happens before a native thread starts.
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
