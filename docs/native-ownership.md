# Native ownership: the rules the generated C follows

This note records the ownership rules of the native backends and the defects that were found and fixed while checking them
with AddressSanitizer, LeakSanitizer and UndefinedBehaviorSanitizer. The evidence is in
[sanitizer-evidence.md](sanitizer-evidence.md), produced by `scripts/native-sanitize.mjs`.

## Rules

- A **borrowed** value is a local, a field or an index. Storing or passing a borrowed managed value takes one more reference
  (`retain`). A **fresh** value (a literal, a concatenation, a call result, a closure literal) already carries one reference,
  and its new owner takes that reference over without a `retain`.
- Assigning to a managed location releases the old value after the new one is evaluated, so self-assignment through an
  alias stays valid. Locals, record fields and capture cells follow the same rule.
- A function value is `{ fn, env }`. Its environment owns one reference to each managed capture. The environment is built
  with a drop function that releases the captures; `ostrin_release` frees the environment itself.
- A fresh function value that is only used for one call (an argument, or the callee of `f(...)()`) is released by the
  caller after the call. A function value read from a name is not released by the call.
- A fresh value consumed by a builtin (for example the String printed by `print`) is released by the statement that
  consumes it.

## Defects fixed in this phase

| Defect | Route(s) before the fix | Fix |
| --- | --- | --- |
| A managed field assignment retained a fresh value, so the value leaked | AST | Retain only borrowed values, as local assignment does |
| `print` of a fresh String was never released | HIR | Release the String after printing; borrowed values are not released |
| A fresh closure passed as an argument kept its environment | AST, HIR, IR | Closure literals and function-typed calls are owned arguments |
| A fresh function value called directly (`build(4)()`) kept its environment | AST, HIR | The callee is released after the call; the result is kept in a temporary first |
| C keywords used as Ostrin identifiers (`double`, `int`, `struct`) produced C that does not compile | AST, HIR, IR | The lexer escapes C-reserved identifiers with a `K_` prefix while native output is built; the map is injective |
| A closure parameter whose type could not be inferred gave the misleading "unknown function" diagnostic | AST | The diagnostic names the function value and the limitation |

No sanitizer report was produced by any program that compiled, so no use-after-free, double free or invalid read was found
in the fixtures. Negative controls with the same flags showed that the detector reports a use-after-free, a double free and
a leak in this toolchain (see sanitizer-evidence.md).

## Known limitations

- Native emission is refused, with a diagnostic, for an untyped closure parameter that holds a function value. The
  interpreter accepts those programs. The native backend does not infer such types.
- The generated C can emit a trailing unreachable `return;` after a `return` in some AST functions. gcc warns about it; it
  does not change behaviour.
- Only GCC 13 under WSL was used for the sanitizer runs. Clang is not installed here, so `NOT_TESTED` applies to Clang.
