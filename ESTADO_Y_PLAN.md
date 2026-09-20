# Ostrin — estado del proyecto y plan de avance

*Corte: 2026-09-19 · rama `main` · 6 pruebas diferenciales y 127 de integración en verde.*

Este documento resume **qué existe hoy**, **qué no**, y **por dónde se puede avanzar**.
Para la historia detallada, ver `CONTEXTO_PROYECTO.md` (secciones 1–149); para el diseño
del lenguaje, `docs/design/` (20 documentos).

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
| HIR/IR | `hir.rs`, `hir_c.rs`, `ir.rs` | HIR verificado, primera CFG con temporales explícitos y generación C por familias, con fallback AST |
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
`join`, `spawn_scope` y `channel<T>()`. El backend nativo mantiene ese modo por defecto
para la paridad reproducible, y `--native-threads` habilita hilos del SO, mutexes/condiciones
y canales bloqueantes con la misma API. Se verifica en el checker que no se capturen bindings
`mut` (E1100) y el análisis HIR/IR rechaza por defecto reutilizar un valor movible después de
enviarlo (E1101); `--ownership-check` conserva el informe detallado.

**Numérico/científico**: enteros de ancho fijo, `Float32`, `Array<T>` (difusión, máscaras,
rebanadas, `@`), estadística, regresión, `det/inv/eigvals/norm`, `Rng` reproducible,
funciones elementales deterministas (idénticas en intérprete y nativo).

**Datos**: métodos de `String`, `parse_csv`; `Map` y `Set` usan índice hash para claves/elementos
escalares y conservan orden de iteración; paquetes de ejemplo en Ostrin: `tables`
(DataFrame mínimo), `plot` (SVG), `autodiff` (modo directo).

**Igualdad estructural**: `==`/`!=` compara recursivamente `List`, `Map`, `Set`, `Option` y
`Result` por valor en el intérprete y en el backend nativo; mapas y conjuntos no dependen del
orden de inserción.

**Biblioteca estándar** (pequeña): `print`, `args`, `env`, `path_join`, `cwd`, `file_exists`,
`format`, `sum`, `panic`, `read_file`, `write_file`, `parse_int`, métodos de `List`
(`map/filter/fold/any/all/find/push/remove_at/length`),
`Map` (`get/set/remove/contains_key/count/keys/values`), `Set`, `Option`, `Result`.
`hash` ofrece hashes estables para escalares soportados por `Map`/`Set`
(`Int`, enteros fijos, `Bool`, `Float`, `Float32` y `String`) con paridad entre intérprete y nativo.

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

El runtime C generado centraliza las reservas en `ostrin_alloc`/`ostrin_calloc`/
`ostrin_realloc`, registra cada bloque y lo libera mediante `atexit` al terminar el
programa. Expone ya el ABI `ostrin_retain`/`ostrin_release` y `--leak-check` reporta
asignaciones vivas, pico y total antes de la limpieza. Los buffers temporales de arrays, CSV,
strings, cantidades, RNG y el detector E1101 también usan esa API. Records y colecciones
registran callbacks de destrucción tipados; sus campos/elementos por referencia se
retienen al almacenarse y se liberan al destruir el contenedor. El backend inserta ahora
`retain` para aliases y valores prestados, libera valores reemplazados y limpia los locales
propietarios directos al retornar; el mismo contrato se aplica al emisor HIR y al fallback AST.
`clone(x)` y `drop(x)` siguen disponibles para probar explícitamente el contrato en programas
nativos. El alcance deliberado de esta etapa es el camino lineal/directo de cada función:
los bindings creados dentro de bloques anidados y los escapes complejos siguen pendientes de
la bajada completa de ownership sobre la IR.

**Soportado** (todos los ejemplos ejecutables del repo, salvo lo listado en §6):
- Escalares, strings, recursión, `if/while/for`, `match` (con guardas y patrones anidados).
- Records (heap, por referencia) y enums (unión etiquetada por valor), ambos genéricos;
  métodos estáticos y genéricos; métodos por defecto de traits.
- `dyn Trait` con vtable real (único punto de despacho en tiempo de ejecución).
- `List/Map/Set/Option/Result`, combinadores con lambdas expandidas en línea, `try`.
- Funciones genéricas monomorfizadas por uso: sus instancias concretas elegibles también
  pueden generar el cuerpo desde el HIR ya especializado (`identity<T>`, `List<T>`,
  `Option<T>` y métodos estructurales), manteniendo AST como fallback para familias no migradas.
