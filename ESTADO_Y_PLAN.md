# Ostrin — estado del proyecto y plan de avance

*Corte: 2026-09-21 · rama `main` · 6 pruebas diferenciales, 170 de integración y 2 unitarias en verde.*

Este documento resume **qué existe hoy**, **qué no**, y **por dónde se puede avanzar**.
Para la historia detallada, ver `CONTEXTO_PROYECTO.md` (secciones 1–190); para el diseño
del lenguaje, `docs/design/` (21 documentos).

---

## 1. Qué es Ostrin

Un lenguaje de propósito general experimental centrado en tres ideas:

1. **Cantidades físicas como tipos**: `5 m / 2 s` lleva su dimensión en el sistema de tipos;
   sumar longitud con tiempo es un error de compilación.
2. **Inmutable por defecto**: `mut` es explícito; un binding mutable no se captura en `spawn`.
3. **Legibilidad**: expresiones, `match` exhaustivo, `try` explícito, `and/or/not`,
   saltos de línea significativos, sin `let`.

Implementación: compilador + intérprete + herramientas de editor, todo en Rust
(`compiler/`, ~14 500 líneas), más una extensión de VS Code (`vscode-ostrin/`).

---

## 2. Arquitectura

| Componente | Archivo(s) | Función |
|---|---|---|
| Léxico / parser | `lexer/`, `parser/mod.rs` | Tokens, AST con rangos de origen |
| Módulos y paquetes | `modules.rs`, `package.rs`, `ostrin.toml` | Imports, `--project`, dependencias locales y lockfile portable |
| Verificador de tipos | `typeck/mod.rs` (~3 500 l.) | Tipos, dimensiones, traits, exhaustividad, genéricos |
| HIR/IR | `hir.rs`, `hir_c.rs`, `ir.rs`, `ir_c.rs` | HIR verificado, CFG con temporales explícitos y emisor C desde IR para escalares, `String`, `Result<Int, String>`/`Result<Float, String>` con `try` y `try catch` inline, records concretos, `Option<Record>`, listas escalares, mapas/conjuntos escalares, enteros de ancho fijo y control de flujo, con fallback HIR/AST acotado |
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
enviarlo (E1101); `--ownership-check` conserva el informe detallado.

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
paridad entre intérprete y nativo; funciones, arrays, canales y tareas siguen fuera.

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
`Option_<T>` por valor y retienen/transfieren sus strings correctamente. Agregados complejos,
iteradores, patrones distintos de `Some/None` y payloads gestionados que no sean `String` caen
de forma verificable a HIR y después al AST.

La misma ruta ya cubre la familia escalar de `Result`: `String.to_int()` y `to_float()` producen
`Result<Int,String>`/`Result<Float,String>` desde la IR, junto con `Ok`/`Err`, `match`, bindings
de variantes, `is_ok`, `is_err`, `unwrap_or`, `ok` y `try` sin `catch`. La rama de error construye
el `Result` de retorno y termina la función; la de éxito extrae el payload. El discriminante se
conserva en el path del binding y el ownership libera condicionalmente el payload activo.
`try catch` con lambda inline ya expande el handler en la rama de error y retorna un `Err` del
tipo envolvente; combinadores y payloads compuestos de `Result` siguen en HIR/AST hasta tener
el mismo contrato.

El runtime C generado centraliza las reservas en `ostrin_alloc`/`ostrin_calloc`/
`ostrin_realloc`, registra cada bloque y lo libera mediante `atexit` al terminar el
programa. Expone ya el ABI `ostrin_retain`/`ostrin_release` y `--leak-check` reporta
asignaciones vivas, pico y total antes de la limpieza. Los buffers temporales de arrays, CSV,
strings, cantidades, RNG y el detector E1101 también usan esa API. La primera familia
gestionada migrada a la IR es `String`, seguida por los núcleos escalares de `List<T>`,
`Map<K,V>` y `Set<T>`: literales, concatenación, igualdad, llamadas, ramas con `phi`,
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
complejos, los patrones anidados, los payloads gestionados dentro de `Option` que no sean
`String` o records concretos y la bajada completa de ownership sobre la IR siguen pendientes.

**Soportado** (todos los ejemplos ejecutables del repo, salvo lo listado en §6):
- Escalares, strings, recursión, `if/while/for`, `match` (con guardas y patrones anidados).
- Records (heap, por referencia) y enums (unión etiquetada por valor), ambos genéricos;
  métodos estáticos y genéricos; métodos por defecto de traits.
