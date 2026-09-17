# Ostrin Language Support for Visual Studio Code

This is the first VS Code integration for Ostrin. It currently provides:

- `.ostrin` file recognition;
- syntax highlighting;
- bracket and comment configuration;
- the official Ostrin logo;
- `Ostrin: Check Current File`;
- `Ostrin: Run Current File`;
- optional compiler checks on save.

The extension calls the `ostrinc` executable configured in `ostrin.compilerPath`.
The default assumes that `ostrinc` is available on `PATH`. During development,
set the full path to the compiler binary in VS Code settings.

The language server, type-aware completion, hover information, navigation,
formatting and debugging will be added after the compiler exposes structured
diagnostics and a stable LSP entry point.
