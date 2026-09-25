# 20. HIR e IR: plan de migración del backend

*Estado: HIR implementado y primera bajada HIR→CFG ejecutable. `--ir`, `--ownership-check` y
`--ownership-ir` consumen temporales y bloques verificados; además, `ir_c.rs` ya genera C
para funciones escalares con ramas, recursión, bucles, `phi` y aritmética comprobada de
enteros de ancho fijo desde esa IR. Las familias gestionadas `String`, el núcleo de `List<T>`
con elementos escalares, las operaciones escalares de `Map<K,V>`/`Set<T>`, records concretos
y `Option`/`Result` con payload escalar, `String`, `Record`, una colección escalar
(`List`/`Map`/`Set`) u otro wrapper `Option`/`Result` también atraviesan ya el emisor IR.
`Array<T>` numérico escalar también cruza la frontera en constructores 1D–3D, parámetros,
indexación y métodos estructurales/reducciones:
`array(List<T>)`, `array(List<List<T>>)` y `array(List<List<List<T>>>)` usan el runtime C
generado (`Array_Float_from1`/`from2`/`from3`, y sus variantes por tipo), la indexación unidimensional usa sus helpers
`*_index1`, y `shape`/`rank`/`size`, reducciones, `to_list`, `sort`/`cumsum`,
`reshape`/`transpose`/`row`/`col`, `sum_axis`, `dot`/`matmul`, `get`/`set` y
`to_float` usan los helpers tipados disponibles. El pase de ownership conserva
`retain/release` de arrays y de las listas de forma consumidas por esos métodos.
`read_file`/`write_file` añaden `Result<String, String>` y `Result<Void, String>` con errores
de archivo administrados, comprobación de lectura/escritura/cierre y un checkpoint de cancelación
antes de cruzar la libc; la operación de archivo sigue siendo bloqueante mientras está dentro del
host. Todos estos valores conservan sus marcadores de ownership, transferencia de `Phi` simples y
patrones simples
`Some`/`None`; las funciones globales sin entorno y las lambdas con capturas inmutables usadas
como valores —incluidas closures anidadas con capturas transitivas— también cruzan ahora la IR
mediante `ClosureMake`/`ClosureCall` y adaptadores al ABI `(env, args...)`; el backend mantiene
HIR/AST como fallback verificado para otros payloads gestionados e iteradores propios indirectos,
patrones anidados, agregados complejos y escapes mientras la migración crece. Los `for`
sobre rangos enteros —incluidos `to`/`until`, pasos positivos/negativos y paso cero— ya
se bajan a CFG y se emiten desde IR. Los iteradores de records concretos y los genéricos
monomorfizados con
`Iterator<T>` y `next() -> Option<T>` también bajan a CFG, especializan los patrones de
receiver (`Cursor<T>` → `Cursor<Int>`), resuelven su método C estático y
conservan la liberación del record iterador. Los canales bajan a la misma forma de polling
`Option<T>` para `send`, `close`, `receive` y `for`. `spawn {}` con CFG de bloques soportados,
con capturas inmutables por valor, genera un callback C nativo con entorno y `Task.join()` consume
el helper real del runtime; `spawn_scope {}` abre y drena el grupo estructurado mediante la misma
IR cuando sus hijos usan la ABI soportada; las tareas anidadas con capturas propagadas ya comparten
esa ABI, mientras los scopes anidados y escapes complejos siguen en fallback.*

## 1. Qué existe ya (comprobado en el código)

| Pieza | Dónde | Papel en la migración |
|---|---|---|
| `TypedProgram.expr_types` | `typeck` | Tipo real (`Ty`) de cada expresión (clave: archivo + rango) |
| `TypedProgram.call_substs` | `typeck` | Argumentos de tipo por llamada genérica |
| `TypedProgram.literal_kinds` | `typeck` | Tipo elegido para cada literal numérico |
| `NativeTypeReport` | `codegen` | Detecta divergencias checker↔backend (0 hoy, en ~1 350 expresiones) |
| `IrProgram` / `IrFunction` / `IrBlock` | `ir.rs` | Primera CFG con temporales explícitos, terminadores y verificador de destinos |
| Emisor IR | `ir_c.rs` | Genera C desde SSA/CFG para funciones escalares, rangos enteros direccionales, `String`, constructores 1D–3D, parámetros, indexación y métodos de arrays numéricos escalares, `read_file`/`write_file` (`Result<String,String>`/`Result<Void,String>`), records concretos y records genéricos monomorfizados, iteradores de records concretos y genéricos monomorfizados mediante métodos registrados, canales (`send`/`close`/`receive`), `Option<Record>`, `List<T>` escalar, operaciones hash escalares de `Map`/`Set`, wrappers sobre `List`/`Map`/`Set` y wrappers `Option`/`Result` anidados con `match`, `Option<T>` escalar/`String` con `Some`/`None`, `try catch` con handlers globales, aliases locales sin entorno y handlers locales capturados compatibles, cierres capturados con entorno tipado y destructor, llamadas indirectas, ramas, bucles, `phi` y enteros de ancho fijo comprobados; emite ownership para las familias migradas y deja fallback seguro para lo demás |
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
   de compilación en `--check`, `--run`, `--emit-c` y `--compile`. El pase propaga el estado sobre
   el CFG alcanzable hasta punto fijo: no mezcla ramas mutuamente excluyentes, conserva el posible
   movimiento en joins y backedges, y evalúa cada operando `Phi` sólo en la arista de su predecesor.
   Records inmutables quedan fuera de la regla; las comprobaciones dinámicas se conservan como red
   de seguridad y su estado acompaña al allocation mientras vive, nunca a una dirección reciclable;
   un `receive()`/`select` que extrae el record limpia el estado de vuelo para el binding receptor.
