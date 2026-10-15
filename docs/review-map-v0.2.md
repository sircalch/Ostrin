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
| Validation/manifests/scorecard | `validation/ast-fallback-ledger.json`, `validation/quality-scorecard.json`, `docs/release-readiness-v0.2.json`, `scripts/project-plan-check.mjs` | Frozen source `cb9a524` measures 7,198 IR + 41 HIR and zero AST fallbacks; the manifest records 12 READY criteria | fallback-ledger, Linguist, project-plan and readiness scripts | Candidate evidence and generated scorecard must be synchronized to the final main commit; the public tag/release remains uncreated pending owner approval |
| Packaging and CI | `.github/workflows/`, `compiler/Cargo.toml`, `README.md`, `CHANGELOG.md`, `docs/releases/v0.2.0.md` | Host/WASI packages for frozen source `cb9a524` have checksums, extraction and smoke evidence; notes distinguish candidate scope from a published release | CI matrix, sanitizers, CodeQL, dependency audit, Browser audit, WASI and release workflow | A later docs/metadata commit changes the candidate head and requires a fresh artifact build; dispatch does not publish, and publication requires explicit owner approval |

## Current revision evidence

This review records the frozen source revision `cb9a52480f0d016921e4d0bccb195ad0faac958b`. CI reports 333 integration tests, 6 differential tests and 3 unit tests; `website-check` reports 17 pages and 302 examples. The public Linguist inventory reports 302 examples and tracks upstream adoption separately.

- [Ostrin CI](https://github.com/sircalch/Ostrin/actions/runs/38520134678) passed
  on Ubuntu, Windows and macOS, including tests and numerical validation.
  [Sanitizers](https://github.com/sircalch/Ostrin/actions/runs/38520142898),
  [CodeQL](https://github.com/sircalch/Ostrin/actions/runs/38520134640),
  [Security audit](https://github.com/sircalch/Ostrin/actions/runs/38520134613)
  and [Browser audit](https://github.com/sircalch/Ostrin/actions/runs/38520134566)
  passed for the same frozen source.
- The [WASI build and runtime matrix](https://github.com/sircalch/Ostrin/actions/runs/38520140712)
  passed all 28 programs. The [release workflow](https://github.com/sircalch/Ostrin/actions/runs/38520144819)
  passed archive build, checksum, extraction and packaged smoke tests on macOS
  ARM64, Linux x86_64 and Windows x86_64; its publish job was skipped because
  the ref was untagged. [Benchmarks](https://github.com/sircalch/Ostrin/actions/runs/38609662881)
  and [website deployment](https://github.com/sircalch/Ostrin/actions/runs/38520134760)
  also passed.
- The readiness manifest marks all 12 criteria `READY`; the candidate remains
  unpublished and untagged pending explicit owner authorization. The local
  approval audit found stale status/scorecard references on `main`; this branch
  updates those docs but does not change the frozen compiler source. A final
  main commit requires a new artifact build before publication approval.
- Nested generic wrappers, broader aggregate combinations, and cancellation
  states beyond the focused pre-run case remain outside the ownership claim.
