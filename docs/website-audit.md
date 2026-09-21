# Ostrin website audit

*Cut: 2026-09-21 · repository `main` at `4610095`.*

This audit is the starting point for the website and discovery work. It records what is
already real so the public site can expose it without inventing maturity or replacing the
existing implementation.

## Existing surfaces

| Surface | Evidence | Current state |
| --- | --- | --- |
| Homepage | `website/index.html` | Branded landing page with quantities, language principles and ecosystem links; no live compiler surface yet |
| Example catalogue | `website/examples.html` | Filters, source links and illustrative panels; one panel still says the preview is static |
| Browser playground | `website/playground.html`, `website/playground.js` | Real `ostrinc.wasm` compiled by `pages.yml`, using WASI in memory; supports Run, Check, Test and Format |
| Documentation | `website/docs.html`, `docs/design/` | Quick start and design archive; 22 design documents are present |
| Toolchain status | `website/ecosystem.html` | Compiler, native backend, WASI, packages and VS Code are described; several counters are stale |
| Deployment | `.github/workflows/pages.yml` | Builds `wasm32-wasip1` and copies `ostrinc.wasm` into the Pages artifact |
| Language tooling | `vscode-ostrin/`, `compiler/src/lsp.rs`, `compiler/src/dap.rs` | Syntax support, LSP and DAP are implemented in the repository; Marketplace publication is not claimed |

## Verified inventory

- 183 `.ostrin` source files exist under `examples/`, including the `tables`, `plot` and
  `autodiff` package projects.
- 22 design documents exist under `docs/design/`.
- The compiler test suite currently has 2 unit tests, 6 differential tests and 176 example
  integration tests (the last full run is recorded in the development log and is rerun before
  each website change is pushed).
- The browser playground is not a simulation: it loads the compiler WASM and runs the selected
  source through the real CLI entry point in an in-memory WASI directory.

## Public inconsistencies found

- Repeated `prototype / 0.1` labels coexist with `development / 0.1.0`.
- Homepage, examples and ecosystem pages still show old example and test counts.
- `docs.html` and `language.html` describe the browser playground as future work even though
  `pages.yml` deploys it.
- `examples.html` says its browser preview is static without linking the executable playground
  at the point of discovery.
- No canonical URLs, sitemap, robots policy or structured data are present.
- The current site has no `/showcase`, `/community` or blog surface; these remain later phases,
  not capabilities to imply as available today.

## Prioritized delivery order

1. **Discovery foundation:** canonical/OG metadata, sitemap, robots policy, consistent version
   and counters.
2. **Live entry point:** reuse `playground.js` and the real WASM in a compact homepage playground.
3. **Honest catalogue:** update examples with real source links and direct Run/Playground paths.
4. **Learning funnel:** add a showcase and community surface only after their content has real
   repository evidence.
5. **Scientific experience:** expose statistics, data, plotting and autodiff using the existing
   tested examples; keep SVG plotting marked early and do not claim interactive plotting yet.
6. **Validation:** add a static website smoke check for local links, metadata and the required
   playground entry point, then connect it to Pages CI.

This sequence preserves the current visual identity, compiler-backed playground and static
GitHub Pages architecture while making the next public claims evidence-based.

## Follow-up delivered in the next increment

- `website/examples.html` now has four editable live examples backed by the same WASM module:
  quantities, standard library, records/enums and concurrency.
- `website/playground.js` shares its invocation and loading path between the full playground and
  the catalogue cards, with Run, Check, Reset and Copy actions for each card.
- `scripts/website-check.mjs` validates public HTML, metadata, local references and anchors,
  sitemap, robots, playground wiring and drift between live source strings and their validated
  repository examples. `ci.yml` runs the static check; `pages.yml` runs it again after generating
  and copying the WASM artifact.
- `website/showcase.html` now presents four repository-backed programs with links to source and
  protecting tests; SVG is explicitly marked early and no interactive graph is claimed.
- `website/community.html`, `CONTRIBUTING.md`, issue templates, the pull-request template and
  `docs/community-labels.md` prepare contribution without asserting that unverified channels or
  external projects exist.
- `website/docs.html` now contains a 14-step guided learning path from first run through real projects.
  It links to source-backed examples and existing reference/playground/showcase surfaces, labels
  concurrency/packages/native compilation as early where the implementation or distribution story is
  still growing, and is covered by `scripts/website-check.mjs`.
- The playground now requests structured JSON diagnostics for checks and failed runs, renders their
  code/location/message fields in an accessible status output, and keeps a plain-text fallback for
  non-JSON runtime output. The homepage and catalogue reuse the same module and output contract.
- The first structured diagnostic now selects its source line and exposes `line N · column M` in the
  editor header without adding a heavyweight editor dependency. A dedicated mobile viewport remains
  untested in the current CUA surface, while the existing one-column responsive rule is preserved.
