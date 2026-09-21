# 20. HIR e IR: plan de migración del backend

*Estado: HIR implementado y primera bajada HIR→CFG ejecutable. `--ir`, `--ownership-check` y
`--ownership-ir` consumen temporales y bloques verificados; además, `ir_c.rs` ya genera C
para funciones escalares con ramas, recursión, bucles, `phi` y aritmética comprobada de
enteros de ancho fijo desde esa IR. Las familias gestionadas `String`, el núcleo de `List<T>`
con elementos escalares, las operaciones escalares de `Map<K,V>`/`Set<T>`, records concretos
y `Option`/`Result` con payload escalar, `String`, `Record`, una colección escalar
(`List`/`Map`/`Set`) u otro wrapper `Option`/`Result` también atraviesan ya el emisor IR,
incluidos sus marcadores de ownership, transferencia de `Phi` simples y patrones simples
`Some`/`None`; el backend mantiene
HIR/AST como fallback verificado para otros payloads gestionados, patrones anidados,
iteradores, agregados complejos y escapes mientras la migración crece.*

## 1. Qué existe ya (comprobado en el código)

| Pieza | Dónde | Papel en la migración |
|---|---|---|
| `TypedProgram.expr_types` | `typeck` | Tipo real (`Ty`) de cada expresión (clave: archivo + rango) |
| `TypedProgram.call_substs` | `typeck` | Argumentos de tipo por llamada genérica |
| `TypedProgram.literal_kinds` | `typeck` | Tipo elegido para cada literal numérico |
| `NativeTypeReport` | `codegen` | Detecta divergencias checker↔backend (0 hoy, en ~1 350 expresiones) |
| `IrProgram` / `IrFunction` / `IrBlock` | `ir.rs` | Primera CFG con temporales explícitos, terminadores y verificador de destinos |
| Emisor IR | `ir_c.rs` | Genera C desde SSA/CFG para funciones escalares, `String`, records concretos, `Option<Record>`, `List<T>` escalar, operaciones hash escalares de `Map`/`Set`, wrappers sobre `List`/`Map`/`Set` y wrappers `Option`/`Result` anidados con `match`, `Option<T>` escalar/`String` con `Some`/`None`, ramas, bucles, `phi` y enteros de ancho fijo comprobados; emite ownership para las familias migradas y deja fallback seguro para lo demás |
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
2. **E1101 estático**: enviar por un canal *mueve* un valor gestionado; usarlo después es error
   de compilación en `--check`, `--run`, `--emit-c` y `--compile`. Records inmutables quedan
   fuera de la regla; las comprobaciones dinámicas se conservan como red de seguridad.
3. **Escape** (para arenas): un valor que no sale de su función puede vivir en una arena.
4. **Cierres**: capturas explícitas → estructura `{ fn_ptr, entorno }` (funciones como valores de primera clase).
5. Optimización: inlining, plegado de constantes, eliminación de código muerto, fusión de bucles sobre `Array`.

## 4. Orden de implementación (cada paso mantiene verdes las pruebas diferenciales)

1. **HIR + verificador + `--hir`** para *todo* lo que el checker tipa; medida de cobertura por ejemplo (ratchet).
2. Migrar el backend C por **familias de nodos** al HIR (literales/operadores → llamadas → records/enums → patrones → colecciones → genéricos), eliminando la reinferencia correspondiente en cada paso.
3. **IR de bloques básicos** y generación de C desde el IR (el HIR deja de generar C directamente). La primera CFG observable ya existe en `--ir` y el emisor consume ramas, recursión, bucles con `phi`, aritmética comprobada de ancho fijo, `String`, records concretos con campos anidados, el núcleo de `List<T>`, operaciones hash escalares de `Map`/`Set` y lookups `Option` escalares/String/Record con `Some`/`None`; faltan otros `Option` gestionados, patrones anidados, iteradores, agregados complejos y la retirada progresiva del fallback.
4. **RC + último uso** sobre el IR (`--leak-check`: los ejemplos deben terminar sin objetos vivos).
   El runtime ya expone `ostrin_retain`/`ostrin_release`; el emisor C cubre la primera subetapa
   de forma lineal en locales directos: aliases y campos prestados retienen, las reasignaciones
   liberan el valor anterior y los retornos transfieren o retienen según su origen. `String` es
   la primera familia gestionada consumida por el emisor IR: concatena, imprime y compara desde
   temporales C, y prueba `retain/release` alrededor de aliases, `phi` y elementos de listas
   con `--leak-check`; los `Phi` simples transfieren la referencia entrante sin un `retain`
   duplicado y los `Phi` de bucle liberan el valor corriente en backedges con último uso seguro.
   Las listas escalares y las operaciones hash escalares usan los helpers C
   que retienen sus elementos; records construidos desde HIR o IR registran además destructores
   tipados y retienen sus campos. `Option` escalar se copia por valor y `Option<String>`/
   `Option<Record>` retienen/liberan condicionalmente su payload en constructores, lookups y
   binds simples; la IR aún deja
   barreras explícitas para otros `Option` gestionados, patrones anidados, llamadas que
   transfieren ownership, scopes anidados y escapes complejos.
5. **Cierres y funciones como valores**; retirar la comprobación dinámica de E1101 cuando el
   backend consuma la IR transformada de forma completa.
6. Optimizador y, después, otros backends (LLVM, WASM, GPU) que consumen el mismo IR.

## 5. Riesgos y mitigaciones

- *El HIR duplica al AST*: se mitiga generándolo, no escribiéndolo dos veces; el AST sigue siendo lo que ve el LSP.
- *Cambiar el backend sin perder equivalencia*: las pruebas diferenciales intérprete↔nativo (todos los ejemplos), el detector de divergencias de tipos y el fuzzing del front-end se ejecutan en CI en tres sistemas.
- *Tiempo de compilación*: el HIR es por función y se construye una vez; las instanciaciones genéricas se comparten.
