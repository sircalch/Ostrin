# v0.2 foundation review map

This map divides the draft integration PR into reviewable subsystems. It records
the current claims and their evidence; it is not a substitute for the final
head's CI or a release approval.

| Subsystem | Main files | Primary claim | Validation | Review risk / open boundary |
|---|---|---|---|---|
| Typed HIR/IR and native lowering | `compiler/src/hir.rs`, `compiler/src/ir.rs`, `compiler/src/hir_c.rs`, `compiler/src/ir_c.rs`, `compiler/src/codegen.rs` | `dyn Trait` remains one typed `data + vtable` value through boxing and dispatch; generic default methods resolve to registered specialized vtables | Full compiler suite; `native_trait_defaults_are_lowered_to_ir`; differential ratchet: 7,198 IR + 41 HIR, 0 AST | The backend modules are large; inspect ABI type checks and reject unsupported combinations instead of accepting opaque values |
| Ownership analysis/runtime | `compiler/src/ownership.rs`, `compiler/src/ir_c.rs`, runtime emitters | retain/release preserves payload lifetime for aliases, fields, collections, Phi, closures, tasks and cancellation where covered | `--leak-check`, `live_allocations=0`; focused `dyn Trait` tests; CI ASan/UBSan | Generic dyn fields and broader aggregate combinations remain unverified; ownership readiness stays OPEN |
| Trait-object semantics | `docs/design/15-dyn-trait.md`, `examples/native_trait_defaults.ostrin`, `compiler/tests/fixtures/native_dyn_trait_task_*.ostrin` | Vtable is shared/immutable; payload lifecycle is driven through typed hooks | Interpreter/native parity, task modes, cancellation, WASI matrix | Current task cancellation test covers cancellation before execution; arbitrary cancellation states remain outside the claim |
| Scientific measurements/numerics | `examples/measurement_*.ostrin`, `examples/std_numeric_*.ostrin`, `scripts/numeric-validation.mjs`, `validation/numeric/` | Scalar Measurement ownership and numerical algorithms have explicit parity, residual and reproducibility evidence | CI numerical validation, native leak checks, WASI; linked readiness evidence | `Measurement<Quantity<D>>`, covariance and array measurement remain guarded boundaries |
| Effects/provenance/reproducibility | `compiler/src/effects.rs`, `compiler/src/provenance.rs`, `scripts/experiment-replay.mjs`, `docs/design/26-efectos-cientificos.md` | Inventory reports known and unresolved calls without treating completeness as purity; replay scope is explicit | Effect/provenance reports, replay bundles and WASI checks | Static effect semantics and R3 user-facing artifact maturity remain future work |
| Website/browser | `website/`, `tests/browser/`, `scripts/website-check.mjs`, `docs/website-audit.md` | Public counts/status derive from repository facts and browser checks exercise key flows | `website-check --wasm`, Browser audit, cross-browser/accessibility checks | Keep maturity labels aligned; do not imply production readiness or upstream Linguist adoption |
| Validation/manifests/scorecard | `validation/ast-fallback-ledger.json`, `validation/quality-scorecard.json`, `docs/release-readiness-v0.2.json`, `scripts/project-plan-check.mjs` | Ledger and scorecard reflect measured baselines; release gate is derived and ownership stays open | fallback-ledger, Linguist, project-plan and readiness scripts | Generated artifacts must be regenerated after source changes; never hand-set ownership READY |
| Packaging and CI | `.github/workflows/`, `compiler/Cargo.toml`, `README.md`, `CHANGELOG.md` | Supported host/WASI builds and package workflows are reproducible; release notes separate candidate from published releases | CI matrix, sanitizers, CodeQL, dependency audit, Browser audit, WASI and install checks | PR is a large draft integration; all relevant checks must pass on the final head; do not merge without approval |

## Current revision evidence

- Local head: `886627e` (`feat: lower dyn trait ownership through IR`).
- Full local compiler suite: 3 unit tests, 6 differential tests and 330 integration tests passed.
- Local WASI program matrix: 26 programs passed.
- Website, Linguist, fallback-ledger and project-plan checks passed.
- Fresh PR #185 CI, ASan, UBSan, CodeQL, security and Browser audit checks are
  running for this revision; this document does not mark those checks passed.
