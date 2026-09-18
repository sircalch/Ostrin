# Ostrin Language Support for Visual Studio Code

This is the first VS Code integration for Ostrin. It currently provides:

- `.ostrin` file recognition;
- syntax highlighting;
- bracket and comment configuration;
- the official Ostrin logo;
- `Ostrin: Check Current File`;
- `Ostrin: Run Current File`;
- compiler errors in the VS Code Problems panel, with line and column when available;
- keyword, type, unit and standard-library completion;
- type-aware member completion for known local bindings and declared types;
- signature help for functions and methods, including the active argument;
- hover documentation and an outline for top-level declarations;
- precise inferred expression ranges and types in hover when the compiler can resolve them;
- conservative `Format Document` support for Ostrin blocks;
- workspace-scoped semantic caching that invalidates while editing;
- basic definition navigation for indexed user declarations and members;
- reference search for unique top-level symbols across the workspace;
- scoped rename for local bindings and unique top-level symbols;
- optional compiler checks on save.

The extension calls the `ostrinc` executable configured in `ostrin.compilerPath`.
The default first looks for a locally built compiler in
`compiler/target/debug/ostrinc.exe` (or the platform equivalent) when the
workspace is the Ostrin repository, and otherwise falls back to `ostrinc` on
`PATH`. You can always set the full path manually in VS Code settings.

## Install from the repository

VS Code recognizes `.ostrin` files as soon as this extension is installed. From
the repository root:

```powershell
cd vscode-ostrin
npx --yes @vscode/vsce package
code --install-extension .\ostrin-language-support-0.1.7.vsix
```

After restarting or reloading VS Code, opening any `.ostrin` file selects the
Ostrin language automatically. Build the compiler first from the repository
root with `cd compiler; cargo build` to enable diagnostics, hover data and
type-aware completion without configuring a global `ostrinc` command.

The compiler check command uses `ostrinc --check --json` internally, while the
background semantic index also consumes `ostrinc --types --json` for inferred
expression hover. The cache is invalidated while a document is dirty and
rebuilt after saving. The current editor providers are intentionally lightweight:
cross-file navigation is conservative when two modules expose the same short
name. Signature help is driven by the same compiler-produced signatures used by
completion and hover. A full semantic language server with persistent state and
debugging is future work.
