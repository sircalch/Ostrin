# Contributing to Ostrin

Ostrin is being developed as a language-design and implementation project.
Contributions are welcome when they include a clear motivation and a way to
verify the behavior.

## Before changing the language

1. Read the relevant document in [`docs/design/`](docs/design/).
2. Check the current implementation and tests.
3. Describe whether the proposal changes syntax, type semantics, runtime
   behavior or tooling.
4. Add a positive `.ostrin` example and a negative example when applicable.

## Development

```powershell
cd compiler
cargo fmt --check
cargo test
```

Keep the compiler free of warnings and preserve existing behavior unless a
documented language decision intentionally changes it.

## Pull requests

Please include a concise description, the relevant design decision, tests or
examples, documentation updates and known limitations.
