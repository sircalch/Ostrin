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

## Known defect: closure assignment to an outer name (compiler/tests/fixtures/loop_closure_mutation.ostrin)

```
hits = 0
bump = fn(x) { hits = hits + x }
for i in 0 until 4 { bump(i) }
```

- Interpreter: `closure mutation: 6`. The closure rebinds the outer name, which follows lexical scoping.
- Native backend: `closure mutation: 0`. The closure does not update the outer binding.

The interpreter result is pinned by `loop_closure_mutation_pins_the_interpreter_semantics`. The native result is
recorded by `loop_closure_mutation_agrees_on_native_backend`, which is `#[ignore]`d until the native backend is fixed. It
is not weakened to the native value. Status: **open defect in the native backend**.

## Status of the optimisation

- Done: the frame representation (`ffe90033`, see the micro-benchmark evidence in `benchmarks/micro/`).
- Not done: frame reuse in range loops. It has to be limited to bodies that cannot capture or retain the loop frame,
  and no such analysis exists yet. Until it does, the per-iteration frame is kept.
- Not done: slot-based lexical resolution. It would change how every variable is found, so it needs its own measured
  change.
