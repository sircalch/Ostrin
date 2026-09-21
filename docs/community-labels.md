# Suggested GitHub labels

This is a proposal for repository organization, not an API action. Labels should
be created or adjusted by maintainers after reviewing how the issue tracker is
actually used.

## Areas

- `compiler` — lexer, parser, checker, HIR or IR
- `runtime` — interpreter, native runtime, ownership or concurrency
- `stdlib` — embedded standard library and core modules
- `scientific` — quantities, arrays, statistics, regression or autodiff
- `plot` — SVG and future visualization work
- `packages` — local packages, manifests and future registry work
- `tooling` — CLI, formatter, LSP or DAP
- `website` — documentation site, playground and discovery
- `docs` — design notes, tutorials and examples

## Work shape

- `good first issue` — bounded contribution with a clear verification path
- `help wanted` — maintainer guidance or implementation help would be useful
- `performance` — measurement is required before making a claim
- `proposal` — design discussion before implementation

The repository should prefer labels that explain ownership and verification. A
label must not imply that a planned feature is already implemented.