3. **Escape** (para arenas): un valor que no sale de su función puede vivir en una arena.
4. **Cierres**: capturas explícitas → estructura `{ fn_ptr, entorno }` (funciones como valores de primera clase). `ClosureMake` conserva los ValueIds capturados y genera una función auxiliar IR más un entorno C registrado; `ClosureCall` valida la firma y llama mediante `(env, args...)`.
5. Optimización: inlining, plegado de constantes, eliminación de código muerto, fusión de bucles sobre `Array`.

## 4. Orden de implementación (cada paso mantiene verdes las pruebas diferenciales)

1. **HIR + verificador + `--hir`** para *todo* lo que el checker tipa; medida de cobertura por ejemplo (ratchet).
   `--native-type-report` publica también `ir-generated`, `hir-generated` y `ast-fallback`.
   El mismo informe agrupa esas cifras por archivo fuente con líneas `native-source`, y la
   prueba diferencial comprueba que la suma por módulo coincide con los totales globales.
   La prueba diferencial conserva el baseline actual de fallback (1 325 funciones agregadas
   sobre los ejemplos); el incremento acotado viene de `std.viz.boxplot`, que queda pendiente
   de migrar a IR, y solo permite reducirlo o justificar explícitamente otro aumento.
   Los destructores de tareas generados se registran con la firma ABI `void (*)(void*)` del
   runtime; la suite completa de ejemplos nativos corre bajo UBSan e incluye cancelación de
   tareas para evitar regresiones de punteros a función incompatibles.
2. Migrar el backend C por **familias de nodos** al HIR (literales/operadores → llamadas → records/enums → patrones → colecciones → genéricos), eliminando la reinferencia correspondiente en cada paso.
3. **IR de bloques básicos** y generación de C desde el IR (el HIR deja de generar C directamente). La primera CFG observable ya existe en `--ir` y el emisor consume ramas, recursión, bucles con `phi`, rangos enteros direccionales, aritmética comprobada de ancho fijo, `String`, constructores 1D–3D, parámetros, indexación y métodos de `Array<T>` numérico escalar, records concretos con campos anidados, records genéricos monomorfizados con campos escalares, iteradores de records concretos y genéricos monomorfizados (`next() -> Option<T>`), canales con `send`/`close`/`receive` y `for`, `spawn {}` con CFG soportado, capturas inmutables, closures capturados con entorno tipado, `spawn_scope {}` inline con drenado de grupos, tareas anidadas con capturas propagadas y `Task.join()`, el núcleo de `List<T>`, operaciones hash escalares de `Map`/`Set` y lookups `Option` escalares/String/Record con `Some`/`None`; faltan arrays cuyo elemento sea gestionado, rangos con cantidades, iteradores indirectos, scopes anidados, otros `Option` gestionados, patrones anidados, agregados complejos y la retirada progresiva del fallback.
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
   binds simples; la IR aún deja barreras explícitas para otros `Option` gestionados, patrones
   anidados, llamadas que transfieren ownership, scopes anidados y escapes complejos. Las cadenas
   lineales de `unwrap`/`unwrap_or`/`ok`/`ok_or` sobre `Option<Option<String>>` y
   `Result<Option<String>, String>` ya extraen y retienen payloads recursivos en IR/C; el ejemplo
   `native_ir_nested_wrappers.ostrin` comprueba ramas y fallbacks con paridad y cero fugas.
5. **Cierres y funciones como valores**: las funciones globales sin entorno y las lambdas capturadas
   ya tienen `ClosureCall`/`ClosureMake`, adaptadores nativos y ownership del entorno; las closures
   anidadas con capturas transitivas también se propagan a helpers IR anidados. Quedan formas no
   lineales con scopes/escapes complejos y handlers locales no lineales. Retirar la comprobación dinámica de
   E1101 requiere que el backend consuma la IR transformada de forma completa.
6. Optimizador y, después, otros backends (LLVM, WASM, GPU) que consumen el mismo IR.

## 5. Riesgos y mitigaciones

- *El HIR duplica al AST*: se mitiga generándolo, no escribiéndolo dos veces; el AST sigue siendo lo que ve el LSP.
- *Cambiar el backend sin perder equivalencia*: las pruebas diferenciales intérprete↔nativo (todos los ejemplos), el detector de divergencias de tipos y el fuzzing del front-end se ejecutan en CI en tres sistemas.
- *Tiempo de compilación*: el HIR es por función y se construye una vez; las instanciaciones genéricas se comparten.