- `dyn Trait` con vtable real (único punto de despacho en tiempo de ejecución).
- `List/Map/Set/Option/Result`, combinadores con lambdas expandidas en línea, `try` y `try catch`
  inline para `Result` escalar con error `String`; `Option.map`/`then` para payloads
  escalares y `String` también usan IR.
- Funciones genéricas monomorfizadas por uso: sus instancias concretas elegibles también
  pueden generar el cuerpo desde el HIR ya especializado (`identity<T>`, `List<T>`,
  `Option<T>` y métodos estructurales), manteniendo AST como fallback para familias no migradas.
- `Option`/`Result` estructurales también se generan desde HIR: `Some`/`None`, `Ok`/`Err`,
  `match`, `is_some`/`is_none`, `is_ok`/`is_err`, `unwrap`, `unwrap_or`, `ok`/`ok_or` y
  propagación con `try`; `try catch` con lambda inline ya usa IR para `Result` escalar y los
  combinadores `Result.map`, `Result.map_err`, `Result.then` y `Option.map`/`Option.then` con
  lambda inline también usan IR; handlers no inline y payloads compuestos siguen usando el
  fallback AST.
- `Quantity` (dimensión estática, unidad como cadena en ejecución) e `impl` sobre cantidades.
- Operadores de usuario, `derive(Eq/Ord)`, `Ordering` incorporado.
- `print` de records, enums, listas, mapas, sets, `Option`, `Result` (mismo formato que el intérprete).
- Argumentos nombrados y por defecto, iteradores propios, funciones incorporadas de E/S.
- `spawn`/`join`/`spawn_scope`/canales con scheduler cooperativo determinista por defecto, igual
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
| Cierres en nativo | Captura **por valor** (una variable `mut` cambiada después no se ve dentro); lambda sin contexto de tipos exige anotación |
| Chequeo «movido tras enviar» (E1101) | Integrado por defecto en `--check`, `--run`, `--emit-c` y `--compile`; `--ownership-check` conserva el informe explícito |
| Paralelismo nativo (`--native-threads`, canales bloqueantes, `select`) | Hilos del SO, mutexes/condiciones, canales bloqueantes y `select(List<Channel<T>>)` implementados de forma opt-in; `select` conserva prioridad determinista y cede el hilo nativo entre intentos; `Task.cancel()` cancela pendientes, propaga a grupos activos de `spawn_scope` o solicita cancelación a tareas `Running`, observada en checkpoints seguros; `receive()` vuelve periódicamente al runtime sin conservar el mutex durante el checkpoint |
| Memoria en nativo | Registro, destructores tipados para records/colecciones y entornos de tareas, `clone`/`drop`, cleanup automático de locales directos, bloques anidados, ramas, loops y cancelación de tareas en AST/HIR, y `--leak-check`; `String`, `String.split/lines`, `Result<Int, String>`, `Result<Float, String>`, records concretos, `Option<Record>`, listas escalares/String, mapas/conjuntos escalares y `Option<String>` ya consumen ownership desde IR, pero ARC completa de agregados sigue pendiente |
| IR de bloques | HIR→CFG disponible con `--ir`; el emisor C cubre ramas, bucles escalares con `phi`, `String` (incluidos `split/lines`), `Result` escalar de parseo con `match`/consultas, `try`, `try catch` inline y `map`/`map_err`/`then` de `Result`, `Option.map`/`then` escalar/String, records concretos con campos anidados, `Option<Record>`, listas escalares/String, operaciones hash escalares y `Option` escalar/String con patrones `Some/None`; `for`/iteradores, handlers no inline, patrones anidados y colecciones complejas aún no reemplazan el backend C completo |
| Ownership/último uso | `--ownership-report`, `--ownership-check` y `--ownership-ir`; la IR ya inserta y consume retain/release lineal para `String`, `Result` con payload `String`, records concretos, `Option<Record>`, listas escalares/String, mapas/conjuntos escalares y `Option<String>`, con transferencia en `Phi` simples y liberación de `Phi` de bucle en backedges probados; los buffers temporales de `split/lines` transfieren y liberan sus strings, y `Result` libera condicionalmente `value`/`error`; `Option` escalar es por valor, mientras llamadas transferentes no lineales, agregados complejos, scopes y escapes siguen conservadores |
| Biblioteca estándar | Mínima: `args`, entorno/rutas, `format`, E/S y `hash` estructural para escalares, colecciones y tipos con `derive(Hash)`; faltan fechas, JSON y red |
| Mensajes de error de E/S | `strerror` ≠ texto de Rust (difieren entre backends) |
| `Result<Void,E>` | Campo de valor de relleno (`char`) en C |
| Migración HIR | Escalares, records, enums/match, Option/Result, colecciones, cierres, instancias concretas de genéricos, llamadas anidadas, records/enums genéricos aplicados y métodos genéricos centrales migrados; formas complejas restantes siguen con fallback |
| Paquetes | `--project` usa `entry`; lockfiles deterministas con rutas relativas; Git solo mediante `--fetch`, con caché local y commit resuelto; builds normales reutilizan el lock y `--locked` lo exige; sin registro remoto |
| Rendimiento del intérprete | Tree‑walking simple; sin optimizaciones |
| `newlines.ostrin`, `advanced.ostrin` | Son muestras de sintaxis, no programas ejecutables |
| CI | Linux, macOS y Windows; incluye las pruebas diferenciales intérprete↔nativo; el backend nativo enlaza `libm` explícitamente en Unix para paquetes con `sqrt`/`round` |
| Distribución | Workflow WASI reproducible para `ostrinc.wasm`, `hello.wasm` y `pkg_project.wasm`, con toolchain fijado y SHA-256; playground de navegador sobre el compilador WASM; el runtime C cooperativo generado evita pthreads cuando no se pide `--native-threads`; binarios nativos publicados e instalador siguen pendientes |

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
4. **`==` estructural** para `List/Option/Map`.
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
no queda dormida indefinidamente. El hueco restante es integrar la misma política con
E/S externa bloqueante y completar la administración de recursos del runtime.

