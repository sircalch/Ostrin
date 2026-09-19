# Ostrin — estado del proyecto y plan de avance

*Corte: 2026-09-18 · rama `main` · 6 pruebas diferenciales y 115 de integración en verde.*

Este documento resume **qué existe hoy**, **qué no**, y **por dónde se puede avanzar**.
Para la historia detallada, ver `CONTEXTO_PROYECTO.md` (secciones 1–141); para el diseño
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
| Módulos y paquetes | `modules.rs`, `package.rs`, `ostrin.toml` | Imports, resolución, dependencias |
| Verificador de tipos | `typeck/mod.rs` (~3 500 l.) | Tipos, dimensiones, traits, exhaustividad, genéricos |
| HIR/IR | `hir.rs`, `hir_c.rs`, `ir.rs` | HIR verificado, primera CFG con temporales explícitos y generación C por familias, con fallback AST |
| Intérprete | `interpreter/mod.rs` | Ejecución tree‑walking y scheduler cooperativo; referencia semántica |
| Servidor de lenguaje | `lsp.rs`, `symbols.rs`, `protocol.rs` | LSP sobre stdio |
| Adaptador de depuración | `dap.rs` + hooks del intérprete | DAP sobre stdio |
| **Backend nativo** | `codegen.rs` (~3 900 l.), `qty_runtime.c` | Transpila a C y compila con gcc/clang/cc |
| CLI | `main.rs` | `--check --run --ast --tokens --json --lsp --dap --emit-c --compile` |
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

**Concurrencia (scheduler cooperativo del intérprete)**: `spawn`, `join`, `spawn_scope`,
`channel<T>()`, `send/receive/close`; las tareas se difieren, `join` las ejecuta y los
canales bombean tareas pendientes cuando esperan datos. Esto todavía no usa hilos del SO ni
paralelismo de CPU. Se verifica en el checker que no se capturen bindings `mut` (E1100) y en
ejecución que un record enviado no se reutilice (E1101); `--ownership-check` ya puede
detectar ese uso posterior directamente sobre la IR.

**Numérico/científico**: enteros de ancho fijo, `Float32`, `Array<T>` (difusión, máscaras,
rebanadas, `@`), estadística, regresión, `det/inv/eigvals/norm`, `Rng` reproducible,
funciones elementales deterministas (idénticas en intérprete y nativo).

**Datos**: métodos de `String`, `parse_csv`; paquetes de ejemplo en Ostrin: `tables`
(DataFrame mínimo), `plot` (SVG), `autodiff` (modo directo).

**Biblioteca estándar** (pequeña): `print`, `sum`, `panic`, `read_file`, `write_file`,
`parse_int`, métodos de `List` (`map/filter/fold/any/all/find/push/remove_at/length`),
`Map` (`get/set/remove/contains_key/count/keys/values`), `Set`, `Option`, `Result`.

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
programa. Los buffers temporales de arrays, CSV, strings, cantidades, RNG y el detector
E1101 también usan esa API. Esto es una base de limpieza y observabilidad, no todavía ARC
por ámbito ni destrucción basada en último uso.

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
- `spawn`/`join`/`spawn_scope`/canales con scheduler cooperativo determinista, igual que el intérprete;
  no son todavía hilos del sistema operativo.

**Rechazado a propósito con mensaje claro** (mejor error que comportamiento distinto):
`for` sobre `Map/Set` (el intérprete tampoco lo permite), `?` dentro de una lambda, usar una
función genérica como valor, `Array` de tipos que no sean Int/Float/Float32/Bool.

---

## 6. Brechas conocidas

