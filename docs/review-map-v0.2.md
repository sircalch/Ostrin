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
| Validation/manifests/scorecard | `validation/ast-fallback-ledger.json`, `validation/quality-scorecard.json`, `docs/release-readiness-v0.2.json`, `scripts/project-plan-check.mjs` | Compiler source `d0abbad` measures 7,198 IR + 41 HIR and zero AST fallbacks; the refreshed manifest records 12 READY criteria | fallback-ledger, Linguist, project-plan and readiness scripts | `sourceRevision` names the code used for RC binaries; `evidenceRevision` names the fixed evidence input commit; `releaseTag` remains null until a separately authorized publication |
| Packaging and CI | `.github/workflows/`, `compiler/Cargo.toml`, `README.md`, `CHANGELOG.md`, `docs/releases/v0.2.0.md` | Host/WASI RC packages built from source `d0abbad` have verified checksums, extraction and smoke evidence | CI matrix, sanitizers, CodeQL, dependency audit, Browser audit, WASI and release workflow | RC hashes apply only to d0abbad. Final release artifacts must be rebuilt from the exact tag. A tag push triggers publishing; workflow_dispatch at a tag is not a safe non-publishing rehearsal |

## Current revision evidence

This review records compiler source revision `d0abbad0f674317c95c389b953507365f3fd1a59`, which was `main` when the RC artifacts were built. CI reports 339 Rust-marked tests; `website-check` reports 17 pages, 302 examples and 333 integration tests. The public Linguist inventory reports 302 examples and tracks upstream adoption separately.

Revision terms are deliberately separate: **source revision** is the exact commit compiled into the RC archives below; **evidence revision** is the fixed commit containing the source-backed evidence inputs and publication procedure, referenced by generated scorecard/status files committed afterward; **release tag** is the exact final `v0.2.0` ref and remains unset. The merge commit after this evidence PR is a proposed future tag target, not the source revision of the existing RC artifacts. A tag workflow must rebuild and verify new archives and hashes from that tag.

- [Ostrin CI](https://github.com/sircalch/Ostrin/actions/runs/38681256328) passed
  on Ubuntu, Windows and macOS, including tests and numerical validation.
  [Sanitizers](https://github.com/sircalch/Ostrin/actions/runs/38681322946),
  [CodeQL](https://github.com/sircalch/Ostrin/actions/runs/38681256503),
  [Security audit](https://github.com/sircalch/Ostrin/actions/runs/38681256363)
  and [Browser audit](https://github.com/sircalch/Ostrin/actions/runs/38681256349)
  passed for the same frozen source.
- The [WASI build and runtime matrix](https://github.com/sircalch/Ostrin/actions/runs/38681318079)
  passed all 28 programs. The [release workflow](https://github.com/sircalch/Ostrin/actions/runs/38681320600)
  passed archive build, checksum, extraction and packaged smoke tests on macOS
  ARM64, Linux x86_64 and Windows x86_64; its publish job was skipped because
  the ref was untagged. [Benchmarks](https://github.com/sircalch/Ostrin/actions/runs/38609662881)
  and [website deployment](https://github.com/sircalch/Ostrin/actions/runs/38681256306)
  also passed.
- The readiness manifest marks all 12 criteria `READY` for the evidence scope; the candidate remains
  unpublished and untagged pending explicit owner authorization. The RC
  evidence manifest and readiness links point to source revision `d0abbad`
  and its verified artifact runs. Later evidence-only metadata commits do not
  change that compiler source revision or the artifact hashes.
- Nested generic wrappers, broader aggregate combinations, and cancellation
  states beyond the focused pre-run case remain outside the ownership claim.
