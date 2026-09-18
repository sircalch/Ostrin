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
traits, pattern matching, quantities and a simulated concurrency model.

The compiler suite currently passes **67 integration tests**. Function calls
support named/default arguments, collection lookups preserve `Option<T>`, and
record fields are checked statically. The CLI also exposes JSON Lines
diagnostics with source locations for editor integrations, plus a compiler
index of type members, local bindings and inferred expression types for editor
tooling. The runtime is still synchronous, the standard library is small, and native code generation,
full LSP support and production I/O are planned work.

## Quick start

Requirements: Rust and Cargo.

```powershell
cd compiler
cargo test
cargo run -- --run ..\\examples\\physics.ostrin
```

The compiler currently supports:

```text
ostrinc file.ostrin             # type-check
ostrinc --check file.ostrin     # explicit type-check
ostrinc --json --check file.ostrin # diagnostics for tools and editors
ostrinc --symbols --json file.ostrin # symbols/signatures for editors
ostrinc --members --json file.ostrin # type members/bindings for editors
ostrinc --types --json file.ostrin   # inferred expression types for editors
ostrinc --run file.ostrin       # type-check and run
ostrinc --ast file.ostrin       # print the AST
ostrinc --tokens file.ostrin   # print lexer tokens
```

## Visual Studio Code

The first editor integration is in [`vscode-ostrin/`](vscode-ostrin/). It
recognizes `.ostrin` files, provides syntax highlighting, uses the official
logo, exposes commands to check or run the current file with `ostrinc`, and
shows compiler diagnostics directly in the Problems panel. It also provides
syntax-aware completion, type-aware member completion, hover documentation,
definition navigation, reference search, scoped rename, inferred expression
hover and an outline for top-level declarations. Enable
`ostrin.checkOnSave` to check automatically after saving.
A complete semantic Language Server Protocol implementation and debugging
remain future work.

To install the current extension locally, build the compiler and package the
extension:

```powershell
cd compiler
cargo build
cd ..\vscode-ostrin
npx --yes @vscode/vsce package
code --install-extension .\ostrin-language-support-0.1.3.vsix
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
2. Expand source spans to expression-level precision and add the LSP server.
3. Grow the standard library and runtime.
4. Finish the VS Code language server.
5. Package applications as `.exe` files, then add native code generation.
6. Implement real concurrency, WebAssembly and platform bindings.

See [`CONTEXTO_PROYECTO.md`](CONTEXTO_PROYECTO.md) for the complete project
history and current implementation notes.

## License

Ostrin is released under the [MIT License](LICENSE).
