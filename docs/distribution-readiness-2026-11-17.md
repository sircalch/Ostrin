# Distribution and integration readiness (2026-11-17)

Audit of the Ostrin scientific core on branch `horizon/scientific-platform-core`, performed before any integration with
`main` or Horizon Epsilon. Nothing was pushed, opened as a pull request, merged, released or published.

Status terms used exactly as defined here:

- **IMPLEMENTADO**: the code exists and is reachable from the official route.
- **VERIFICADO**: a check was run in this audit and its result is recorded below (command and exit code).
- **PARCIAL**: verified for a stated family only; the limit is stated.
- **BLOQUEADO**: a check cannot run, or a defect blocks distribution; the blocker is named.
- **NO PROBADO**: not run in this audit.

## 1. Repository state

| Item | Value |
|---|---|
| Worktree | `C:\Users\Andre\Downloads\_Ostrin-worktrees\scientific-platform-core` |
| Branch | `horizon/scientific-platform-core` (not pushed; no upstream) |
| HEAD at the start of this stage | `c0fad4f3` |
| Local `main` | `9b5043f7`, **stale**: `origin/main` is `0e005d77` |
| `origin/main` | `0e005d77` (includes PR #296 `horizon/workbench-delta`, merge `71f70545`, and PR #298 `CITATION.cff`) |
| Merge base HEAD / `origin/main` | `71f70545` |
| HEAD-only commits vs `origin/main` | 77 |
| `origin/main`-only commits vs HEAD | 2 (`2e222bf9` CITATION.cff, `0e005d77` merge of #298) |
| Files changed HEAD vs merge base | 325 |
| `horizon/workbench-epsilon` | `d951312e`, 12 commits not in HEAD, base `71f70545` |
| PR #297 (Horizon Epsilon) | open, **draft**, base `main`, head `horizon/workbench-epsilon`, GitHub reports `MERGEABLE` (last update 2026-11-11). Read only. |
| Remote CI on HEAD | none: the branch has never been pushed. CI on `origin/main` `0e005d77` ran `Ostrin CI`, `CodeQL`, `Compiler benchmarks`, `Security audit`, `Compiler coverage`, all `success`. |
| Latest published release | `v0.2.0` (2026-10-15, latest), `v0.1.0` (2026-09-24). v0.2.0 assets: Linux x86_64, macOS arm64, Windows x64, WASI wasm32. |

Other branches: `horizon/workbench-alpha`, `-beta`, `-gamma`, `-delta`, `horizon/viz-safe-ids`,
`horizon/svg-resource-identity` and `horizon/stabilization` are all ancestors of HEAD. Their work is already in this
branch. Many `codex/*` and `feat/*` branches exist in worktrees; none was audited beyond the listing.

### 1.1 Integration dry run (read only)

`git merge-tree --write-tree` was used; no ref, index or working tree was changed by it.

| Merge | Result |
|---|---|
| HEAD + `origin/main` | clean |
| `horizon/workbench-epsilon` + `origin/main` | clean |
| HEAD + `horizon/workbench-epsilon` | **3 conflicts**: `docs/linguist-usage.md`, `docs/linguist.md`, `tests/browser/playwright.config.mjs` |

Analysis of the three conflicts:

- `docs/linguist*.md`: counts of tracked Ostrin files and examples. Both sides changed the numbers; the correct values
  must be recomputed after the merge (`scripts/linguist-check.mjs`), not chosen by hand.
- `tests/browser/playwright.config.mjs`: both sides extended the `testMatch` lists (HEAD adds `notebook*.spec.mjs` and
  `linalg.spec.mjs`; Epsilon adds `workbench-epsilon`, `workbench-persistence`, `workbench-evidence`). The union is the
  correct result; neither side may be dropped.

Epsilon touches 29 files relative to its base; only the three files above overlap with HEAD. Epsilon does not touch the
gallery, the visual baselines or the compiler.

## 2. Component inventory

Each row gives the real status. "Tests" names the checks that exist; "Run here" says whether this audit ran them.

| # | Component | Implementation | Status | Public API / route | Tests | Run here | Main limit or risk | Work for a first stable distribution |
|---|---|---|---|---|---|---|---|---|
| 1 | Language and syntax | `compiler/src/lexer`, `parser`, `ast.rs` | IMPLEMENTADO | `.ostrin` files, `ostrinc --check` | `compiler/tests/examples.rs` (472 integration tests in total) | Yes, Windows and Linux | Language is experimental; no stability promise | Freeze a syntax subset in the docs |
| 2 | Type system | `typeck/mod.rs` (7.3k lines), `types.rs`, `hir.rs` | IMPLEMENTADO | `--types`, `--typed-report`, `--ownership-check` | `compile_fail_examples_report_their_error_code`, `hir_covers_the_examples_with_known_types` | Yes | Diagnostics are in English with Spanish OS messages mixed in | Diagnostic wording review |
| 3 | Compiler and backends | `codegen.rs` (12k lines), `ir.rs`, `ir_c.rs`, `hir_c.rs`, `array_*.c`, `*_runtime.c` | IMPLEMENTADO | `--compile`, `--emit-c`, `--leak-check`, routes IR (default), HIR, AST | `examples.rs`, `differential.rs`, `native_backend_types_agree_with_the_checker` | Yes (Windows and WSL gcc) | Native backend needs a C compiler; sanitizers only under GCC 13 in WSL | Document the C compiler requirement per platform |
| 4 | Interpreter | `interpreter/mod.rs`, `array.rs`, `qarray.rs`, `regress.rs` | IMPLEMENTADO | `--run` | `differential.rs` (interpreter vs native) | Yes | Slower than native (measured in `benchmarks/`) | State the measured slowdown in the user docs |
| 5 | Runtime and WASM | `website/ostrinc.wasm`, `scripts/wasi-program-check.mjs`, `pendulum-wasi-check.mjs`, `linalg-wasm-check.mjs` | IMPLEMENTADO, VERIFICADO (WASI) | `--target wasm32-wasi` | 51 WASI programs, pendulum (10 cases), linalg (6 checks) | Yes, Windows | Browser runs only the interpreter route; no native backend in the browser | Keep the WASI matrix in CI (see section 4) |
| 6 | Standard library | `compiler/std/*.ostrin` (21 modules, about 15.8k lines) | IMPLEMENTADO | `import std.<name>` | the examples that import each module | Yes | Coverage is uneven: `symbolic`, `measurements` and `viz` are the largest | Publish a module index with status per module |
| 7 | Symbolic algebra and equations | `std/symbolic.ostrin`, `solve.ostrin`, `rational.ostrin`, `polynomial.ostrin`, `mpolynomial.ostrin` | IMPLEMENTADO, PARCIAL | `symbolic.*`, `solve.*`, rational equations (`RationalSolution`) | `symbolic_solve`, `rational_equations`, oracles (`rational-equations-oracle.mjs`, `mpolynomial-oracle.mjs`) | Yes (example run; oracles in CI) | No gcd in several variables, no partial fractions, no closed form for degree 3+ except rational roots; no complex roots as values. Documented in `docs/CAPABILITY-MATRIX.md` section 8 | Keep limits in the first release notes |
| 8 | Linear algebra | `std/linalg.ostrin`, `array_linalg.c`, `array_stats.c` | IMPLEMENTADO, VERIFICADO (browser and WASI) | `linalg.*`, `solve_checked` | `linalg_oracle`, `linalg_properties`, `linalg_regression`, `linalg_units`; browser `linalg.spec.mjs` (11 tests); WASI checker (6/6) | Yes (interpreter run, WASI, browser on Windows) | Dense only; no BLAS; `linalg_units` one-expression unit-array conversion is PARCIAL | Decide on the BLAS question before claiming performance |
| 9 | Numerical calculus | `std/numeric.ostrin`, `interpreter/detmath.rs`, `detmath_runtime.c` | IMPLEMENTADO | `numeric.*` (Simpson, Brent, RK4/RK45, least squares, LU, QR, Cholesky, eigen, SVD) | `numeric_polyfit`, `numeric_lu`, `numeric_methods`, `numeric-validation.mjs` | Yes (`numeric_polyfit` ran) | Accuracy claims limited to the tests; no interval arithmetic | Publish tolerances per method |
| 10 | Physical units | `std/measurements.ostrin` (1.9k lines), `qty_runtime.c`, `typeck` dimensions | IMPLEMENTADO, PARCIAL | `Quantity<...>`, `Measurement`, dimensional symbolic output | `symbolic_units` (interpreter, native and WASI), `measurement_*` examples | Yes (`symbolic_units` on interpreter and native; WASI after fix) | Dimensional checks are compile time for literal types; `to_string()` on quantity arrays is PARCIAL on the AST route | Keep the AST-route limit in notes |
| 11 | Tables, files and data | `std/data.ostrin` (930 lines), `file_io_runtime.c`, `strings_runtime.c`, `examples/data_project`, `data_engine.ostrin` | IMPLEMENTADO | `data.read_table`, `complete_rows`, CSV/TSV, regression on tables | `data_engine` (run from `examples/`), `data_figures`, `data_io_checks`, `lab_data` | Yes (interpreter) | `data_engine` run from another directory exits 0 after printing "cannot read the experiment" (defect, section 5) | Fix exit status of the example or make it fail |
| 12 | Scientific visualization | `std/viz.ostrin` (2.7k lines), 36 `viz_*` examples, `website/` gallery | IMPLEMENTADO, VERIFICADO (visual baselines on Windows) | `viz.*`, SVG output, `--run` | `viz_scatter_fit` ran; gallery visual test (Playwright `visual.spec.mjs`, 2 tests) | Yes (example run; Playwright on Windows) | Visual baselines exist for `chromium-win32` and `chromium-linux` (25 images per platform); they depend on card position (see `docs/visual-evidence/gallery-2026-11-17/`) | Keep platform-specific baselines; refresh rule documented |
| 13 | Notebooks | `website/notebooks/*.ostrin.notebook.json` (6), `notebook-examples.mjs`, `website/workbench-notebook.js` | IMPLEMENTADO, VERIFICADO (browser) | web Notebook page | `notebook-examples.test.mjs`, `notebook.spec.mjs` (25), `notebook-rational.spec.mjs` (5) | Yes (`notebook-examples --check` rc 0; Playwright 101/101 on Windows chromium) | Browser only; no offline desktop notebook | Document the browser-only status |
| 14 | Workbench and Playground | `website/playground.*`, `workbench-*.js`, `website/workbench-demos/` | IMPLEMENTADO on this branch (alpha to delta); Epsilon (history, evidence packages) is **not on this branch** | web Workbench | `workbench.spec.mjs`, `workbench-demo.spec.mjs`, `workbench-gamma.spec.mjs`, `workbench-delta.spec.mjs` | Yes (chromium) | Epsilon work is in PR #297 (draft) and will conflict in three files (section 1.1) | Integrate Epsilon after the core, per section 6 |
| 15 | Packages, manifests and lockfiles | `compiler/src/package.rs` (794 lines), `ostrin.toml`, `ostrin.lock` | IMPLEMENTADO, PARCIAL | `--project`, `--locked`, `--fetch`, path and Git dependencies | `lockfile_rewrite_is_byte_stable_and_ends_with_one_newline`, package tests in `examples.rs` (copies) | Yes (Windows and WSL) | No registry; Git dependencies need `--fetch` and network; the writer rewrites a lock on every non-locked build | Document `--locked` for CI users |
| 16 | CLI tools | `compiler/src/main.rs`: `--check`, `--run`, `--compile`, `--emit-c`, `--fmt`, `--lsp`, `--dap`, `--provenance-report`, `--test`, `--new`, `--members` and others | IMPLEMENTADO | `ostrinc` | `ostrinc --version`, `--check`, `--run`, `--compile` exercised in this audit | Yes (Windows) | Missing-file message was unclear (fixed in this stage, section 5) | `--help` review |
| 17 | Documentation and examples | `README.md`, `docs/` (32 Markdown files), `examples/` (368 `.ostrin`) | IMPLEMENTADO | | `website-check` (18 pages, 368 examples), `linguist-check` (368) | Yes | `examples/data_project/app/main.exe` is a tracked 76 KB binary (also on `origin/main`) | Remove tracked binary; document build-from-source |
| 18 | Tests, oracles and reproducibility | `compiler/tests/`, `tests/oracle`, `scripts/*-oracle.mjs`, `experiment-bundle*.mjs`, `numeric-validation.mjs` | IMPLEMENTADO, VERIFICADO | `cargo test`, `node --test`, Playwright | section 5 | Yes | Oracles are Node scripts; their coverage is listed in `CAPABILITY-MATRIX.md` | Keep the list of mandatory checks (section 6.3) |
| 19 | Distribution and installation | `scripts/install.sh`, `scripts/install.ps1`, `release.yml`, `install-check.yml` | IMPLEMENTADO, BLOQUEADO for a clean end-to-end run in this audit | `install.sh --version`, `install.ps1 -Version` | `install-check.yml` (manual, `workflow_dispatch`) | **No**: the installer downloads a release archive; that requires the owner's approval | See section 4 | Run `install-check` on a clean runner (needs owner approval) |
| 20 | Continuous integration | 11 workflows in `.github/workflows/` | IMPLEMENTADO | see section 4 | | No runner here; `origin/main` CI is green | HEAD has never run on GitHub | Push only after owner approval (section 6) |

## 3. Results of new tests in this stage

Commands are run from the worktree root. Exit codes are the shell's exit codes.

| Check | Result | Exit |
|---|---|---|
| WASI program matrix, before the fix | **Trap** `unreachable` in `temperature_point_affine.wasm`, from `strtold` (long double formatting disabled in wasi-libc) | 1 |
| WASI program matrix, after the fix (`command.arg("-lc-printscan-long-double")`) | `wasi-program-check: ok (51 programs, ...)` | 0 |
| `node scripts/pendulum-wasi-check.mjs` (before and after) | fails before; `ok (10 compiled WASI cases, ...)` after | 1 then 0 |
| `node scripts/linalg-wasm-check.mjs` | `6 of 6 WASM checks passed` | 0 |
| `ostrinc --run examples/symbolic_solve.ostrin` | output as expected | 0 |
| `ostrinc --run examples/rational_equations.ostrin` (run from repo root) | prints the three equation results and "all rational equation checks passed" | 0 |
| `ostrinc --run examples/linalg_properties.ostrin` | 44 property lines | 0 |
| `ostrinc --run examples/linalg_regression.ostrin` | `status ok`, `residual below 1e-9: true` | 0 |
| `ostrinc --run examples/numeric_polyfit.ostrin` | all `true` lines | 0 |
| `ostrinc --run examples/lab_ode.ostrin` | `steps 268, final angle -12.39`, SVG output | 0 |
| `ostrinc --run examples/viz_scatter_fit.ostrin` | `slope 1.8, intercept 2.19, r2 0.98`, SVG output | 0 |
| `cd examples && ostrinc --run data_engine.ostrin` | full table and fits, `r2 0.999` | 0 |
| `ostrinc --run data_engine.ostrin` from the repo root | `cannot read the experiment` | **0** (defect) |
| `node scripts/notebook-examples.mjs --check` | `notebook examples: ok` | 0 |
| Linux (WSL Ubuntu 24.04, gcc 13.3, cargo 1.98.1), `cargo test --release --no-fail-fast`, before the fix | 40 unit, 6 differential, 472 integration; log `EXIT=0` (the tool wrapper reported 1; see section 5) | 0 (log) |

Full validation results for the final state are in section 7.

## 4. Platform matrix

| Platform | Evidence in this audit | CI evidence | Status |
|---|---|---|---|
| Windows 10/11 x64 (this machine) | cargo, Playwright chromium, WASI, notebook and example runs | none on HEAD | VERIFICADO locally |
| Linux x86_64 (WSL Ubuntu 24.04) | full cargo test (compiler suite); sanitizer runs from earlier phases | `origin/main` `ci.yml` green (ubuntu-latest) | VERIFICADO for the compiler suite; Node-based suites NO PROBADO here (no Node in WSL) |
| macOS arm64 / x86_64 | none | `ci.yml`, `benchmarks.yml`, `release.yml` include macOS runners; `origin/main` green for `ci.yml` | NO PROBADO on HEAD |
| WASI (wasm32-wasip1 via wasi-sdk 34.0) | 51-program matrix, pendulum, linalg checks on Windows | `wasi.yml` is `workflow_dispatch` only: it has not run on HEAD | VERIFICADO locally after the fix; CI NO PROBADO |
| Browser WASM (Chromium 153) | Playwright 101/101 on Windows | `browser-audit.yml` on `push` to main | VERIFICADO (Chromium, Windows); Firefox and WebKit NO PROBADO |
| Native sanitizers (ASan, UBSan, LSan) | GCC 13 in WSL (from earlier phases) | `sanitizers.yml` on pull requests | VERIFICADO earlier; not rerun in this stage |

Legitimate differences kept: visual baselines are per platform (`chromium-win32` and Linux); the Windows baselines were
refreshed with evidence in the previous stage and must not be copied to Linux.

### 4.1 CI improvements made in this stage

- `wasi.yml` had only `workflow_dispatch`, which is why the WASI trap above was not caught by any run. It now also runs on
  `push` to `main` and `pull_request` to `main`, filtered to the compiler, WASI scripts, examples and fixtures. This only
  takes effect after the branch is pushed, which requires owner approval.

Not done here, recommended: add `linalg-wasm-check.mjs` to `wasi.yml` (it needs `website/ostrinc.wasm` built by
`tests/browser/prepare-runtime.mjs`).

## 5. Defects found in this stage

| # | Defect | Cause | Fix in this stage | Evidence |
|---|---|---|---|---|
| D1 | WASI programs that print floats trap at run time | Commit `547e01f6` (on HEAD only) emits `strtold` in the generated C; wasi-libc needs `-lc-printscan-long-double` | Linker flag added for `wasm32-wasi` in `compiler/src/main.rs`; fixture `compiler/tests/fixtures/wasi_float_print.ostrin.txt` and matrix entry `wasi_float_print` | Before: trap, exit 1. After: 51 programs pass |
| D2 | WASI expectation for `symbolic_units` stale | Radical and symbolic normalisation work on HEAD changed the printed form of `T` to `2*pi*L^(1/2)*g^(-1/2)`; the interpreter and native paths already print this form | Expectation updated. The example declares `L` and `g` as `positive`, so the identity holds; the interpreter and native outputs agree | `examples/symbolic_units.ostrin` lines 10 to 11 (declared `positive`) and line 28 (the `T` expression) |
| D3 | WASI check rewrote a tracked lockfile | `scripts/wasi-program-check.mjs` built `examples/pkg_project/main_app` in place without `--locked`; the writer canonicalised the lock | The check builds from a copy in `target/wasi-program-check/project-copies`. The tracked lock is now the canonical bytes (drops the trailing blank line) | `git diff` shows the one-line removal |
| D4 | Missing main file reported as module `''` with an OS message | `compiler/src/modules.rs` formats the entry module with an empty path | Message now reads `could not read the main source file (<path>)` | Run on a missing path: exit 1 |
| D5 | `data_engine.ostrin` exits 0 when run from outside `examples/` | The example prints a message and continues; its input path is relative | **Not changed**: it is example behaviour, not a compiler defect. Listed for the owner to decide | Run from repo root: `cannot read the experiment`, exit 0 |
| D6 | Tracked binary `examples/data_project/app/main.exe` | Committed in `b09b5611`; also on `origin/main` | **Not changed** (history decision) | `git ls-files` |
| D7 | Wrapper exit code of the Linux run | The background wrapper reported exit 1 while the log ends with `EXIT=0` | Not explained. The second Linux run (section 7) is the authority | `linux-cargo-test.log` |

## 6. Integration plan (not executed)

This section is a plan for review. No merge, rebase, push or PR was made.

### 6.1 Recommended order

1. **Refresh local `main`** to `origin/main` (`0e005d77`) with a fetch and fast-forward. The local `main` at `9b5043f7` is
   stale and must not be used as the integration base.
2. **Bring `origin/main` into HEAD** with a merge commit (not a rebase: the branch has never been pushed, but the merge
   keeps the 77 validated commits intact). Expected clean (dry run). The two `origin/main` commits add `CITATION.cff` and
   the merge of PR #298. Rerun the mandatory checks (6.3).
3. **Integrate the scientific core into `main`** through a pull request from HEAD, once the owner approves. This is the
   bulk of the work (325 files) and it is already validated.
4. **Integrate Horizon Epsilon** after step 3, in a separate branch from the new `main`. Expected conflicts: the three
   files in section 1.1. Resolve them as follows: `playwright.config.mjs` takes the union of both `testMatch` lists; the
   `linguist` counts are recomputed with `scripts/linguist-check.mjs` and the website counts with `scripts/website-check.mjs`.
   Keep PR #297 as the review object; do not reuse its draft state for the core.

Reason for this order: the core has the larger, validated change and no open review; Epsilon is draft and its
workbench history feature depends on the Workbench files that the core also changes (the WASM is rebuilt). Doing the
core first means Epsilon is rebased once onto the final state.

### 6.2 Possible API incompatibilities

- The WASM compiler is rebuilt from the core. Epsilon's workbench must run against that rebuilt `website/ostrinc.wasm`;
  the Epsilon browser tests must run after the rebuild, not against the old file.
- The `--locked` behaviour and the canonical lock bytes changed (D3). Any tool that compares lock bytes must use the new
  writer output.
- No public `std` API was removed in the audited range; `CAPABILITY-MATRIX.md` lists new functions only.

### 6.3 Mandatory tests

Before integration, on the source branch and again after each merge:

- `cargo test --release --manifest-path compiler/Cargo.toml --no-fail-fast` (Windows and Linux)
- `node tests/browser/prepare-runtime.mjs`, then `npx playwright test --project=chromium` (all specs)
- `node scripts/wasi-program-check.mjs`, `node scripts/pendulum-wasi-check.mjs`, `node scripts/linalg-wasm-check.mjs`
- `node scripts/website-check.mjs`, `node scripts/linguist-check.mjs`, `node scripts/notebook-examples.mjs --check`
- `node scripts/experiment-bundle-determinism.mjs`, `node scripts/numeric-validation.mjs`
- `node --test` for `scripts/*.test.mjs`
- Native sanitizers under WSL GCC (`scripts/native-sanitize.mjs`)

After the merge into `main`, the GitHub workflows that matter are `ci.yml`, `wasi.yml` (now on push and pull request),
`sanitizers.yml`, `browser-audit.yml` and `codeql.yml`. None of them has run on HEAD.

### 6.4 Regression risks

- Visual baselines depend on the gallery layout. A merge that moves cards above the gallery breaks `visual.spec.mjs`
  again. Check the baselines after any gallery change.
- The native backend and the interpreter must still agree on `symbolic_units` and the float printer (D1 and D2 are
  both about this).
- Counts in `README.md`, `website/site-data.js`, `docs/website-audit.md` and `CHANGELOG.md` are asserted by checks; each
  merge changes them and they must be regenerated, not edited by hand.

### 6.5 Rollback strategy

- Create a tag on the pre-merge `main` (for example `pre-scientific-core`) before the merge; no force push.
- The core merge is a single merge commit: revert it with `git revert -m 1 <merge>` if needed. Epsilon is a second merge
  and reverts the same way.
- Keep `horizon/scientific-platform-core` and `horizon/workbench-epsilon` as branches until the merge is confirmed good.

### 6.6 Keep separate

- Horizon Epsilon history and evidence packages (IndexedDB) stay out of the first stable release until PR #297 leaves
  draft and is reviewed.
- Benchmark claims stay marked as in `CAPABILITY-MATRIX.md` (start-up INCONCLUSIVE).
- `examples/data_project/app/ostrin.lock`: the pre-existing blank-line change is the owner's decision (see section 8).
- `examples/data_project/app/main.exe`: decide whether it is removed from history or kept as a fixture.

## 7. Final validation

Final results are recorded after the last change. Each row is the command, its exit code and the summary line.

| Command | Result | Exit |
|---|---|---|
| `cargo test --release --manifest-path compiler/Cargo.toml --no-fail-fast` (Windows, final tree) | 40 unit, 6 differential, 472 integration | 0 |
| `cargo test --release --no-fail-fast` (WSL Ubuntu 24.04, final tree, log `EXIT=0`) | 40 unit, 6 differential, 472 integration | 0 |
| `node scripts/wasi-program-check.mjs` (after the project-copy change) | `ok (51 programs ...)` | 0 |
| `node scripts/pendulum-wasi-check.mjs` (final binary) | `ok (10 compiled WASI cases ...)` | 0 |
| `node scripts/linalg-wasm-check.mjs` | `6 of 6 WASM checks passed` | 0 |
| `node scripts/notebook-examples.mjs --check` | `notebook examples: ok` | 0 |

Not re-run after the last compiler change (the missing-file message in `modules.rs`): the Playwright browser suite and
`website-check`/`linguist-check`. The browser WASM is built locally (`website/ostrinc.wasm` is untracked), so the browser
suite must be rerun after `node tests/browser/prepare-runtime.mjs` before any release. The last browser result, 101/101 on
Windows Chromium, comes from the previous stage and predates D1 to D4.

## 8. Distribution plan

### 8.1 Installation path for a new user (audited)

1. Download a release archive from `v0.2.0` (Linux x86_64, macOS arm64, Windows x64, WASI). Checksum is verified by
   `scripts/install.sh` or `scripts/install.ps1`.
2. Run `ostrinc --version`, `ostrinc --check examples/hello.ostrin`, `ostrinc --run examples/hello.ostrin`. This is the
   check that `install-check.yml` performs on a clean runner. Output must be `hola desde Ostrin`.
3. Build from source: `cargo build --release --manifest-path compiler/Cargo.toml` needs Rust stable and a C compiler for the
   native backend (GCC or Clang on Linux and macOS, MSVC or MinGW on Windows).

Not verified here: step 1 (download) and the clean-runner check. Both need owner approval because they download or run
release assets.

### 8.2 Blockers for distribution

| Blocker | Status |
|---|---|
| Owner decision on the pre-existing change in `examples/data_project/app/ostrin.lock` | BLOQUEADO (owner) |
| HEAD never pushed; no CI run on HEAD | BLOQUEADO until push is approved |
| macOS evidence on HEAD | NO PROBADO |
| Firefox and WebKit browser runs | NO PROBADO |
| Versioning: `Cargo.toml` says `0.2.0`, the template `ostrin.toml` says `0.1.0`, and the latest release is `v0.2.0` from an older commit | BLOQUEADO for a new version number until the owner chooses it |
| Tracked binary in history (D6) | owner decision |
| D5 example exit status | owner decision |

### 8.3 Recommended scope for the first stable version

Include (with the limits stated in the release notes):

- The compiler, the interpreter and the native C backend on Linux, macOS and Windows.
- WASI compilation of the programs in the matrix.
- The standard library modules that the matrix and examples cover: `symbolic` (with its limits), `solve`, `rational`,
  `polynomial`, `linalg`, `numeric`, `data`, `viz`, `measurements` (dimensional checks).
- Notebooks and the Workbench in the browser (interpreter route only).
- Path and Git dependencies with `--locked`.

Exclude or label experimental:

- Horizon Epsilon history and evidence packages until PR #297 is reviewed.
- Performance claims: the benchmarks remain interpreter-start-up INCONCLUSIVE and linalg_units PARTIAL.
- Any statement that Ostrin matches Mathematica, MATLAB, Julia or Python. None is verified.

### 8.4 Roadmap after the first stable version (prioritised)

| Priority | Item | Impact | Dependency | Difficulty |
|---|---|---|---|---|
| 1 | Integrate the core into `main` and run the GitHub workflows on it | high | owner approval | medium |
| 2 | Add `linalg-wasm-check` to `wasi.yml`; run `wasi.yml` on push | high | 1 | low |
| 3 | macOS run of the Node-based suites and `install-check` | high | 1 | medium |
| 4 | Integrate Horizon Epsilon (PR #297) after the core | medium | 1 | medium |
| 5 | Example exit status (D5) and clear missing-file messages in all examples | medium | none | low |
| 6 | Repository hygiene: decide on the tracked `main.exe` and the lock change | medium | owner | low |
| 7 | Version numbers aligned across `Cargo.toml`, `ostrin.toml` and the release | medium | 1 | low |
| 8 | Firefox and WebKit Playwright projects | medium | none | medium |
| 9 | Package registry | high | design decision | high |

## 9. What this audit did not do

- No push, pull request, merge, release or package publication.
- No change to the branch or to `main`.
- No download of release archives, so the installer was not run end to end.
- No execution of Horizon Epsilon code; PR #297 was read through `gh pr view` only.
- No macOS run.
