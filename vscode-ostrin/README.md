# Ostrin Language Support for Visual Studio Code

This is the first VS Code integration for Ostrin. It currently provides:

- `.ostrin` file recognition;
- syntax highlighting;
- bracket and comment configuration;
- the official Ostrin logo;
- `Ostrin: Check Current File`;
- `Ostrin: Run Current File`;
- compiler errors in the VS Code Problems panel, with line and column when available;
- optional compiler checks on save.

The extension calls the `ostrinc` executable configured in `ostrin.compilerPath`.
The default assumes that `ostrinc` is available on `PATH`. During development,
set the full path to the compiler binary in VS Code settings.

The compiler check command uses `ostrinc --check --json` internally. The full
language server, type-aware completion, hover information, navigation,
formatting and debugging are future work.
