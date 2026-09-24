# Ostrin website audit

*Cut: 2026-09-24 · source counts and public fallbacks are checked by `scripts/website-check.mjs`.*

This is a current inventory, not a roadmap claim. Public statements should remain tied to code,
tests or an explicitly labeled early-stage surface.

## Existing surfaces

| Surface | Evidence | Current state |
| --- | --- | --- |
| Homepage | `website/index.html`, `website/lab.js`, `website/lab-data.js`, `website/playground.js`, `website/assets/ostrin-social.png` | Hero with release status derived from `CHANGELOG.md`, eight-tab Scientific Lab that recomputes `examples/lab_*` programs and the `plot`/`autodiff` lab projects with `ostrinc.wasm`, evidence-linked capability cards, a source → HIR → IR → C pipeline recorded from the compiler, and the real playground; all public pages share a 1200x630 social preview |
| Cookbook | `website/cookbook.html`, `scripts/lab-data.mjs` | The Lab programs as recipes with source, recorded output, documentation links and stated limits |
| Guides | `website/guides.html` | Install, projects, testing, native, WASI, editor and site-evidence workflows; every command exists in `ostrinc --help` |
| Example catalogue | `website/examples.html` | Filterable repository catalogue plus live quantity, standard library, record/enum and concurrency programs |
| Browser playground | `website/playground.html`, `website/playground.js`, `.github/workflows/pages.yml` | Generated `ostrinc.wasm`; Run, Check, Test, Format, share links and source diagnostics execute in an in-memory WASI filesystem and are exercised in Chromium before CI passes or Pages uploads |
| Learn and Reference | `website/docs.html`, `website/reference.html`, `website/language.html`, `docs/design/` | Guided 14-step learning path; Reference indexes all 23 design documents and the CLI flags, checked against `ostrinc --help` |
| Showcase | `website/showcase.html` | Four repository-backed demonstrations; every displayed output line is verified against its program by `scripts/lab-data.mjs` |
| Community | `website/community.html`, `CONTRIBUTING.md`, issue templates | Contribution path and repository channels; no unverified chat, registry or external community is claimed |
| Ecosystem and roadmap | `website/ecosystem.html`, `website/roadmap.html` | Current capabilities, early areas and future work are distinguished |
| Deployment and editor | `.github/workflows/pages.yml`, `vscode-ostrin/`, LSP/DAP sources | Pages builds and checks the WASM artifact; editor support is implemented, Marketplace publication is not claimed |

## Verified inventory

- **233** `.ostrin` source files under `examples/`, including package-project sources and
  intentional error cases.
- **23** Markdown design documents under `docs/design/`.
- Compiler suite: **214 integration**, **6 differential** and **2 unit** tests.
- WASI release smoke matrix: nine program modules covering the hello program, a local-path
  package, arguments/environment, file I/O, managed ownership and nested `Option`/`Result`
  consumers; compiler and program modules
  are executed under Node WASI.
- The browser playground uses the generated compiler WASM and the real CLI in an in-memory WASI
  filesystem; it is not a JavaScript reimplementation of the language.

`website/site-data.js` is generated from the repository and holds the public display facts consumed
by every page before `website/site.js` runs. `scripts/site-facts.mjs` derives the expected version
and counts from `compiler/Cargo.toml`, the source tree and Rust test attributes, while
`scripts/website-metadata.mjs` writes or checks the generated artifact. `scripts/website-check.mjs`
then verifies the artifact, every HTML placeholder, README, roadmap and this audit. It also validates
the social card's PNG signature and dimensions and requires consistent Open Graph/Twitter metadata
on every public page. CI and the Pages build run both checks, so stale facts or missing social assets
fail before publication.

## Remaining constraints

- The site is static GitHub Pages. There is no public package registry or default published compiler
  release; installer scripts require a maintainer-published matching tag.
- Some compiler/runtime and scientific-library capabilities remain explicitly early or incomplete;
  the site should preserve those maturity labels and avoid implying production readiness.
- Chromium regression coverage now checks all nine pages at phone/tablet widths, mobile/tablet
  navigation, the real compiler output and a real diagnostic. Firefox/WebKit behavior, broader
  screen-reader checks and visual baselines are not covered yet.
- Keep external services, analytics and community-channel claims out of the site until they have a
  real operational contract and explicit review.

## Next product work

Continue closing the production path in dependency order: ownership and memory behavior across
control-flow boundaries; native backend correctness; real concurrency stress and cancellation;
standard-library and package contracts; then WASM distribution and broader platform tests. Keep
each block linked to executable tests, update the development log, and push only after checks pass.
