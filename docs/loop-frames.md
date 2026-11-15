# Loop frames and closures: semantics before optimisation

This page records the semantic checks that any change to loop frames has to keep, and the state of the work on them. The
rule is that a loop may reuse a frame only where no closure, record or other value can observe the difference. Where that
cannot be shown, the ordinary path stays.

## Semantic checks (examples/loop_semantics.ostrin)

Each value was checked by hand and must agree on the interpreter and on the native backend:

| Check | Expected |
| --- | --- |
| A closure created in each iteration captures that iteration's variable | `capture per iteration: 33` |
| Closures that outlive their iteration keep their own values | `survivors: 10 11 12` |
| A body assignment to an outer name rebinds it and keeps the last value | `outer after loop: 9` |
| Nested loops; the inner closure captures both variables | `nested sum: 99` |
| `continue` skips, `break` stops | `continue sum: 25 break at: 5` |
| Early `return` from inside a loop | `first above 50: 8` |
| Recursion inside a loop | `fib sum: 88` |
| Mutation of a record field shared by every iteration | `record count: 10` |
| A body name that is new in the body does not change an outer name of the same spelling | `shadow untouched: 7` |

## Closures that capture a `mut` binding (fixed in the native backend)

```
hits = 0
bump = fn(x) { hits = hits + x }
for i in 0 until 4 { bump(i) }
```

`docs/design/02-funciones-y-firmas.md` §4 says a closure that captures a `mut` binding reads and reassigns that same
binding. Before the fix the native backends copied the value into the environment, so the closure's writes were lost
(`closure mutation: 0`) and it never saw later outer writes.

Fix: `compiler/src/capture_cells.rs` runs only on the native path, after the original program type-checks. Each captured
`mut` binding becomes a one-field record cell (`OstrinCell<k>`), and the rewritten program is type-checked again. The
interpreter does not run the pass.

Bindings are identified by declaration: each use is resolved through the scope chain to the innermost visible declaration,
so a shadowing declaration and the outer binding of the same name are separate. Only captured `mut` declarations get a
cell. The pass refuses, with an explicit error, the one case it cannot represent: a captured function value whose type is
unknown (for example a lambda whose binding has no annotation, such as `mut op = fn(k) { ... }`; annotate it as
`mut op: fn(Int) -> Int = ...`). The language has no lambda-parameter annotations, so the annotation goes on the binding.

Tests: `loop_closure_mutation_agrees_on_native_backend` (native, 6); `captured_mutable_bindings_share_storage_with_closures_in_both_backends`
(native and interpreter, `--leak-check`, `live_allocations=0`); `captured_mutable_binding_shadowed_in_the_same_function_agrees_natively`
(a captured name declared twice in one function); `captured_mutable_bindings_across_scopes_agree_in_both_backends` (independent
closures, shared cell, a captured name shadowed in a loop, nested captures, a closure that outlives its creator).

Known leak (AST fallback, reproduced on HEAD): a closure created inside another closure's body is not released when the
program is compiled by the AST codegen, because ownership tracking is disabled in lambda bodies. The IR path releases it.
The AST path is taken when a lambda's parameter type cannot be inferred (for example `add = fn(d) { ... }` without any
annotation on the binding). `captured_lambda_env_leak_on_ast_path` is ignored and fails with `live_allocations=1`.
Annotating the binding (`add: fn(Int) -> Int = fn(d) { ... }`) selects the IR path, and that program has no leak.

## Shadowing in native code (fixed)

The interpreter gives `outer q: 7` for `compiler/tests/fixtures/loop_body_shadow.ostrin`. HEAD's native IR gave `200`.

Cause (`compiler/src/ir.rs`): a declaration (`let`) went through `Builder::bind`, which rebinds the nearest visible binding
of the same name. A body `mut q` therefore overwrote the outer `q`, and the value survived the block. A second cause:
`snapshot_visible` flattened the scopes with the innermost value winning, so a body shadow leaked into the loop's and
the branches' snapshots.

Fix: a declaration uses `Builder::declare` (innermost scope only). An assignment keeps `bind` (nearest visible binding).
A snapshot keeps the outermost binding of each name, which is the one that persists after a scope ends.

HEAD's native IR also diverged on `compiler/tests/fixtures/shadowing_scopes.ostrin` (`block shadow: 51` instead of 1,
`early: 6 12` instead of `6 10`, `nested … k: 11` instead of `k: 100`, `bc outer: 3` instead of 0). After the fix the IR,
the forced HIR path and the forced AST path all give the interpreter's output
(`shadowing_scopes_agree_in_every_backend_path`). The generated C of the HIR and AST paths emits a `return with no value`
warning for functions returning from inside a block; the output is correct and the warning predates this change.

## Frames

- Frame representation (`ffe90033`): short ordered list. Changing it to a hash map had been measured faster for large
  frames only; the hybrid below supersedes it.
- Hybrid frame (this commit): up to 16 names are searched linearly; larger frames build a name index. A frame keeps a
  `live` count, so the stale slots of a reused loop frame are kept and not reallocated. A large body that redefines
  120 names on every iteration went from 2.1x slower (list only) to 0.92x of the pre-list build.
- Loop frame reuse for range `for` loops: when the body contains no closure (`fn`) and no `spawn`/`spawn scope`, one frame
  serves every iteration; after each iteration the frame is cut back to the loop variable. Tests:
  `captured_loop_frame_reuse_agrees_in_both_backends` (interpreter and native) and `loop_frame_semantics_agree_in_both_backends`.
- Audit of the reuse condition (`compiler/src/ast_scan.rs`): the only interpreter values that hold an environment are
  `Value::Closure` (created only by `Expr::Lambda`) and `TaskState` (created only by `spawn`/`spawn scope`). A callee
  starts from a root environment, so it cannot see the caller's frame. A body containing a lambda or a spawn, at any depth
  (closure in a branch, match arm, guard, record field, list, call argument, nested loop), is therefore never reused.
  `ast_scan` tests cover twelve positions. The debugger's `current_env` points to the frame of the statement it executes,
  which is replaced at the next statement, so it never exposes a stale frame.
- Stale slots: lookups read only the live part of a frame; an index entry is trusted only when its position is live and
  holds that name. Frame unit tests are in `interpreter::frame_tests`.
- Not done: slot-based lexical resolution (would change variable lookup in every function; needs its own measured change).
