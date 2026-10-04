# Ostrin website audit

*Cut: 2026-10-04 · source counts and public fallbacks are checked by
`scripts/website-check.mjs`; plan and maturity claims are checked by
`scripts/project-plan-check.mjs`.*

This is a current inventory, not a roadmap claim. Public statements should remain tied to code,
tests or an explicitly labeled early-stage surface.

## Existing surfaces

| Surface | Evidence | Current state |
| --- | --- | --- |
| Homepage | `website/index.html`, `website/lab.js`, `website/lab-data.js`, `website/playground.js`, `website/assets/ostrin-social.png` | Hero with release status derived from `CHANGELOG.md`, twelve-demo Scientific Lab that recomputes repository programs (including scalar measurements) and the `plot`/`autodiff` lab projects with `ostrinc.wasm`, shareable URL state for the active demo and parameters, evidence-linked capability cards, a source → HIR → IR → C pipeline recorded from the compiler, the real playground, and direct entry points for animated, tabular and provenance-carrying visual artifacts; all public pages share a 1200x630 social preview |
| Cookbook | `website/cookbook.html`, `scripts/lab-data.mjs` | The Lab programs as recipes with source, recorded output, documentation links and stated limits |
| Viz gallery | `website/viz.html`, `website/viz.js`, `website/lab-data.js`, `scripts/viz-jsonld.mjs` | Twenty-nine repository-backed figures with live WASM reruns, 2D/3D exploration, measurement error bars, animation/table controls, exports and shareable URLs that restore parameters and camera state; the same catalogue emits a checked JSON-LD `ItemList` with each SVG and its Ostrin source |
| Featured workflows | `scripts/lab-data.mjs`, `website/viz.html`, `website/viz.js` | Three source-backed paths — simulate, analyze and explore 3D — group existing figures, tables, animations and provenance records behind shareable `?workflow=...` URLs; available and experimental labels remain explicit |
| Guides | `website/guides.html` | Install, projects, testing, native, WASI, editor, experimental effect inventory and site-evidence workflows; every command exists in `ostrinc --help` |
| Example catalogue | `website/examples.html` | Filterable repository catalogue plus live quantity, standard library, record/enum and concurrency programs |
| Browser playground | `website/playground.html`, `website/playground.js`, `.github/workflows/pages.yml` | Generated `ostrinc.wasm`; Run, Check, Test, Format, share links and source diagnostics execute in an in-memory WASI filesystem and are exercised in Chromium before CI passes or Pages uploads |
| Learn and Reference | `website/docs.html`, `website/reference.html`, `website/language.html`, `docs/design/` | Guided 14-step learning path; Reference indexes all 27 design documents and the CLI flags, checked against `ostrinc --help` |
| Showcase | `website/showcase.html` | Four repository-backed demonstrations; every displayed output line is verified against its program by `scripts/lab-data.mjs` |
| Benchmarks | `website/benchmarks.html`, `website/benchmark-data.js`, `scripts/benchmark.mjs` | Eight deterministic workloads compare interpreter and native process medians; the page publishes commit, environment, sampling and output provenance without cross-language claims |
| Provenance artifact | `website/provenance.html`, `website/provenance-data.js`, `website/provenance.js`, `scripts/provenance-page.mjs`, `compiler/src/provenance.rs` | Experimental machine-readable source hash, target and effect inventory with explicit replay limitations; regenerated and checked before Pages deployment |
| Experiment bundle | `scripts/experiment-bundle.mjs`, `scripts/experiment-bundle.test.mjs`, `website/assets/experiments/*.ostrin-experiment.json`, `website/viz.js` | Parametrizable `ostrin.experiment/v0` source-backed download registry: the provenance and 3D surface fixtures are four-file R0 bundles (source, declared inputs, SVG and provenance) with calculated SHA-256/byte hashes, parameters/camera where declared and deterministic source revisions; R1 replay verification is labelled planned and R2/R3 are not claimed |
| Plan and maturity gate | `scripts/project-plan-check.mjs`, `scripts/project-plan-check.test.mjs`, `ESTADO_Y_PLAN.md`, `ROADMAP.md`, `compiler/tests/differential.rs` | Compares the documented HIR/IR/AST baseline with the compiler ratchet, preserves the verified Quantity visualization ownership boundary, and requires explicit Available/Planned Viz labels plus Linguist and R0/R1 reproducibility evidence |
| Community | `website/community.html`, `CONTRIBUTING.md`, issue templates | Contribution path and repository channels; no unverified chat, registry or external community is claimed |
| Ecosystem and roadmap | `website/ecosystem.html`, `website/roadmap.html` | Current capabilities, early areas and future work are distinguished |
| Deployment and editor | `.github/workflows/pages.yml`, `vscode-ostrin/`, LSP/DAP sources | Pages builds and checks the WASM artifact; editor support is implemented, Marketplace publication is not claimed |

