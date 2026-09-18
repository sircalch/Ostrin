# Change Log

All notable changes to the Ostrin VS Code extension are documented here.

## [0.1.6] - 2026-09-17

- Preserve complete start/end ranges for compiler-inferred expressions.
- Select the smallest expression containing the cursor for hover results.
- Return precise hover ranges for nested expressions.

## [0.1.5] - 2026-09-17

- Add workspace-scoped semantic cache lifecycle management.
- Invalidate stale semantic data while a document is being edited.
- Ignore out-of-order compiler results and clear indexes when documents close.

## [0.1.4] - 2026-09-17

- Add conservative document formatting for Ostrin blocks and indentation.
- Register `Format Document` support for `.ostrin` files.
- Preserve strings, comments and operator text while normalizing indentation.

## [0.1.3] - 2026-09-17

- Show compiler-inferred expression types in hover documentation.
- Refresh expression metadata alongside symbols, members and local bindings.
- Add source-location wrappers in the compiler AST without changing runtime
  semantics.

## [0.1.2] - 2026-09-17

- Extend reference search and rename to unique top-level symbols across the
  workspace.
- Load semantic symbol and member indexes together for editor navigation.
- Publish inferred expression types with source positions for editor hover.
- Keep compiler-generated expression metadata separate from diagnostics.
- Keep ambiguous cross-module names local until module ownership is available.
- Preserve declaration scope rules for local bindings.

## [0.1.1] - 2026-09-17

- Add reference search for local bindings.
- Add scoped rename edits for local bindings.
- Index top-level symbols alongside members for editor navigation.

## [0.1.0] - 2026-09-17

- Recognize `.ostrin` files automatically.
- Add Ostrin syntax highlighting and editor configuration.
- Add compiler commands, structured diagnostics and optional check-on-save.
- Add type-aware completion, hover, outline and definition navigation.
- Add the Ostrin logo as the `.ostrin` file icon in VS Code.
- Package the extension with the official Ostrin logo.
