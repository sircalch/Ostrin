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

## Good first contribution

New contributors do not need to begin in the type checker. Useful first changes
include:

- add or improve a positive or negative `.ostrin` example;
- document an existing compiler, standard-library or package behavior;
- improve the static website, live examples or website validation;
- make a diagnostic clearer while preserving its error code;
- reproduce an issue with the smallest source file possible.

For website changes, run `node scripts/website-check.mjs` and use the local
WASM playground when the change affects browser execution. For compiler changes,
run `cargo fmt --check` and `cargo test --manifest-path compiler/Cargo.toml`.
Do not claim a feature is available until its source, tests and documentation
agree about the current behavior.

## Pull requests

Please include a concise description, the relevant design decision, tests or
examples, documentation updates and known limitations.

## Branch lifecycle

Use a temporary branch for each focused change and open a pull request against
`main`. GitHub is configured to delete the remote branch after merge. Keep local
branches that contain work not covered by `main` or a merged pull request. Before
removing a local branch, verify its commits are integrated and that no linked
worktree still uses it. A clean merged worktree can be detached at its current
commit before removing the branch; preserve the checkout and its ignored files.
Do not detach or remove branches from worktrees with uncommitted changes or unique
commits. Periodically prune stale remote-tracking refs with `git fetch --prune`.

The current project state and dated repository-hygiene audit are recorded in
[`ESTADO_Y_PLAN.md`](ESTADO_Y_PLAN.md).
