# 20. HIR e IR: plan de migración del backend

*Estado: HIR implementado y primera bajada HIR→CFG ejecutable. `--ir`, `--ownership-check` y
`--ownership-ir` consumen temporales y bloques verificados; además, `ir_c.rs` ya genera C
para funciones escalares con ramas, recursión, bucles, `phi` y aritmética comprobada de
enteros de ancho fijo desde esa IR. Las familias gestionadas `String` (incluidos `char_at`, `slice` y `codepoint`), el núcleo de `List<T>`
con elementos escalares, las operaciones escalares de `Map<K,V>`/`Set<T>`, records concretos
y `Option`/`Result` con payload escalar, `String`, `Record`, una colección escalar
(`List`/`Map`/`Set`) u otro wrapper `Option`/`Result` también atraviesan ya el emisor IR.
La forma anidada `List<Option<String>>` comparte ahora ese emisor: cada helper de
lista conserva el payload activo de `Option` y deja los valores `None` sin referencias.
La forma `List<Result<T,E>>` reutiliza el mismo contrato para conservar y liberar
únicamente el payload activo de `Ok` o `Err`.
Las listas de mapas y conjuntos escalares (`List<Map<K,V>>` y `List<Set<T>>`)
reutilizan también sus callbacks existentes de retain/release.
Las listas de arrays numéricos (`List<Array<Int|Float|Float32|Bool>>`) usan el mismo
runtime C con ownership retenido por el destructor de la lista; `viz_orbits` valida
la ruta IR/C, WASI y `live_allocations=0`. Las listas de arrays con cantidades
(`List<Array<Quantity<D>>>`) reutilizan el backing `Array_Float`, conservan la etiqueta
de unidad del array y liberan cada array desde el destructor de la lista.
`Array<T>` numérico escalar también cruza la frontera en constructores 1D–3D, parámetros,
indexación y métodos estructurales/reducciones:
`array(List<T>)`, `array(List<List<T>>)` y `array(List<List<List<T>>>)` usan el runtime C
generado (`Array_Float_from1`/`from2`/`from3`, y sus variantes por tipo), la indexación unidimensional usa sus helpers
`*_index1`, y `shape`/`rank`/`size`, reducciones, `to_list`, `sort`/`cumsum`,
`reshape`/`transpose`/`row`/`col`, `sum_axis`, `dot`/`matmul`, `get`/`set` y
`to_float` usan los helpers tipados disponibles. El pase de ownership conserva
`retain/release` de arrays y de las listas de forma consumidas por esos métodos.
La selección booleana (`a[mask]`), la negación elemento a elemento de `Array<Bool>`,
los cortes `to`/`until` y `where(mask, a, b)` para `Int`, `Float`, `Float32` y `Bool`
también bajan a los kernels tipados de IR/C; los operandos escalares de `where` se
materializan y liberan dentro del mismo temporal nativo.
Los builtins numéricos `zeros`/`ones`, `norm`, `abs` escalar y `sqrt` usan también
los helpers tipados de IR/C (`Array_Float_full`, `Array_Float_norm` y las funciones
de `libm`), de modo que QR y Cholesky ya no necesitan el emisor AST. LU ya está disponible en
`std.numeric`, y los métodos de records concretos no recursivos ya intentan la ruta IR/C cuando
sus campos y ownership son compatibles. Los records recursivos y los recorridos matriciales que
requieren ownership anidado permanecen en el fallback verificado hasta que el backend de agregados
complete esa migración. La SVD densa (`std.numeric.svd`) añade
el mismo tipo de record y recorridos Jacobi; su paridad está cubierta y los métodos concretos
compatibles ya usan IR/C, mientras los recorridos que requieren agregados anidados siguen como
deuda explícita del lowering de agregados.
`read_file`/`write_file` añaden `Result<String, String>` y `Result<Void, String>` con errores
de archivo administrados, comprobación de lectura/escritura/cierre y un checkpoint de cancelación
antes de cruzar la libc; la operación de archivo sigue siendo bloqueante mientras está dentro del
host. Todos estos valores conservan sus marcadores de ownership, transferencia de `Phi` simples y
patrones simples
`Some`/`None`; las funciones globales sin entorno y las lambdas con capturas inmutables usadas
como valores —incluidas closures anidadas con capturas transitivas— también cruzan ahora la IR
mediante `ClosureMake`/`ClosureCall` y adaptadores al ABI `(env, args...)`; el backend mantiene
HIR/AST como fallback verificado para otros payloads gestionados e iteradores propios indirectos,
patrones anidados con payloads gestionados, agregados complejos y escapes mientras la migración crece. Los `for`
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
| Emisor IR | `ir_c.rs` | Genera C desde SSA/CFG para funciones escalares, rangos enteros direccionales, `String` (incluidos `char_at`, `slice` y `codepoint`), `hash(String)` y otros hashes escalares estables, conversiones numéricas comprobadas a `Int` y enteros de ancho fijo, constructores 1D–3D, parámetros, indexación y métodos de arrays numéricos escalares, builtins científicos `histogram`, `linspace`, `pow`, `atan2` (`Float`/`Float32`), `norm_pdf` y `norm_cdf`, `parse_csv` con ownership de `List<List<String>>`, `read_file`/`write_file` (`Result<String,String>`/`Result<Void,String>`), records concretos y records genéricos monomorfizados, iteradores de records concretos y genéricos monomorfizados mediante métodos registrados, canales (`send`/`close`/`receive`), `Option<Record>`, `List<T>` escalar (incluido `List<String>.join`) y listas de arrays numéricos y con cantidades (`List<Array<Int|Float|Float32|Bool|Quantity<D>>>`), operaciones hash escalares de `Map`/`Set`, wrappers sobre `List`/`Map`/`Set` y wrappers `Option`/`Result` anidados con `match`, `Option<T>` escalar/`String` con `Some`/`None`, `try catch` con handlers globales, aliases locales sin entorno y handlers locales capturados compatibles, cierres capturados con entorno tipado y destructor, llamadas indirectas, ramas, bucles, `phi` y enteros de ancho fijo comprobados; emite ownership para las familias migradas y deja fallback seguro para lo demás |
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
   La prueba diferencial conserva el baseline actual de fallback (35 funciones AST, con
   6 632 funciones generadas por HIR/IR —6 609 IR y 23 HIR— sobre los ejemplos); `atan2` escalar para
   `Float` y `Float32` usa el runtime determinista desde IR/C; `pow` para `Float` y
   `Float32` usa `ostrin_dm_pow` y `ostrin_dm_powf`; las coerciones
   `Float↔Float32` de bindings explícitos y agregados `List/Set<Float32>`, las operaciones escalares
   reflejadas de `Complex` (`Float +|−|×|÷ Complex`) y `Bool.to_string()` ya usan IR/C, con paridad
   intérprete/nativo/WASI y leak-check; las llamadas genéricas registradas por el checker se resuelven
   en HIR antes de construir la IR, de modo que
   `numeric_units.ostrin` ya no cae al emisor AST. `Quantity.to_string()` escalar y
   `Array<Float32>.to_string()` usan helpers nativos desde IR/C y el ejemplo `arrays_3d` ya no cae al emisor AST. Las coerciones `Float↔Float32` de bindings explícitos y agregados `List/Set<Float32>` se insertan en IR con la misma precisión que el emisor legado. Los patrones superficiales de
   records con campos escalares y genéricos, junto con enums no genéricos por valor y sus patrones anidados,
   ya usan IR/C con paridad intérprete/nativo/WASI; los payloads gestionados y enums genéricos siguen fuera
   del slice.
   El incremento acotado incluye
   también `Rng` (constructor, métodos escalares, muestreo de arrays y permutación), con
   liberación gestionada y paridad intérprete/nativo. Los combinadores de listas (`map`, `filter`,
   `fold`, `any`, `all`, `find`) aceptan cierres IR con capturas. Los iteradores genéricos
   sustituyen su elemento concreto para payloads gestionados anidados. `parse_csv` ya construye
   `List<List<String>>` desde IR/C con liberación de filas temporales y validación WASI; los agregados anidados e
   iteradores compuestos permanecen en la cola de migración. El flujo `dataframe.ostrin` usa
   además el builtin `panic` y `corr` de arrays desde IR/C, con recorrido de columnas anidadas
   y salida nativa comprobada.
   El pase de ownership considera los receptores y argumentos de métodos como llamadas prestadas,
   igual que los parámetros de una función: los emisores nativos retienen lo que un método
   almacena o devuelve y el caller puede liberar su último alias después de la llamada. Esto
   permite bajar cadenas de construcción de `Figure`, `Table` y `Scene3D` sin una lista manual
   de nombres; `viz_hexbin` es la regresión representativa con `ast=0`.
   La preparación numérica de histogramas y violines ya atraviesa IR/C mediante `histogram`,
   `linspace`, `pow`, `norm_pdf` y `norm_cdf`, `std.viz::uid` usa `hash(String)` en IR/C y `std.viz::render` recorre
   explícitamente sus series. HIR sustituye `Self` por el propietario concreto en las firmas de
   métodos de traits, por lo que los operadores de `Complex` también cruzan IR/C; las
   las instanciaciones genéricas de trazado con `Quantity` permanecen en fallback HIR/AST:
   el intento de bajar `Figure.unit_line`/`unit_scatter` a IR/C expuso un alias prestado de la
   etiqueta de unidad después de liberar el array propietario; se retomarán cuando el contrato de
   ownership de `Quantity` escalar esté explícito,
   además de los agregados de LU, SVD, número de condición, autovectores y `ComplexVector`/`ComplexMatrix`,
   que quedan pendientes de migrar a IR, mientras los métodos de records recursivos conservan
   HIR/AST por seguridad, y solo permite reducirlo o justificar explícitamente
   otro aumento.
   Los destructores de tareas generados se registran con la firma ABI `void (*)(void*)` del
   runtime; la suite completa de ejemplos nativos corre bajo UBSan e incluye cancelación de
   tareas para evitar regresiones de punteros a función incompatibles.
