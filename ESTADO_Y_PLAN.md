# Ostrin — estado del proyecto y plan de avance

*Corte: 2026-09-24 · rama `main` · release experimental `v0.1.0` publicada (2026-09-24) · 6 pruebas diferenciales, 202 de integración y 2 unitarias en verde.*

Validación remota: Pages y CI pasaron para `8bc24bd` en Windows, Linux, macOS y web. La
prueba de hilos nativos valida los pares concurrentes sin imponer un orden del planificador
y conserva las barreras de `join()`/scope y cero fugas.

Este documento resume **qué existe hoy**, **qué no**, y **por dónde se puede avanzar**.
Para la historia detallada, ver `CONTEXTO_PROYECTO.md` (secciones 1–266); para el diseño
del lenguaje, `docs/design/` (22 documentos). La auditoría del sitio vive en
`docs/website-audit.md`.

## 0. Estado de un vistazo

| Categoría | Contenido |
|---|---|
| **Implementado** | Compilador + intérprete (referencia semántica), checker con cantidades físicas, records/enums/traits/genéricos, `Option`/`Result`, colecciones, `Array<T>`, módulos y paquetes con lockfile, concurrencia cooperativa determinista, `--native-threads`, backend C con `--leak-check`, build WASI, playground WASM, LSP/DAP y extensión VS Code (VSIX local) |
| **En fallback** | El backend nativo emite desde IR las familias cubiertas (§5); records/enums genéricos aplicados, iteradores indirectos, scopes anidados, handlers no lineales y agregados/escapes complejos caen de forma verificada a HIR y después al AST |
| **Experimental** | Todo el lenguaje (versión 0.x, sin garantía de estabilidad); `--native-threads`; paquetes científicos de ejemplo `tables`, `plot`, `autodiff` (modo directo); dependencias Git con `--fetch` |
| **Pendiente** | Retirar el fallback AST, ownership completo, red, registry público, GPU, autodiff inverso, canales de distribución (Homebrew, winget, Scoop, Chocolatey, AUR), extensión en Marketplace |
| **Release** | [`v0.1.0`](https://github.com/sircalch/Ostrin/releases/tag/v0.1.0) publicada el 2026-09-24 con tres archivos (Linux x86_64, macOS ARM64, Windows x64) y sus `.sha256`; instaladores verificados contra ella en runners limpios (`install-check.yml`); workflows de CI, release y WASI en verde |

---

## 1. Qué es Ostrin

Un lenguaje de propósito general experimental centrado en tres ideas:

1. **Cantidades físicas como tipos**: `5 m / 2 s` lleva su dimensión en el sistema de tipos;
   sumar longitud con tiempo es un error de compilación.
2. **Inmutable por defecto**: `mut` es explícito; un binding mutable no se captura en `spawn`.
3. **Legibilidad**: expresiones, `match` exhaustivo, `try` explícito, `and/or/not`,
   saltos de línea significativos, sin `let`.

Implementación: compilador + intérprete + herramientas de editor, todo en Rust
(`compiler/src`, ~34 500 líneas de Rust), más una extensión de VS Code (`vscode-ostrin/`).

---

## 2. Arquitectura

| Componente | Archivo(s) | Función |
|---|---|---|
| Léxico / parser | `lexer/`, `parser/mod.rs` | Tokens, AST con rangos de origen |
| Módulos y paquetes | `modules.rs`, `package.rs`, `ostrin.toml` | Imports, `--project`, grafo transitivo de dependencias y lockfile portable |
| Verificador de tipos | `typeck/mod.rs` (~3 500 l.) | Tipos, dimensiones, traits, exhaustividad, genéricos |
| HIR/IR | `hir.rs`, `hir_c.rs`, `ir.rs`, `ir_c.rs` | HIR verificado, CFG con temporales explícitos y emisor C para escalares, `String`, `Result<Int, String>`/`Result<Float, String>` con `try`, `try catch` inline, handlers globales, aliases locales y handlers locales capturados compatibles, wrappers `Option`/`Result` sobre `List`, `Map` y `Set` con payload gestionado, `Result<String, String>`/`Result<Void, String>` de E/S de archivos, records concretos y records genéricos monomorfizados, iteradores de records concretos (`next() -> Option<T>`), iteradores genéricos monomorfizados (`Cursor<Int>`), iteración de canales mediante `receive() -> Option<T>`, `spawn {}` con CFG y capturas inmutables, `spawn_scope {}` inline con grupos nativos, `Task.join()` y `Task.cancel()` tipados, `Option<Record>`, listas escalares, mapas/conjuntos escalares, enteros de ancho fijo y control de flujo, con fallback HIR/AST acotado |
| Intérprete | `interpreter/mod.rs` | Ejecución tree‑walking y scheduler cooperativo; referencia semántica |
| Servidor de lenguaje | `lsp.rs`, `symbols.rs`, `protocol.rs` | LSP sobre stdio |
| Adaptador de depuración | `dap.rs` + hooks del intérprete | DAP sobre stdio |
| **Backend nativo** | `codegen.rs`, `hir_c.rs` y runtimes C | Transpila a C y compila con gcc/clang/cc |
| CLI | `main.rs` | `--check --run --ast --tokens --json --lsp --dap --emit-c --compile --native-threads` |
| Editor | `vscode-ostrin/` (v0.4.0) | Resaltado, LSP, DAP, comandos, VSIX |

Regla de oro del proyecto: **el intérprete es la referencia**. Cada capacidad del
backend nativo se valida comparando su salida con `--run`.

---

## 3. El lenguaje (lo que funciona)

**Tipos y valores**: `Int`, `Float`, `Bool`, `String`, `Char`, `Void`; `List<T>`, `Map<K,V>`,
`Set<T>`, `Option<T>`, `Result<T,E>`; cantidades `Quantity<Dim>` con unidades y conversiones
(`as`, `within`, `approximately`); `Int / Int` es división entera.

**Datos**: `record` (identidad por referencia, campos `mut`), `enum` con variantes
con/sin campos (tipos valor), genéricos en ambos, `derive(Eq, Ord, …)`.

**Funciones**: genéricas con cotas, argumentos nombrados y por defecto, lambdas y
**valores de función** (`fn(Int) -> Int`, cierres con captura por valor en nativo),
`try … catch`, recursión.

**Traits**: traits nominales, métodos por defecto, herencia de traits, `impl` genéricos
y especializados (`impl X for Box<Int>`), `impl` sobre `Quantity<D>`, `dyn Trait`,
sobrecarga de operadores vía `impl Add/Sub/Mul/Div/Eq/Ord…`, negación unaria (`neg`) y
escalar a la izquierda (`rmul`, `radd`, …).

**Control**: `if/else`, `while`, `for` (rangos `to`/`until`, listas, canales, iteradores
propios con `next`), `match` con guardas, patrones anidados, rangos y destructuración.

**Concurrencia**: el intérprete conserva el scheduler cooperativo determinista con `spawn`,
`join`, `spawn_scope`, `channel<T>()`, `select([channels])` y `yield()`. El backend nativo mantiene ese modo por defecto
para la paridad reproducible, y `--native-threads` habilita hilos del SO, mutexes/condiciones,
canales bloqueantes, selección entre canales y cancelación cooperativa en puntos seguros con la misma API; `spawn_scope` propaga cancelación a sus grupos activos y drena las tareas hijas. Se verifica en el checker que no se capturen bindings
`mut` (E1100) y el análisis HIR/IR rechaza por defecto reutilizar un valor movible después de
enviarlo (E1101); el detector sigue el CFG alcanzable hasta punto fijo, con operandos `Phi` ligados
a su arista predecesora; `--ownership-check` conserva el informe detallado.

La primera tarea que cruza completamente HIR→IR→C es `spawn {}` con CFG de bloques soportados,
incluyendo capturas inmutables por valor mediante un entorno C con retain/release, y `Task.join()`
cooperativo o sobre hilos nativos. `spawn_scope {}` también baja el grupo estructurado inline
cuando sus hijos usan la ABI nativa soportada; tareas anidadas con capturas propagadas comparten
esa ABI, `Task.cancel()` invoca el helper tipado del runtime para los handles que ya cruzan la
IR, y `yield()` usa el mismo punto de polling/checkpoint que el emisor HIR. Scopes anidados y
escapes complejos conservan el fallback verificado.

**Numérico/científico**: enteros de ancho fijo, `Float32`, `Array<T>` (difusión, máscaras,
rebanadas, `@`), estadística, regresión, `det/inv/eigvals/norm`, `Rng` reproducible,
funciones elementales deterministas (idénticas en intérprete y nativo).

**Datos**: métodos de `String`, `parse_csv`; `Map` y `Set` usan índice hash para claves/elementos
hashables, incluyendo colecciones estructurales, y conservan orden de iteración. Los records/enums
con `Hash + Eq` derivados usan buckets; una igualdad personalizada cae de forma segura a búsqueda
lineal. Paquetes de ejemplo en Ostrin: `tables`
(DataFrame mínimo), `plot` (SVG), `autodiff` (modo directo).

**Igualdad estructural**: `==`/`!=` compara recursivamente `List`, `Map`, `Set`, `Option` y
`Result` por valor en el intérprete y en el backend nativo; mapas y conjuntos no dependen del
orden de inserción.

**Biblioteca estándar** (pequeña): `print`, `args`, `yield`, `env`, `path_join`, `cwd`, `file_exists`,
`format`, `sum`, `panic`, `read_file`, `write_file`, `parse_int`, métodos de `List`
(`map/filter/fold/any/all/find/push/remove_at/length`),
`Map` (`get/set/remove/contains_key/count/keys/values`), `Set`, `Option`, `Result`.
`hash` ofrece hashes estables para escalares, `Option`/`Result`, colecciones y
records/enums con `derive(Hash)` cuyos campos sean recursivamente hashables, con
paridad entre intérprete y nativo; `std.json` añade un DOM, parseo estricto y serialización
portable con soporte para pares sustitutos UTF-16 y rechazo explícito de `\\u0000`; funciones,
arrays, canales y tareas siguen fuera.

**Diagnósticos**: códigos `OSTRIN-Exxxx` con ubicación; salida JSON Lines para editores.

---

## 4. Herramientas de editor

- **LSP** (v0.2→0.3.1): diagnósticos en vivo, hover, completado, definición, signature help,
  referencias y renombrado sobre **todo el workspace** (incluye buffers sin guardar y
  dependencias de `ostrin.toml`), tokens semánticos.
- **DAP** (v0.4.0): breakpoints reales, step over/into/out, pila de llamadas, variables,
  evaluación de expresiones; corre sobre el mismo intérprete.
- **VS Code**: resaltado, comandos check/run, formateo, outline, configuración de depuración;
  usa el servidor y cae a proveedores propios si no está disponible. VSIX empaquetado.

---

## 5. Backend nativo (`--emit-c` / `--compile`)

Transpila a C con expresiones‑sentencia GNU (`({ … })`); compilador vía `OSTRIN_CC`.
Monomorfización bajo demanda (funciones, records, enums, métodos, vtables, listas, mapas…).

La primera familia de funciones ya se emite desde la IR explícita: funciones escalares y la
familia gestionada `String` convierten temporales SSA en temporales C, preservan división
entera, aritmética comprobada de enteros de ancho fijo, concatenación/comparación/impresión
de texto y salida numérica, emiten ramas, recursión, bucles con estado y `phi`, y se cuentan
por separado en `--native-type-report` como `ir-generated`. Los marcadores de ownership de
`String`, `List<T>`, los núcleos escalares de `Map<K,V>`/`Set<T>` y `Option<T>` con payload
escalar o `String` también se consumen al generar C; `Map.get/remove` producen structs
`Option_<T>` por valor y retienen/transfieren sus strings correctamente. Los `for` sobre rangos
enteros también se bajan a CFG con dirección derivada del signo del paso: `to`/`until`, pasos
positivos/negativos y paso cero conservan la semántica del intérprete, incluyendo `break`/`continue`.
Agregados complejos, iteradores propios indirectos, tareas anidadas dentro de `spawn`,
`spawn_scope` y patrones distintos de `Some/None` y
payloads gestionados que no sean `String` caen de forma verificable a HIR y después al AST. Los
iteradores de records concretos y genéricos monomorfizados con `Iterator<T>` y `next() -> Option<T>` ya cruzan la IR, incluida
la llamada de método nativa y la liberación del record iterador. Los canales sin spawn también cruzan
la IR con `send`, `close`, `receive` y `for`, incluida la liberación del handle al último uso.
Las instancias concretas de funciones y métodos genéricos pasan ahora por especialización,
ownership IR y emisor C cuando sus operaciones son representables; `native_hir_generics.ostrin`
registra 6 funciones IR y ninguna HIR, mientras el caso de métodos mantiene una función HIR de
fallback para el retorno de un record genérico. Las pruebas ejecutan ambas rutas con paridad y
`live_allocations=0`; records/enums genéricos aplicados y otras formas complejas aún usan fallback.

La misma ruta ya cubre la familia escalar de `Result`: `String.to_int()` y `to_float()` producen
`Result<Int,String>`/`Result<Float,String>` desde la IR, junto con `Ok`/`Err`, `match`, bindings
de variantes, `is_ok`, `is_err`, `unwrap`, `unwrap_or`, `ok`, `ok_or` y `try` sin `catch`. La rama de error construye
el `Result` de retorno y termina la función; la de éxito extrae el payload. El discriminante se
conserva en el path del binding y el ownership libera condicionalmente el payload activo.
`try catch` con lambda inline, handler global, alias local o handler local capturado compatible ya expande la rama de error y retorna un `Err`
del tipo envolvente. `Option<List<T>>` y `Result<List<T>, E>` ya cruzan la IR cuando la lista
usa elementos escalares o records, con ownership condicional del payload. También cruzan la IR
`Option<Map<Int,String>>` y `Result<Set<Int>,String>`, y los wrappers anidados conservan su
ownership recursivo en constructores y `match`. Los consumidores estructurales `unwrap`,
`unwrap_or`, `ok` y `ok_or` ya retienen el payload gestionado elegido antes de liberar el wrapper
o el argumento fallback; `examples/native_ir_managed_consumers.ostrin` verifica ambas ramas para
`Option<String>` y `Result<String,String>` desde la IR. Cadenas de extracción también cruzan IR/C
para `Option<Option<String>>` y `Result<Option<String>,String>`; el ejemplo
`native_ir_nested_wrappers.ostrin` prueba `unwrap`, `unwrap_or`, `ok` y `ok_or`, incluidos fallbacks,
con `hir-generated: 0` y `live_allocations=0`. Consumidores no lineales y wrappers fuera de los
tipos representables siguen usando fallback.

El runtime C generado centraliza las reservas en `ostrin_alloc`/`ostrin_calloc`/
`ostrin_realloc`, registra cada bloque y lo libera mediante `atexit` al terminar el
programa. Expone ya el ABI `ostrin_retain`/`ostrin_release` y `--leak-check` reporta
asignaciones vivas, pico y total antes de la limpieza. Los buffers temporales de arrays, CSV,
strings, cantidades, RNG y el detector E1101 también usan esa API. La primera familia
gestionada migrada a la IR es `String`, seguida por los núcleos escalares de `List<T>`,
`Map<K,V>` y `Set<T>`: literales, concatenación, igualdad estructural, llamadas, ramas con `phi`,
indexación, mutación, consultas hash, `print` y marcadores
`retain/release` ya se prueban en el emisor nativo con `--leak-check`. Los `Phi` simples de
ramas transfieren la referencia entrante sin retenerla de nuevo y los `Phi` de bucle liberan
el valor corriente tras su último uso seguro en el backedge. Records concretos,
campos anidados y `Option<Record>` ya atraviesan también la IR; records y colecciones
registran callbacks de destrucción tipados; sus campos/elementos por referencia se
retienen al almacenarse y se liberan al destruir el contenedor. El backend inserta ahora
`retain` para aliases y valores prestados, libera valores reemplazados y limpia los locales
propietarios directos al retornar; el mismo contrato se aplica al emisor HIR y al fallback AST.
`clone(x)` y `drop(x)` siguen disponibles para probar explícitamente el contrato en programas
nativos. El emisor también limpia bindings de referencia creados por expresiones de bloque
anidadas y por ramas/iteraciones de `while`/`for`, incluyendo `break`/`continue`; los escapes
complejos, los patrones anidados y la bajada completa de ownership sobre la IR siguen pendientes
fuera de las familias y consumidores cubiertos; las cadenas lineales de consumidores sobre los
wrappers anidados soportados ya tienen cobertura IR/C.

**Soportado** (todos los ejemplos ejecutables del repo, salvo lo listado en §6):
- Escalares, strings, recursión, `if/while/for`, `match` (con guardas y patrones anidados).
- Records (heap, por referencia) y enums (unión etiquetada por valor), ambos genéricos;
  métodos estáticos y genéricos; métodos por defecto de traits.
- `dyn Trait` con vtable real (único punto de despacho en tiempo de ejecución).
- `List/Map/Set/Option/Result`, combinadores con lambdas expandidas en línea, `try` y `try catch`
  inline para `Result` escalar con error `String`; `Option.map`/`then` para payloads
  escalares y `String` también usan IR. Wrappers de una capa sobre `List`, `Map` y `Set`
  escalares conservan sus marcadores de ownership en la IR.
- Funciones genéricas monomorfizadas por uso: sus instancias concretas elegibles también
  pueden generar el cuerpo desde el HIR ya especializado (`identity<T>`, `List<T>`,
  `Option<T>` y métodos estructurales), manteniendo AST como fallback para familias no migradas.
- `Option`/`Result` estructurales también se generan desde HIR: `Some`/`None`, `Ok`/`Err`,
  `match`, `is_some`/`is_none`, `is_ok`/`is_err`, `unwrap`, `unwrap_or`, `ok`/`ok_or`; los
  consumidores directos con payload `String` soportado ya usan IR/C con retención explícita, y
  propagación con `try`; `try catch` con lambda inline y handlers locales capturados compatibles
  ya usa IR para `Result` escalar y los combinadores `Result.map`, `Result.map_err`,
  `Result.then` y `Option.map`/`Option.then` con lambda inline también usan IR; aliases locales de
  funciones globales sin entorno se resuelven a llamadas estáticas. Handlers locales no
  compatibles o no lineales conservan el fallback AST. Las cadenas lineales `unwrap`/`unwrap_or`/`ok`/`ok_or` a través de
  `Option`/`Result` anidados soportados ya cruzan IR/C.
- `Quantity` (dimensión estática, unidad como cadena en ejecución) e `impl` sobre cantidades.
- Operadores de usuario, `derive(Eq/Ord)`, `Ordering` incorporado.
- `print` de records, enums, listas, mapas, sets, `Option`, `Result` (mismo formato que el intérprete).
- Argumentos nombrados y por defecto, iteradores propios, funciones incorporadas de E/S.
- `spawn`/`join`/`cancel`/`yield`/`select`/`spawn_scope`/canales con scheduler cooperativo determinista por defecto, igual
  que el intérprete. Con `--native-threads`, las tareas se ejecutan en hilos del SO y los
  canales usan espera/señalización bloqueante; el modo determinista sigue disponible para
  pruebas diferenciales.
- Primitivas de ownership `clone`/`drop` para valores gestionados; el ejemplo
  `ownership_primitives.ostrin` verifica que una lista y su buffer terminen con cero
  asignaciones vivas bajo `--leak-check`.

**Rechazado a propósito con mensaje claro** (mejor error que comportamiento distinto):
`for` sobre `Map/Set` (el intérprete tampoco lo permite), `?` dentro de una lambda, usar una
función genérica como valor, `Array` de tipos que no sean Int/Float/Float32/Bool.

---

## 6. Brechas conocidas

| Área | Estado |
|---|---|
| Cierres en nativo | Captura **por valor** (una variable `mut` cambiada después no se ve dentro), incluidas capturas transitivas de closures anidadas; lambda sin contexto de tipos exige anotación |
| Chequeo «movido tras enviar» (E1101) | Integrado por defecto en `--check`, `--run`, `--emit-c` y `--compile`; análisis de flujo sobre CFG alcanzable con punto fijo para loops y uso de operandos `Phi` en su arista; las guardas dinámicas siguen la vida del allocation, sin dejar direcciones reutilizadas marcadas; `--ownership-check` conserva el informe explícito |
| Paralelismo nativo (`--native-threads`, canales bloqueantes, `select`) | Hilos del SO, mutexes/condiciones, canales bloqueantes y `select(List<Channel<T>>)` implementados de forma opt-in; `select` conserva prioridad determinista y cede el hilo nativo entre intentos; `Task.cancel()` cancela pendientes, propaga a grupos activos de `spawn_scope` o solicita cancelación a tareas `Running`, observada en checkpoints seguros; `receive()` vuelve periódicamente al runtime sin conservar el mutex durante el checkpoint. En tareas nativas, `read_file`/`write_file` delegan la libc a un worker desacoplado y esperan con condición temporizada, por lo que la cancelación libera la tarea y el worker limpia su solicitud; la libc no se aborta de forma forzada. El runtime cooperativo/WASI conserva E/S síncrona y observa la cancelación al regresar |
| Memoria en nativo | Registro, destructores tipados para records/colecciones y entornos de tareas, `clone`/`drop`, cleanup automático de locales directos, bloques anidados, ramas, loops y cancelación de tareas en AST/HIR, y `--leak-check`; `String`, `String.split/lines`, `Result<Int, String>`, `Result<Float, String>`, records concretos, canales, `Option<Record>`, listas escalares/String, mapas/conjuntos escalares, wrappers sobre `List`/`Map`/`Set` y wrappers `Option`/`Result` anidados ya consumen ownership desde IR; `unwrap`, `unwrap_or`, `ok` y `ok_or` retienen el payload gestionado que escapa del wrapper; el generador diferencial de wrappers cubre ambas ramas y parámetros con cero fugas; el fallback AST ahora materializa y libera temporales gestionados de argumentos/`print` y campos de registros, pero consumidores complejos y escapes siguen pendientes |
| IR de bloques | HIR→CFG disponible con `--ir`; el emisor C cubre ramas, bucles escalares con `phi`, rangos enteros direccionales (`to`/`until`, pasos y `break`/`continue`), `String` (incluidos `split/lines`), igualdad estructural `==`/`!=` para `List`/`Map`/`Set` y wrappers `Option`/`Result` soportados, consumidores `unwrap`/`unwrap_or`/`ok`/`ok_or` con payload gestionado, `Result` escalar de parseo con `match`/consultas, `read_file`/`write_file` con resultados administrados y errores de archivo comprobados, `try`, `try catch` inline, handlers globales, aliases locales y handlers locales capturados compatibles, `map`/`map_err`/`then` de `Result`, `Option.map`/`then` escalar/String, records concretos con campos anidados, iteradores de records concretos cuyo elemento sea un payload soportado y cuyo `next` esté registrado, canales con `send`/`close`/`receive` y `for` sobre `Option<T>`, `select(List<Channel<T>>)` sobre payloads soportados, `spawn {}` con CFG soportado, capturas inmutables, closures capturados con entorno y destructor, `ClosureCall`, `Task.join()`, `Task.cancel()` y `yield()`, listas escalares/String, operaciones hash escalares, wrappers de una capa sobre `List`/`Map`/`Set`, wrappers `Option`/`Result` anidados con `match` y builtins portables de proceso/rutas (`args`, `env`, `cwd`, `path_join`, `file_exists`, `clone`, `drop`); iteradores genéricos/indirectos, tareas anidadas o scopes, handlers locales no lineales y consumidores complejos aún no reemplazan el backend C completo |
| Ownership/último uso | `--ownership-report`, `--ownership-check` y `--ownership-ir`; la IR ya inserta y consume retain/release lineal para `String`, `Result` con payload `String` (incluido `Result<Void, String>`), records concretos, `Option<Record>`, listas escalares/String (incluida la lista fresca de `args()`), mapas/conjuntos escalares y `Option<String>` (incluido el valor fresco de `env()`), con transferencia en `Phi` simples y liberación de `Phi` de bucle en backedges probados; `clone`/`drop` tienen lowering explícito en IR, los entornos de tareas retienen también cada `Channel<T>` capturado, los buffers temporales de `split/lines` transfieren y liberan sus strings, `Result` libera condicionalmente `value`/`error`, y `unwrap`/`unwrap_or`/`ok`/`ok_or` retienen payloads extraídos o fallbacks antes de la liberación del wrapper; la ruta AST conserva la misma regla para temporales frescos en llamadas genéricas, `print` y acceso a campos; `Option` escalar es por valor, mientras llamadas transferentes no lineales, agregados complejos, scopes y escapes siguen conservadores |
| Biblioteca estándar | Incluye `std.math`, `std.lists`, `std.strings` (incluidos `trim`, `split`, `lines`, `is_blank`, `format_text`, `format_float`, `char_at`, `slice` y `codepoint`), `std.time` (calendario gregoriano determinista, validación, ordinales, día de semana, ISO y `Result` de parseo), `std.json` (DOM, parser/serializer estricto y Unicode), `std.args`, `std.env` y `std.maps` (consultas genéricas de `Map<K,V>` con `Hash + Eq`); red sigue pendiente |
| Mensajes de error de E/S | `strerror` ≠ texto de Rust (difieren entre backends) |
| Unidades | `q as unidad` convierte desde 2026-09-24 (antes solo reetiquetaba); `as` acepta un único identificador de unidad (`as m / s` falla en ejecución); las unidades derivadas se imprimen sin simplificar (`m/s*s`); `unit`/`define` definidos por el usuario están especificados pero no implementados |
| `Result<Void,E>` | Campo de valor de relleno (`char`) en C |
| Migración HIR/IR | HIR cubre escalares, records, enums/match, Option/Result, colecciones, cierres y formas genéricas; la IR/C ya emite instancias concretas soportadas de funciones y métodos genéricos (incluidos casos recursivos), closures anidadas con capturas transitivas y ownership de entornos, con paridad y leak-check; records/enums genéricos aplicados y retornos complejos conservan el fallback verificado |
| Paquetes | `--project` usa `entry`; resolución transitiva de manifiestos con alias globales sin colisión; lockfiles deterministas con rutas relativas, versión y SHA-256 de `ostrin.toml`/fuentes `.ostrin`; Git solo mediante `--fetch`, con caché local y commit resuelto; builds normales reutilizan y validan el lock, `--locked` lo exige; sin registro remoto |
| Rendimiento del intérprete | Tree‑walking simple; sin optimizaciones |
| `newlines.ostrin`, `advanced.ostrin` | Son muestras de sintaxis, no programas ejecutables |
| CI | Linux, macOS y Windows; incluye las pruebas diferenciales intérprete↔nativo; el backend nativo enlaza `libm` explícitamente en Unix para paquetes con `sqrt`/`round` |
| Distribución | Workflow WASI reproducible para `ostrinc.wasm`, `hello.wasm`, `pkg_project.wasm`, un contrato de `args`/`env`, E/S de archivos y ownership gestionado, con toolchain fijado, ejecución bajo Node WASI y SHA-256; playground de navegador sobre el compilador WASM; workflow de release `v0.1.0` para Linux x86_64, macOS arm64 y Windows x64 que valida versión, checksums, archivos extraídos, `hello.ostrin` y un proyecto con dependencia `path`, y publica `docs/releases/v0.1.0.md` como notas; `v0.1.0` está publicada y `install-check.yml` instala la release con los instaladores Unix/PowerShell en Linux, macOS y Windows limpios y ejecuta `hello.ostrin`; canales externos y registry público siguen pendientes |

Deuda técnica notable: `codegen.rs` y `typeck/mod.rs` son archivos muy grandes y
convendría dividirlos; el backend nativo no comparte el sistema de tipos del checker
(ya consume los tipos del checker y compara cada nodo; **119 funciones/métodos de los ejemplos
ya se generan desde el HIR y las primeras funciones escalares y gestionadas con CFG ya se generan desde la IR** —escalares, `String`, records concretos, `Option<Record>`, `List<T>` escalar, `Map`/`Set` escalares, `Option` escalar/String y patrones `Some/None`, `Float32`, enteros de ancho fijo, records, enums, `match`, `Option`/`Result`,
listas/colecciones, cierres, instancias concretas de genéricos, records/enums aplicados y métodos
genéricos centrales, módulo `hir_c.rs`—, con un trinquete mínimo de 127; el resto sigue por el AST;
ver documento 20 y secciones 123–133 de `CONTEXTO_PROYECTO.md`);
Las claves/elementos compuestos ya pueden usar el índice interno cuando su contrato `Hash`/`Eq`
es compatible; si contienen estado mutable se reindexan antes de buscar y los comparadores
personalizados conservan el fallback lineal. El checker exige ahora `Hash + Eq` de forma estática
para `Map`/`Set`, también dentro de colecciones anidadas y bounds genéricos.

---

## 7. Opciones de avance

Ordenadas por mi recomendación (valor / riesgo). Cada una es independiente.

### A. Cerrar la semántica del backend nativo (corto plazo)
1. **Retirar progresivamente el fallback AST**: métodos genéricos complejos y formas restantes; mantener la resolución de llamadas anidadas, los records/enums aplicados y las identidades de impl como base.
2. **Retirada progresiva del fallback AST**: conservar solo familias aún no migradas.
3. **Gestión de memoria**: convertir el registro global en ownership real: tipos con destructor, conteo/borrows y liberación por último uso.
4. **`==` estructural** para `List/Option/Map` — **cerrado en la IR** para `List`/`Map`/`Set` y wrappers `Option`/`Result` soportados, con paridad y leak-check; queda fuera la semántica matricial elemento a elemento de `Array`.
6. Reutilizar el checker: que `typeck` entregue tipos resueltos al codegen y eliminar la
   reinferencia (reduce errores y abre optimizaciones).

### B. Paralelismo nativo (medio plazo, requiere runtime)
El primer bloque ya está implementado detrás de `--native-threads`: hilos del sistema
operativo, mutexes/condiciones y canales bloqueantes, sin romper E1100/E1101 ni la paridad
determinista por defecto. `select(List<Channel<T>>)` ya está cerrado con prioridad
 determinista y polling no bloqueante; `Task.cancel()` cubre la transición segura de
tareas pendientes, propaga cancelación a los grupos activos de `spawn_scope` y consume
las solicitudes de tareas `Running` en checkpoints cooperativos. Las esperas vacías de
 canal en `--native-threads` usan ahora una condición temporizada: liberan el mutex,
 alcanzan el checkpoint y solo después vuelven a esperar, por lo que una tarea cancelada
 no queda dormida indefinidamente. `read_file`/`write_file` aplican la misma política
 mediante workers nativos desacoplados: la tarea cancelada deja de esperar mientras el
 worker termina la llamada libc y libera su solicitud. La frontera restante es la E/S
 externa sin worker (red), la semántica equivalente para WASI/cooperativo y la
 administración global de recursos de I/O de más larga duración.

### C. Biblioteca estándar y ecosistema
Cadenas (split/trim/format), fechas, JSON, argumentos y entorno, `HashMap` real y formateo
configurable de floats ya están implementados en un `std` escrito en Ostrin y compilable por ambos
backends. El bloque JSON, `std.maps` y `std.strings.format_float` están verificados; después:
red y paquetes con registro (con consentimiento explícito del usuario para la red).

### D. Experiencia de desarrollador
Formateador oficial (`ostrinc fmt`), `ostrinc test`, documentación generada, acciones de
código en LSP (quick fixes), inlay hints, publicación de la extensión en el Marketplace,
canales de distribución adicionales (la release `v0.1.0`, la CI con matriz y los instaladores ya existen).

### E. Backends adicionales
El compilador, un programa Ostrin independiente y un proyecto con dependencia `path` ya se
construyen como `wasm32-wasip1` mediante el workflow WASI, con toolchain fijado, ejecución bajo
Node WASI y checksums reproducibles. La matriz de nueve programas también compila y ejecuta un
contrato real de `args`/`env`, E/S de archivos, ownership gestionado y consumidores anidados
`Option`/`Result`; un script único captura stdout/stderr,
compara salidas exactas y limpia los artefactos temporales. La compilación usa el triple vigente
`wasm32-wasip1`, mantiene el alias CLI histórico y exige un host con WebAssembly exception
handling para la cancelación cooperativa basada en SJLJ. El hash de punteros y sus shifts están
verificados también para el ancho de 32 bits de WASI. El backend de programas conserva C
como representación intermedia y su runtime cooperativo separa los headers y primitivas de
`--native-threads`; la emisión local verifica que ningún programa WASI habilita hilos nativos.
El playground de navegador ya ejecuta el compilador WASM; la siguiente frontera es aislar más
contratos de plataforma antes de LLVM IR.

### F. Calidad y confianza
Fuzzing del parser, pruebas diferenciales automáticas intérprete↔nativo sobre programas
generados escalares y con ownership (`Option`/`Result`), cobertura de mensajes de error,
benchmarks (nativo vs intérprete).

### G. Producto
**Homepage 3.0 (2026-09-24).** La portada muestra el estado de la release derivado de
`CHANGELOG.md`, un Scientific Lab de ocho demos (Plot, Linear Algebra, Statistics, Monte Carlo,
Autodiff, Units, Data, Concurrency) que ejecutan programas de `examples/` con `ostrinc.wasm`
en el navegador —incluidos proyectos multiarchivo con los paquetes `plot` y `autodiff`—, la
etapa source → HIR → IR → C de una función real, y tarjetas de capacidades con enlaces a su
evidencia. `scripts/lab-data.mjs` registra las salidas con el mismo WASM y su modo check falla
ante cualquier deriva; `website-check.mjs` valida evidencias, enlaces al repositorio y claims de
release. La documentación se separa en Learn, Reference, Guides, Examples y Cookbook.

El sitio web y el playground real WASM ya están publicados; la primera base de descubrimiento
añade SEO técnico, cifras centralizadas, una demo viva en la portada y enlaces compartibles.
El catálogo ya tiene demos live para cantidades, biblioteca estándar, records/enums y concurrencia,
con Run/Check/Reset/Copy sobre el mismo módulo WASM. CI valida ahora enlaces, metadata, sitemap,
robots, el artefacto WASM generado por Pages y la metadata generada de versión/métricas. `showcase.html` expone programas source-backed de
cantidades, tablas, SVG y autodiff; `community.html` y las plantillas de GitHub preparan la
contribución sin inventar canales externos. `docs.html` añade una ruta guiada de 14 pasos con
enlaces a fuentes, referencia, playground y showcase; los pasos distinguen `available` de `early`
sin prometer capacidades no implementadas. El playground y sus live examples renderizan ahora
diagnósticos JSON reales con código, ubicación, severidad y mensaje, y seleccionan la línea
diagnosticada en el editor. La versión y las métricas públicas ahora salen de
`scripts/site-facts.mjs`/`website/site-data.js`, con una comprobación de frescura en CI. Siguiente:
prueba responsive móvil con viewport dedicado y mejora incremental del learning funnel.
La gramática para Linguist queda separada porque requiere uso público suficiente.

---

## 8. Ruta sugerida

| Fase | Contenido | Resultado |
|---|---|---|
| **1** (1–2 semanas) | A.1–A.4 + F (diferenciales) | Nativo semánticamente equivalente al intérprete |
| **2** | C básico + D (`fmt`, `test`, CI) | Lenguaje usable para programas reales pequeños |
| **3** | A.2, A.5 + gestión de memoria | Backend nativo robusto y eficiente |
| **4** | B (concurrencia real) | Cumple la promesa de «concurrencia segura por defecto» |
| **5** | E (WASM) + G (playground) | Difusión |

Plan activo (2026-09-24): los bloques de release `v0.1.0` y homepage 3.0 están cerrados. El
siguiente ciclo vuelve al núcleo (retirada del fallback AST, ownership completo) con las
brechas que expuso el Scientific Lab: `as` con unidades compuestas, simplificación de unidades
derivadas en la salida (§6); el sombreado de funciones globales por parámetros función ya está
corregido. GPU, autodiff inverso, registry público y red siguen fuera.

---

## 9. Cómo verificar el estado

```powershell
cd compiler
    cargo test                                   # 6 diferenciales + 202 de integración + 2 unitarias
cargo run -- --run ..\examples\physics.ostrin
cargo run -- --compile ..\examples\collections.ostrin
```

Ejemplos nativos dedicados: `native_*.ostrin` (records, métodos, enums, genéricos,
dyn, listas, closures, option, result, unidades, display, derive, args nombrados,
colecciones, trait defaults, métodos genéricos, builtins, concurrencia).
