# v0.2 foundation review map

This map divides the draft integration PR into reviewable subsystems. It records
the current claims and their evidence; it is not a substitute for the final
head's CI or a release approval.

| Subsystem | Main files | Primary claim | Validation | Review risk / open boundary |
|---|---|---|---|---|
| Typed HIR/IR and native lowering | `compiler/src/hir.rs`, `compiler/src/ir.rs`, `compiler/src/hir_c.rs`, `compiler/src/ir_c.rs`, `compiler/src/codegen.rs` | `dyn Trait` remains one typed `data + vtable` value through boxing and dispatch; generic default methods resolve to registered specialized vtables | Full compiler suite; `native_trait_defaults_are_lowered_to_ir`; differential ratchet: 7,198 IR + 41 HIR, 0 AST | The backend modules are large; inspect ABI type checks and reject unsupported combinations instead of accepting opaque values |
| Ownership analysis/runtime | `compiler/src/ownership.rs`, `compiler/src/ir_c.rs`, runtime emitters | retain/release preserves payload lifetime for aliases, fields, collections, Phi, closures, tasks and cancellation where covered | `--leak-check`, `live_allocations=0`; focused `dyn Trait` tests; CI ASan/UBSan | Nested generic wrappers and broader aggregate combinations remain unverified; ownership readiness stays OPEN |
| Trait-object semantics | `docs/design/15-dyn-trait.md`, `examples/native_trait_defaults.ostrin`, `compiler/tests/fixtures/native_dyn_trait_task_*.ostrin` | Vtable is shared/immutable; payload lifecycle is driven through typed hooks | Interpreter/native parity, task modes, cancellation, WASI matrix | Current task cancellation test covers cancellation before execution; arbitrary cancellation states remain outside the claim |
| Scientific measurements/numerics | `examples/measurement_*.ostrin`, `examples/std_numeric_*.ostrin`, `scripts/numeric-validation.mjs`, `validation/numeric/` | Scalar Measurement ownership and numerical algorithms have explicit parity, residual and reproducibility evidence | CI numerical validation, native leak checks, WASI; linked readiness evidence | `Measurement<Quantity<D>>`, covariance and array measurement remain guarded boundaries |
| Effects/provenance/reproducibility | `compiler/src/effects.rs`, `compiler/src/provenance.rs`, `scripts/experiment-replay.mjs`, `docs/design/26-efectos-cientificos.md` | Inventory reports known and unresolved calls without treating completeness as purity; replay scope is explicit | Effect/provenance reports, replay bundles and WASI checks | Static effect semantics and R3 user-facing artifact maturity remain future work |
| Website/browser | `website/`, `tests/browser/`, `scripts/website-check.mjs`, `docs/website-audit.md`, `docs/linguist.md`, `docs/linguist-usage.md` | Public counts/status derive from repository facts; source-backed catalogue cards cover direct generic fields and `Option`/`Result` wrappers, the dashboard test reads the loaded scorecard, and the Linguist inventory states 334 tracked `.ostrin` files (302 examples) | Local Chromium suite (32 tests), `website-check --wasm`, Linguist gate and unit tests, PR Browser audit and Website checks | Regenerate public facts when inventories or evidence change; upstream Linguist recognition still has 0 external files against a 2,000-file threshold |
| Validation/manifests/scorecard | `validation/ast-fallback-ledger.json`, `validation/quality-scorecard.json`, `docs/release-readiness-v0.2.json`, `scripts/project-plan-check.mjs` | Ledger and scorecard reflect measured baselines; release gate is derived and ownership stays open | fallback-ledger, Linguist, project-plan and readiness scripts | Generated artifacts must be regenerated after source changes; never hand-set ownership READY |
| Packaging and CI | `.github/workflows/`, `compiler/Cargo.toml`, `README.md`, `CHANGELOG.md` | Supported host/WASI builds and package workflows are reproducible; release notes separate candidate from published releases | CI matrix, sanitizers, CodeQL, dependency audit, Browser audit, WASI and install checks | PR is a large draft integration; all relevant checks must pass on the final head; do not merge without approval |

## Current revision evidence

- Implementation revision audited: `52e902a` (`Support dyn trait values in Option and Result`),
  including `Option<dyn Speaker>` and `Result<dyn Speaker, String>` wrappers, the generic
  `Holder<dyn Speaker>` field and the public examples catalogue.
- At this head, [Ostrin CI](https://github.com/sircalch/Ostrin/actions/runs/38429889889)
  passed on Ubuntu, macOS and Windows, including the generated differential
  corpus, numerical validation and Website/browser checks;
  [ASan and UBSan](https://github.com/sircalch/Ostrin/actions/runs/38429889992),
  [CodeQL](https://github.com/sircalch/Ostrin/actions/runs/38429890224),
  [dependency audit](https://github.com/sircalch/Ostrin/actions/runs/38429890227)
  and [Browser audit](https://github.com/sircalch/Ostrin/actions/runs/38429890260)
  also passed.
- Local evidence for the latest web changes: all 32 Chromium website tests,
  `website-check --wasm`, `linguist-check` and its 8 unit tests passed.
- Local WASI program matrix: 28 programs passed, including generic
  `dyn Trait` field and `Option`/`Result` wrapper cases. The repository reports 7,198 IR + 41 HIR functions,
  zero AST fallbacks and 334 tracked `.ostrin` files (302 examples). Sanitizer
  CI for the new fixture passed on this revision.
- The release gate remains `NOT_READY`: ownership and changelog are still open.
  Nested generic wrappers, broader aggregate combinations and cancellation
  states beyond the focused pre-run case remain outside the ownership claim. This map
  records implementation evidence for `b3bce84`; any later code revision
  requires fresh workflows.