2. Migrar el backend C por **familias de nodos** al HIR (literales/operadores → llamadas → records/enums → patrones → colecciones → genéricos), eliminando la reinferencia correspondiente en cada paso.
3. **IR de bloques básicos** y generación de C desde el IR (el HIR deja de generar C directamente). La primera CFG observable ya existe en `--ir` y el emisor consume ramas, recursión, bucles con `phi`, rangos enteros direccionales, aritmética comprobada de ancho fijo, `String`, constructores 1D–3D, parámetros, indexación, selección booleana `a[mask]`, cortes `to`/`until`, `not` sobre `Array<Bool>`, `where(mask, a, b)` y métodos de `Array<T>` numérico escalar, aritmética y reducciones de `Array<Quantity<D>>` con etiqueta de unidad, records concretos con campos anidados, records genéricos monomorfizados con campos escalares, iteradores de records concretos y genéricos monomorfizados (`next() -> Option<T>`), canales con `send`/`close`/`receive` y `for`, `spawn {}` con CFG soportado, capturas inmutables, closures capturados con entorno tipado, `spawn_scope {}` inline con drenado de grupos, tareas anidadas con capturas propagadas y `Task.join()`, el núcleo de `List<T>`, operaciones hash escalares de `Map`/`Set` y lookups `Option` escalares/String/Record con `Some`/`None`; faltan otras familias de arrays cuyo elemento sea gestionado, rangos con cantidades, cambios de forma complejos, iteradores indirectos, scopes anidados, otros `Option` gestionados, patrones anidados con payloads gestionados, agregados complejos y la retirada progresiva del fallback.
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
   anidados con payloads gestionados, llamadas que transfieren ownership, scopes anidados y escapes complejos. Las cadenas
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
