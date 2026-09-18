# Change Log

All notable changes to the Ostrin VS Code extension are documented here.

## [0.4.0] - 2026-09-17

- Add a real debugger: `ostrinc --dap` runs the same tree-walking
  interpreter under the Debug Adapter Protocol, with actual breakpoints,
  step over/into/out, a call stack, local variables and expression
  evaluation — not a "run and show output" simulation.
- Register an `ostrin` debug type, a debug configuration provider (defaults
  to debugging the file open in the active editor) and a launch snippet
  ("Ostrin: Debug current file").

## [0.3.1] - 2026-09-17

- Find-references and rename in the language server now also scan every
  `.ostrin` file under the workspace root, not only the documents currently
  open in the editor.

## [0.3.0] - 2026-09-17

- Resolve a document's imports (and any `ostrin.toml` dependencies) across
  every open, possibly-unsaved buffer inside the persistent language server,
  instead of analyzing each file in isolation.
- Serve hover, go-to-definition, completion, signature help, find-references
  and rename natively over the LSP protocol, backed by that workspace-wide
  index; the compiler-backed client-side providers now only run as a fallback
  when the server is unavailable.
- Add semantic tokens (`textDocument/semanticTokens/full`) for functions,
  types, enums, enum members, traits, fields, methods and local bindings.

## [0.2.0] - 2026-09-17

- Add a persistent `ostrinc --lsp` backend over standard input/output.
- Connect VS Code document lifecycle events to the LSP server.
- Consume `textDocument/publishDiagnostics` with exact LSP ranges.
- Keep direct compiler-backed providers and stdin diagnostics as a fallback.

## [0.1.8] - 2026-09-17

- Check unsaved Ostrin documents through the compiler's stdin mode.
- Add debounced live diagnostics while editing.
- Discard diagnostics from older document generations after rapid edits.
- Add `ostrin.diagnosticsOnType` and `ostrin.diagnosticsDebounceMs` settings.

## [0.1.7] - 2026-09-17

- Add signature help for Ostrin functions and methods.
- Highlight the active call argument after nested commas and parentheses.
- Resolve generic member signatures before presenting parameter information.

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
