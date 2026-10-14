# Ostrin architecture

*Current implementation overview. Verified state and changing counts live in
[`../ESTADO_Y_PLAN.md`](../ESTADO_Y_PLAN.md); this document describes the
architecture, not release readiness.*

## Compilation and execution

```text
Ostrin source
    │
    ├── lexer → parser → AST → module/package resolution
    │                         │
    │                         ├── type checker → diagnostics, symbols, typed metadata
    │                         │                   ├── HIR → CFG/SSA IR → C → native executable
    │                         │                   └── verified HIR/AST fallback → C
    │                         └── interpreter → runtime values
    │
    ├── compiler → WASI module → browser playground / Node WASI
    └── LSP and DAP → editor tooling
```

The interpreter is the semantic reference for differential testing. The native
backend selects the most advanced supported lowering per function: typed IR/C,
HIR/C, or the retained AST/C path. The fallback is intentional where the typed
backend cannot yet prove semantic or ownership equivalence. Its size is tracked
by the differential suite and `--native-type-report`; see the current count in
[`../ESTADO_Y_PLAN.md`](../ESTADO_Y_PLAN.md).

## Main components

| Component | Responsibility |
|---|---|
| Lexer, parser, AST | Source syntax, spans, and recoverable diagnostics |
| Module/package resolver | Imports, package graph, lockfile, and source discovery |
| Type checker | Static types, dimensions, traits, generics, pattern exhaustiveness, and editor metadata |
| HIR and IR | Lowered control flow, explicit values, typed operations, and ownership markers |
| Interpreter | Reference execution and cooperative deterministic scheduling |
| Native backend | C generation from IR/HIR or verified AST fallback; optional native-thread runtime |
| WASI backend | C/WASI compilation and execution in supported runtimes |
| LSP/DAP and VS Code extension | Diagnostics, navigation, completion, debugging, and editor integration |
| Scientific libraries | Units, arrays, numerical methods, measurements, data, and deterministic visualization |
| Website and playground | Documentation, examples, scientific lab, and browser execution through WASM |

## Ownership and backend boundaries

Managed values use explicit retain/release behavior in generated native code.
The tested combinations and known limits are recorded in
[`ownership-matrix.md`](ownership-matrix.md). `Quantity<D>` has an additional
contract for dynamic unit labels in
[`design/27-ownership-de-quantity-escalar.md`](design/27-ownership-de-quantity-escalar.md).

A feature moves to IR/C only when its lowering preserves interpreter behavior
and has appropriate ownership evidence. Depending on the value and target,
that evidence includes native leak checks, sanitizers, or WASI execution. An
AST fallback count is a migration signal, not permission to weaken those
requirements.

## Scientific validation and reproducibility

Numerical algorithms are checked with analytic cases and residuals in
[`../validation/numeric/matrix_algorithms.json`](../validation/numeric/matrix_algorithms.json),
using the independent runner
[`../scripts/numeric-validation.mjs`](../scripts/numeric-validation.mjs).
Experiment replay and provenance contracts are described in
[`design/26-efectos-cientificos.md`](design/26-efectos-cientificos.md).
These checks are development tooling; external reference libraries are not
production dependencies of the language runtime.

## Source of truth

- [`../ESTADO_Y_PLAN.md`](../ESTADO_Y_PLAN.md): verified implementation state and active plan.
- [`../ROADMAP.md`](../ROADMAP.md): future work and ordering.
- [`VISION.md`](VISION.md): long-term direction.
- [`design/`](design/): specific design decisions and proposals.

