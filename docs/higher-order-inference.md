# Contextual inference for higher-order functions: findings and status

Status: BLOCKED for general inference in this phase. The compiler refuses the native case with an explicit diagnostic
(`cannot determine the type of the function value 'f'`). The interpreter runs the program. No special case was added.

## What the checker does today

- `typeck/mod.rs` (`infer_lambda`) gives each lambda parameter the expected type when one is available: an annotation, the
  declared `fn` type of a parameter of a plain user function (`check_call`), or the element/argument type of a collection
  method (`collection_method_expected_args`). Otherwise the parameter is `Ty::Unknown`.
- There are no inference variables. `Ty` (types.rs) has `Unknown` as a placeholder, not a variable that can be solved
  later. Generic functions are instantiated by matching declared type parameters against argument types
  (`unify_generic_type`), one call at a time; nothing is solved across statements.
- Consequence: `apply2 = fn(f, v) { f(v) }` followed by `apply2(fn(z) { z + 1 }, 2)` is accepted by the interpreter (it
  dynamically dispatches), but the static type of `f` is unknown. The native backends need static types, so they refuse.

## What a general solution needs

1. Inference variables for unannotated lambda parameters and for unannotated `let` bindings of lambdas.
2. Constraints from the body (`f(v)` constrains `f` to `fn(T) -> R` and `v` to `T`) and from every use of the binding
   (the call `apply2(g, 2)` constrains the parameters).
3. A solver with occurs-check and a rule for ambiguity: a variable with no constraint is an error, not a default type.
4. The result must be written back into the typed program (`node_types` is keyed by expression address). The native
   backends read those types, so the solved types must reach them. That means the checker, the typed-program
   representation and both lowering paths (IR, HIR) change together.

## Options considered

- A. Full unification with inference variables (described above). Compatible with the design (static types, explicit
  diagnostics), but it changes the checker and the typed representation. Estimate: a multi-week change touching
  `typeck/mod.rs` (7,352 lines), `types.rs`, and the HIR and IR lowering.
- B. Monomorphisation per call site for unannotated lambdas bound to a name: check the body again for each unique call
  signature. Also general, but it creates one native specialisation per signature, which the current emitters do not
  name or track, so it needs the same lowering changes plus a naming scheme.
- C. Keep the current rule and make the diagnostic precise (done in an earlier phase). This is what is implemented.

## Recommendation

Option A, as a separate design task with its own tests: the inference rules, the ambiguity diagnostics, and the agreement
test across interpreter, IR, HIR and AST. This phase does not implement it, because a partial solver would silently
accept programs the design has not yet defined, which the request rules out.

## Tests that pin the current behaviour

- `untyped_closure_parameter_is_refused_by_the_native_backend_with_a_diagnostic` (examples.rs): the native backend refuses
  the program with the explicit message.
- `function_value_capture.ostrin` and `capture_declarations.ostrin` (fixtures/ownership): annotated function values and
  declaration-based captures agree on every route.