| Área | Estado |
|---|---|
| Cierres en nativo | Captura **por valor** (una variable `mut` cambiada después no se ve dentro); lambda sin contexto de tipos exige anotación |
| Chequeo «movido tras enviar» (E1101) | Red dinámica en ejecución y `--ownership-check` estático sobre la IR; falta integrarlo al checker por defecto |
| Paralelismo nativo (hilos, canales bloqueantes, `select`) | No existe todavía; ambos backends tienen scheduler cooperativo |
| Memoria en nativo | Registro de allocations y limpieza global al salir; ARC/último uso y destructores por tipo siguen pendientes |
| IR de bloques | HIR→CFG disponible con `--ir`; `if/while/for/match/try/spawn/channel` ya tienen operaciones explícitas, aún no reemplaza el backend C |
| Ownership/último uso | `--ownership-report`, `--ownership-check` y `--ownership-ir`; inserta solo `release` en transferencias lineales demostrables, sin ARC completa |
| Biblioteca estándar | Mínima: sin `HashMap` eficiente, fechas, red, formateo, `args`, entorno |
| `==` sobre `List/Option/Map` | No soportado (tampoco en intérprete para Option) |
| Mensajes de error de E/S | `strerror` ≠ texto de Rust (difieren entre backends) |
| `Result<Void,E>` | Campo de valor de relleno (`char`) en C |
| Migración HIR | Escalares, records, enums/match, Option/Result, colecciones, cierres, instancias concretas de genéricos, llamadas anidadas, records/enums genéricos aplicados y métodos genéricos centrales migrados; formas complejas restantes siguen con fallback |
| Paquetes | Diseño y lockfile básicos; sin registro remoto (decisión: **no** añadir red automática al compilador) |
| Rendimiento del intérprete | Tree‑walking simple; sin optimizaciones |
| `newlines.ostrin`, `advanced.ostrin` | Son muestras de sintaxis, no programas ejecutables |
| CI | Linux, macOS y Windows; incluye las pruebas diferenciales intérprete↔nativo |
| Distribución | Sin instalador ni binarios publicados; `.exe` de aplicación pendiente |

Deuda técnica notable: `codegen.rs` y `typeck/mod.rs` son archivos muy grandes y
convendría dividirlos; el backend nativo no comparte el sistema de tipos del checker
(ya consume los tipos del checker y compara cada nodo; **119 funciones/métodos de los ejemplos
ya se generan desde el HIR** —escalares, records, enums, `match`, `Option`/`Result`,
listas/colecciones, cierres, instancias concretas de genéricos, records/enums aplicados y métodos
genéricos centrales, módulo `hir_c.rs`—, con un trinquete mínimo de 115; el resto sigue por el AST;
ver documento 20 y secciones 123–133 de `CONTEXTO_PROYECTO.md`);
la búsqueda en `Map/Set` es lineal.

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
El scheduler cooperativo ya existe en intérprete y backend C. El siguiente salto es añadir
hilos del sistema operativo y canales bloqueantes sin romper E1100/E1101; después vendrán
cancelación y `select` (ya esbozados en `docs/design/09`). Sigue siendo el mayor salto de
capacidad y riesgo.

### C. Biblioteca estándar y ecosistema
Cadenas (split/trim/format), fechas, JSON, argumentos y entorno, `HashMap` real,
formateo de floats configurable; un `std` en Ostrin propio compilable por ambos backends.
Después: paquetes con registro (con consentimiento explícito del usuario para la red).

### D. Experiencia de desarrollador
Formateador oficial (`ostrinc fmt`), `ostrinc test`, documentación generada, acciones de
código en LSP (quick fixes), inlay hints, publicación de la extensión en el Marketplace,
CI de GitHub con matriz Windows/Linux/macOS, binarios de release.

### E. Backends adicionales
WebAssembly (desde el mismo C con clang/emscripten, o generación directa), LLVM IR.
El plan de la hoja de ruta original lista WebAssembly y bindings de plataforma.

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
cargo test                                   # 6 diferenciales + 115 de integración
cargo run -- --run ..\examples\physics.ostrin
cargo run -- --compile ..\examples\collections.ostrin
```

Ejemplos nativos dedicados: `native_*.ostrin` (records, métodos, enums, genéricos,
dyn, listas, closures, option, result, unidades, display, derive, args nombrados,
colecciones, trait defaults, métodos genéricos, builtins, concurrencia).
