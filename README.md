# <img src="assets/ostrin-logo.png" alt="Ostrin" width="72" align="right"> OSTRIN

**A programming language built from first principles.**

Ostrin is an experimental general-purpose programming language focused on
readability, physically meaningful types and safe-by-default concurrency.
The project is currently a design-validation compiler and interpreter written
in Rust.

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

The compiler suite currently passes **47 integration tests**. The runtime is
still synchronous, the standard library is small, and native code generation,
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
ostrinc --run file.ostrin       # type-check and run
ostrinc --ast file.ostrin       # print the AST
ostrinc --tokens file.ostrin   # print lexer tokens
```

## Visual Studio Code

The first editor integration is in [`vscode-ostrin/`](vscode-ostrin/). It
recognizes `.ostrin` files, provides syntax highlighting, uses the official
logo and exposes commands to check or run the current file with `ostrinc`.
Language Server Protocol features, type-aware completion and debugging will be
added as the compiler begins exposing structured source diagnostics.

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
2. Add source spans and structured diagnostics.
3. Grow the standard library and runtime.
4. Finish the VS Code language server.
5. Package applications as `.exe` files, then add native code generation.
6. Implement real concurrency, WebAssembly and platform bindings.

See [`CONTEXTO_PROYECTO.md`](CONTEXTO_PROYECTO.md) for the complete project
history and current implementation notes.

## License

Ostrin is released under the [MIT License](LICENSE).
