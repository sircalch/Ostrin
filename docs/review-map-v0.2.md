# v0.2 foundation review map

This map divides the draft integration PR into reviewable subsystems. It records
the current claims and their evidence; it is not a substitute for the final
head's CI or a release approval.

| Subsystem | Main files | Primary claim | Validation | Review risk / open boundary |
|---|---|---|---|---|
| Typed HIR/IR and native lowering | `compiler/src/hir.rs`, `compiler/src/ir.rs`, `compiler/src/hir_c.rs`, `compiler/src/ir_c.rs`, `compiler/src/codegen.rs` | `dyn Trait` remains one typed `data + vtable` value through boxing and dispatch; generic default methods resolve to registered specialized vtables | Full compiler suite; `native_trait_defaults_are_lowered_to_ir`; differential ratchet: 7,198 IR + 41 HIR, 0 AST | The backend modules are large; inspect ABI type checks and reject unsupported combinations instead of accepting opaque values |
| Ownership analysis/runtime | `compiler/src/ownership.rs`, `compiler/src/ir_c.rs`, runtime emitters | retain/release preserves payload lifetime for aliases, fields, collections, Phi, closures, tasks and cancellation where covered | `--leak-check`, `live_allocations=0`; focused `dyn Trait` tests; current-head CI ASan/UBSan | Nested generic wrappers and broader aggregate combinations remain explicit limits in the ownership matrix; do not generalize the tested cells |
| Trait-object semantics | `docs/design/15-dyn-trait.md`, `examples/native_trait_defaults.ostrin`, `compiler/tests/fixtures/native_dyn_trait_task_*.ostrin` | Vtable is shared/immutable; payload lifecycle is driven through typed hooks | Interpreter/native parity, task modes, cancellation, WASI matrix | Current task cancellation test covers cancellation before execution; arbitrary cancellation states remain outside the claim |
| Scientific measurements/numerics | `examples/measurement_*.ostrin`, `examples/std_numeric_*.ostrin`, `scripts/numeric-validation.mjs`, `validation/numeric/` | Scalar Measurement ownership and numerical algorithms have explicit parity, residual and reproducibility evidence | CI numerical validation, native leak checks, WASI; linked readiness evidence | `Measurement<Quantity<D>>`, covariance and array measurement remain guarded boundaries |
| Effects/provenance/reproducibility | `compiler/src/effects.rs`, `compiler/src/provenance.rs`, `scripts/experiment-replay.mjs`, `docs/design/26-efectos-cientificos.md` | Inventory reports known and unresolved calls without treating completeness as purity; replay scope is explicit | Effect/provenance reports, replay bundles and WASI checks | Static effect semantics and R3 user-facing artifact maturity remain future work |
| Website/browser | `website/`, `tests/browser/`, `scripts/website-check.mjs`, `docs/website-audit.md`, `docs/linguist.md`, `docs/linguist-usage.md` | Public counts/status derive from repository facts; source-backed catalogue cards cover direct generic fields and `Option`/`Result` wrappers, the dashboard test reads the loaded scorecard, and the Linguist inventory states 334 tracked `.ostrin` files (302 examples) | Local Chromium suite (32 tests), `website-check --wasm`, Linguist gate and unit tests, PR Browser audit and Website checks | Regenerate public facts when inventories or evidence change; upstream Linguist recognition still has 0 external files against a 2,000-file threshold |
| Validation/manifests/scorecard | `validation/ast-fallback-ledger.json`, `validation/quality-scorecard.json`, `docs/release-readiness-v0.2.json`, `scripts/project-plan-check.mjs` | Ledger reports the current 7,198 IR + 41 HIR and zero AST fallbacks; readiness remains evidence-derived | fallback-ledger, Linguist, project-plan and readiness scripts | Regenerate public facts after source changes; overall v0.2 release stays `NOT_READY` until its post-merge candidate/version/tag and final artifacts exist |
| Packaging and CI | `.github/workflows/`, `compiler/Cargo.toml`, `README.md`, `CHANGELOG.md`, `docs/releases/v0.2.0.md` | Host/WASI packaging and candidate smoke paths are reviewable; notes distinguish candidate scope from a published release | CI matrix, sanitizers, CodeQL, dependency audit, Browser audit, WASI and release workflow dispatch | The release workflow dispatch on a branch builds and smoke-tests packages but does not publish; compiler version/tag remains 0.1.0 until the post-merge v0.2 release step |

## Current revision evidence

Documentation and the public status site are synchronized with this revision. The website/browser checks passed on this head (17 pages, 302 examples, 333 integration tests); the public Linguist inventory reports 302 examples and tracks upstream adoption separately.

- Implementation revision audited: `54123b76` (`fix: validate R3 readiness in local and CI bundles`).
  It includes `Option<dyn Speaker>` stored and replaced in a mutable record,
  `Result<dyn Speaker, String>`, `List<Option<dyn Speaker>>`, the direct generic
  `Holder<dyn Speaker>` field, and the matching public catalogue description.
- [Ostrin CI](https://github.com/sircalch/Ostrin/actions/runs/38508656204) passed
  on Ubuntu, Windows and macOS, including the differential corpus, numerical
  validation and website checks. [Sanitizers](https://github.com/sircalch/Ostrin/actions/runs/38508656243),
  [CodeQL](https://github.com/sircalch/Ostrin/actions/runs/38508656238),
  [Security audit](https://github.com/sircalch/Ostrin/actions/runs/38508656197)
  and [Browser audit](https://github.com/sircalch/Ostrin/actions/runs/38508656214)
  passed on this head.
- The [WASI build and runtime matrix](https://github.com/sircalch/Ostrin/actions/runs/38508663521)
  passed all 28 programs and packaged/checksummed the compiler artifact. Local
  `website-check`, Linguist, project-plan, fallback-ledger and distribution
  checks also pass; the repository reports 7,198 IR + 41 HIR functions, zero
  AST fallbacks and 334 tracked `.ostrin` files (302 examples).
- The [release workflow dispatch](https://github.com/sircalch/Ostrin/actions/runs/38508665475)
  passed package tests, builds, smoke checks and archive verification on macOS
  ARM64, Linux x86_64 and Windows x86_64. It does not publish a release; the
  v0.2 version bump and tag are a post-merge release step.
- The readiness manifest remains `NOT_READY` until the v0.2 candidate is
  versioned and its final release artifacts are produced. Nested generic
  wrappers, broader aggregate combinations and cancellation states beyond the
  focused pre-run case remain outside the ownership claim. Later code changes
  require fresh workflows.
