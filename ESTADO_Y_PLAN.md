# Ostrin — estado del proyecto y plan de avance

*Corte: 2026-09-18 · commit `90e525e` · 93 pruebas de integración en verde, sin warnings.*

Este documento resume **qué existe hoy**, **qué no**, y **por dónde se puede avanzar**.
Para la historia detallada, ver `CONTEXTO_PROYECTO.md` (secciones 1–84); para el diseño
del lenguaje, `docs/design/` (17 documentos).

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
| Intérprete | `interpreter/mod.rs` | Ejecución tree‑walking; referencia semántica |
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

**Funciones**: genéricas con cotas, argumentos nombrados y por defecto, lambdas,
`try … catch`, recursión.

**Traits**: traits nominales, métodos por defecto, herencia de traits, `impl` genéricos
y especializados (`impl X for Box<Int>`), `impl` sobre `Quantity<D>`, `dyn Trait`,
sobrecarga de operadores vía `impl Add/Eq/Ord…`.

**Control**: `if/else`, `while`, `for` (rangos `to`/`until`, listas, canales, iteradores
propios con `next`), `match` con guardas, patrones anidados, rangos y destructuración.

**Concurrencia (simulada)**: `spawn`, `join`, `channel<T>()`, `send/receive/close`;
`spawn` se ejecuta de forma síncrona e inmediata. Se verifica en el checker que no se
capturen bindings `mut` (E1100) y en ejecución que un record enviado no se reutilice (E1101).

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

**Soportado** (todos los ejemplos ejecutables del repo, salvo lo listado en §6):
- Escalares, strings, recursión, `if/while/for`, `match` (con guardas y patrones anidados).
- Records (heap, por referencia) y enums (unión etiquetada por valor), ambos genéricos;
  métodos estáticos y genéricos; métodos por defecto de traits.
- `dyn Trait` con vtable real (único punto de despacho en tiempo de ejecución).
- `List/Map/Set/Option/Result`, combinadores con lambdas expandidas en línea, `try`.
- `Quantity` (dimensión estática, unidad como cadena en ejecución) e `impl` sobre cantidades.
- Operadores de usuario, `derive(Eq/Ord)`, `Ordering` incorporado.
- `print` de records, enums, listas, mapas, sets, `Option`, `Result` (mismo formato que el intérprete).
- Argumentos nombrados y por defecto, iteradores propios, funciones incorporadas de E/S.
- `spawn`/`join`/canales (modelo síncrono, igual que el intérprete).

**Rechazado a propósito con mensaje claro** (mejor error que comportamiento distinto):
enviar un *record* por un canal (no hay equivalente del chequeo E1101), `for` sobre `Map/Set`
(el intérprete tampoco lo permite).

---

## 6. Brechas conocidas

| Área | Estado |
|---|---|
| Valores de función de primera clase (guardar/pasar `fn` como valor) | Solo intérprete; nativo solo admite lambdas *en línea* en combinadores |
| Chequeo «movido tras enviar» (E1101) | Solo intérprete (dinámico) |
| Concurrencia real (hilos, planificador, `select`) | No existe; `spawn` es síncrono |
| Memoria en nativo | Se usa `malloc` sin liberar (sin GC ni conteo de referencias) |
| Biblioteca estándar | Mínima: sin `HashMap` eficiente, fechas, red, formateo, `args`, entorno |
| `==` sobre `List/Option/Map` | No soportado (tampoco en intérprete para Option) |
| Mensajes de error de E/S | `strerror` ≠ texto de Rust (difieren entre backends) |
| `Result<Void,E>` | Campo de valor de relleno (`char`) en C |
| Paquetes | Diseño y lockfile básicos; sin registro remoto (decisión: **no** añadir red automática al compilador) |
| Rendimiento del intérprete | Tree‑walking simple; sin optimizaciones |
| `newlines.ostrin`, `advanced.ostrin` | Son muestras de sintaxis, no programas ejecutables |
| Distribución | Sin instalador ni binarios publicados; `.exe` de aplicación pendiente |

Deuda técnica notable: `codegen.rs` y `typeck/mod.rs` son archivos muy grandes y
convendría dividirlos; el backend nativo no comparte el sistema de tipos del checker
(reinfiere por su cuenta, con una inferencia bidireccional mínima vía `expected`);
la búsqueda en `Map/Set` es lineal.

---

## 7. Opciones de avance

Ordenadas por mi recomendación (valor / riesgo). Cada una es independiente.

### A. Cerrar la semántica del backend nativo (corto plazo)
1. **Chequeo E1101 nativo** (records por canal): seguimiento estático de uso tras `send`.
2. **Funciones como valores** (punteros a función + cierres con entorno capturado).
3. **Gestión de memoria**: conteo de referencias o arena por ámbito; hoy nada se libera.
4. **`==` estructural** para `List/Option/Map`.
5. Reutilizar el checker: que `typeck` entregue tipos resueltos al codegen y eliminar la
   reinferencia (reduce errores y abre optimizaciones).

### B. Concurrencia real (medio plazo, requiere diseño)
Decidir modelo: hilos de SO + canales bloqueantes, o tareas cooperativas (async). Implica
revisar E1100/E1101, `select`, cancelación (ya esbozados en `docs/design/09`). Es el mayor
salto de capacidad y el de mayor riesgo.

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
cargo test                                   # 93 pruebas
cargo run -- --run ..\examples\physics.ostrin
cargo run -- --compile ..\examples\collections.ostrin
```

Ejemplos nativos dedicados: `native_*.ostrin` (records, métodos, enums, genéricos,
dyn, listas, closures, option, result, unidades, display, derive, args nombrados,
colecciones, trait defaults, métodos genéricos, builtins, concurrencia).
