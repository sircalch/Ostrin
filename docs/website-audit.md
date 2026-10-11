# Ostrin website audit

*Cut: 2026-10-11 · source counts and public fallbacks are checked by
`scripts/website-check.mjs`; plan and maturity claims are checked by
`scripts/project-plan-check.mjs` and `scripts/quantity-ownership-check.mjs`.*

This is a current inventory, not a roadmap claim. Public statements should remain tied to code,
tests or an explicitly labeled early-stage surface.

## Dated visual and responsive review

Review performed on **2026-10-11** from the current checkout with the real static site and
generated `ostrinc.wasm` artifact:

| Check | Result | Evidence and disposition |
| --- | --- | --- |
| Chromium visual regression | PASS | `npm test -- --project=chromium visual.spec.mjs`: 2 tests passed; six public surfaces were checked at desktop (1440×960), tablet (768×1024) and mobile (390×844), plus seven representative plots. Existing snapshots remained within the configured pixel-diff threshold. |
| Chromium functional/accessibility suite | PASS | `npm test -- --project=chromium website.spec.mjs`: 30 tests passed, including the benchmark report, keyboard navigation, accessible names, real WASM compiler flows, Viz/3D/animation/table/provenance controls and the public status dashboard. |
| WebKit responsive smoke | PASS | `npm test -- --project=webkit-smoke cross-browser.spec.mjs`: public-page overflow and keyboard semantics passed. |
| Firefox responsive smoke | PASS | Pinned Ubuntu 24.04 browser audit installed Firefox and passed the public-page overflow, keyboard and runtime-error suite in [CI run 38111107188](https://github.com/sircalch/Ostrin/actions/runs/38111107188). The Windows local `spawn UNKNOWN` limitation is a tooling note, not a site result. |
| Static accessibility and metadata checks | PASS | `node scripts/website-check.mjs`: 17 public pages, heading hierarchy, social card, accessible status regions, generated facts and benchmark data passed. |

The cross-browser CI gate is green in the pinned Ubuntu 24.04 environment. The local Windows
Firefox launch limitation remains documented as a tooling note and does not replace the CI result.

## Existing surfaces

| Surface | Evidence | Current state |
| --- | --- | --- |
| Homepage | `website/index.html`, `website/lab.js`, `website/lab-data.js`, `website/playground.js`, `website/assets/ostrin-social.png`, `README-reproduce.md` | Hero with release status derived from `CHANGELOG.md`, twelve-demo Scientific Lab that recomputes repository programs (including scalar measurements) and the `plot`/`autodiff` lab projects with `ostrinc.wasm`, shareable URL state for the active demo and parameters, evidence-linked capability cards, a source → HIR → IR → C pipeline recorded from the compiler, the real playground, direct entry points for animated, tabular and provenance-carrying visual artifacts, and a linked reproduction guide; all public pages share a 1200x630 social preview |
| Cookbook | `website/cookbook.html`, `scripts/lab-data.mjs` | The Lab programs as recipes with source, recorded output, documentation links and stated limits |
| Viz gallery | `website/viz.html`, `website/viz.js`, `website/lab-data.js`, `scripts/viz-jsonld.mjs` | Twenty-nine repository-backed figures with live WASM reruns, 2D/3D exploration, measurement error bars, animation/table controls, exports and shareable URLs that restore parameters and camera state; downloadable figure manifests now preserve that selected state and expose citation metadata, while the gallery offers a BibTeX handoff; the same catalogue emits a checked JSON-LD `ItemList` with each SVG and its Ostrin source |
| Featured workflows | `scripts/lab-data.mjs`, `website/viz.html`, `website/viz.js` | Three source-backed paths — simulate, analyze and explore 3D — group existing figures, tables, animations and provenance records behind shareable `?workflow=...` URLs; available and experimental labels remain explicit |
| Flagship workflow | `website/workflow.html`, `examples/viz_ode.ostrin`, `website/assets/experiments/ode.ostrin-experiment.json` | A public case study follows the driven pendulum from Ostrin source through WASM execution, recorded SVG, companion animation/table/provenance artifacts and an explicit R0 evidence boundary |
| Evidence dashboard | `website/status.html`, `website/status.js`, `website/status-data.js`, `validation/quality-scorecard.json`, `validation/linguist-usage.json` | Public scorecard with separate backend, ownership, reproducibility, GitHub Linguist usage and v0.2 readiness dimensions; open criteria remain visible and no aggregate quality number is claimed |
| Guides | `website/guides.html` | Install, projects, testing, native, WASI, editor, experimental effect inventory and site-evidence workflows; every command exists in `ostrinc --help` |
| Example catalogue | `website/examples.html` | Filterable repository catalogue plus live quantity, standard library, record/enum and concurrency programs |
| Browser playground | `website/playground.html`, `website/playground.js`, `.github/workflows/pages.yml` | Generated `ostrinc.wasm`; Run, Check, Test, Format, share links and source diagnostics execute in an in-memory WASI filesystem and are exercised in Chromium before CI passes or Pages uploads |
| Learn and Reference | `website/docs.html`, `website/reference.html`, `website/language.html`, `docs/design/` | Guided 14-step learning path; Reference indexes all 27 design documents and the CLI flags, checked against `ostrinc --help` |
| Showcase | `website/showcase.html` | Four repository-backed demonstrations; every displayed output line is verified against its program by `scripts/lab-data.mjs` |
| Benchmarks | `website/benchmarks.html`, `website/benchmark-data.js`, `scripts/benchmark.mjs` | Eight deterministic workloads compare interpreter and native process medians; the page publishes commit, environment, sampling and output provenance without cross-language claims |
| Provenance artifact | `website/provenance.html`, `website/provenance-data.js`, `website/provenance.js`, `scripts/provenance-page.mjs`, `compiler/src/provenance.rs` | Experimental machine-readable source hash, target/WASI, `ostrin.runtime/v1` and available lockfile hashes with explicit replay limitations; the companion experiment bundles add input/output hashes, compiler effects and CI host metadata under their separate R3 gate; regenerated and checked before Pages deployment |
| Experiment bundle | `scripts/experiment-bundle.mjs`, `scripts/r3-readiness.mjs`, `scripts/experiment-replay.mjs`, `scripts/lab-data.mjs`, `scripts/lab-data-replay.test.mjs`, `scripts/experiment-bundle.test.mjs`, `scripts/experiment-replay.test.mjs`, `website/assets/experiments/*.ostrin-experiment.json`, `website/viz.js` | The complete 29-figure `ostrin.experiment/v0` source-backed registry provides exact SVG replay evidence with calculated SHA-256/byte hashes, declared seeds/parameters and 3D cameras where applicable; every bundle now carries a validated partial `ostrin.runtime/v1` snapshot with target/WASI, compiler/project lockfile hashes, declared dependencies and scheduler policy, plus a verified invocation output hash; all 29 bundles carry the base execution contract and a normalized `ostrin.effect-report/v0` summary from the release compiler, while the eleven input-driven bundles also carry embedded snapshots, declared filesystem capabilities and verified sensitivity, so Website CI labels them R2; the nine seeded fixtures retain their R1 algorithm/partition evidence and Website CI discovers all R1/R2 fixtures from the source-backed registry with `experiment-replay.mjs --all`; recorded SVG equality applies only within the CI replay configuration, while cross-platform numerical comparisons are explicitly unassessed and require metric-specific tolerances; the remaining 18 fixtures remain R0; selected figure manifests capture current parameters, camera and share URL; Pages CI regenerates bundles with public runner and visible-hardware metadata under `OSTRIN_CAPTURE_HOST=1`, while local bundles omit that field; R3 remains unclaimed in the checkout until CI produces the host-bound artifact |
| Plan and maturity gate | `scripts/project-plan-check.mjs`, `scripts/project-plan-check.test.mjs`, `ESTADO_Y_PLAN.md`, `ROADMAP.md`, `compiler/tests/differential.rs` | Compares the documented HIR/IR/AST baseline with the compiler ratchet, preserves the verified Quantity visualization ownership boundary, and requires explicit Available/Planned Viz labels plus Linguist and R0/R1/R2 reproducibility evidence |
| Browser visual regression | `tests/browser/visual.spec.mjs`, `tests/browser/visual.spec.mjs-snapshots/`, `.github/workflows/pages.yml` | Screenshot baselines cover six public surfaces (homepage, Lab, gallery, playground, provenance and benchmarks) at desktop/tablet/mobile widths, plus seven representative scientific plots. Chromium snapshots are checked in for Windows and Ubuntu 24.04; the Pages job is pinned to Ubuntu 24.04 so Linux rendering baselines stay comparable. |
| Scalar Quantity ownership gate | `scripts/quantity-ownership-check.mjs`, `scripts/quantity-ownership-check.test.mjs`, `docs/design/27-ownership-de-quantity-escalar.md`, `compiler/tests/examples.rs`, `compiler/src/ownership.rs`, `compiler/src/ir_c.rs`, `compiler/src/qty_runtime.c` | Requires scalar label helpers and native/WASI/leak evidence; `viz_units.ostrin` now verifies dimension-specialized `unit_line`/`unit_scatter` IR lowering, differential SVG parity, WASI C emission and zero native leaks, while ASan/UBSan CI and list-based plot ownership remain open |
| Community | `website/community.html`, `CONTRIBUTING.md`, issue templates | Contribution path and repository channels; no unverified chat, registry or external community is claimed |
| Ecosystem and roadmap | `website/ecosystem.html`, `website/roadmap.html` | Current capabilities, early areas and future work are distinguished |
| Deployment and editor | `.github/workflows/pages.yml`, `vscode-ostrin/`, LSP/DAP sources | Pages builds and checks the WASM artifact; editor support is implemented, Marketplace publication is not claimed |

## Verified inventory

- **295** `.ostrin` source files under `examples/`, including package-project sources and
  intentional error cases.
- **27** Markdown design documents under `docs/design/`.
- Compiler suite: **319 integration**, **6 differential** and **2 unit** tests.
- WASI release smoke matrix: **20 program modules** passed in the latest full runtime run with
  wasi-sdk 34 and Node WASI. It covers hello, quantity arrays, a local-path package,
  arguments/environment, file I/O, managed ownership, nested `Option`/`Result`, managed enum
  `Result`, branch/loop `Phi`, closures, task capture/cancellation and function values stored in
  lists and records.
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
on every public page. The same check now enforces one `h1` per public page and rejects skipped
heading levels, so the visual hierarchy remains a usable document outline for keyboard and assistive
technology users. CI and the Pages build run both checks, so stale facts or missing social assets fail
before publication.

## Remaining constraints

- The site is static GitHub Pages. The experimental `v0.1.0` compiler release is published, but
  there is no public package registry or additional distribution channel; installer scripts require
  a maintainer-published matching tag.
- Some compiler/runtime and scientific-library capabilities remain explicitly early or incomplete;
  the site should preserve those maturity labels and avoid implying production readiness.
- Chromium regression coverage checks the full compiler, playground, Lab, Viz and provenance flows.
  Firefox and WebKit smoke tests check all seventeen pages at phone/desktop widths, mobile navigation,
  keyboard Escape recovery, filter and tab semantics, overflow and page errors. Chromium also checks
  that all 29 gallery figures expose their scientific summary as image text alternatives and that
  the explorer dialog and iframe are named for the selected figure. Screenshot regression checks six
  public surfaces at three viewport sizes and seven representative plot cards using checked-in,
  platform-specific Chromium baselines. A browser DOM audit also checks accessible names for visible
  links, controls, images and iframes across all public pages; a full manual screen-reader audit remains
  outside the automated gate.
- Keep external services, analytics and community-channel claims out of the site until they have a
  real operational contract and explicit review.

## Next product work

Continue closing the production path in dependency order: ownership and memory behavior across
control-flow boundaries; native backend correctness; real concurrency stress and cancellation;
standard-library and package contracts; then WASM distribution and broader platform tests. Keep
each block linked to executable tests, update the development log, and push only after checks pass.

The featured workflows are a web discovery layer over existing evidence, not new scientific
results. All 29 gallery fixtures now have machine-readable `ostrin.experiment/v0` bundles with
source, declared inputs, selected SVG, calculated hashes and parameter/camera snapshots where
applicable. Each bundle also carries the partial `ostrin.runtime/v1` target/WASI snapshot and
available lockfile hashes. Website CI replays all 29 source files with the same `ostrinc.wasm` target and rejects
any byte-level SVG drift. Figure manifests now also capture the live selected parameters, optional
3D camera and share URL. The `surface` bundle is the first input-driven executable slice: its
source reads `data.json`, the replay command checks the manifest and exact SVG, and a changed
parameter must change the output. The `lines` bundle now exercises the same generic sensitivity
contract. RNG-consuming bundles record the xoshiro256**/splitmix64 implementation evidence and
source hashes. `histogram` was the first input-driven seeded-randomness bundle, `boxplot`
the second, `scatter-fit` the third, `violin` the fourth, `hexbin` the fifth, `bars` the sixth, `point-cloud` the seventh, `fft` the eighth and `dashboard` the ninth; every gallery fixture that consumes `rng(seed)` retains the R1 algorithm contract inside its R2 bundle. All eleven input-driven fixtures now claim R2 because their embedded `data.json` snapshots, filesystem capabilities and sensitivity are verified. The remaining 18 R0 fixtures are deterministic or lack a declared input-sensitivity contract. The R3 host/effects gate is implemented; a real CI artifact is still required before claiming R3.