### C. Biblioteca estándar y ecosistema
Cadenas (split/trim/format), fechas, JSON, argumentos y entorno, `HashMap` real,
formateo de floats configurable; un `std` en Ostrin propio compilable por ambos backends.
Después: paquetes con registro (con consentimiento explícito del usuario para la red).

### D. Experiencia de desarrollador
Formateador oficial (`ostrinc fmt`), `ostrinc test`, documentación generada, acciones de
código en LSP (quick fixes), inlay hints, publicación de la extensión en el Marketplace,
CI de GitHub con matriz Windows/Linux/macOS, binarios de release.

### E. Backends adicionales
El compilador, un programa Ostrin independiente y un proyecto con dependencia `path` ya se
construyen como `wasm32-wasip1` mediante el workflow WASI, con toolchain fijado, ejecución bajo
Node WASI y checksums reproducibles. El backend de programas conserva C como representación
intermedia y su runtime cooperativo separa los headers y primitivas de `--native-threads`. El
playground de navegador ya ejecuta el compilador WASM; el siguiente paso es ampliar la matriz de
programas (I/O) y aislar contratos WASI explícitos antes de LLVM IR.

### F. Calidad y confianza
Fuzzing del parser, pruebas diferenciales automáticas intérprete↔nativo sobre programas
generados, cobertura de mensajes de error, benchmarks (nativo vs intérprete).

### G. Producto
Sitio web/playground (WASM), tutorial guiado, gramática para Linguist (para que GitHub
reconozca Ostrin; requiere uso público suficiente).

---

## 8. Ruta sugerida

| Fase | Contenido | Resultado |
|---|---|---|
| **1** (1–2 semanas) | A.1–A.4 + F (diferenciales) | Nativo semánticamente equivalente al intérprete |
| **2** | C básico + D (`fmt`, `test`, CI) | Lenguaje usable para programas reales pequeños |
| **3** | A.2, A.5 + gestión de memoria | Backend nativo robusto y eficiente |
| **4** | B (concurrencia real) | Cumple la promesa de «concurrencia segura por defecto» |
| **5** | E (WASM) + G (playground) | Difusión |

Decisiones que necesito de ti para afinar el plan:
1. ¿Prioridad: **usabilidad** (stdlib/herramientas) o **potencia** (concurrencia real, rendimiento)?
2. ¿Objetivo del nativo: producto final (exige gestión de memoria) o vía de validación?
3. Concurrencia: ¿hilos de SO con canales, o tareas cooperativas?
4. ¿Apuntar a publicar la extensión y binarios pronto?

---

## 9. Cómo verificar el estado

```powershell
cd compiler
    cargo test                                   # 6 diferenciales + 170 de integración + 2 unitarias
cargo run -- --run ..\examples\physics.ostrin
cargo run -- --compile ..\examples\collections.ostrin
```

Ejemplos nativos dedicados: `native_*.ostrin` (records, métodos, enums, genéricos,
dyn, listas, closures, option, result, unidades, display, derive, args nombrados,
colecciones, trait defaults, métodos genéricos, builtins, concurrencia).
