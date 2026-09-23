# <img src="assets/ostrin-logo.png" alt="Ostrin" width="72" align="right"> OSTRIN

**A programming language built from first principles.**

Ostrin is an experimental general-purpose programming language focused on
readability, physically meaningful types and safe-by-default concurrency.
The project is currently a design-validation compiler and interpreter written
in Rust.

## Repository language map

GitHub currently reports Rust in its language bar because the compiler and
interpreter implementation live in `compiler/src/*.rs`. That is the
implementation of Ostrin, not a claim that Rust and Ostrin are the same
language. The actual Ostrin source files are the `.ostrin` programs in
`examples/` and the validation cases in `tests/`.

| Path | Role |
| --- | --- |
| `examples/*.ostrin` | Programs written in Ostrin |
| `tests/**/*.ostrin` | Positive and negative Ostrin test programs |
| `compiler/src/*.rs` | Ostrin compiler, checker and interpreter implementation in Rust |
| `vscode-ostrin/` | VS Code tooling for the Ostrin language |

GitHub Linguist does not yet know `Ostrin` as an official language, so the
language bar cannot display it as a new category until Ostrin is accepted into
that upstream catalog. The project will submit a Linguist definition once the
language has enough public usage and its syntax/tooling are stable.

## Why Ostrin?

- **Physical quantities as types.** `5 m / 2 s` carries its dimension through
  the type system, and incompatible arithmetic is rejected before execution.
- **Immutable by default.** Mutable state is explicit and cannot be captured
  directly by a concurrent task.
- **Readable control flow.** Expressions, pattern matching, explicit `try`,
  `and`/`or`/`not`, and significant newlines keep intent visible.
- **Extensible types.** Records, enums, generics, nominal traits, defaults and
  applied implementations form the foundation for reusable libraries.

## Current status

Ostrin is not yet a production compiler. The current implementation includes a
lexer, parser, static checker, interpreter, modules, packages, collections,
traits, pattern matching, quantities and two concurrency modes: deterministic
cooperative scheduling by default, plus opt-in native threads for compiled
programs.

The compiler suite currently passes **198 integration tests, 2 unit tests and 6 differential
interpreter↔native tests**. Function calls
support named/default arguments, scalar and `String` collection lookups preserve `Option<T>`
through the native IR path; concrete records and simple `Option<Record>` values
now use the same IR path with ownership markers, and record fields are checked
statically. `String.to_int()` and `to_float()` now also lower through the IR as
scalar `Result` values, including `Ok`/`Err` matching, core queries, `try` propagation and
inline `try catch` handlers and `map`/`map_err`/`then` combinators; scalar and `String`
`Option.map`/`then` use the same IR path.
The CLI also exposes JSON Lines
diagnostics with source locations for editor integrations, plus a compiler
index of type members, local bindings and inferred expression types for editor
tooling. A persistent language server (`--lsp`) resolves a document's real
import graph across unsaved buffers and serves hover, completion, references,
rename, signature help and semantic tokens over the protocol; a debug adapter
(`--dap`) gives real breakpoints, stepping, a call stack and variable
inspection on top of the same interpreter. A native backend (`--emit-c`/
`--compile`) transpiles a real subset of the language to C and compiles it to
a native executable: plain functions — including generic ones, monomorphized
per concrete instantiation the way C++/Rust templates are, with a fresh C
function generated the first time a given (function, concrete types) pair is
called and reused after that — plain records (heap-allocated, always by
reference, to match the interpreter's identity semantics) with their
non-generic `impl` methods (resolved statically at compile time), plain
non-generic enums with `match` (compiled to a tagged union and a sequence of
`if`s), `dyn Trait` values (a real vtable, the one place in this backend
anything is actually resolved at runtime rather than compile time), and
`List<T>` (heap-allocated, by reference, monomorphized per element type the
same way a generic function is — `length`/`push`/`remove_at`, indexing and
`for x in list`) — over `Int`/`Float`/`Bool`/`String`, recursion,
`if`/`while`/`for <range>`. Lambdas are supported as inline arguments of
`List`'s `map`/`filter`/`fold`/`any`/`all` (expanded to loops, so captures
need no closure object); the project's own `dyn_trait.ostrin` compiles and
runs natively, as do `Option<T>` and `Result<T, E>` (Some/None/Ok/Err,
match, `try`, `find`). Dimensional `Quantity` compiles too (dimension
checked statically, unit carried at runtime exactly as in the interpreter).
Methods (including generic ones and trait defaults) work on records, enums and
quantities; generic records/enums, `Map`/`Set`, operators and `derive`, named
and default arguments, custom iterators, `Option`/`Result` combinators, the
built-in file/parse functions and synchronous `spawn`/channels all compile too.
The default native scheduler remains deterministic for differential testing;
`--native-threads` enables OS threads, blocking channels and deterministic-priority
`select([channels])`; `yield()` advances the cooperative scheduler, and
`Task.cancel()` cancels pending tasks immediately and requests cancellation for running
tasks at safe checkpoints; native-thread file I/O waits through a cancelable worker request,
while the cooperative/WASI runtime remains synchronous. `yield()` is the explicit native
checkpoint, and cancellation never forcibly aborts a libc call.
The standard library is still intentionally small, but now includes embedded Ostrin modules
for math, collections, strings, deterministic dates, portable JSON parsing/serialization,
program arguments, process/environment helpers, generic map queries and configurable float formatting.

