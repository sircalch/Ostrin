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

AST fallback leak (fixed). A closure created inside another closure's body was never released on the AST codegen path, and
the closure environments were not sound either. There were three causes, all in `compiler/src/codegen.rs`:

1. Ownership tracking was off in lambda bodies (`lambda_depth != 0`), so the body's locals were never released. A lambda body
   now gets its own ownership stacks (`enter_owned_context` / `leave_owned_context`), and its locals are released by the block
   that owns them.
2. The environment did not retain its captured managed values, so a captured cell could be freed while a closure still pointed
   at it (a use-after-free that was hidden on earlier runs).
3. The environment had no drop function, so a released environment did not release its captures.

The fix retains each managed capture when the environment is built and releases it in the environment's drop function. The
drop function does not free the environment: `ostrin_release` does that. `captured_lambda_env_leak_on_ast_path` is now an active
test (`live_allocations=0`). The IR path was already correct.

## Shadowing in native code (fixed)

The interpreter gives `outer q: 7` for `compiler/tests/fixtures/loop_body_shadow.ostrin`. HEAD's native IR gave `200`.

Cause (`compiler/src/ir.rs`): a declaration (`let`) went through `Builder::bind`, which rebinds the nearest visible binding
of the same name. A body `mut q` therefore overwrote the outer `q`, and the value survived the block. A second cause:
`snapshot_visible` flattened the scopes with the innermost value winning, so a body shadow leaked into the loop's and
the branches' snapshots.

Fix: a declaration uses `Builder::declare` (innermost scope only). An assignment keeps `bind` (nearest visible binding).
A snapshot keeps the outermost binding of each name, which is the one that persists after a scope ends. (This rule was
wrong inside a scope that itself shadows the name; see "Shadowing inside a shadowing scope" below, which replaces it.)

HEAD's native IR also diverged on `compiler/tests/fixtures/shadowing_scopes.ostrin` (`block shadow: 51` instead of 1,
`early: 6 12` instead of `6 10`, `nested … k: 11` instead of `k: 100`, `bc outer: 3` instead of 0). After the fix the IR,
the forced HIR path and the forced AST path all give the interpreter's output
(`shadowing_scopes_agree_in_every_backend_path`). The generated C of the HIR and AST paths emits a `return with no value`
warning for functions returning from inside a block; the output is correct and the warning predates this change.
(That bare `return;` in a function with a result is an error for clang, `-Wreturn-mismatch`, and gcc accepts it. It is no longer
emitted for functions with a result since commit `66b5df9a`.)

## Shadowing inside a shadowing scope (fixed)

The outermost-wins snapshot above was wrong for a branch, a loop or a `match` nested inside a scope that itself shadows
a variable. For `mut x = 1; if true { mut x = 10; if true { x = 20 }; print(x) }; print(x)` the interpreter and the HIR and AST
paths print `20` and `1`; the native IR printed `1` and `1`: the nested join read the outer `x` (value 1) instead of the
shadow, and `restore_visible` wrote the outer value into every scope that had the name. The same happened for `else`
branches, `while` and `for` loops inside the shadow and three nested levels of one name (9 of the 17 lines of
`compiler/tests/fixtures/shadowing_nested_control_flow.ostrin`). On `main` the same program printed `20` and `20` on the
native IR path, wrong in the other direction: a body-local `mut` overwrote the outer variable.

Fix (`compiler/src/ir.rs`): `snapshot_visible` resolves each name to the innermost binding again, as a read of the name does.
What outermost-wins was protecting is a shadow declared *inside* a loop body, in a block nested in it or in a `match` arm
leaking out on `break`, `continue` and arm edges. That is handled by bounding those snapshots to the scopes that were open
when the construct started (`snapshot_visible_to`, `LoopEdges::depth`, and the entry depth in `lower_match`).
`restore_visible` writes a snapshot back only to the nearest scope that has the name, and `lower_match` closes the arm's
scope before it restores. Tests: `shadowing_nested_control_flow_agrees_in_every_backend_path` (the interpreter and the IR, HIR
and AST paths, 17 lines worked out by hand), with `shadowing_scopes_agree_in_every_backend_path` and `loop_body_shadow`
unchanged.

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
