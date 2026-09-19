# 20. HIR e IR: plan de migración del backend

*Estado: especificación. Lo implementado hasta hoy (tabla de tipos por expresión, sustituciones por llamada genérica, tipos de literales, detector de divergencias) es la **Etapa 1–2 del plan**; este documento fija lo que falta y en qué orden, para que la gestión de memoria (documento 18) y las funciones como valores tengan dónde apoyarse.*

## 1. Qué existe ya (comprobado en el código)

| Pieza | Dónde | Papel en la migración |
|---|---|---|
| `TypedProgram.expr_types` | `typeck` | Tipo real (`Ty`) de cada expresión (clave: archivo + rango) |
| `TypedProgram.call_substs` | `typeck` | Argumentos de tipo por llamada genérica |
| `TypedProgram.literal_kinds` | `typeck` | Tipo elegido para cada literal numérico |
| `NativeTypeReport` | `codegen` | Detecta divergencias checker↔backend (0 hoy, en ~1 350 expresiones) |
| Intérprete como oráculo | `interpreter` | Semántica de referencia; pruebas diferenciales automáticas |

Por tanto el backend **ya no infiere solo**: la reinferencia que queda (`bind_type`, `expected`, `settle_literal`) es respaldo verificado.

## 2. HIR (árbol tipado y desazucarado)

Un árbol por función, **con un tipo en cada nodo**, construido a partir del AST y de `TypedProgram`:

```text
HirFunction { name, params: [(name, Ty)], ret: Ty, body: HirBlock }
HirExpr { ty: Ty, kind: HirKind }
HirKind =
  Lit(value) | Local(id) | Global(name)
  | Call { callee: Callee, args: [HirExpr], subst: {T ↦ Ty} }     // subst ya resuelta
  | MethodCall { recv, method: Resolved(impl, method), args }       // método ya resuelto (estático o vtable)
  | Binary/Unary { op, .. }                                         // operadores de usuario ya resueltos a llamadas
  | Field { obj, index } | Index { obj, idx }
  | If | Match { arms con patrones ya normalizados } | Loop | Block
  | Construct { ty, fields } | VariantCtor { enum, variant, args }
  | Lambda { captures: [Local], body }                              // capturas explícitas
  | Spawn | ChannelNew | Try { .. }
```

Desazucarado: `for` sobre rangos/listas/iteradores → `Loop` con `next()`; `try` → `Match` + `return`; `a @ b` → `matmul` (ya en el parser);
argumentos nombrados/por defecto → posicionales; patrones anidados → cadenas de tests; `x = v` sin `mut` → binding nuevo.

**Invariantes verificables** (un verificador recorre el HIR tras la construcción): todo nodo tiene `ty` sin `Unknown`; toda llamada tiene su
`subst` completa; todo `Local` está declarado; ningún azúcar sobrevive.

## 3. IR (bloques básicos)

Del HIR se baja a un IR de **valores temporales explícitos y bloques básicos** (CFG):

```text
fn f(a: T) -> U { bb0: %1 = call g(a); %2 = field %1.x; br %2 ? bb1 : bb2; ... ret %n }
```

Sobre este IR se hacen los análisis que el texto C no permite:

1. **Último uso / movimiento** (documento 18): decide dónde van `retain`/`release` y elimina los innecesarios.
2. **E1101 estático**: enviar por un canal *mueve*; usar después es error de compilación (hoy es una comprobación dinámica en ambos backends).
3. **Escape** (para arenas): un valor que no sale de su función puede vivir en una arena.
4. **Cierres**: capturas explícitas → estructura `{ fn_ptr, entorno }` (funciones como valores de primera clase).
5. Optimización: inlining, plegado de constantes, eliminación de código muerto, fusión de bucles sobre `Array`.

## 4. Orden de implementación (cada paso mantiene verdes las pruebas diferenciales)

1. **HIR + verificador + `--hir`** para *todo* lo que el checker tipa; medida de cobertura por ejemplo (ratchet).
2. Migrar el backend C por **familias de nodos** al HIR (literales/operadores → llamadas → records/enums → patrones → colecciones → genéricos), eliminando la reinferencia correspondiente en cada paso.
3. **IR de bloques básicos** y generación de C desde el IR (el HIR deja de generar C directamente).
4. **RC + último uso** sobre el IR (`--leak-check`: los ejemplos deben terminar sin objetos vivos).
5. **Cierres y funciones como valores**; retirar la comprobación dinámica de E1101.
6. Optimizador y, después, otros backends (LLVM, WASM, GPU) que consumen el mismo IR.

## 5. Riesgos y mitigaciones

- *El HIR duplica al AST*: se mitiga generándolo, no escribiéndolo dos veces; el AST sigue siendo lo que ve el LSP.
- *Cambiar el backend sin perder equivalencia*: las pruebas diferenciales intérprete↔nativo (todos los ejemplos), el detector de divergencias de tipos y el fuzzing del front-end se ejecutan en CI en tres sistemas.
- *Tiempo de compilación*: el HIR es por función y se construye una vez; las instanciaciones genéricas se comparten.