The compiler itself also has a reproducible `wasm32-wasip1` release workflow with a pinned
WASI C toolchain, checksums, and smoke tests for standalone and path-dependent programs,
program arguments/environment, file I/O and managed ownership. Emitted programs use cooperative
task cancellation through WebAssembly exception handling, so their WASI host must support that
proposal. The
native release workflow targets Linux x86_64, macOS arm64 and Windows x64; before upload it checks
the tag/version contract, runs `ostrinc --version`, executes `examples/hello.ostrin`, verifies the
archive checksum, and runs both the extracted example and the packaged path-dependency project.
There is still no published release by default: a maintainer must push a matching
`v<compiler-version>` tag. Repository installers are prepared, but they cannot install anything
until such a release exists.

The cooperative runtime avoids thread-only headers unless `--native-threads` is requested. The
same compiler is also deployed as `ostrinc.wasm` for the browser playground, where it runs the
interpreter locally through an in-memory WASI directory. Native C compilation remains a desktop/
WASI toolchain feature rather than a browser capability.

## Quick start

Requirements: Rust and Cargo.

```powershell
cd compiler
cargo test
cargo run -- --run ..\\examples\\physics.ostrin
```

### Install a published release

Release archives are built for Linux x86_64, macOS arm64 and Windows x64 by
`.github/workflows/release.yml`. Once a matching `v<compiler-version>` release exists, the
repository installers download the archive and verify its published SHA-256 before installing
`ostrinc`. They fail clearly when no release exists; this repository does not claim a release is
currently published.

Unix (Linux x86_64 or macOS arm64):

```sh
curl --fail --location https://raw.githubusercontent.com/sircalch/Ostrin/main/scripts/install.sh \
  --output /tmp/ostrinc-install.sh
sh /tmp/ostrinc-install.sh --version 0.1.0
```

Windows PowerShell:

```powershell
$installer = Join-Path $env:TEMP 'ostrinc-install.ps1'
Invoke-WebRequest https://raw.githubusercontent.com/sircalch/Ostrin/main/scripts/install.ps1 -OutFile $installer
powershell -ExecutionPolicy Bypass -File $installer -Version 0.1.0 -AddToPath
```

Use `--install-dir` on Unix or `-InstallDir` on Windows to choose another destination. The
scripts are also usable with `OSTRIN_REPOSITORY`/`-Repository` for a compatible fork.

The compiler currently supports:

```text
ostrinc file.ostrin             # type-check
ostrinc --check file.ostrin     # explicit type-check
ostrinc --json --check file.ostrin # diagnostics for tools and editors
ostrinc --symbols --json file.ostrin # symbols/signatures for editors
ostrinc --members --json file.ostrin # type members/bindings for editors
ostrinc --types --json file.ostrin   # inferred expression types for editors
ostrinc --stdin --check --json --file file.ostrin # check unsaved editor text
ostrinc --lsp                     # language server over stdio
ostrinc --dap                     # debug adapter over stdio
ostrinc --run file.ostrin       # type-check and run
ostrinc --ast file.ostrin       # print the AST
ostrinc --tokens file.ostrin   # print lexer tokens
ostrinc --emit-c file.ostrin       # transpile a supported subset to C
ostrinc --compile file.ostrin      # transpile and compile to a native executable
ostrinc --compile --native-threads file.ostrin # compile with OS threads and blocking channels
ostrinc --run --project path/to/project # use the entry declared by ostrin.toml
ostrinc --fetch --run --project path/to/project # explicitly fetch Git dependencies
ostrinc --locked --run --project path/to/project # require the existing lockfile/cache
```

## Visual Studio Code

The first editor integration is in [`vscode-ostrin/`](vscode-ostrin/). It
recognizes `.ostrin` files, provides syntax highlighting, uses the official
logo, exposes commands to check or run the current file with `ostrinc`, and
shows compiler diagnostics directly in the Problems panel. It also provides
syntax-aware completion, type-aware member completion, signature help, live
diagnostics, hover documentation, definition navigation, reference search,
scoped rename, precise inferred expression hover, document formatting, persistent semantic indexing and an outline for
top-level declarations. Enable
`ostrin.checkOnSave` to check automatically after saving.
The persistent stdio LSP backend resolves a document's real import graph
(including unsaved buffers and `ostrin.toml` dependencies) and serves hover,
completion, definition, signature help, references, rename and semantic
tokens natively; the extension's own compiler-backed providers only run as a
fallback when the server isn't available. A debug adapter (`ostrin` debug
type) launches `ostrinc --dap` for real breakpoints, stepping, a call stack
and variable inspection.

To install the current extension locally, build the compiler and package the
extension:

```powershell
cd compiler
cargo build
cd ..\vscode-ostrin
npx --yes @vscode/vsce package
code --install-extension .\ostrin-language-support-0.2.0.vsix
```

Once installed, VS Code detects `.ostrin` files automatically. The extension
will use `compiler/target/debug/ostrinc.exe` from this repository when it is
available, so a global compiler installation is not required during
development.

## Documentation

The design is documented in [`docs/design/`](docs/design/), including:

- variables, types and quantities;
- functions, closures and generics;
- traits and operator design;
- errors, `Option` and `Result`;
- enums and exhaustive matching;
- ranges and iterators;
- modules and visibility;
- concurrency;
- memory model;
- derive and dynamic traits;
- packages and the consolidated language reference.

The project website is published at [sircalch.github.io/Ostrin](https://sircalch.github.io/Ostrin/)
when GitHub Pages is enabled.

## Roadmap

1. Complete the semantic core and remove unnecessary `Unknown` types.
2. Expand source spans and complete the LSP workspace semantic service.
3. Grow the standard library and runtime.
4. Finish semantic tokens, workspace resolution and debugging in the VS Code client.
5. Publish verified native archives/installers and make project lockfiles reproducible.
6. Extend real concurrency with cancellation, then add WebAssembly and platform bindings.

See [`CONTEXTO_PROYECTO.md`](CONTEXTO_PROYECTO.md) for the complete project
history and current implementation notes.

## License

Ostrin is released under the [MIT License](LICENSE).