## Verified inventory

- **266** `.ostrin` source files under `examples/`, including package-project sources and
  intentional error cases.
- **27** Markdown design documents under `docs/design/`.
- Compiler suite: **271 integration**, **6 differential** and **2 unit** tests.
- WASI release smoke matrix: thirteen program modules covering the hello program, quantity arrays, a local-path
  package, arguments/environment, file I/O, managed ownership and nested `Option`/`Result`
  consumers; compiler and program modules
  are executed under Node WASI.
- The browser playground uses the generated compiler WASM and the real CLI in an in-memory WASI
  filesystem; it is not a JavaScript reimplementation of the language.
- The benchmark page records one deliberate run at a time. Its ratios compare Ostrin's
  interpreter and native executable on the same machine; they do not represent Python, Julia,
  Rust or another platform.

`website/site-data.js` is generated from the repository and holds the public display facts consumed
by every page before `website/site.js` runs. `scripts/site-facts.mjs` derives the expected version
and counts from `compiler/Cargo.toml`, the source tree and Rust test attributes, while
`scripts/website-metadata.mjs` writes or checks the generated artifact. `scripts/website-check.mjs`
then verifies the artifact, every HTML placeholder, README, roadmap and this audit. It also validates
the social card's PNG signature and dimensions and requires consistent Open Graph/Twitter metadata
on every public page. CI and the Pages build run both checks, so stale facts or missing social assets
fail before publication.

## Remaining constraints

- The site is static GitHub Pages. The experimental `v0.1.0` compiler release is published, but
  there is no public package registry or additional distribution channel; installer scripts require
  a maintainer-published matching tag.
- Some compiler/runtime and scientific-library capabilities remain explicitly early or incomplete;
  the site should preserve those maturity labels and avoid implying production readiness.
- Chromium regression coverage checks the full compiler, playground, Lab, Viz and provenance flows. A Firefox and
  WebKit smoke matrix now checks all fifteen pages at phone/desktop widths, mobile navigation,
  keyboard Escape recovery, filter and tab semantics, overflow and page errors. Broader screen-reader
  checks and visual baselines remain outside the automated gate.
- Keep external services, analytics and community-channel claims out of the site until they have a
  real operational contract and explicit review.

## Next product work

Continue closing the production path in dependency order: ownership and memory behavior across
control-flow boundaries; native backend correctness; real concurrency stress and cancellation;
standard-library and package contracts; then WASM distribution and broader platform tests. Keep
each block linked to executable tests, update the development log, and push only after checks pass.

The featured workflows are a web discovery layer over existing evidence, not new scientific
results. The provenance and surface gallery fixtures now have machine-readable
`ostrin.experiment/v0` R0 bundles with source, declared inputs, selected SVG, calculated hashes and
the surface parameters/camera snapshot. Extending this to every figure and verifying R1 replay
(including live parameter and camera state) remains follow-up work; the bundles do not claim R2/R3.