- `Option`/`Result` estructurales también se generan desde HIR: `Some`/`None`, `Ok`/`Err`,
  `match`, `is_some`/`is_none`, `is_ok`/`is_err`, `unwrap`, `unwrap_or`, `ok`/`ok_or` y
  propagación con `try`; los combinadores con lambdas y `catch` siguen usando el fallback AST.
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
| Paralelismo nativo (`--native-threads`, canales bloqueantes, `select`) | Hilos del SO, mutexes/condiciones y canales bloqueantes implementados de forma opt-in; el registro de tareas tiene mutex, referencias temporales, desregistro en `join` y drenado de nodos; `select` y cancelación siguen pendientes |
| Memoria en nativo | Registro, destructores tipados para records/colecciones, `clone`/`drop`, limpieza automática de locales directos y `--leak-check`; scopes anidados y ARC completa sobre IR siguen pendientes |
| IR de bloques | HIR→CFG disponible con `--ir`; `if/while/for/match/try/spawn/channel` ya tienen operaciones explícitas, aún no reemplaza el backend C |
| Ownership/último uso | `--ownership-report`, `--ownership-check` y `--ownership-ir`; el backend C ya aplica retain/release lineal en locales directos, pero la IR aún no es la fuente única |
| Biblioteca estándar | Mínima: `args`, entorno/rutas, `format`, E/S, `hash` estable y `Map` hash para claves escalares; faltan `Hash` para tipos de usuario, fechas, JSON y red |
| Mensajes de error de E/S | `strerror` ≠ texto de Rust (difieren entre backends) |
| `Result<Void,E>` | Campo de valor de relleno (`char`) en C |
| Migración HIR | Escalares, records, enums/match, Option/Result, colecciones, cierres, instancias concretas de genéricos, llamadas anidadas, records/enums genéricos aplicados y métodos genéricos centrales migrados; formas complejas restantes siguen con fallback |
| Paquetes | `--project` usa `entry`; lockfiles deterministas con rutas relativas; sin registro remoto ni red automática |
| Rendimiento del intérprete | Tree‑walking simple; sin optimizaciones |
| `newlines.ostrin`, `advanced.ostrin` | Son muestras de sintaxis, no programas ejecutables |
| CI | Linux, macOS y Windows; incluye las pruebas diferenciales intérprete↔nativo |
| Distribución | Workflow WASI reproducible para `ostrinc.wasm` con SHA-256; binarios nativos publicados e instalador siguen pendientes |

Deuda técnica notable: `codegen.rs` y `typeck/mod.rs` son archivos muy grandes y
convendría dividirlos; el backend nativo no comparte el sistema de tipos del checker
(ya consume los tipos del checker y compara cada nodo; **119 funciones/métodos de los ejemplos
ya se generan desde el HIR** —escalares, records, enums, `match`, `Option`/`Result`,
listas/colecciones, cierres, instancias concretas de genéricos, records/enums aplicados y métodos
genéricos centrales, módulo `hir_c.rs`—, con un trinquete mínimo de 115; el resto sigue por el AST;
ver documento 20 y secciones 123–133 de `CONTEXTO_PROYECTO.md`);
Las claves/elementos compuestos aún usan búsqueda lineal; el siguiente paso es formalizar `Hash + Eq`
en el checker y extender el índice a tipos de usuario.

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
determinista por defecto. El siguiente paso es cerrar `spawn_scope` con grupos de tareas
nativos, cancelación y `select` (esbozados en `docs/design/09`).

### C. Biblioteca estándar y ecosistema
Cadenas (split/trim/format), fechas, JSON, argumentos y entorno, `HashMap` real,
formateo de floats configurable; un `std` en Ostrin propio compilable por ambos backends.
Después: paquetes con registro (con consentimiento explícito del usuario para la red).

### D. Experiencia de desarrollador
Formateador oficial (`ostrinc fmt`), `ostrinc test`, documentación generada, acciones de
código en LSP (quick fixes), inlay hints, publicación de la extensión en el Marketplace,
CI de GitHub con matriz Windows/Linux/macOS, binarios de release.

### E. Backends adicionales
El compilador ya puede distribuirse como `wasm32-wasip1` mediante el workflow WASI, con
checksum reproducible. El backend de programas sigue emitiendo C; el siguiente paso es aislar
un runtime WASI portable y producir un smoke test de un programa Ostrin en WASM, antes de un
playground de navegador o de LLVM IR.

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
cargo test                                   # 6 diferenciales + 125 de integración
cargo run -- --run ..\examples\physics.ostrin
cargo run -- --compile ..\examples\collections.ostrin
```

Ejemplos nativos dedicados: `native_*.ostrin` (records, métodos, enums, genéricos,
dyn, listas, closures, option, result, unidades, display, derive, args nombrados,
colecciones, trait defaults, métodos genéricos, builtins, concurrencia).
